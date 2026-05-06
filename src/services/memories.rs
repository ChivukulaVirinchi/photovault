//! Memories — "N years ago today" style rediscovery.
//!
//! Three generators (OnThisDay, FallbackWindow, SeasonalRecap), one ranker,
//! one hero-selector, one block filter. Memories are computed per-day from
//! `photos` / `faces` / `memory_blocks`; nothing is persisted except the
//! user's block preferences.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, Result as SqliteResult};

pub type MemoryId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    OnThisDay,
    FallbackWindow,
    SeasonalRecap,
    /// Ultimate fallback: surfaces "X years ago" with photos from a prior
    /// year when no other generator produced anything. Guarantees a sparse
    /// library still sees something if it has any history at all.
    YearRecap,
}

#[derive(Debug, Clone)]
pub struct Memory {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub title: String,
    pub photo_ids: Vec<i64>,
    pub hero_photo_id: i64,
    /// Relative thumbnail path for the chosen hero (set during hero
    /// selection). None if the hero photo hasn't had a thumbnail
    /// generated yet.
    pub hero_thumbnail_path: Option<String>,
    pub score: f32,
    pub year: i32,
    pub has_faces: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryCard {
    pub id: MemoryId,
    /// Kind tag — used by tests and reserved for future filter UI.
    #[allow(dead_code)]
    pub kind: MemoryKind,
    pub title: String,
    pub hero_photo_id: i64,
    pub hero_thumbnail_path: Option<String>,
    pub photo_count: usize,
    pub photo_ids: Vec<i64>,
}

impl From<Memory> for MemoryCard {
    fn from(m: Memory) -> Self {
        Self {
            id: m.id,
            kind: m.kind,
            title: m.title,
            hero_photo_id: m.hero_photo_id,
            hero_thumbnail_path: m.hero_thumbnail_path,
            photo_count: m.photo_ids.len(),
            photo_ids: m.photo_ids,
        }
    }
}

/// Minimum library age before Memories surface. If the oldest non-trashed
/// photo is newer than this many months ago, hide the feature.
const MIN_LIBRARY_AGE_MONTHS: i64 = 3;

/// Minimum age (in months) for a photo to appear in memories. Photos newer
/// than this are too recent to be nostalgic.
const MIN_PHOTO_AGE_MONTHS: i64 = 3;

/// Maximum memory cards returned by the generator.
const MAX_CARDS: usize = 20;

/// SeasonalRecap threshold — month needs at least this many photos to
/// surface as a recap. Lowered from 10 so sparse libraries (a few dozen
/// photos a year) can still see monthly memories.
const SEASONAL_MIN_PHOTOS: i64 = 5;

/// Cap on photos per YearRecap card.
const YEAR_RECAP_MAX_PHOTOS: usize = 50;

/// Top entry point: full pipeline. Runs all three generators, scores,
/// filters blocks, picks heroes, returns cards.
pub fn generate_for_today(conn: &Connection, today: NaiveDate) -> Result<Vec<MemoryCard>, String> {
    let current_year = today.year();

    let mut all: Vec<Memory> = Vec::new();

    let on_this_day_results = on_this_day(conn, today, current_year)
        .map_err(|e| format!("OnThisDay query failed: {}", e))?;
    let had_exact = !on_this_day_results.is_empty();
    all.extend(on_this_day_results);

    if !had_exact {
        let fb = fallback_window(conn, today, current_year)
            .map_err(|e| format!("FallbackWindow query failed: {}", e))?;
        all.extend(fb);
    }

    let seasonal = seasonal_recap(conn, today, current_year)
        .map_err(|e| format!("SeasonalRecap query failed: {}", e))?;
    all.extend(seasonal);

    // Ultimate fallback. Only fires when no specific anniversary or season
    // qualifies; surfaces "X years ago" cards from prior years so a sparse
    // library still has something to show.
    if all.is_empty() {
        let recaps = year_recap(conn, today, current_year)
            .map_err(|e| format!("YearRecap query failed: {}", e))?;
        all.extend(recaps);
    }

    if all.is_empty() {
        return Ok(Vec::new());
    }

    populate_hero_and_faces(conn, &mut all).map_err(|e| format!("Hero selection failed: {}", e))?;
    rank(&mut all, current_year);

    let blocks = load_person_blocks(conn).map_err(|e| format!("Block load failed: {}", e))?;
    if !blocks.is_empty() {
        filter_blocked(conn, &mut all, &blocks)
            .map_err(|e| format!("Block filter failed: {}", e))?;
    }

    if all.len() > MAX_CARDS {
        all.truncate(MAX_CARDS);
    }

    Ok(all.into_iter().map(MemoryCard::from).collect())
}

/// Check whether the library has enough history to justify Memories.
pub fn library_is_old_enough(conn: &Connection, today: NaiveDate) -> bool {
    let oldest: Option<String> = conn
        .query_row(
            "SELECT MIN(date_taken) FROM photos WHERE is_trashed = FALSE AND date_taken IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    let Some(oldest_str) = oldest else {
        return false;
    };

    // Parse "YYYY-MM-DD HH:MM:SS" or "YYYY-MM-DD" prefix.
    let parsed = chrono::NaiveDateTime::parse_from_str(&oldest_str, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.date())
        .or_else(|_| {
            NaiveDate::parse_from_str(&oldest_str[..oldest_str.len().min(10)], "%Y-%m-%d")
        });

    let Ok(oldest_date) = parsed else {
        return false;
    };

    let age_days = today.signed_duration_since(oldest_date).num_days();
    age_days >= MIN_LIBRARY_AGE_MONTHS * 30
}

fn memory_id(kind: MemoryKind, year: i32, today: NaiveDate) -> MemoryId {
    format!(
        "{}-{}-{:02}-{:02}",
        match kind {
            MemoryKind::OnThisDay => "otd",
            MemoryKind::FallbackWindow => "fw",
            MemoryKind::SeasonalRecap => "sr",
            MemoryKind::YearRecap => "yr",
        },
        year,
        today.month(),
        today.day()
    )
}

fn parse_ids(csv: &str) -> Vec<i64> {
    csv.split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect()
}

// ---------- Generators ----------

fn on_this_day(
    conn: &Connection,
    today: NaiveDate,
    _current_year: i32,
) -> SqliteResult<Vec<Memory>> {
    let today_md = format!("{:02}-{:02}", today.month(), today.day());
    // Include same-year photos as long as they're old enough (>= MIN_PHOTO_AGE_MONTHS).
    // The cutoff date excludes photos from the recent N months.
    let cutoff = today - Duration::days(MIN_PHOTO_AGE_MONTHS * 30);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
    let mut stmt = conn.prepare(
        r#"
        SELECT CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
               GROUP_CONCAT(id) AS photo_ids
        FROM photos
        WHERE is_trashed = FALSE
          AND content_category = 'photo'
          AND date_taken IS NOT NULL
          AND strftime('%m-%d', date_taken) = ?1
          AND date_taken < ?2
        GROUP BY yr
        ORDER BY yr DESC
        LIMIT 10
        "#,
    )?;

    let rows = stmt.query_map(params![today_md, cutoff_str], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (yr, csv) = row?;
        let photo_ids = parse_ids(&csv);
        if photo_ids.is_empty() {
            continue;
        }
        let photo_date = NaiveDate::from_ymd_opt(yr, today.month(), today.day())
            .unwrap_or(NaiveDate::from_ymd_opt(yr, 1, 1).unwrap());
        let title = format!("{} today", age_label(photo_date, today));
        out.push(Memory {
            id: memory_id(MemoryKind::OnThisDay, yr, today),
            kind: MemoryKind::OnThisDay,
            title,
            photo_ids,
            hero_photo_id: 0,
            hero_thumbnail_path: None,
            score: 0.0,
            year: yr,
            has_faces: false,
        });
    }
    Ok(out)
}

fn fallback_window(
    conn: &Connection,
    today: NaiveDate,
    _current_year: i32,
) -> SqliteResult<Vec<Memory>> {
    let mds: Vec<String> = (-3..=3)
        .map(|offset| {
            let d = today + Duration::days(offset);
            format!("{:02}-{:02}", d.month(), d.day())
        })
        .collect();

    let cutoff = today - Duration::days(MIN_PHOTO_AGE_MONTHS * 30);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

    let mut stmt = conn.prepare(
        r#"
        SELECT CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
               GROUP_CONCAT(id) AS photo_ids
        FROM photos
        WHERE is_trashed = FALSE
          AND content_category = 'photo'
          AND date_taken IS NOT NULL
          AND strftime('%m-%d', date_taken) IN (?1, ?2, ?3, ?4, ?5, ?6, ?7)
          AND date_taken < ?8
        GROUP BY yr
        ORDER BY yr DESC
        LIMIT 10
        "#,
    )?;

    let rows = stmt.query_map(
        params![mds[0], mds[1], mds[2], mds[3], mds[4], mds[5], mds[6], cutoff_str],
        |row| Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?)),
    )?;

    let mut out = Vec::new();
    for row in rows {
        let (yr, csv) = row?;
        let photo_ids = parse_ids(&csv);
        if photo_ids.is_empty() {
            continue;
        }
        let photo_date = NaiveDate::from_ymd_opt(yr, today.month(), today.day())
            .unwrap_or(NaiveDate::from_ymd_opt(yr, 1, 1).unwrap());
        let title = format!("{} this week", age_label(photo_date, today));
        out.push(Memory {
            id: memory_id(MemoryKind::FallbackWindow, yr, today),
            kind: MemoryKind::FallbackWindow,
            title,
            photo_ids,
            hero_photo_id: 0,
            hero_thumbnail_path: None,
            score: 0.0,
            year: yr,
            has_faces: false,
        });
    }
    Ok(out)
}

fn seasonal_recap(
    conn: &Connection,
    today: NaiveDate,
    _current_year: i32,
) -> SqliteResult<Vec<Memory>> {
    let month = format!("{:02}", today.month());
    let cutoff = today - Duration::days(MIN_PHOTO_AGE_MONTHS * 30);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

    let mut stmt = conn.prepare(
        r#"
        SELECT CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
               GROUP_CONCAT(id) AS photo_ids,
               COUNT(*) AS photo_count
        FROM photos
        WHERE is_trashed = FALSE
          AND content_category = 'photo'
          AND date_taken IS NOT NULL
          AND strftime('%m', date_taken) = ?1
          AND date_taken < ?2
        GROUP BY yr
        HAVING photo_count >= ?3
        ORDER BY yr DESC
        LIMIT 5
        "#,
    )?;

    let rows = stmt.query_map(params![month, cutoff_str, SEASONAL_MIN_PHOTOS], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;

    let month_name = month_name(today.month());
    let mut out = Vec::new();
    for row in rows {
        let (yr, csv) = row?;
        let photo_ids = parse_ids(&csv);
        if photo_ids.is_empty() {
            continue;
        }
        out.push(Memory {
            id: memory_id(MemoryKind::SeasonalRecap, yr, today),
            kind: MemoryKind::SeasonalRecap,
            title: format!("{} {}", month_name, yr),
            photo_ids,
            hero_photo_id: 0,
            hero_thumbnail_path: None,
            score: 0.0,
            year: yr,
            has_faces: false,
        });
    }
    Ok(out)
}

fn year_recap(
    conn: &Connection,
    today: NaiveDate,
    _current_year: i32,
) -> SqliteResult<Vec<Memory>> {
    let cutoff = today - Duration::days(MIN_PHOTO_AGE_MONTHS * 30);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

    let mut stmt = conn.prepare(
        r#"
        SELECT CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
               GROUP_CONCAT(id) AS photo_ids
        FROM photos
        WHERE is_trashed = FALSE
          AND content_category = 'photo'
          AND date_taken IS NOT NULL
          AND date_taken < ?1
        GROUP BY yr
        ORDER BY yr DESC
        LIMIT 5
        "#,
    )?;

    let rows = stmt.query_map(params![cutoff_str], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (yr, csv) = row?;
        let mut photo_ids = parse_ids(&csv);
        if photo_ids.is_empty() {
            continue;
        }
        photo_ids.sort_unstable();
        photo_ids.reverse();
        photo_ids.truncate(YEAR_RECAP_MAX_PHOTOS);

        // Use July 1 of that year as a reasonable midpoint for age calculation.
        let photo_date =
            NaiveDate::from_ymd_opt(yr, 7, 1).unwrap_or(NaiveDate::from_ymd_opt(yr, 1, 1).unwrap());
        let title = age_label(photo_date, today);
        out.push(Memory {
            id: memory_id(MemoryKind::YearRecap, yr, today),
            kind: MemoryKind::YearRecap,
            title,
            photo_ids,
            hero_photo_id: 0,
            hero_thumbnail_path: None,
            score: 0.0,
            year: yr,
            has_faces: false,
        });
    }
    Ok(out)
}

fn month_name(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

/// Human-readable age string from a photo date to today.
/// Uses months when < 24 months; years otherwise.
fn age_label(photo_date: NaiveDate, today: NaiveDate) -> String {
    if photo_date >= today {
        return "Recent".to_string();
    }

    let mut months = (today.year() - photo_date.year()) * 12
        + (today.month() as i32 - photo_date.month() as i32);
    if today.day() < photo_date.day() {
        months -= 1;
    }
    let months = months.max(0);

    if months < 12 {
        match months {
            0 => "Recent".to_string(),
            1 => "1 month ago".to_string(),
            _ => format!("{} months ago", months),
        }
    } else {
        let years = months / 12;
        if years == 1 {
            "1 year ago".to_string()
        } else {
            format!("{} years ago", years)
        }
    }
}

// ---------- Hero selection ----------

/// Populate `hero_photo_id` and `has_faces` on each memory via a single
/// batched query over all unique photo_ids.
fn populate_hero_and_faces(conn: &Connection, memories: &mut [Memory]) -> SqliteResult<()> {
    // Collect all unique photo ids.
    let mut all_ids: HashSet<i64> = HashSet::new();
    for m in memories.iter() {
        all_ids.extend(m.photo_ids.iter().copied());
    }
    if all_ids.is_empty() {
        return Ok(());
    }

    let ids_list: Vec<String> = all_ids.iter().map(|i| i.to_string()).collect();
    let in_clause = ids_list.join(",");
    let sql = format!(
        r#"
        SELECT p.id,
               COALESCE(p.orientation, 1) AS orientation,
               COALESCE(p.width, 0) AS width,
               COALESCE(p.height, 0) AS height,
               COALESCE((SELECT COUNT(*) FROM faces WHERE photo_id = p.id), 0) AS face_count,
               p.thumbnail_path
        FROM photos p
        WHERE p.id IN ({}) AND p.is_trashed = FALSE
        "#,
        in_clause
    );

    #[derive(Clone)]
    struct PhotoMeta {
        orientation: i32,
        width: i32,
        height: i32,
        face_count: i64,
        thumbnail_path: Option<String>,
    }

    let mut stmt = conn.prepare(&sql)?;
    let mut meta: HashMap<i64, PhotoMeta> = HashMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            PhotoMeta {
                orientation: row.get(1)?,
                width: row.get(2)?,
                height: row.get(3)?,
                face_count: row.get(4)?,
                thumbnail_path: row.get(5)?,
            },
        ))
    })?;
    for row in rows {
        let (id, m) = row?;
        meta.insert(id, m);
    }

    for mem in memories.iter_mut() {
        let mut best_id = mem.photo_ids[0];
        let mut best_score = f32::MIN;
        let mut any_faces = false;

        for pid in &mem.photo_ids {
            let Some(m) = meta.get(pid) else { continue };

            // Landscape orientations are 1 (unrotated wide) and 3 (180).
            // 6/8 are rotated portraits. Treat width >= height as landscape too.
            let is_landscape = matches!(m.orientation, 1 | 3) && m.width >= m.height && m.width > 0;

            let mut score = 0.0_f32;
            if is_landscape {
                score += 3.0;
            }
            if m.face_count > 0 {
                score += 2.0;
                any_faces = true;
            }
            if m.face_count >= 2 {
                score += 1.0;
            }
            // Prefer photos that already have a thumbnail — avoids
            // an empty card while on-demand generation runs in the
            // background.
            if m.thumbnail_path.is_some() {
                score += 0.5;
            }
            // Tie-breaker toward smaller id (stable).
            score -= (*pid as f32) * 1e-9;

            if score > best_score {
                best_score = score;
                best_id = *pid;
            }
        }

        mem.hero_photo_id = best_id;
        mem.hero_thumbnail_path = meta.get(&best_id).and_then(|m| m.thumbnail_path.clone());
        mem.has_faces = any_faces;
    }

    Ok(())
}

// ---------- Ranking ----------

fn rank(memories: &mut [Memory], current_year: i32) {
    for m in memories.iter_mut() {
        let years_ago = (current_year - m.year).max(0);
        let count_factor = ((m.photo_ids.len() as f32) + 1.0).log2();
        let age_factor = ((years_ago as f32) + 1.0).sqrt();
        let face_factor = if m.has_faces { 1.3 } else { 1.0 };
        let kind_factor = match m.kind {
            MemoryKind::SeasonalRecap => 1.5,
            _ => 1.0,
        };
        m.score = count_factor * age_factor * face_factor * kind_factor;
    }
    memories.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

// ---------- Block filter ----------

fn load_person_blocks(conn: &Connection) -> SqliteResult<HashSet<i64>> {
    let mut stmt = conn.prepare("SELECT target_key FROM memory_blocks WHERE kind = 'person'")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for row in rows {
        if let Ok(id) = row?.parse::<i64>() {
            out.insert(id);
        }
    }
    Ok(out)
}

/// Drop memories where > 50% of photos contain a face in a blocked cluster.
fn filter_blocked(
    conn: &Connection,
    memories: &mut Vec<Memory>,
    blocks: &HashSet<i64>,
) -> SqliteResult<()> {
    if memories.is_empty() || blocks.is_empty() {
        return Ok(());
    }

    // Collect all photo ids.
    let mut all_ids: HashSet<i64> = HashSet::new();
    for m in memories.iter() {
        all_ids.extend(m.photo_ids.iter().copied());
    }
    let ids_list: Vec<String> = all_ids.iter().map(|i| i.to_string()).collect();
    let blocks_list: Vec<String> = blocks.iter().map(|i| i.to_string()).collect();

    let sql = format!(
        "SELECT DISTINCT photo_id FROM faces
         WHERE cluster_id IN ({}) AND photo_id IN ({})",
        blocks_list.join(","),
        ids_list.join(",")
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    let blocked_photos: HashSet<i64> = rows.filter_map(|r| r.ok()).collect();

    memories.retain(|m| {
        let blocked_count = m
            .photo_ids
            .iter()
            .filter(|id| blocked_photos.contains(id))
            .count();
        blocked_count * 2 <= m.photo_ids.len() // keep if blocked photos are not majority
    });

    Ok(())
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::create_schema(&conn).unwrap();
        conn
    }

    fn insert_photo(conn: &Connection, id: i64, date: &str) {
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken, width, height, orientation) \
             VALUES (?1, ?2, ?3, 'h', 1, ?4, 1920, 1080, 1)",
            rusqlite::params![
                id,
                format!("photo{}.jpg", id),
                format!("photo{}.jpg", id),
                date,
            ],
        )
        .unwrap();
    }

    #[test]
    fn empty_library_returns_empty() {
        let conn = fresh_db();
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert!(cards.is_empty());
    }

    #[test]
    fn on_this_day_matches_prior_year() {
        let conn = fresh_db();
        insert_photo(&conn, 1, "2022-04-15 10:00:00");
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].kind, MemoryKind::OnThisDay);
        assert_eq!(cards[0].photo_count, 1);
    }

    #[test]
    fn seasonal_recap_respects_threshold() {
        let conn = fresh_db();
        // 4 photos in April 2020 — below SEASONAL_MIN_PHOTOS (5).
        for i in 0..4 {
            insert_photo(&conn, 100 + i, "2020-04-03 10:00:00");
        }
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert!(cards.iter().all(|c| c.kind != MemoryKind::SeasonalRecap));
    }

    #[test]
    fn seasonal_recap_surfaces_above_threshold() {
        let conn = fresh_db();
        // 5 photos in April 2020 — exactly at threshold.
        for i in 0..5 {
            insert_photo(&conn, 200 + i, "2020-04-03 10:00:00");
        }
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert!(cards.iter().any(|c| c.kind == MemoryKind::SeasonalRecap));
    }

    #[test]
    fn fallback_only_when_on_this_day_empty() {
        let conn = fresh_db();
        insert_photo(&conn, 1, "2023-04-13 10:00:00"); // 3 days before, NOT today
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].kind, MemoryKind::FallbackWindow);
    }

    #[test]
    fn fallback_skipped_if_on_this_day_present() {
        let conn = fresh_db();
        insert_photo(&conn, 1, "2022-04-15 10:00:00"); // exact today
        insert_photo(&conn, 2, "2023-04-13 10:00:00"); // ±3 window
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert!(cards.iter().all(|c| c.kind != MemoryKind::FallbackWindow));
    }

    #[test]
    fn year_recap_fires_when_nothing_else_qualifies() {
        let conn = fresh_db();
        // Sparse library: 5 photos all on Jan 5, 2022 - won't match today's
        // exact date, won't fill the ±3 day window of today (April 15), won't
        // hit the seasonal threshold for April. YearRecap should still fire.
        for i in 0..5 {
            insert_photo(&conn, 300 + i, "2022-01-05 10:00:00");
        }
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert!(cards.iter().any(|c| c.kind == MemoryKind::YearRecap));
        // And no on-this-day / fallback / seasonal should be present.
        assert!(cards.iter().all(|c| c.kind == MemoryKind::YearRecap));
    }

    #[test]
    fn year_recap_does_not_fire_when_others_qualify() {
        let conn = fresh_db();
        // Photo from today's date 4 years ago — on_this_day will fire.
        insert_photo(&conn, 1, "2022-04-15 10:00:00");
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        assert!(cards.iter().any(|c| c.kind == MemoryKind::OnThisDay));
        assert!(cards.iter().all(|c| c.kind != MemoryKind::YearRecap));
    }

    #[test]
    fn library_age_gate() {
        let conn = fresh_db();
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();

        // No photos → not old enough.
        assert!(!library_is_old_enough(&conn, today));

        // Photo from 2 months ago → still not old enough.
        insert_photo(&conn, 1, "2026-02-15 10:00:00");
        assert!(!library_is_old_enough(&conn, today));

        // Photo from 1 year ago → old enough.
        insert_photo(&conn, 2, "2025-04-15 10:00:00");
        assert!(library_is_old_enough(&conn, today));
    }

    #[test]
    fn memory_id_is_stable() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let a = memory_id(MemoryKind::OnThisDay, 2022, today);
        let b = memory_id(MemoryKind::OnThisDay, 2022, today);
        assert_eq!(a, b);
        let c = memory_id(MemoryKind::OnThisDay, 2021, today);
        assert_ne!(a, c);
    }
}
