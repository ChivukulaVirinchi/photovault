//! Picking photos for collages: the best few from any set, and the
//! "Made for you" collages offered on the Memories page.
//!
//! Nothing here is stored. Suggestions are recomputed from the library each
//! time and are stable for a day; the only write is a dismissal, kept in
//! `memory_blocks` under kind `collage` so a dismissed source stays hidden.

use std::collections::HashSet;

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, params_from_iter, Connection, Result as SqliteResult};

use crate::db::album_suggestion_repo::AlbumSuggestionRepo;
use crate::services::memories::{MemoryCard, MemoryKind};
use crate::services::story;

/// The largest collage the auto picker builds. Busier frames stop reading
/// as one picture at the sizes collages are shared at.
pub const MAX_AUTO_PHOTOS: usize = 6;
/// Fewer good photos than this and a set isn't worth a collage.
pub const MIN_AUTO_PHOTOS: usize = 3;
/// How many "Made for you" collages to offer at once.
pub const MAX_SUGGESTIONS: usize = 3;
/// Larger sets are thinned evenly before scoring; a collage needs six photos,
/// not an exhaustive ranking of a 50,000-photo album.
const MAX_CANDIDATES: usize = 4000;
/// Two photos whose perceptual hashes differ in at most this many bits look
/// alike at collage size.
const PHASH_NEAR: u32 = 10;
/// Photos taken this close together are usually the same moment.
const SAME_MOMENT_SECS: i64 = 60;
/// SQLite parameters per statement, kept well under every build's limit.
const CHUNK: usize = 500;

const BLOCK_KIND: &str = "collage";
const GROUP_BURST: u8 = 0;
const GROUP_DUPLICATE: u8 = 1;

#[derive(Debug, Clone)]
struct Candidate {
    id: i64,
    /// Seconds since the epoch, when the photo is dated.
    taken: Option<i64>,
    score: f32,
    file_hash: String,
    phash: Option<u64>,
    /// Burst and duplicate groups: one photo per group at most.
    groups: Vec<(u8, i64)>,
}

/// Options for [`pick_best`].
#[derive(Debug, Clone, Copy)]
pub struct PickOptions {
    pub max: usize,
    /// Leave out photos of people hidden from Memories.
    pub skip_hidden_people: bool,
}

impl Default for PickOptions {
    fn default() -> Self {
        Self {
            max: MAX_AUTO_PHOTOS,
            skip_hidden_people: false,
        }
    }
}

/// The best few photos of `photo_ids` for a collage: the strongest photo
/// first (it gets the feature cell), then the rest in the order they were
/// taken. Videos, screenshots, near-duplicates and badly exposed shots are
/// left out, and picks are spread across the set's time span so a trip
/// isn't summed up by one afternoon.
pub fn pick_best(
    conn: &Connection,
    photo_ids: &[i64],
    options: PickOptions,
) -> SqliteResult<Vec<i64>> {
    let ids = thin_evenly(photo_ids, MAX_CANDIDATES);
    let mut candidates = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(CHUNK) {
        candidates.extend(load_candidates(conn, chunk, options.skip_hidden_people)?);
    }
    Ok(choose(candidates, options.max.min(MAX_AUTO_PHOTOS)))
}

/// Every `len / max`-th id, so a huge set is sampled across its whole span.
fn thin_evenly(ids: &[i64], max: usize) -> Vec<i64> {
    if ids.len() <= max {
        return ids.to_vec();
    }
    (0..max).map(|i| ids[i * ids.len() / max]).collect()
}

fn load_candidates(
    conn: &Connection,
    ids: &[i64],
    skip_hidden_people: bool,
) -> SqliteResult<Vec<Candidate>> {
    let placeholders = vec!["?"; ids.len()].join(",");
    let hidden_people = if skip_hidden_people {
        "AND NOT EXISTS (
           SELECT 1 FROM faces f JOIN memory_blocks mb
             ON mb.kind = 'person' AND mb.target_key = CAST(f.cluster_id AS TEXT)
           WHERE f.photo_id = p.id)"
    } else {
        ""
    };
    let sql = format!(
        r#"SELECT p.id, CAST(strftime('%s', p.date_taken) AS INTEGER), p.is_favorite,
                  COALESCE(p.width, 0), COALESCE(p.height, 0), p.brightness,
                  p.phash, p.file_hash,
                  (SELECT COUNT(*) FROM faces f WHERE f.photo_id = p.id),
                  (SELECT MAX(f.bbox_width * f.bbox_height) FROM faces f WHERE f.photo_id = p.id),
                  (SELECT bgm.group_id FROM burst_group_members bgm WHERE bgm.photo_id = p.id LIMIT 1),
                  COALESCE((SELECT MAX(bgm.is_suggested_best) FROM burst_group_members bgm
                            WHERE bgm.photo_id = p.id), 0),
                  (SELECT dgm.group_id FROM duplicate_group_members dgm WHERE dgm.photo_id = p.id LIMIT 1),
                  COALESCE((SELECT MAX(dgm.is_suggested_keep) FROM duplicate_group_members dgm
                            WHERE dgm.photo_id = p.id), 0)
           FROM photos p
           WHERE p.id IN ({placeholders})
             AND p.is_trashed = FALSE
             AND p.media_type = 'photo'
             AND lower(p.file_name) NOT LIKE 'screenshot%'
             -- Stacked look-alikes: only the stack's cover is a candidate.
             AND NOT EXISTS (
               SELECT 1 FROM photo_stack_members psm
               JOIN photo_stacks ps ON ps.id = psm.stack_id
               WHERE psm.photo_id = p.id AND ps.dismissed = FALSE AND psm.is_cover = FALSE)
             {hidden_people}"#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(ids.iter()), |row| {
        let width: i64 = row.get(3)?;
        let height: i64 = row.get(4)?;
        let signals = Signals {
            favorite: row.get::<_, Option<bool>>(2)?.unwrap_or(false),
            width,
            height,
            brightness: row.get(5)?,
            faces: row.get(8)?,
            largest_face: row.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
            burst_runner_up: row.get::<_, Option<i64>>(10)?.is_some() && !row.get::<_, bool>(11)?,
            duplicate_runner_up: row.get::<_, Option<i64>>(12)?.is_some()
                && !row.get::<_, bool>(13)?,
        };
        let mut groups = Vec::new();
        if let Some(group) = row.get::<_, Option<i64>>(10)? {
            groups.push((GROUP_BURST, group));
        }
        if let Some(group) = row.get::<_, Option<i64>>(12)? {
            groups.push((GROUP_DUPLICATE, group));
        }
        let Some(score) = signals.score() else {
            return Ok(None);
        };
        Ok(Some(Candidate {
            id: row.get(0)?,
            taken: row.get(1)?,
            score,
            file_hash: row.get(7)?,
            phash: row.get::<_, Option<i64>>(6)?.map(|h| h as u64),
            groups,
        }))
    })?;
    let mut out = Vec::new();
    for row in rows {
        if let Some(candidate) = row? {
            out.push(candidate);
        }
    }
    Ok(out)
}

struct Signals {
    favorite: bool,
    width: i64,
    height: i64,
    /// Average luma in 0..1, when the face pipeline has measured it.
    brightness: Option<f64>,
    faces: i64,
    /// Largest face as a fraction of the photo's area.
    largest_face: f64,
    burst_runner_up: bool,
    duplicate_runner_up: bool,
}

impl Signals {
    /// How much this photo deserves a place in a collage, or `None` when it
    /// shouldn't be used at all.
    fn score(&self) -> Option<f32> {
        let (w, h) = (self.width.max(0) as f64, self.height.max(0) as f64);
        // Too small to fill a cell without looking soft.
        if w > 0.0 && h > 0.0 && w * h < 300_000.0 {
            return None;
        }
        // Nearly black or blown out.
        if self.brightness.is_some_and(|b| !(0.1..=0.92).contains(&b)) {
            return None;
        }
        let mut score = 1.0_f32;
        if self.favorite {
            score += 3.0;
        }
        if self.faces > 0 {
            score += 1.2;
            // A face you can actually see at collage size.
            if self.largest_face >= 0.01 {
                score += 0.6;
            }
            // Group shots are good, crowds are hard to frame.
            if (2..=5).contains(&self.faces) {
                score += 0.3;
            }
        }
        if let Some(b) = self.brightness {
            if !(0.18..=0.85).contains(&b) {
                score -= 0.6;
            }
        }
        if w > 0.0 && h > 0.0 {
            let aspect = (w / h).max(h / w);
            // Panoramas lose most of themselves in a cell.
            if aspect > 2.4 {
                score -= 1.5;
            }
        }
        if self.burst_runner_up {
            score -= 0.8;
        }
        if self.duplicate_runner_up {
            score -= 0.8;
        }
        Some(score)
    }
}

/// The selection itself, on already scored candidates.
fn choose(mut candidates: Vec<Candidate>, max: usize) -> Vec<i64> {
    if candidates.is_empty() || max == 0 {
        return Vec::new();
    }
    let target = target_count(candidates.len()).min(max);
    // Time order; undated photos go last in id order.
    candidates.sort_by_key(|c| (c.taken.is_none(), c.taken.unwrap_or(0), c.id));

    let mut picks: Vec<usize> = Vec::with_capacity(target);
    // First pass: one photo from each equal slice of the timeline, so the
    // picks span the whole set. Same-moment shots are skipped here.
    for b in 0..target {
        let slice = (b * candidates.len() / target)..((b + 1) * candidates.len() / target);
        let best = slice
            .filter(|&i| !clashes(&candidates, &picks, i, Repeat::Moment))
            .max_by(|&a, &b| by_score(&candidates[a], &candidates[b]));
        if let Some(i) = best {
            picks.push(i);
        }
    }
    // Then fill any gaps from the best of the rest: first allowing shots
    // of the same moment, then, for small sets, burst siblings too. Copies
    // and look-alikes are never used twice.
    let mut rest: Vec<usize> = (0..candidates.len()).collect();
    rest.sort_by(|&a, &b| by_score(&candidates[b], &candidates[a]));
    for level in [Repeat::Burst, Repeat::Copy] {
        for &i in &rest {
            if picks.len() >= target {
                break;
            }
            if !clashes(&candidates, &picks, i, level) {
                picks.push(i);
            }
        }
    }

    // Strongest first (it takes the feature cell), the rest in time order.
    picks.sort_unstable();
    if let Some(lead) = (0..picks.len()).max_by(|&a, &b| {
        by_score(&candidates[picks[a]], &candidates[picks[b]])
            // Ties go to the earlier photo.
            .then(picks[b].cmp(&picks[a]))
    }) {
        let first = picks.remove(lead);
        picks.insert(0, first);
    }
    picks.into_iter().map(|i| candidates[i].id).collect()
}

fn by_score(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    a.score
        .partial_cmp(&b.score)
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// How many photos to use for a set of `pool` usable ones.
fn target_count(pool: usize) -> usize {
    match pool {
        0..=3 => pool,
        4..=7 => 4,
        8..=15 => 5,
        _ => MAX_AUTO_PHOTOS,
    }
}

/// How close to an earlier pick a photo may be, loosest last.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Repeat {
    /// Not even the same moment.
    Moment,
    /// Not from the same burst.
    Burst,
    /// Only never a copy or a look-alike.
    Copy,
}

/// Would adding candidate `i` repeat something already picked?
fn clashes(candidates: &[Candidate], picks: &[usize], i: usize, level: Repeat) -> bool {
    let c = &candidates[i];
    picks.iter().any(|&p| {
        let other = &candidates[p];
        let shared = |kind: u8| {
            other
                .groups
                .iter()
                .any(|g| g.0 == kind && c.groups.contains(g))
        };
        p == i
            || other.file_hash == c.file_hash
            || shared(GROUP_DUPLICATE)
            || matches!((other.phash, c.phash), (Some(a), Some(b)) if (a ^ b).count_ones() <= PHASH_NEAR)
            || (level <= Repeat::Burst && shared(GROUP_BURST))
            || (level == Repeat::Moment
                && matches!((other.taken, c.taken), (Some(a), Some(b)) if (a - b).abs() < SAME_MOMENT_SECS))
    })
}

// ---------- Suggestions ----------

/// Where a suggested collage's photos come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionSource {
    Memory { id: String, kind: MemoryKind },
    AlbumSuggestion { kind: String, fingerprint: String },
    Album { id: i64 },
    Favourites,
}

impl SuggestionSource {
    /// Stable key, also used to remember a dismissal.
    pub fn key(&self) -> String {
        match self {
            Self::Memory { id, .. } => format!("memory:{id}"),
            Self::AlbumSuggestion { fingerprint, .. } => format!("suggestion:{fingerprint}"),
            Self::Album { id } => format!("album:{id}"),
            Self::Favourites => "favourites".into(),
        }
    }

    /// A short line above the title saying why this collage was made:
    /// "On this day", "Your trip · Dec 2023", "Your album · 2019 – 2023".
    fn eyebrow(&self, conn: &Connection, ids: &[i64]) -> String {
        let when = || story::span_label(conn, ids);
        let with_when = |lead: &str| match when() {
            Some(when) => format!("{lead} · {when}"),
            None => lead.to_string(),
        };
        match self {
            Self::Memory { kind, .. } => match kind {
                MemoryKind::OnThisDay => "On this day".into(),
                MemoryKind::FallbackWindow => "This week, years ago".into(),
                MemoryKind::SeasonalRecap => "This month, years ago".into(),
                MemoryKind::PersonStory => "Through the years".into(),
                MemoryKind::PlaceStory => "A place you keep going back to".into(),
                MemoryKind::YearRecap => "A year to remember".into(),
            },
            Self::AlbumSuggestion { kind, .. } => with_when(match kind.as_str() {
                "trip" => "Your trip",
                "gathering" => "Together",
                _ => "A day out",
            }),
            Self::Album { .. } => with_when("Your album"),
            Self::Favourites => "Starred by you".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollageSuggestion {
    pub source: SuggestionSource,
    pub title: String,
    /// Why it was suggested, shown above the title.
    pub eyebrow: String,
    /// In collage order: the feature photo first.
    pub photo_ids: Vec<i64>,
}

/// Collages for today, built from today's memories, pending trip and event
/// suggestions, the user's albums and favourites. The same library gives the
/// same suggestions all day; rotating sources change from one day to the
/// next. A photo is used by one suggestion at most.
pub fn suggestions_for_today(
    conn: &Connection,
    today: NaiveDate,
    memories: &[MemoryCard],
) -> SqliteResult<Vec<CollageSuggestion>> {
    let dismissed = dismissed_keys(conn)?;
    let day = today.ordinal0() as usize + today.year() as usize * 366;

    // Candidate sources in order of preference. Memories are already
    // ranked; the other pools rotate by day.
    let mut sources: Vec<(SuggestionSource, String, Vec<i64>)> = Vec::new();
    let mut memory_sources = memories.iter().map(|card| {
        (
            SuggestionSource::Memory {
                id: card.id.clone(),
                kind: card.kind,
            },
            card.title.clone(),
            card.photo_ids.clone(),
        )
    });
    sources.extend(memory_sources.next());
    for suggestion in rotate(AlbumSuggestionRepo::new(conn).get_pending()?, day) {
        let ids = suggestion.photo_ids();
        sources.push((
            SuggestionSource::AlbumSuggestion {
                kind: suggestion.kind,
                fingerprint: suggestion.fingerprint,
            },
            // Stored as "title  ·  dates"; the dates move to the eyebrow.
            story::split(&suggestion.title).0.to_string(),
            ids,
        ));
    }
    for (id, name) in rotate(albums_with_photos(conn)?, day / 2) {
        sources.push((
            SuggestionSource::Album { id },
            name,
            album_photo_ids(conn, id)?,
        ));
    }
    sources.extend(memory_sources);
    sources.push((
        SuggestionSource::Favourites,
        "Your favourites".into(),
        favourite_ids(conn)?,
    ));

    let mut out = Vec::new();
    let mut used: HashSet<i64> = HashSet::new();
    let mut kinds_used: HashSet<std::mem::Discriminant<SuggestionSource>> = HashSet::new();
    // Two passes: first one suggestion per kind of source for variety, then
    // fill from whatever is left.
    for pass in 0..2 {
        for (source, title, ids) in &sources {
            if out.len() >= MAX_SUGGESTIONS {
                return Ok(out);
            }
            let kind = std::mem::discriminant(source);
            if dismissed.contains(&source.key())
                || out.iter().any(|s: &CollageSuggestion| &s.source == source)
                || (pass == 0 && kinds_used.contains(&kind))
            {
                continue;
            }
            let fresh: Vec<i64> = ids
                .iter()
                .copied()
                .filter(|id| !used.contains(id))
                .collect();
            let photo_ids = pick_best(
                conn,
                &fresh,
                PickOptions {
                    skip_hidden_people: true,
                    ..PickOptions::default()
                },
            )?;
            if photo_ids.len() < MIN_AUTO_PHOTOS {
                continue;
            }
            used.extend(photo_ids.iter().copied());
            kinds_used.insert(kind);
            out.push(CollageSuggestion {
                source: source.clone(),
                title: title.clone(),
                eyebrow: source.eyebrow(conn, ids),
                photo_ids,
            });
        }
    }
    Ok(out)
}

/// Hide a suggested collage's source for good.
pub fn dismiss(conn: &Connection, key: &str) -> SqliteResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO memory_blocks (kind, target_key) VALUES (?1, ?2)",
        params![BLOCK_KIND, key],
    )?;
    Ok(())
}

fn dismissed_keys(conn: &Connection) -> SqliteResult<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT target_key FROM memory_blocks WHERE kind = ?1")?;
    let rows = stmt.query_map([BLOCK_KIND], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// `items` starting at `day % len`, wrapping round.
fn rotate<T>(mut items: Vec<T>, day: usize) -> Vec<T> {
    if !items.is_empty() {
        let len = items.len();
        items.rotate_left(day % len);
    }
    items
}

/// User albums with enough live photos to pick from, oldest first so the
/// rotation order doesn't shift every time an album is edited.
fn albums_with_photos(conn: &Connection) -> SqliteResult<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        r#"SELECT a.id, a.name FROM albums a
           JOIN album_photos ap ON ap.album_id = a.id
           JOIN photos p ON p.id = ap.photo_id
           WHERE p.is_trashed = FALSE AND p.media_type = 'photo'
           GROUP BY a.id
           HAVING COUNT(*) >= ?1
           ORDER BY a.id"#,
    )?;
    let rows = stmt.query_map([MIN_AUTO_PHOTOS as i64], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    rows.collect()
}

/// Photos of an album in time order.
pub fn album_photo_ids(conn: &Connection, album_id: i64) -> SqliteResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        r#"SELECT p.id FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
           WHERE ap.album_id = ?1 AND p.is_trashed = FALSE
           ORDER BY p.date_taken IS NULL, p.date_taken, p.id"#,
    )?;
    let rows = stmt.query_map([album_id], |row| row.get(0))?;
    rows.collect()
}

/// Favourite photos in time order.
pub fn favourite_ids(conn: &Connection) -> SqliteResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        r#"SELECT id FROM photos WHERE is_favorite = TRUE AND is_trashed = FALSE
           ORDER BY date_taken IS NULL, date_taken, id"#,
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::memories::MemoryKind;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::create_schema(&conn).unwrap();
        conn
    }

    /// A 12 MP landscape photo, one hour apart per id, with a distinct hash.
    fn insert(conn: &Connection, id: i64) {
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken, width, height)
             VALUES (?1, ?2, ?2, ?3, 1, datetime('2022-06-01 08:00:00', ?4), 4000, 3000)",
            params![
                id,
                format!("p{id}.jpg"),
                format!("hash{id}"),
                format!("+{id} hours")
            ],
        )
        .unwrap();
    }

    fn pick(conn: &Connection, ids: &[i64]) -> Vec<i64> {
        pick_best(conn, ids, PickOptions::default()).unwrap()
    }

    #[test]
    fn leaves_out_videos_screenshots_trash_and_tiny_or_dark_photos() {
        let conn = fresh_db();
        for id in 1..=7 {
            insert(&conn, id);
        }
        conn.execute_batch(
            "UPDATE photos SET media_type = 'video' WHERE id = 2;
             UPDATE photos SET file_name = 'Screenshot 1.png' WHERE id = 3;
             UPDATE photos SET is_trashed = TRUE WHERE id = 4;
             UPDATE photos SET width = 320, height = 240 WHERE id = 5;
             UPDATE photos SET brightness = 0.03 WHERE id = 6;",
        )
        .unwrap();
        let mut picked = pick(&conn, &(1..=7).collect::<Vec<_>>());
        picked.sort();
        assert_eq!(picked, vec![1, 7]);
    }

    #[test]
    fn favourite_leads_and_the_rest_follow_in_time_order() {
        let conn = fresh_db();
        for id in 1..=12 {
            insert(&conn, id);
        }
        conn.execute("UPDATE photos SET is_favorite = TRUE WHERE id = 9", [])
            .unwrap();
        let picked = pick(&conn, &(1..=12).collect::<Vec<_>>());
        assert_eq!(picked.len(), 5);
        assert_eq!(picked[0], 9);
        assert!(picked[1..].windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn picks_span_the_whole_set() {
        let conn = fresh_db();
        for id in 1..=40 {
            insert(&conn, id);
        }
        let mut picked = pick(&conn, &(1..=40).collect::<Vec<_>>());
        picked.sort();
        assert_eq!(picked.len(), MAX_AUTO_PHOTOS);
        assert!(picked[0] <= 7, "{picked:?}");
        assert!(*picked.last().unwrap() >= 34, "{picked:?}");
    }

    #[test]
    fn look_alikes_and_stack_members_appear_once() {
        let conn = fresh_db();
        for id in 1..=8 {
            insert(&conn, id);
        }
        conn.execute_batch(
            "UPDATE photos SET phash = 0 WHERE id IN (1, 2, 3, 4);
             UPDATE photos SET file_hash = 'same' WHERE id IN (5, 6);
             INSERT INTO photo_stacks (id, kind, source_group_id, cover_photo_id) VALUES (1, 'burst', 1, 7);
             INSERT INTO photo_stack_members (stack_id, photo_id, is_cover) VALUES (1, 7, TRUE), (1, 8, FALSE);",
        )
        .unwrap();
        let picked = pick(&conn, &(1..=8).collect::<Vec<_>>());
        assert_eq!(
            picked.iter().filter(|id| **id <= 4).count(),
            1,
            "{picked:?}"
        );
        assert_eq!(picked.iter().filter(|id| [5, 6].contains(*id)).count(), 1);
        assert!(!picked.contains(&8));
    }

    #[test]
    fn same_moment_shots_are_avoided_while_others_remain() {
        let conn = fresh_db();
        for id in 1..=8 {
            insert(&conn, id);
        }
        // Photos 1 and 2 taken seconds apart; 2 is a favourite.
        conn.execute_batch(
            "UPDATE photos SET date_taken = '2022-06-01 09:00:10' WHERE id = 2;
             UPDATE photos SET is_favorite = TRUE WHERE id IN (1, 2);",
        )
        .unwrap();
        let picked = pick(&conn, &(1..=8).collect::<Vec<_>>());
        assert_eq!(picked.len(), 5);
        assert!(!(picked.contains(&1) && picked.contains(&2)), "{picked:?}");
    }

    #[test]
    fn a_small_burst_still_makes_a_collage_but_copies_never_repeat() {
        let conn = fresh_db();
        for id in 1..=3 {
            insert(&conn, id);
        }
        conn.execute_batch(
            "UPDATE photos SET date_taken = '2022-06-01 09:00:00';
             INSERT INTO burst_groups (id, start_time, end_time) VALUES (1, '2022-06-01', '2022-06-01');
             INSERT INTO burst_group_members (group_id, photo_id) VALUES (1, 1), (1, 2), (1, 3);
             INSERT INTO duplicate_groups (id, duplicate_type) VALUES (1, 'exact');
             INSERT INTO duplicate_group_members (group_id, photo_id) VALUES (1, 2), (1, 3);",
        )
        .unwrap();
        let mut picked = pick(&conn, &[1, 2, 3]);
        picked.sort();
        assert_eq!(picked.len(), 2, "{picked:?}");
        assert!(picked.contains(&1));
    }

    #[test]
    fn huge_sets_are_sampled_not_ignored() {
        assert_eq!(
            thin_evenly(&(0..10).collect::<Vec<_>>(), 5),
            vec![0, 2, 4, 6, 8]
        );
        assert_eq!(thin_evenly(&[1, 2], 5), vec![1, 2]);
    }

    fn memory(id: &str, ids: Vec<i64>) -> MemoryCard {
        MemoryCard {
            id: id.into(),
            kind: MemoryKind::OnThisDay,
            title: format!("Memory {id}"),
            hero_photo_id: ids[0],
            hero_thumbnail_path: None,
            photo_count: ids.len(),
            photo_ids: ids,
        }
    }

    #[test]
    fn suggestions_vary_sources_never_share_photos_and_respect_dismissals() {
        let conn = fresh_db();
        for id in 1..=30 {
            insert(&conn, id);
        }
        conn.execute_batch(
            "INSERT INTO albums (id, name) VALUES (1, 'Goa');
             INSERT INTO album_photos (album_id, photo_id)
               SELECT 1, id FROM photos WHERE id BETWEEN 11 AND 20;",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        let memories = vec![
            memory("a", (1..=10).collect()),
            memory("b", (1..=25).collect()),
        ];

        let suggestions = suggestions_for_today(&conn, today, &memories).unwrap();
        let keys: Vec<String> = suggestions.iter().map(|s| s.source.key()).collect();
        assert_eq!(keys, vec!["memory:a", "album:1", "memory:b"]);
        let mut all: Vec<i64> = suggestions
            .iter()
            .flat_map(|s| s.photo_ids.clone())
            .collect();
        let total = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), total, "a photo was used twice");

        // Same day, same library: same suggestions.
        let again = suggestions_for_today(&conn, today, &memories).unwrap();
        assert_eq!(
            again
                .iter()
                .map(|s| s.photo_ids.clone())
                .collect::<Vec<_>>(),
            suggestions
                .iter()
                .map(|s| s.photo_ids.clone())
                .collect::<Vec<_>>()
        );

        dismiss(&conn, "album:1").unwrap();
        dismiss(&conn, "album:1").unwrap();
        let after = suggestions_for_today(&conn, today, &memories).unwrap();
        assert!(after.iter().all(|s| s.source.key() != "album:1"));
    }

    #[test]
    fn hidden_people_stay_out_of_suggestions() {
        let conn = fresh_db();
        for id in 1..=4 {
            insert(&conn, id);
        }
        conn.execute_batch(
            "INSERT INTO face_clusters (id, name) VALUES (1, 'Hidden');
             INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_width, bbox_height, confidence, embedding, cluster_id)
               VALUES (2, 0.4, 0.4, 0.2, 0.2, 0.9, zeroblob(16), 1);
             INSERT INTO memory_blocks (kind, target_key) VALUES ('person', '1');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        let suggestions =
            suggestions_for_today(&conn, today, &[memory("a", vec![1, 2, 3, 4])]).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(!suggestions[0].photo_ids.contains(&2));
        // Asking for a collage directly still uses everyone.
        assert!(pick(&conn, &[1, 2, 3, 4]).contains(&2));
    }
}
