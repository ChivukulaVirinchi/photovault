//! Memories — "N years ago today" style rediscovery.
//!
//! Three generators (OnThisDay, FallbackWindow, SeasonalRecap), one ranker,
//! one hero-selector, one block filter. Memories are computed per-day from
//! `photos` / `faces` / `memory_blocks`; nothing is persisted except the
//! user's block preferences.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, Result as SqliteResult};

use crate::services::semantic::{SemanticIndexCache, SemanticSearchService, SEMANTIC_MODEL_KEY};

pub type MemoryId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    OnThisDay,
    FallbackWindow,
    SeasonalRecap,
    PersonStory,
    PlaceStory,
    VisualPattern,
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
const MAX_MEMORY_PHOTOS: usize = 500;

/// Top entry point: full pipeline. Runs all three generators, scores,
/// filters blocks, picks heroes, returns cards.
pub fn generate_for_today(conn: &Connection, today: NaiveDate) -> Result<Vec<MemoryCard>, String> {
    generate_for_today_inner(conn, today, None)
}

pub fn generate_for_today_with_semantic(
    conn: &Connection,
    today: NaiveDate,
    drive_root: &Path,
    cache: &mut SemanticIndexCache,
) -> Result<Vec<MemoryCard>, String> {
    let visual = visual_pattern(conn, today, drive_root, cache).unwrap_or_else(|err| {
        tracing::debug!("visual-pattern memory skipped: {err}");
        None
    });
    generate_for_today_inner(conn, today, visual)
}

fn generate_for_today_inner(
    conn: &Connection,
    today: NaiveDate,
    visual: Option<Memory>,
) -> Result<Vec<MemoryCard>, String> {
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

    if let Some(person) =
        person_story(conn, today).map_err(|e| format!("PersonStory query failed: {e}"))?
    {
        all.push(person);
    }
    if let Some(place) =
        place_story(conn, today).map_err(|e| format!("PlaceStory query failed: {e}"))?
    {
        all.push(place);
    }
    if let Some(visual) = visual {
        all.push(visual);
    }

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
            MemoryKind::PersonStory => "person",
            MemoryKind::PlaceStory => "place",
            MemoryKind::VisualPattern => "visual",
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
        WITH matched AS (
            SELECT id,
                   CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
                   ROW_NUMBER() OVER (
                       PARTITION BY CAST(strftime('%Y', date_taken) AS INTEGER)
                       ORDER BY date_taken DESC, id DESC
                   ) AS rn
            FROM photos
            WHERE is_trashed = FALSE
              AND content_category = 'photo'
              AND date_taken IS NOT NULL
              AND strftime('%m-%d', date_taken) = ?1
              AND date_taken < ?2
        )
        SELECT yr, GROUP_CONCAT(id) AS photo_ids
        FROM matched
        WHERE rn <= ?3
        GROUP BY yr
        ORDER BY yr DESC
        LIMIT 10
        "#,
    )?;

    let rows = stmt.query_map(
        params![today_md, cutoff_str, MAX_MEMORY_PHOTOS as i64],
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
        WITH matched AS (
            SELECT id,
                   CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
                   ROW_NUMBER() OVER (
                       PARTITION BY CAST(strftime('%Y', date_taken) AS INTEGER)
                       ORDER BY date_taken DESC, id DESC
                   ) AS rn
            FROM photos
            WHERE is_trashed = FALSE
              AND content_category = 'photo'
              AND date_taken IS NOT NULL
              AND strftime('%m-%d', date_taken) IN (?1, ?2, ?3, ?4, ?5, ?6, ?7)
              AND date_taken < ?8
        )
        SELECT yr, GROUP_CONCAT(id) AS photo_ids
        FROM matched
        WHERE rn <= ?9
        GROUP BY yr
        ORDER BY yr DESC
        LIMIT 10
        "#,
    )?;

    let rows = stmt.query_map(
        params![
            mds[0],
            mds[1],
            mds[2],
            mds[3],
            mds[4],
            mds[5],
            mds[6],
            cutoff_str,
            MAX_MEMORY_PHOTOS as i64
        ],
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
        WITH matched AS (
            SELECT id,
                   CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
                   COUNT(*) OVER (
                       PARTITION BY CAST(strftime('%Y', date_taken) AS INTEGER)
                   ) AS total_count,
                   ROW_NUMBER() OVER (
                       PARTITION BY CAST(strftime('%Y', date_taken) AS INTEGER)
                       ORDER BY date_taken DESC, id DESC
                   ) AS rn
            FROM photos
            WHERE is_trashed = FALSE
              AND content_category = 'photo'
              AND date_taken IS NOT NULL
              AND strftime('%m', date_taken) = ?1
              AND date_taken < ?2
        )
        SELECT yr, GROUP_CONCAT(id) AS photo_ids
        FROM matched
        WHERE rn <= ?4
        GROUP BY yr
        HAVING MAX(total_count) >= ?3
        ORDER BY yr DESC
        LIMIT 5
        "#,
    )?;

    let rows = stmt.query_map(
        params![
            month,
            cutoff_str,
            SEASONAL_MIN_PHOTOS,
            MAX_MEMORY_PHOTOS as i64
        ],
        |row| Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?)),
    )?;

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
            title: format!("{} {}, worth another look", month_name, yr),
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

/// Find one recurring visual pattern without trying to name it. The image
/// embeddings do the discovery directly; dates make sure the result is a
/// genuine rediscovery rather than a burst or duplicate set.
fn visual_pattern(
    conn: &Connection,
    today: NaiveDate,
    drive_root: &Path,
    cache: &mut SemanticIndexCache,
) -> Result<Option<Memory>, String> {
    let cutoff = (today - Duration::days(MIN_PHOTO_AGE_MONTHS * 30))
        .format("%Y-%m-%d")
        .to_string();
    let mut stmt = conn
        .prepare(
            r#"SELECT p.id
               FROM photos p
               JOIN semantic_index_state s
                 ON s.photo_id = p.id AND s.model_key = ?1 AND s.status = 'indexed'
               WHERE p.is_trashed = FALSE
                 AND p.content_category = 'photo'
                 AND p.date_taken IS NOT NULL AND p.date_taken < ?2
               ORDER BY p.is_favorite DESC,
                        p.date_taken DESC, p.id DESC
               LIMIT 64"#,
        )
        .map_err(|e| e.to_string())?;
    let seeds = stmt
        .query_map(params![SEMANTIC_MODEL_KEY, cutoff], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| e.to_string())?
        .collect::<SqliteResult<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    if seeds.is_empty() {
        return Ok(None);
    }

    let service = SemanticSearchService::new(drive_root);
    let start = today.ordinal0() as usize % seeds.len();
    for offset in 0..seeds.len().min(12) {
        let seed = seeds[(start + offset) % seeds.len()];
        let neighbors = service
            .similar_to_photo_cached(conn, cache, seed, 48)
            .map_err(|e| e.to_string())?;
        let Some(top_score) = neighbors.first().map(|candidate| candidate.score) else {
            continue;
        };
        if top_score < 0.70 {
            continue;
        }
        let threshold = (top_score - 0.04).max(top_score * 0.90);
        let candidate_ids: Vec<i64> = std::iter::once(seed)
            .chain(
                neighbors
                    .into_iter()
                    .filter(|candidate| candidate.score >= threshold)
                    .map(|candidate| candidate.photo_id),
            )
            .collect();
        let dated = dated_active_photos(conn, &candidate_ids)?;
        let mut seen_days = HashSet::new();
        let mut photo_ids = Vec::new();
        let mut years = HashSet::new();
        let mut first_year = today.year();
        for (photo_id, date) in dated {
            if seen_days.insert(date) {
                years.insert(date.year());
                first_year = first_year.min(date.year());
                photo_ids.push(photo_id);
            }
            if photo_ids.len() == 24 {
                break;
            }
        }
        if photo_ids.len() < 4 || years.len() < 2 {
            continue;
        }
        if dominant_named_person_coverage(conn, &photo_ids)? >= 0.60 {
            continue;
        }
        return Ok(Some(Memory {
            id: format!("visual-{seed}-{}-{:03}", today.year(), today.ordinal()),
            kind: MemoryKind::VisualPattern,
            title: "Something you kept noticing".to_string(),
            photo_ids,
            hero_photo_id: 0,
            hero_thumbnail_path: None,
            score: 0.0,
            year: first_year,
            has_faces: false,
        }));
    }
    Ok(None)
}

fn dated_active_photos(
    conn: &Connection,
    photo_ids: &[i64],
) -> Result<Vec<(i64, NaiveDate)>, String> {
    if photo_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rank: HashMap<i64, usize> = photo_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let mut out = Vec::new();
    let mut seen_hashes = HashSet::new();
    for chunk in photo_ids.chunks(900) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, date_taken, file_hash FROM photos
             WHERE is_trashed = FALSE AND content_category = 'photo'
               AND date_taken IS NOT NULL AND id IN ({placeholders})
               AND NOT EXISTS (
                   SELECT 1
                   FROM photo_stack_members psm
                   JOIN photo_stacks ps ON ps.id = psm.stack_id
                   WHERE psm.photo_id = photos.id
                     AND ps.dismissed = FALSE
                     AND psm.is_cover = FALSE
               )"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, raw, file_hash) = row.map_err(|e| e.to_string())?;
            if !seen_hashes.insert(file_hash) {
                continue;
            }
            if let Ok(date) =
                NaiveDate::parse_from_str(raw.get(..10).unwrap_or_default(), "%Y-%m-%d")
            {
                out.push((id, date));
            }
        }
    }
    out.sort_by_key(|(id, _)| rank.get(id).copied().unwrap_or(usize::MAX));
    Ok(out)
}

fn dominant_named_person_coverage(conn: &Connection, photo_ids: &[i64]) -> Result<f32, String> {
    if photo_ids.is_empty() {
        return Ok(0.0);
    }
    let placeholders = (0..photo_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"SELECT COUNT(DISTINCT f.photo_id)
           FROM faces f
           JOIN face_clusters fc ON fc.id = f.cluster_id
           WHERE f.photo_id IN ({placeholders})
             AND fc.name IS NOT NULL AND TRIM(fc.name) != ''
           GROUP BY f.cluster_id
           ORDER BY COUNT(DISTINCT f.photo_id) DESC
           LIMIT 1"#
    );
    let count: i64 = conn
        .query_row(
            &sql,
            rusqlite::params_from_iter(photo_ids.iter().copied()),
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(count as f32 / photo_ids.len() as f32)
}

/// Rotate one named person with a real history through the library. Requiring
/// three separate years keeps this personal without turning every face cluster
/// into a card.
fn person_story(conn: &Connection, today: NaiveDate) -> SqliteResult<Option<Memory>> {
    let mut stmt = conn.prepare(
        r#"
        WITH appearances AS (
            SELECT DISTINCT f.cluster_id, fc.name, p.id AS photo_id,
                   CAST(strftime('%Y', p.date_taken) AS INTEGER) AS yr,
                   p.date_taken, p.is_favorite
            FROM faces f
            JOIN face_clusters fc ON fc.id = f.cluster_id
            JOIN photos p ON p.id = f.photo_id
            LEFT JOIN memory_blocks mb
              ON mb.kind = 'person' AND mb.target_key = CAST(f.cluster_id AS TEXT)
            WHERE p.is_trashed = FALSE
              AND p.content_category = 'photo'
              AND p.date_taken IS NOT NULL
              AND fc.name IS NOT NULL AND TRIM(fc.name) != ''
              AND mb.id IS NULL
        ),
        ranked AS (
            SELECT *,
                   ROW_NUMBER() OVER (
                       PARTITION BY cluster_id, yr
                       ORDER BY is_favorite DESC, date_taken DESC, photo_id DESC
                   ) AS rn
            FROM appearances
        )
        SELECT cluster_id, name, GROUP_CONCAT(photo_id), MIN(yr), MAX(yr),
               COUNT(DISTINCT yr) AS year_count, COUNT(*) AS photo_count
        FROM ranked
        WHERE rn <= 8
        GROUP BY cluster_id, name
        HAVING year_count >= 3 AND photo_count >= 9
        ORDER BY year_count DESC, photo_count DESC, name
        LIMIT 24
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i32>(3)?,
            row.get::<_, i32>(4)?,
        ))
    })?;
    let candidates = rows.collect::<SqliteResult<Vec<_>>>()?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let index = today.ordinal0() as usize % candidates.len();
    let (cluster_id, name, ids, first_year, last_year) = &candidates[index];
    Ok(Some(Memory {
        id: format!(
            "person-{cluster_id}-{}-{:03}",
            today.year(),
            today.ordinal()
        ),
        kind: MemoryKind::PersonStory,
        title: format!("{name}, through the years"),
        photo_ids: parse_ids(ids),
        hero_photo_id: 0,
        hero_thumbnail_path: None,
        score: 0.0,
        year: *first_year.min(last_year),
        has_faces: true,
    }))
}

/// Rotate one non-home place photographed in at least three different years.
fn place_story(conn: &Connection, today: NaiveDate) -> SqliteResult<Option<Memory>> {
    let home_city: Option<String> = conn
        .query_row(
            r#"SELECT location_city
               FROM photos
               WHERE is_trashed = FALSE
                 AND location_city IS NOT NULL AND location_city != ''
                 AND date_taken IS NOT NULL
               GROUP BY location_city
               ORDER BY COUNT(DISTINCT strftime('%Y-%W', date_taken)) DESC
               LIMIT 1"#,
            [],
            |row| row.get(0),
        )
        .ok();
    let mut stmt = conn.prepare(
        r#"
        WITH ranked AS (
            SELECT id, location_city AS city,
                   CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
                   ROW_NUMBER() OVER (
                       PARTITION BY location_city, CAST(strftime('%Y', date_taken) AS INTEGER)
                       ORDER BY is_favorite DESC, date_taken DESC, id DESC
                   ) AS rn
            FROM photos
            WHERE is_trashed = FALSE
              AND content_category = 'photo'
              AND date_taken IS NOT NULL
              AND location_city IS NOT NULL AND location_city != ''
              AND (?1 IS NULL OR location_city != ?1)
        )
        SELECT city, GROUP_CONCAT(id), MIN(yr), MAX(yr),
               COUNT(DISTINCT yr) AS year_count, COUNT(*) AS photo_count
        FROM ranked
        WHERE rn <= 8
        GROUP BY city
        HAVING year_count >= 3 AND photo_count >= 9
        ORDER BY year_count DESC, photo_count DESC, city
        LIMIT 24
        "#,
    )?;
    let rows = stmt.query_map(params![home_city], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, i32>(3)?,
        ))
    })?;
    let candidates = rows.collect::<SqliteResult<Vec<_>>>()?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let index = (today.ordinal0() as usize / 3) % candidates.len();
    let (city, ids, first_year, last_year) = &candidates[index];
    Ok(Some(Memory {
        id: format!(
            "place-{}-{}-{:03}",
            stable_text_id(city),
            today.year(),
            today.ordinal()
        ),
        kind: MemoryKind::PlaceStory,
        title: format!("Back to {city}, through the years"),
        photo_ids: parse_ids(ids),
        hero_photo_id: 0,
        hero_thumbnail_path: None,
        score: 0.0,
        year: *first_year.min(last_year),
        has_faces: false,
    }))
}

fn stable_text_id(value: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
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
        WITH matched AS (
            SELECT id,
                   CAST(strftime('%Y', date_taken) AS INTEGER) AS yr,
                   ROW_NUMBER() OVER (
                       PARTITION BY CAST(strftime('%Y', date_taken) AS INTEGER)
                       ORDER BY date_taken DESC, id DESC
                   ) AS rn
            FROM photos
            WHERE is_trashed = FALSE
              AND content_category = 'photo'
              AND date_taken IS NOT NULL
              AND date_taken < ?1
        )
        SELECT yr, GROUP_CONCAT(id) AS photo_ids
        FROM matched
        WHERE rn <= ?2
        GROUP BY yr
        ORDER BY yr DESC
        LIMIT 5
        "#,
    )?;

    let rows = stmt.query_map(params![cutoff_str, YEAR_RECAP_MAX_PHOTOS as i64], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (yr, csv) = row?;
        let photo_ids = parse_ids(&csv);
        if photo_ids.is_empty() {
            continue;
        }

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
        for mem in memories.iter_mut() {
            mem.hero_photo_id = 0;
            mem.hero_thumbnail_path = None;
            mem.has_faces = false;
        }
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
        let Some(mut best_id) = mem.photo_ids.first().copied() else {
            mem.hero_photo_id = 0;
            mem.hero_thumbnail_path = None;
            mem.has_faces = false;
            continue;
        };
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
            MemoryKind::PersonStory | MemoryKind::PlaceStory | MemoryKind::VisualPattern => 1.8,
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
    let blocked_photos: HashSet<i64> = rows.collect::<SqliteResult<HashSet<_>>>()?;

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
    use tempfile::tempdir;

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
    fn hero_selection_tolerates_empty_memory_photo_ids() {
        let conn = fresh_db();
        let mut memories = vec![Memory {
            id: "empty".into(),
            kind: MemoryKind::FallbackWindow,
            title: "Empty".into(),
            photo_ids: Vec::new(),
            hero_photo_id: 99,
            hero_thumbnail_path: Some("stale.jpg".into()),
            score: 0.0,
            year: 2020,
            has_faces: true,
        }];

        populate_hero_and_faces(&conn, &mut memories).unwrap();

        assert_eq!(memories[0].hero_photo_id, 0);
        assert!(memories[0].hero_thumbnail_path.is_none());
        assert!(!memories[0].has_faces);
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
    fn dense_memory_day_is_capped_before_detail_payload() {
        let conn = fresh_db();
        for id in 1..=(MAX_MEMORY_PHOTOS as i64 + 25) {
            insert_photo(&conn, id, "2022-04-15 10:00:00");
        }

        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        let on_this_day = cards
            .iter()
            .find(|card| card.kind == MemoryKind::OnThisDay)
            .unwrap();

        assert_eq!(on_this_day.photo_ids.len(), MAX_MEMORY_PHOTOS);
        assert_eq!(on_this_day.photo_count, MAX_MEMORY_PHOTOS);
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

    #[test]
    fn named_person_across_three_years_becomes_a_personal_story() {
        let conn = fresh_db();
        conn.execute(
            "INSERT INTO face_clusters (id, name, is_user_named) VALUES (7, 'Asha', TRUE)",
            [],
        )
        .unwrap();
        let mut id = 1_i64;
        for year in 2021..=2023 {
            for day in 1..=3 {
                insert_photo(&conn, id, &format!("{year}-06-{day:02} 10:00:00"));
                conn.execute(
                    r#"INSERT INTO faces
                       (id, photo_id, bbox_x, bbox_y, bbox_width, bbox_height,
                        embedding, cluster_id, confidence)
                       VALUES (?1, ?1, 0.1, 0.1, 0.3, 0.3, X'00', 7, 0.99)"#,
                    [id],
                )
                .unwrap();
                id += 1;
            }
        }

        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        let story = cards
            .iter()
            .find(|card| card.kind == MemoryKind::PersonStory)
            .unwrap();
        assert_eq!(story.title, "Asha, through the years");
        assert_eq!(story.photo_count, 9);
    }

    #[test]
    fn recurring_place_story_excludes_the_inferred_home_city() {
        let conn = fresh_db();
        let mut id = 1_i64;
        for year in 2021..=2023 {
            for month in 1..=4 {
                insert_photo(&conn, id, &format!("{year}-{month:02}-01 10:00:00"));
                conn.execute(
                    "UPDATE photos SET location_city = 'Delhi' WHERE id = ?1",
                    [id],
                )
                .unwrap();
                id += 1;
            }
            for day in 1..=3 {
                insert_photo(&conn, id, &format!("{year}-08-{day:02} 10:00:00"));
                conn.execute(
                    "UPDATE photos SET location_city = 'Goa' WHERE id = ?1",
                    [id],
                )
                .unwrap();
                id += 1;
            }
        }

        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let cards = generate_for_today(&conn, today).unwrap();
        let story = cards
            .iter()
            .find(|card| card.kind == MemoryKind::PlaceStory)
            .unwrap();
        assert_eq!(story.title, "Back to Goa, through the years");
    }

    #[test]
    fn semantic_neighbors_across_years_become_one_visual_pattern() {
        let mut conn = fresh_db();
        let dir = tempdir().unwrap();
        let service = SemanticSearchService::new(dir.path());
        for id in 1..=6_i64 {
            let year = 2020 + ((id - 1) / 2) as i32;
            let day = ((id - 1) % 2) + 1;
            insert_photo(&conn, id, &format!("{year}-09-{day:02} 10:00:00"));
            conn.execute(
                "UPDATE photos SET file_hash = ?1 WHERE id = ?2",
                params![format!("hash-{id}"), id],
            )
            .unwrap();
            let mut vector = vec![0.0_f32; crate::services::semantic::SEMANTIC_DIM];
            vector[0] = 1.0;
            vector[1] = id as f32 * 0.001;
            service.mark_indexed(&mut conn, id, &vector).unwrap();
        }

        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let mut cache = SemanticIndexCache::default();
        let cards = generate_for_today_with_semantic(&conn, today, dir.path(), &mut cache).unwrap();
        let visual = cards
            .iter()
            .find(|card| card.kind == MemoryKind::VisualPattern)
            .unwrap();
        assert_eq!(visual.title, "Something you kept noticing");
        assert_eq!(visual.photo_count, 6);
    }
}
