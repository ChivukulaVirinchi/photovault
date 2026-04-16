//! Search service - executes parsed queries against the database.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::search::SearchQuery;

/// A single search result row.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub photo_id: i64,
    pub date_taken: Option<String>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
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

/// Unified search results across all entity types.
#[derive(Debug, Clone, Default)]
pub struct UnifiedSearchResults {
    pub people: Vec<PersonHit>,
    pub albums: Vec<AlbumHit>,
    pub places: Vec<PlaceHit>,
    pub photos: Vec<SearchResult>,
    pub photos_grouped: Vec<SearchResultGroup>,
    /// Flat list for cull mode.
    pub photo_ids: Vec<i64>,
}

/// Search service.
pub struct SearchService;

impl SearchService {
    pub fn search(conn: &Connection, query: &SearchQuery) -> SqliteResult<Vec<SearchResult>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
            "SELECT id, file_path, date_taken, location_city, location_country FROM photos WHERE is_trashed = FALSE",
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
            .filter_map(|r| r.ok())
            .collect();

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
        let query = raw_query.trim();
        if query.is_empty() {
            return Ok(UnifiedSearchResults::default());
        }

        let mut results = UnifiedSearchResults::default();

        // 1. People — fuzzy match on cluster name
        results.people = Self::search_people(conn, query)?;

        // 2. Albums — fuzzy match on album name
        results.albums = Self::search_albums(conn, query)?;

        // 3. Places — fuzzy match on city or country
        results.places = Self::search_places(conn, query)?;

        // 4. Photos — use existing parsed query filter
        let parsed = crate::search::QueryParser::parse(query);
        let photos = Self::search(conn, &parsed)?;
        results.photo_ids = photos.iter().map(|r| r.photo_id).collect();
        results.photos_grouped = Self::group_by_date(photos.clone());
        results.photos = photos;

        Ok(results)
    }

    fn search_people(conn: &Connection, q: &str) -> SqliteResult<Vec<PersonHit>> {
        let like = format!("%{}%", q);
        let mut stmt = conn.prepare(
            r#"
            SELECT fc.id, fc.name, fc.photo_count
            FROM face_clusters fc
            WHERE fc.name IS NOT NULL
              AND LOWER(fc.name) LIKE LOWER(?1)
            ORDER BY fc.photo_count DESC
            LIMIT 10
            "#,
        )?;
        let rows = stmt.query_map(params![like], |row| {
            Ok(PersonHit {
                cluster_id: row.get(0)?,
                name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                photo_count: row.get(2)?,
                face_thumbnail_path: None,
            })
        })?;
        rows.collect()
    }

    fn search_albums(conn: &Connection, q: &str) -> SqliteResult<Vec<AlbumHit>> {
        let like = format!("%{}%", q);
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, photo_count
            FROM albums
            WHERE LOWER(name) LIKE LOWER(?1)
            ORDER BY updated_at DESC
            LIMIT 10
            "#,
        )?;
        let rows = stmt.query_map(params![like], |row| {
            Ok(AlbumHit {
                album_id: row.get(0)?,
                name: row.get(1)?,
                photo_count: row.get(2)?,
                cover_thumbnail_path: None,
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
