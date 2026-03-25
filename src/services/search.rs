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
#[derive(Debug, Clone)]
pub struct SearchResultGroup {
    pub date: String,
    pub location: Option<String>,
    pub results: Vec<SearchResult>,
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
            SELECT DISTINCT f.photo_id
            FROM faces f
            JOIN face_clusters fc ON f.cluster_id = fc.id
            WHERE fc.name IS NOT NULL AND LOWER(fc.name) LIKE LOWER(?1)
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

    pub fn get_suggestions(conn: &Connection, partial: &str) -> SqliteResult<Vec<String>> {
        if partial.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        let like = format!("%{}%", partial);

        let mut names_stmt = conn.prepare(
            "SELECT DISTINCT name FROM face_clusters WHERE name IS NOT NULL AND LOWER(name) LIKE LOWER(?1) LIMIT 5",
        )?;
        let names: Vec<String> = names_stmt
            .query_map(params![&like], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        out.extend(names);

        let mut city_stmt = conn.prepare(
            "SELECT DISTINCT location_city FROM photos WHERE location_city IS NOT NULL AND LOWER(location_city) LIKE LOWER(?1) LIMIT 5",
        )?;
        let cities: Vec<String> = city_stmt
            .query_map(params![&like], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        out.extend(cities);

        out.sort();
        out.dedup();
        Ok(out)
    }
}
