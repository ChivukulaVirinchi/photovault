//! Search service - executes parsed queries against the database.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{params, params_from_iter, types::Value, Connection, Result as SqliteResult};

use crate::search::date_parser::{DateParser, DateRange};
use crate::search::SearchQuery;

/// A single search result row.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub photo_id: i64,
    pub date_taken: Option<String>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
    pub thumbnail_path: Option<String>,
}

/// Search results grouped by date.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SearchResultGroup {
    pub date: String,
    pub location: Option<String>,
    pub results: Vec<SearchResult>,
}

/// A person matching the search query.
#[derive(Debug, Clone)]
pub struct PersonHit {
    pub cluster_id: i64,
    pub name: String,
    pub photo_count: i64,
    /// Resolved at load time (absolute path to face crop jpg).
    pub face_thumbnail_path: Option<String>,
}

/// An album matching the search query.
#[derive(Debug, Clone)]
pub struct AlbumHit {
    pub album_id: i64,
    pub name: String,
    pub photo_count: i64,
    pub cover_photo_id: Option<i64>,
    /// Resolved at load time (absolute path to cover thumbnail).
    pub cover_thumbnail_path: Option<String>,
}

/// A place (city) matching the search query.
#[derive(Debug, Clone)]
pub struct PlaceHit {
    pub city: String,
    pub country: Option<String>,
    pub photo_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpretedFilter {
    pub kind: String,
    pub label: String,
}

/// Unified search results across all entity types.
#[derive(Debug, Clone, Default)]
pub struct UnifiedSearchResults {
    pub interpreted: Vec<InterpretedFilter>,
    pub people: Vec<PersonHit>,
    pub albums: Vec<AlbumHit>,
    pub places: Vec<PlaceHit>,
    pub photos: Vec<SearchResult>,
    pub photos_grouped: Vec<SearchResultGroup>,
    /// Flat list for cull mode.
    pub photo_ids: Vec<i64>,
    /// True when more photo pages exist beyond this one (keyset/offset
    /// pagination — see `search_unified_page`).
    pub has_more: bool,
}

/// One page of unified search results plus a has-more flag.
#[derive(Debug, Clone, Default)]
pub struct UnifiedSearchPage {
    pub results: UnifiedSearchResults,
    pub has_more: bool,
}

/// Search service.
pub struct SearchService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmartMediaType {
    Photo,
    Video,
}

impl SmartMediaType {
    fn as_db(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Video => "video",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Photo => "Photos",
            Self::Video => "Videos",
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedPerson {
    id: i64,
    name: String,
}

#[derive(Debug, Clone)]
struct ResolvedAlbum {
    id: i64,
    name: String,
}

/// A city filter. `country` is kept when the city row carried one so
/// identical city names in different countries stay distinguishable.
#[derive(Debug, Clone)]
struct ResolvedCity {
    city: String,
    country: Option<String>,
    label: String,
}

/// A country filter — resolved from an exact country phrase and never
/// narrowed to the cities inside it.
#[derive(Debug, Clone)]
struct ResolvedCountry {
    country: String,
    label: String,
}

#[derive(Debug, Clone, Default)]
struct SmartIntent {
    date_range: Option<DateRange>,
    text: Option<String>,
    people_all: Vec<ResolvedPerson>,
    people_only: bool,
    cities: Vec<ResolvedCity>,
    countries: Vec<ResolvedCountry>,
    albums: Vec<ResolvedAlbum>,
    favorite: Option<bool>,
    media_type: Option<SmartMediaType>,
    semantic_photo_ids: Vec<i64>,
}

impl SmartIntent {
    fn has_structured_filters(&self) -> bool {
        self.date_range.is_some()
            || !self.people_all.is_empty()
            || !self.cities.is_empty()
            || !self.countries.is_empty()
            || !self.albums.is_empty()
            || self.favorite.is_some()
            || self.media_type.is_some()
    }
}

impl SearchService {
    pub fn search(conn: &Connection, query: &SearchQuery) -> SqliteResult<Vec<SearchResult>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
            "SELECT id, file_path, date_taken, location_city, location_country, thumbnail_path FROM photos WHERE is_trashed = FALSE",
        );
        let mut params_dyn: Vec<String> = Vec::new();

        if let Some(range) = query.date_range() {
            sql.push_str(" AND date_taken >= ? AND date_taken <= ?");
            params_dyn.push(range.start.to_rfc3339());
            params_dyn.push(range.end.to_rfc3339());
        }

        if let Some(location) = query.location() {
            sql.push_str(" AND (LOWER(location_city) LIKE LOWER(?) OR LOWER(location_country) LIKE LOWER(?))");
            let like = format!("%{}%", location);
            params_dyn.push(like.clone());
            params_dyn.push(like);
        }

        sql.push_str(" ORDER BY date_taken DESC LIMIT 1000");

        let mut stmt = conn.prepare(&sql)?;

        let mut rows_vec: Vec<SearchResult> = Vec::new();
        match params_dyn.len() {
            0 => {
                let rows = stmt.query_map([], |row| {
                    Ok(SearchResult {
                        photo_id: row.get(0)?,
                        date_taken: row.get(2)?,
                        location_city: row.get(3)?,
                        location_country: row.get(4)?,
                        thumbnail_path: row.get(5)?,
                    })
                })?;
                for r in rows {
                    rows_vec.push(r?);
                }
            }
            2 => {
                let rows = stmt.query_map(params![params_dyn[0], params_dyn[1]], |row| {
                    Ok(SearchResult {
                        photo_id: row.get(0)?,
                        date_taken: row.get(2)?,
                        location_city: row.get(3)?,
                        location_country: row.get(4)?,
                        thumbnail_path: row.get(5)?,
                    })
                })?;
                for r in rows {
                    rows_vec.push(r?);
                }
            }
            4 => {
                let rows = stmt.query_map(
                    params![params_dyn[0], params_dyn[1], params_dyn[2], params_dyn[3]],
                    |row| {
                        Ok(SearchResult {
                            photo_id: row.get(0)?,
                            date_taken: row.get(2)?,
                            location_city: row.get(3)?,
                            location_country: row.get(4)?,
                            thumbnail_path: row.get(5)?,
                        })
                    },
                )?;
                for r in rows {
                    rows_vec.push(r?);
                }
            }
            _ => {
                let rows = stmt.query_map(
                    params![
                        params_dyn[0],
                        params_dyn[1],
                        params_dyn[2],
                        params_dyn[3],
                        params_dyn[4],
                        params_dyn[5]
                    ],
                    |row| {
                        Ok(SearchResult {
                            photo_id: row.get(0)?,
                            date_taken: row.get(2)?,
                            location_city: row.get(3)?,
                            location_country: row.get(4)?,
                            thumbnail_path: row.get(5)?,
                        })
                    },
                )?;
                for r in rows {
                    rows_vec.push(r?);
                }
            }
        }

        if let Some(person_name) = query.person() {
            rows_vec = Self::filter_by_person(conn, rows_vec, person_name)?;
        }

        Ok(rows_vec)
    }

    fn filter_by_person(
        conn: &Connection,
        results: Vec<SearchResult>,
        person_name: &str,
    ) -> SqliteResult<Vec<SearchResult>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT photo_id FROM (
                SELECT f.photo_id AS photo_id
                FROM faces f
                JOIN face_clusters fc ON f.cluster_id = fc.id
                WHERE fc.name IS NOT NULL AND LOWER(fc.name) LIKE LOWER(?1)

                UNION

                SELECT pii.photo_id AS photo_id
                FROM photo_inferred_identities pii
                JOIN face_clusters fc ON pii.cluster_id = fc.id
                WHERE fc.name IS NOT NULL AND LOWER(fc.name) LIKE LOWER(?1)
            )
            "#,
        )?;

        let ids: HashSet<i64> = stmt
            .query_map(params![format!("%{}%", person_name)], |row| row.get(0))?
            .collect::<SqliteResult<HashSet<_>>>()?;

        Ok(results
            .into_iter()
            .filter(|r| ids.contains(&r.photo_id))
            .collect())
    }

    pub fn group_by_date(results: Vec<SearchResult>) -> Vec<SearchResultGroup> {
        let mut groups: BTreeMap<String, Vec<SearchResult>> = BTreeMap::new();

        for result in results {
            let date = result
                .date_taken
                .as_ref()
                .and_then(|d| d.get(..10))
                .unwrap_or("Unknown")
                .to_string();

            groups.entry(date).or_default().push(result);
        }

        groups
            .into_iter()
            .rev()
            .map(|(date, results)| {
                let location = results.iter().find_map(|r| {
                    r.location_city
                        .clone()
                        .or_else(|| r.location_country.clone())
                });

                SearchResultGroup {
                    date,
                    location,
                    results,
                }
            })
            .collect()
    }

    /// Run a unified multi-entity search.
    ///
    /// The query string is parsed once via `QueryParser`; people/album/place
    /// matches use simple substring LIKE. Photos use the existing
    /// QueryParser-based filter.
    pub fn search_unified(
        conn: &Connection,
        raw_query: &str,
    ) -> SqliteResult<UnifiedSearchResults> {
        Self::search_unified_with_semantic(conn, raw_query, Vec::new())
    }

    /// First page (legacy shape: up to 1000 photos) — kept for callers
    /// that have not moved to `search_unified_page` yet.
    pub fn search_unified_with_semantic(
        conn: &Connection,
        raw_query: &str,
        semantic_photo_ids: Vec<i64>,
    ) -> SqliteResult<UnifiedSearchResults> {
        Ok(Self::search_unified_page(
            conn,
            raw_query,
            semantic_photo_ids,
            0,
            Self::UNIFIED_SEARCH_MAX_PHOTOS,
        )?
        .results)
    }

    /// One page of unified results. `offset`/`limit` page the photo
    /// list; people/albums/places hits are query-wide and identical on
    /// every page. Ordering is stable (`date_taken DESC, id DESC`, or
    /// semantic rank first when semantic candidates are supplied), so
    /// concatenated pages never duplicate or skip a photo.
    pub fn search_unified_page(
        conn: &Connection,
        raw_query: &str,
        semantic_photo_ids: Vec<i64>,
        offset: usize,
        limit: usize,
    ) -> SqliteResult<UnifiedSearchPage> {
        let query = raw_query.trim();
        if query.is_empty() {
            return Ok(UnifiedSearchPage::default());
        }

        let mut intent = Self::parse_smart_intent(conn, query)?;
        intent.semantic_photo_ids = semantic_photo_ids;
        let mut results = UnifiedSearchResults {
            interpreted: Self::interpreted_filters(&intent),
            // Entity hit lists are query-wide and identical on every
            // page — only `photos` is paged.
            people: Self::search_people(conn, query)?,
            albums: Self::search_albums(conn, query)?,
            places: Self::search_places(conn, query)?,
            ..Default::default()
        };

        let (photos, has_more) = Self::search_smart_photos(conn, &intent, offset, limit)?;

        results.photo_ids = photos.iter().map(|r| r.photo_id).collect();
        results.photos_grouped = Self::group_by_date(photos.clone());
        results.photos = photos;
        results.has_more = has_more;

        Ok(UnifiedSearchPage { results, has_more })
    }

    fn parse_smart_intent(conn: &Connection, raw: &str) -> SqliteResult<SmartIntent> {
        let (text_part, date_range) = Self::split_query(raw);
        let mut text = text_part.unwrap_or_default();
        let mut intent = SmartIntent {
            date_range,
            ..Default::default()
        };
        let mut lower = text.to_lowercase();

        if let Some(rest) = lower
            .strip_prefix("only ")
            .or_else(|| lower.strip_prefix("just "))
        {
            intent.people_only = true;
            text = text[text.len() - rest.len()..].trim().to_string();
            lower = text.to_lowercase();
        }

        for (needle, media) in [
            ("videos", SmartMediaType::Video),
            ("video", SmartMediaType::Video),
            ("photos", SmartMediaType::Photo),
            ("photo", SmartMediaType::Photo),
        ] {
            if Self::contains_word(&lower, needle) {
                intent.media_type = Some(media);
                text = Self::remove_word(&text, needle);
                lower = text.to_lowercase();
                break;
            }
        }

        for needle in [
            "favourites",
            "favorites",
            "favourite",
            "favorite",
            "starred",
        ] {
            if Self::contains_word(&lower, needle) {
                intent.favorite = Some(true);
                text = Self::remove_word(&text, needle);
                lower = text.to_lowercase();
                break;
            }
        }

        let album_phrase = if let Some(rest) = lower.strip_prefix("album ") {
            Some(text[text.len() - rest.len()..].trim().to_string())
        } else {
            lower
                .strip_prefix("in album ")
                .map(|rest| text[text.len() - rest.len()..].trim().to_string())
        };
        if let Some(album_query) = album_phrase.as_deref() {
            intent.albums = Self::resolve_albums(conn, album_query)?;
            if !intent.albums.is_empty() {
                text.clear();
            }
        }

        let mut remaining = text.trim().to_string();
        if !remaining.is_empty() {
            let people = Self::resolve_people(conn, &remaining)?;
            if !people.is_empty() {
                remaining =
                    Self::remove_entity_names(&remaining, people.iter().map(|p| p.name.as_str()));
                intent.people_all = people;
            }
        }

        if !remaining.trim().is_empty() {
            let cities = Self::resolve_cities(conn, remaining.trim())?;
            if !cities.is_empty() {
                remaining =
                    Self::remove_entity_names(&remaining, cities.iter().map(|c| c.city.as_str()));
                intent.cities = cities;
            }
        }

        if !remaining.trim().is_empty() {
            let countries = Self::resolve_countries(conn, remaining.trim())?;
            if !countries.is_empty() {
                remaining = Self::remove_entity_names(
                    &remaining,
                    countries.iter().map(|c| c.country.as_str()),
                );
                intent.countries = countries;
            }
        }

        let cleanup = remaining
            .split_whitespace()
            .filter(|w| {
                // Punctuation-only leftovers (e.g. the comma from
                // "Delhi, India") are not meaningful text filters.
                w.chars().any(char::is_alphanumeric)
                    && !matches!(
                        w.to_lowercase().as_str(),
                        "and"
                            | "&"
                            | "with"
                            | "in"
                            | "at"
                            | "from"
                            | "person"
                            | "people"
                            | "containing"
                    )
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !cleanup.trim().is_empty() {
            intent.text = Some(cleanup.trim().to_string());
        }
        Ok(intent)
    }

    fn interpreted_filters(intent: &SmartIntent) -> Vec<InterpretedFilter> {
        let mut out = Vec::new();
        if intent.people_only {
            out.push(InterpretedFilter {
                kind: "only".into(),
                label: "Only".into(),
            });
        }
        for p in &intent.people_all {
            out.push(InterpretedFilter {
                kind: "person".into(),
                label: p.name.clone(),
            });
        }
        for c in &intent.cities {
            out.push(InterpretedFilter {
                kind: "place".into(),
                label: c.label.clone(),
            });
        }
        for c in &intent.countries {
            out.push(InterpretedFilter {
                kind: "place".into(),
                label: c.label.clone(),
            });
        }
        for a in &intent.albums {
            out.push(InterpretedFilter {
                kind: "album".into(),
                label: a.name.clone(),
            });
        }
        if let Some(media) = intent.media_type {
            out.push(InterpretedFilter {
                kind: "media".into(),
                label: media.label().into(),
            });
        }
        if intent.favorite == Some(true) {
            out.push(InterpretedFilter {
                kind: "favorite".into(),
                label: "Favourites".into(),
            });
        }
        if let Some(range) = &intent.date_range {
            out.push(InterpretedFilter {
                kind: "date".into(),
                label: Self::date_label(range),
            });
        }
        if let Some(text) = &intent.text {
            if intent.semantic_photo_ids.is_empty() {
                out.push(InterpretedFilter {
                    kind: "text".into(),
                    label: text.clone(),
                });
            } else {
                out.push(InterpretedFilter {
                    kind: "semantic".into(),
                    label: text.clone(),
                });
            }
        } else if !intent.semantic_photo_ids.is_empty() {
            out.push(InterpretedFilter {
                kind: "semantic".into(),
                label: "Visual meaning".into(),
            });
        }
        out
    }

    /// Peel a date range off the end (or whole) of `q` and return
    /// (remaining_text, date_range). Either component may be None.
    fn split_query(q: &str) -> (Option<String>, Option<DateRange>) {
        let trimmed = q.trim();
        if trimmed.is_empty() {
            return (None, None);
        }
        // Whole-string match first — "March 2019" → date only.
        if let Some(range) = DateParser::parse(trimmed) {
            return (None, Some(range));
        }
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        // Try any two-word date span first — "Goa March 2019 videos",
        // "March 2019 Goa", "last year Paris".
        if words.len() >= 2 {
            for i in 0..(words.len() - 1) {
                let candidate = format!("{} {}", words[i], words[i + 1]);
                if let Some(range) = DateParser::parse(&candidate) {
                    let rest = words
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, word)| {
                            if idx == i || idx == i + 1 {
                                None
                            } else {
                                Some(*word)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    let rest = rest.trim().to_string();
                    return (if rest.is_empty() { None } else { Some(rest) }, Some(range));
                }
            }
        }
        // Try any one-word date span — "2023 Goa", "Goa 2023",
        // "videos 2023 Goa".
        for i in 0..words.len() {
            if let Some(range) = DateParser::parse(words[i]) {
                let rest = words
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, word)| if idx == i { None } else { Some(*word) })
                    .collect::<Vec<_>>()
                    .join(" ");
                let rest = rest.trim().to_string();
                return (if rest.is_empty() { None } else { Some(rest) }, Some(range));
            }
        }
        (Some(trimmed.to_string()), None)
    }

    /// Upper bound on photos a single legacy (non-paged) unified search
    /// returns, and the safety cap the semantic path ranks within.
    /// Hard product ceiling for one query. Callers may page below this
    /// boundary, but no page can expose or request a result beyond it.
    pub const UNIFIED_SEARCH_MAX_PHOTOS: usize = 20_000;

    /// Safety cap on rows fetched for semantic ranking. Ranking happens
    /// in Rust (candidate order first), so pages must come out of one
    /// globally-ordered list; the cap bounds that work.
    const SEMANTIC_FETCH_CAP: usize = 20_000;

    /// Returns (page_rows, has_more).
    fn search_smart_photos(
        conn: &Connection,
        intent: &SmartIntent,
        offset: usize,
        limit: usize,
    ) -> SqliteResult<(Vec<SearchResult>, bool)> {
        let mut sql = String::from(
            "SELECT p.id, p.date_taken, p.location_city, p.location_country, p.thumbnail_path \
             FROM photos p WHERE p.is_trashed = FALSE",
        );
        let mut bind: Vec<Value> = Vec::new();

        if let Some(d) = &intent.date_range {
            sql.push_str(" AND p.date_taken >= ? AND p.date_taken <= ?");
            bind.push(Value::Text(d.start.to_rfc3339()));
            bind.push(Value::Text(d.end.to_rfc3339()));
        }
        if let Some(media) = intent.media_type {
            sql.push_str(" AND p.media_type = ?");
            bind.push(Value::Text(media.as_db().to_string()));
        }
        if intent.favorite == Some(true) {
            sql.push_str(" AND p.is_favorite = TRUE");
        }
        for album in &intent.albums {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM album_photos ap WHERE ap.photo_id = p.id AND ap.album_id = ?)",
            );
            bind.push(Value::Integer(album.id));
        }
        // Cities are alternatives (a photo is in one place at a time):
        // any one of the resolved cities may match. A city keeps its
        // country when known, so identical city names in different
        // countries stay distinguishable.
        if !intent.cities.is_empty() {
            let arms = intent
                .cities
                .iter()
                .map(|c| match &c.country {
                    Some(_) => {
                        "(LOWER(p.location_city) LIKE LOWER(?) \
                                AND LOWER(p.location_country) LIKE LOWER(?))"
                    }
                    None => "LOWER(p.location_city) LIKE LOWER(?)",
                })
                .collect::<Vec<_>>()
                .join(" OR ");
            sql.push_str(&format!(" AND ({arms})"));
            for city in &intent.cities {
                bind.push(Value::Text(format!("%{}%", city.city)));
                if let Some(country) = &city.country {
                    bind.push(Value::Text(format!("%{}%", country)));
                }
            }
        }
        // Countries likewise OR together; a country predicate matches
        // every city inside the country (never narrowed to one city).
        if !intent.countries.is_empty() {
            let arms = std::iter::repeat_n(
                "LOWER(p.location_country) LIKE LOWER(?)",
                intent.countries.len(),
            )
            .collect::<Vec<_>>()
            .join(" OR ");
            sql.push_str(&format!(" AND ({arms})"));
            for country in &intent.countries {
                bind.push(Value::Text(format!("%{}%", country.country)));
            }
        }
        for person in &intent.people_all {
            sql.push_str(
                " AND (EXISTS (
                    SELECT 1 FROM faces f
                    WHERE f.photo_id = p.id AND f.cluster_id = ?
                ) OR EXISTS (
                    SELECT 1 FROM photo_inferred_identities pii
                    WHERE pii.photo_id = p.id AND pii.cluster_id = ?
                ))",
            );
            bind.push(Value::Integer(person.id));
            bind.push(Value::Integer(person.id));
        }
        if intent.people_only && !intent.people_all.is_empty() {
            sql.push_str(" AND p.faces_processed = TRUE");
            let placeholders = std::iter::repeat_n("?", intent.people_all.len())
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(
                " AND NOT EXISTS (
                    SELECT 1 FROM faces f
                    WHERE f.photo_id = p.id
                      AND (f.cluster_id IS NULL OR f.cluster_id NOT IN ({placeholders}))
                )"
            ));
            for person in &intent.people_all {
                bind.push(Value::Integer(person.id));
            }
            sql.push_str(&format!(
                " AND NOT EXISTS (
                    SELECT 1 FROM photo_inferred_identities pii
                    WHERE pii.photo_id = p.id AND pii.cluster_id NOT IN ({placeholders})
                )"
            ));
            for person in &intent.people_all {
                bind.push(Value::Integer(person.id));
            }
        }
        if let Some(t) = &intent.text {
            let semantic_clause = if intent.semantic_photo_ids.is_empty() {
                String::new()
            } else {
                format!(
                    " OR p.id IN ({})",
                    std::iter::repeat_n("?", intent.semantic_photo_ids.len())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            sql.push_str(&format!(
                " AND (
                    LOWER(p.file_name) LIKE LOWER(?) OR
                    LOWER(p.location_city) LIKE LOWER(?) OR
                    LOWER(p.location_country) LIKE LOWER(?) OR
                    LOWER(p.camera_make) LIKE LOWER(?) OR
                    LOWER(p.camera_model) LIKE LOWER(?) OR
                    LOWER(COALESCE(p.camera_make, '') || ' ' || COALESCE(p.camera_model, '')) LIKE LOWER(?) OR
                    EXISTS (
                        SELECT 1
                        FROM faces f
                        JOIN face_clusters fc ON fc.id = f.cluster_id
                        WHERE f.photo_id = p.id AND LOWER(fc.name) LIKE LOWER(?)
                    ) OR
                    EXISTS (
                        SELECT 1
                        FROM photo_inferred_identities pii
                        JOIN face_clusters fc ON fc.id = pii.cluster_id
                        WHERE pii.photo_id = p.id AND LOWER(fc.name) LIKE LOWER(?)
                    )
                    {semantic_clause}
                )",
            ));
            let like = Value::Text(format!("%{}%", t));
            for _ in 0..8 {
                bind.push(like.clone());
            }
            for id in &intent.semantic_photo_ids {
                bind.push(Value::Integer(*id));
            }
        } else if !intent.semantic_photo_ids.is_empty() && !intent.has_structured_filters() {
            sql.push_str(&format!(
                " AND p.id IN ({})",
                std::iter::repeat_n("?", intent.semantic_photo_ids.len())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            for id in &intent.semantic_photo_ids {
                bind.push(Value::Integer(*id));
            }
        }

        if intent.semantic_photo_ids.is_empty() {
            // Pure SQL ordering is stable, so offset paging can be done
            // entirely in SQLite. One extra row detects has_more.
            let remaining = Self::UNIFIED_SEARCH_MAX_PHOTOS.saturating_sub(offset);
            let page_limit = limit.min(remaining);
            sql.push_str(&format!(
                " ORDER BY p.date_taken DESC, p.id DESC LIMIT {} OFFSET {}",
                page_limit.saturating_add(1),
                offset
            ));
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(bind.iter()), |row| {
                Ok(SearchResult {
                    photo_id: row.get(0)?,
                    date_taken: row.get(1)?,
                    location_city: row.get(2)?,
                    location_country: row.get(3)?,
                    thumbnail_path: row.get(4)?,
                })
            })?;
            let mut results = rows.collect::<SqliteResult<Vec<_>>>()?;
            let has_more =
                results.len() > page_limit && offset + page_limit < Self::UNIFIED_SEARCH_MAX_PHOTOS;
            results.truncate(page_limit);
            Ok((results, has_more))
        } else {
            // Semantic ranking is computed in Rust over the candidate
            // order, so all matching rows are fetched up to a safety
            // cap and paged from the globally-ranked list. This keeps
            // ranking stable across pages.
            sql.push_str(&format!(
                " ORDER BY p.date_taken DESC, p.id DESC LIMIT {}",
                Self::SEMANTIC_FETCH_CAP
            ));
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(bind.iter()), |row| {
                Ok(SearchResult {
                    photo_id: row.get(0)?,
                    date_taken: row.get(1)?,
                    location_city: row.get(2)?,
                    location_country: row.get(3)?,
                    thumbnail_path: row.get(4)?,
                })
            })?;
            let mut results = rows.collect::<SqliteResult<Vec<_>>>()?;
            let rank: HashMap<i64, usize> = intent
                .semantic_photo_ids
                .iter()
                .enumerate()
                .map(|(idx, id)| (*id, idx))
                .collect();
            results.sort_by(
                |a, b| match (rank.get(&a.photo_id), rank.get(&b.photo_id)) {
                    (Some(ra), Some(rb)) => ra.cmp(rb),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => b
                        .date_taken
                        .cmp(&a.date_taken)
                        .then(b.photo_id.cmp(&a.photo_id)),
                },
            );
            let page_limit = limit.min(Self::UNIFIED_SEARCH_MAX_PHOTOS.saturating_sub(offset));
            let has_more = offset + page_limit < results.len()
                && offset + page_limit < Self::UNIFIED_SEARCH_MAX_PHOTOS;
            let page: Vec<SearchResult> =
                results.into_iter().skip(offset).take(page_limit).collect();
            Ok((page, has_more))
        }
    }

    fn resolve_people(conn: &Connection, text: &str) -> SqliteResult<Vec<ResolvedPerson>> {
        let mut stmt = conn.prepare(
            "SELECT id, name FROM face_clusters
             WHERE name IS NOT NULL AND trim(name) != ''
             ORDER BY length(name) DESC, photo_count DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ResolvedPerson {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        let lower = text.to_lowercase();
        let mut out = Vec::new();
        for row in rows {
            let person = row?;
            if Self::contains_phrase(&lower, &person.name.to_lowercase()) {
                out.push(person);
            }
        }
        Ok(out)
    }

    fn resolve_albums(conn: &Connection, text: &str) -> SqliteResult<Vec<ResolvedAlbum>> {
        let like = format!("%{}%", text.trim());
        let mut stmt = conn.prepare(
            "SELECT id, name FROM albums
             WHERE LOWER(name) LIKE LOWER(?1)
             ORDER BY updated_at DESC
             LIMIT 5",
        )?;
        let rows = stmt.query_map(params![like], |row| {
            Ok(ResolvedAlbum {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Resolve city filters. Cities come from the grouped (city,
    /// country) photo rows — capped generously, not only from the top
    /// 100 most frequent places. Multiple matching cities are
    /// alternatives, not simultaneous requirements.
    fn resolve_cities(conn: &Connection, text: &str) -> SqliteResult<Vec<ResolvedCity>> {
        let mut stmt = conn.prepare(
            "SELECT location_city, location_country, COUNT(*) AS cnt
             FROM photos
             WHERE is_trashed = FALSE
               AND location_city IS NOT NULL
             GROUP BY location_city, location_country
             ORDER BY cnt DESC
             LIMIT 1000",
        )?;
        let rows = stmt.query_map([], |row| {
            let city: String = row.get(0)?;
            let country: Option<String> = row.get(1)?;
            let label = match &country {
                Some(country) => format!("{}, {}", city, country),
                None => city.clone(),
            };
            Ok(ResolvedCity {
                city,
                country,
                label,
            })
        })?;
        let lower = text.to_lowercase();
        let mut out = Vec::new();
        for row in rows {
            let place = row?;
            if Self::contains_phrase(&lower, &place.city.to_lowercase()) {
                out.push(place);
                if out.len() >= 3 {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Resolve country filters from the full set of countries present
    /// in the library (not derived from the top-N city rows). An exact
    /// country phrase becomes a country predicate and never narrows to
    /// individual cities.
    fn resolve_countries(conn: &Connection, text: &str) -> SqliteResult<Vec<ResolvedCountry>> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT location_country
             FROM photos
             WHERE is_trashed = FALSE
               AND location_country IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            let country: String = row.get(0)?;
            Ok(ResolvedCountry {
                label: country.clone(),
                country,
            })
        })?;
        let lower = text.to_lowercase();
        let mut out = Vec::new();
        for row in rows {
            let resolved = row?;
            if Self::contains_phrase(&lower, &resolved.country.to_lowercase()) {
                out.push(resolved);
                if out.len() >= 3 {
                    break;
                }
            }
        }
        Ok(out)
    }

    fn contains_word(lower: &str, needle: &str) -> bool {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == needle)
    }

    fn contains_phrase(lower: &str, phrase: &str) -> bool {
        if phrase.trim().is_empty() {
            return false;
        }
        // Normalize punctuation to word boundaries so queries such as
        // "Paris, France" match the same entity spans as "Paris France".
        // Token normalization also avoids substring matches inside longer
        // names and works for Unicode city/country names.
        let normalize = |value: &str| {
            value
                .split(|c: char| !c.is_alphanumeric())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let haystack = normalize(lower);
        let needle = normalize(phrase);
        haystack == needle
            || haystack.contains(&format!(" {needle} "))
            || haystack.starts_with(&format!("{needle} "))
            || haystack.ends_with(&format!(" {needle}"))
    }

    fn remove_word(text: &str, needle: &str) -> String {
        text.split_whitespace()
            .filter(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
                    != needle
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Remove every recognized `name` from `text` by blanking the
    /// matched spans (replaced with spaces, so later whitespace-based
    /// passes still work). Matching is case-insensitive with Unicode
    /// case folding, and requires the span to start and end on phrase
    /// boundaries — a name embedded inside a longer word ("Tata" in
    /// "Tataganj") is left alone.
    fn remove_entity_names<'a>(text: &str, names: impl Iterator<Item = &'a str>) -> String {
        let mut out = text.to_string();
        for name in names {
            out = Self::remove_phrase(&out, name);
        }
        out
    }

    fn remove_phrase(text: &str, phrase: &str) -> String {
        let phrase = phrase.trim();
        if phrase.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let t: Vec<char> = text.chars().collect();
        let p: Vec<char> = phrase.chars().collect();
        let is_word_char = |c: char| c.is_alphanumeric();
        let caseless_eq = |a: char, b: char| a.to_lowercase().eq(b.to_lowercase());

        let mut masked = vec![false; t.len()];
        let mut i = 0;
        while i + p.len() <= t.len() {
            if t[i..i + p.len()]
                .iter()
                .zip(p.iter())
                .all(|(&a, &b)| caseless_eq(a, b))
            {
                let end = i + p.len();
                let before_ok = i == 0 || !is_word_char(t[i - 1]);
                let after_ok = end == t.len() || !is_word_char(t[end]);
                if before_ok && after_ok {
                    for m in masked[i..end].iter_mut() {
                        *m = true;
                    }
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
        if masked.iter().all(|m| !*m) {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        for (c, m) in t.iter().zip(masked.iter()) {
            if *m {
                out.push(' ');
            } else {
                out.push(*c);
            }
        }
        out
    }

    fn date_label(range: &DateRange) -> String {
        let start = range.start.format("%Y-%m-%d").to_string();
        let end = range.end.format("%Y-%m-%d").to_string();
        if start == end {
            start
        } else {
            format!("{} to {}", start, end)
        }
    }

    fn search_people(conn: &Connection, q: &str) -> SqliteResult<Vec<PersonHit>> {
        let like = format!("%{}%", q);
        // representative_face_id → relative `.photovault/faces/<id>.jpg`
        // path. Mirrors face_repo::populate_face_thumbnails so the
        // frontend's thumbUrl helper resolves the file uniformly.
        let mut stmt = conn.prepare(
            r#"
            SELECT fc.id, fc.name, fc.photo_count, fc.representative_face_id
            FROM face_clusters fc
            WHERE fc.name IS NOT NULL
              AND LOWER(fc.name) LIKE LOWER(?1)
            ORDER BY fc.photo_count DESC
            LIMIT 10
            "#,
        )?;
        let rows = stmt.query_map(params![like], |row| {
            let face_id: Option<i64> = row.get(3)?;
            Ok(PersonHit {
                cluster_id: row.get(0)?,
                name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                photo_count: row.get(2)?,
                face_thumbnail_path: face_id.map(|id| format!(".photovault/faces/{}.jpg", id)),
            })
        })?;
        rows.collect()
    }

    fn search_albums(conn: &Connection, q: &str) -> SqliteResult<Vec<AlbumHit>> {
        let like = format!("%{}%", q);
        // LEFT JOIN to pull the cover photo's thumbnail_path so the
        // frontend can render the album hit with its cover image.
        let mut stmt = conn.prepare(
            r#"
            SELECT a.id, a.name, a.photo_count, a.cover_photo_id, pcov.thumbnail_path
            FROM albums a
            LEFT JOIN photos pcov ON pcov.id = a.cover_photo_id
            WHERE LOWER(a.name) LIKE LOWER(?1)
            ORDER BY a.updated_at DESC
            LIMIT 10
            "#,
        )?;
        let rows = stmt.query_map(params![like], |row| {
            Ok(AlbumHit {
                album_id: row.get(0)?,
                name: row.get(1)?,
                photo_count: row.get(2)?,
                cover_photo_id: row.get(3)?,
                cover_thumbnail_path: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    fn search_places(conn: &Connection, q: &str) -> SqliteResult<Vec<PlaceHit>> {
        let like = format!("%{}%", q);
        let mut stmt = conn.prepare(
            r#"
            SELECT location_city, location_country, COUNT(*) AS cnt
            FROM photos
            WHERE is_trashed = FALSE
              AND location_city IS NOT NULL
              AND (LOWER(location_city) LIKE LOWER(?1)
                   OR LOWER(location_country) LIKE LOWER(?1))
            GROUP BY location_city, location_country
            ORDER BY cnt DESC
            LIMIT 10
            "#,
        )?;
        let rows = stmt.query_map(params![like], |row| {
            Ok(PlaceHit {
                city: row.get(0)?,
                country: row.get(1)?,
                photo_count: row.get(2)?,
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                date_taken TEXT,
                location_city TEXT,
                location_country TEXT,
                camera_make TEXT,
                camera_model TEXT,
                thumbnail_path TEXT,
                faces_processed BOOLEAN DEFAULT FALSE,
                media_type TEXT NOT NULL DEFAULT 'photo',
                is_favorite BOOLEAN DEFAULT FALSE,
                is_trashed BOOLEAN DEFAULT FALSE
            );
            CREATE TABLE face_clusters (
                id INTEGER PRIMARY KEY,
                name TEXT,
                photo_count INTEGER NOT NULL DEFAULT 0,
                representative_face_id INTEGER
            );
            CREATE TABLE faces (
                id INTEGER PRIMARY KEY,
                photo_id INTEGER NOT NULL,
                cluster_id INTEGER
            );
            CREATE TABLE photo_inferred_identities (
                photo_id INTEGER NOT NULL,
                cluster_id INTEGER NOT NULL
            );
            CREATE TABLE albums (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                photo_count INTEGER NOT NULL DEFAULT 0,
                cover_photo_id INTEGER,
                updated_at TEXT
            );
            CREATE TABLE album_photos (
                album_id INTEGER NOT NULL,
                photo_id INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
        conn
    }

    fn insert_photo(conn: &Connection, id: i64, file_name: &str, date_taken: &str) {
        conn.execute(
            "INSERT INTO photos
                (id, file_path, file_name, file_hash, file_size, date_taken, media_type, thumbnail_path)
             VALUES (?1, ?2, ?3, ?4, 100, ?5, 'photo', ?6)",
            params![
                id,
                format!("{file_name}.jpg"),
                file_name,
                format!("hash-{id}"),
                date_taken,
                format!(".photovault/thumbs/{file_name}.jpg")
            ],
        )
        .unwrap();
    }

    fn set_location(conn: &Connection, id: i64, city: &str, country: &str) {
        conn.execute(
            "UPDATE photos SET location_city = ?2, location_country = ?3 WHERE id = ?1",
            params![id, city, country],
        )
        .unwrap();
    }

    fn insert_person(conn: &Connection, id: i64, name: &str, photo_ids: &[i64]) {
        conn.execute(
            "INSERT INTO face_clusters (id, name, photo_count) VALUES (?1, ?2, ?3)",
            params![id, name, photo_ids.len() as i64],
        )
        .unwrap();
        for (idx, photo_id) in photo_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO faces (id, photo_id, cluster_id) VALUES (?1, ?2, ?3)",
                params![10_000 + idx as i64 + id * 100, photo_id, id],
            )
            .unwrap();
        }
    }

    #[test]
    fn unified_search_returns_semantic_matches_when_text_does_not_match_metadata() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "img001", "2024-01-01T10:00:00Z");
        insert_photo(&conn, 2, "img002", "2024-01-02T10:00:00Z");
        insert_photo(&conn, 3, "img003", "2024-01-03T10:00:00Z");

        let results =
            SearchService::search_unified_with_semantic(&conn, "group photo", vec![3, 1]).unwrap();

        assert_eq!(results.photo_ids, vec![3, 1]);
        assert_eq!(
            results
                .interpreted
                .iter()
                .map(|f| (f.kind.as_str(), f.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("media", "Photos"), ("semantic", "group")]
        );
    }

    #[test]
    fn unified_search_ands_date_filter_with_semantic_matches() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "img001", "2023-06-01T10:00:00Z");
        insert_photo(&conn, 2, "img002", "2024-06-01T10:00:00Z");
        insert_photo(&conn, 3, "img003", "2024-07-01T10:00:00Z");

        let results =
            SearchService::search_unified_with_semantic(&conn, "family 2024", vec![1, 3, 2])
                .unwrap();

        assert_eq!(results.photo_ids, vec![3, 2]);
        assert!(results
            .interpreted
            .iter()
            .any(|f| f.kind == "semantic" && f.label == "family"));
        assert!(results.interpreted.iter().any(|f| f.kind == "date"));
    }

    #[test]
    fn unified_search_excludes_trashed_semantic_matches() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "img001", "2024-01-01T10:00:00Z");
        insert_photo(&conn, 2, "img002", "2024-01-02T10:00:00Z");
        conn.execute("UPDATE photos SET is_trashed = TRUE WHERE id = 1", [])
            .unwrap();

        let results =
            SearchService::search_unified_with_semantic(&conn, "beach", vec![1, 2]).unwrap();

        assert_eq!(results.photo_ids, vec![2]);
    }

    #[test]
    fn unified_search_resolves_person_and_place_independent_of_order() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "vizag-tata", "2024-01-01T10:00:00Z");
        insert_photo(&conn, 2, "vizag-other", "2024-01-02T10:00:00Z");
        insert_photo(&conn, 3, "goa-tata", "2024-01-03T10:00:00Z");
        set_location(&conn, 1, "Vizianagaram", "India");
        set_location(&conn, 2, "Vizianagaram", "India");
        set_location(&conn, 3, "Goa", "India");
        insert_person(&conn, 7, "Tata", &[1, 3]);

        let a = SearchService::search_unified_with_semantic(&conn, "tata vizianagaram", vec![])
            .unwrap();
        let b = SearchService::search_unified_with_semantic(&conn, "vizianagaram tata", vec![])
            .unwrap();

        assert_eq!(a.photo_ids, vec![1]);
        assert_eq!(b.photo_ids, vec![1]);
        assert_eq!(
            a.interpreted
                .iter()
                .map(|f| (f.kind.as_str(), f.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("person", "Tata"), ("place", "Vizianagaram, India")]
        );
        assert_eq!(
            b.interpreted
                .iter()
                .map(|f| (f.kind.as_str(), f.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("person", "Tata"), ("place", "Vizianagaram, India")]
        );
    }

    #[test]
    fn semantic_does_not_delete_fully_structured_matches() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "vizag-tata", "2024-01-01T10:00:00Z");
        insert_photo(&conn, 2, "vizag-tata-older", "2023-01-01T10:00:00Z");
        set_location(&conn, 1, "Vizianagaram", "India");
        set_location(&conn, 2, "Vizianagaram", "India");
        insert_person(&conn, 7, "Tata", &[1, 2]);

        let results =
            SearchService::search_unified_with_semantic(&conn, "tata vizianagaram", vec![1])
                .unwrap();

        assert_eq!(results.photo_ids, vec![1, 2]);
    }

    #[test]
    fn place_search_handles_case_punctuation_and_unicode_without_span_loss() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "paris", "2024-01-01T10:00:00Z");
        insert_photo(&conn, 2, "munich", "2024-01-02T10:00:00Z");
        set_location(&conn, 1, "Paris", "France");
        set_location(&conn, 2, "München", "Germany");

        let paris =
            SearchService::search_unified_with_semantic(&conn, "PARIS, France", vec![]).unwrap();
        assert_eq!(paris.photo_ids, vec![1]);

        let munich = SearchService::search_unified_with_semantic(&conn, "mÜnchen", vec![]).unwrap();
        assert_eq!(munich.photo_ids, vec![2]);
    }

    #[test]
    fn semantic_visual_text_still_filters_inside_structured_matches() {
        let conn = search_test_conn();
        insert_photo(&conn, 1, "vizag-tata-car", "2024-01-01T10:00:00Z");
        insert_photo(&conn, 2, "vizag-tata-home", "2024-01-02T10:00:00Z");
        set_location(&conn, 1, "Vizianagaram", "India");
        set_location(&conn, 2, "Vizianagaram", "India");
        insert_person(&conn, 7, "Tata", &[1, 2]);

        let results =
            SearchService::search_unified_with_semantic(&conn, "car tata vizianagaram", vec![1])
                .unwrap();

        assert_eq!(results.photo_ids, vec![1]);
        assert!(results
            .interpreted
            .iter()
            .any(|f| f.kind == "semantic" && f.label == "car"));
    }
}
