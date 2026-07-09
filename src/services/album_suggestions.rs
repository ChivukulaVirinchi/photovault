//! Album suggestion detection service.
//!
//! Analyses photo metadata (location, time, faces) to propose trip and event
//! albums that the user can accept, dismiss, or ignore.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Datelike, NaiveDate, Utc};
use rusqlite::{params, Connection};

use crate::db::album_suggestion_repo::AlbumSuggestionRepo;

/// A detected suggestion before persistence.
#[derive(Debug, Clone)]
pub struct DetectedSuggestion {
    pub kind: String,
    pub title: String,
    pub photo_ids: Vec<i64>,
    pub cover_photo_id: Option<i64>,
    pub fingerprint: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct SuggestionDiagnostics {
    pub total_photos_with_date: i64,
    pub photos_with_city: i64,
    /// Photos that carry GPS lat/lng (with or without a resolved city).
    /// Distinguishes "no place names yet, run Fill in place names" from
    /// "your photos have no GPS metadata at all" — those need
    /// different advice.
    pub photos_with_gps: i64,
    pub home_city: Option<String>,
    pub trip_rows: usize,
    pub trip_gate_duration_rejected: usize,
    pub trip_gate_photo_count_rejected: usize,
    pub trip_gate_rarity_rejected: usize,
    pub trip_gate_home_distance_rejected: usize,
    pub trip_gate_album_overlap_rejected: usize,
    pub trip_candidates_passed: usize,
    pub event_windows: usize,
    pub event_gate_photo_count_rejected: usize,
    pub event_gate_trip_overlap_rejected: usize,
    pub event_gate_signal_rejected: usize,
    pub event_gate_album_overlap_rejected: usize,
    pub event_candidates_passed: usize,
    pub persisted_new: usize,
    pub skipped_existing_fingerprint: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Haversine distance in kilometres between two lat/lng pairs.
pub fn haversine_km(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6371.0; // Earth radius km
    let dlat = (lat2 - lat1).to_radians();
    let dlng = ((lng2 - lng1 + 540.0).rem_euclid(360.0) - 180.0).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

fn parse_date_prefix(date_str: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date_str.get(..10)?, "%Y-%m-%d").ok()
}

/// Compute a stable fingerprint from a sorted set of photo IDs.
/// Uses std DefaultHasher for speed (no crypto needed).
pub fn compute_fingerprint(photo_ids: &[i64]) -> String {
    let mut sorted = photo_ids.to_vec();
    sorted.sort_unstable();
    let mut hasher = DefaultHasher::new();
    sorted.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Pick the best cover photo from a list of IDs: prefer landscape with faces,
/// newest first. Falls back to the first photo if nothing matches.
pub fn pick_cover(conn: &Connection, photo_ids: &[i64]) -> Option<i64> {
    if photo_ids.is_empty() {
        return None;
    }

    // Build a comma-separated list for IN clause
    let placeholders: Vec<String> = photo_ids.iter().map(|_| "?".to_string()).collect();
    let in_clause = placeholders.join(",");

    let sql = format!(
        r#"SELECT p.id FROM photos p
           LEFT JOIN faces f ON p.id = f.photo_id
           WHERE p.id IN ({})
             AND p.is_trashed = FALSE
           GROUP BY p.id
           ORDER BY
             p.media_type = 'photo' DESC,
             COUNT(f.id) > 0 DESC,
             p.width > p.height DESC,
             p.date_taken DESC
           LIMIT 1"#,
        in_clause,
    );

    let mut stmt = conn.prepare(&sql).ok()?;
    let params: Vec<rusqlite::types::Value> = photo_ids
        .iter()
        .map(|id| rusqlite::types::Value::Integer(*id))
        .collect();
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    stmt.query_row(&*param_refs, |row| row.get::<_, i64>(0))
        .ok()
}

// ---------------------------------------------------------------------------
// Home city detection
// ---------------------------------------------------------------------------

/// Determine the user's "home city" — the city with the most distinct photo-weeks.
/// Returns (city, country, centroid_lat, centroid_lng) or None.
pub fn detect_home_city(
    conn: &Connection,
    override_city: Option<&str>,
) -> Option<(String, String, f64, f64)> {
    // If user provided an override, look it up
    if let Some(city_name) = override_city {
        if !city_name.trim().is_empty() {
            let result: Option<(String, String, f64, f64)> = conn
                .query_row(
                    r#"SELECT location_city, COALESCE(location_country,''),
                              AVG(gps_latitude), AVG(gps_longitude)
                       FROM photos
                       WHERE location_city = ?1
                         AND gps_latitude IS NOT NULL
                         AND is_trashed = FALSE
                       GROUP BY location_city, COALESCE(location_country,'')
                       ORDER BY COUNT(DISTINCT strftime('%Y-%W', date_taken)) DESC
                       LIMIT 1"#,
                    params![city_name.trim()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .ok();
            if result.is_some() {
                return result;
            }
        }
    }

    // Auto-detect: city with most distinct weeks of photos
    conn.query_row(
        r#"SELECT location_city, COALESCE(location_country,''),
                  AVG(gps_latitude), AVG(gps_longitude)
           FROM photos
           WHERE location_city IS NOT NULL
             AND gps_latitude IS NOT NULL
             AND is_trashed = FALSE
           GROUP BY location_city, COALESCE(location_country,'')
           ORDER BY COUNT(DISTINCT strftime('%Y-%W', date_taken)) DESC
           LIMIT 1"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .ok()
}

// ---------------------------------------------------------------------------
// Trip detection
// ---------------------------------------------------------------------------

/// Intermediate: a city visit span.
#[allow(dead_code)]
struct CitySpan {
    city: String,
    country: String,
    start: NaiveDate,
    end: NaiveDate,
    photo_ids: Vec<i64>,
}

/// Detect trip suggestions.
/// A trip is a contiguous span of >= 2 days in a non-home city with >= 8 photos,
/// where the city appears in < 10% of total photo-weeks and is >= 50 km from home.
pub fn detect_trips(
    conn: &Connection,
    home: Option<&(String, String, f64, f64)>,
) -> Vec<DetectedSuggestion> {
    // Query: all photos with city + date, ordered by city then date
    let rows: Vec<(i64, String, String, String)> = {
        let mut stmt = match conn.prepare(
            r#"SELECT p.id, p.location_city, COALESCE(p.location_country,''), p.date_taken
               FROM photos p
               WHERE p.location_city IS NOT NULL
                 AND p.date_taken IS NOT NULL
                 AND p.is_trashed = FALSE
               ORDER BY p.location_city, COALESCE(p.location_country,''), p.date_taken"#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .and_then(|iter| iter.collect::<rusqlite::Result<Vec<_>>>())
        .unwrap_or_else(|e| {
            tracing::warn!("album trip detection skipped: failed reading city rows: {e}");
            Vec::new()
        })
    };

    if rows.is_empty() {
        return Vec::new();
    }

    // Total distinct weeks across entire library (for rarity gate)
    let total_weeks: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT strftime('%Y-%W', date_taken)) FROM photos WHERE date_taken IS NOT NULL AND is_trashed = FALSE",
            [],
            |row| row.get(0),
        )
        .unwrap_or(1)
        .max(1);

    // Count of distinct weeks per city
    let city_weeks: HashMap<(String, String), i64> = {
        let mut stmt = match conn.prepare(
            r#"SELECT location_city, COALESCE(location_country,''), COUNT(DISTINCT strftime('%Y-%W', date_taken))
                   FROM photos
                   WHERE location_city IS NOT NULL AND date_taken IS NOT NULL AND is_trashed = FALSE
                   GROUP BY location_city, COALESCE(location_country,'')"#,
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                row.get::<_, i64>(2)?,
            ))
        })
        .and_then(|iter| iter.collect::<rusqlite::Result<HashMap<_, _>>>())
        .unwrap_or_else(|e| {
            tracing::warn!("album trip detection skipped: failed reading city week rows: {e}");
            HashMap::new()
        })
    };

    // City GPS centroids (for distance check)
    let city_centroids: HashMap<(String, String), (f64, f64)> = {
        let mut stmt = match conn.prepare(
            r#"SELECT location_city, COALESCE(location_country,''), AVG(gps_latitude), AVG(gps_longitude)
                   FROM photos
                   WHERE location_city IS NOT NULL AND gps_latitude IS NOT NULL AND is_trashed = FALSE
                   GROUP BY location_city, COALESCE(location_country,'')"#,
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                (row.get::<_, f64>(2)?, row.get::<_, f64>(3)?),
            ))
        })
        .and_then(|iter| iter.collect::<rusqlite::Result<HashMap<_, _>>>())
        .unwrap_or_else(|e| {
            tracing::warn!("album trip detection skipped: failed reading city centroid rows: {e}");
            HashMap::new()
        })
    };

    // How many photos are already in user albums, keyed by photo_id
    let album_photo_set: HashSet<i64> = {
        match conn.prepare("SELECT DISTINCT photo_id FROM album_photos") {
            Ok(mut stmt) => stmt
                .query_map([], |row| row.get::<_, i64>(0))
                .and_then(|iter| iter.collect::<rusqlite::Result<HashSet<_>>>())
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "album trip detection continuing without album membership filter: {e}"
                    );
                    HashSet::new()
                }),
            Err(_) => HashSet::new(),
        }
    };

    let home_place = home.as_ref().map(|h| (h.0.as_str(), h.1.as_str()));
    let home_coords = home.as_ref().map(|h| (h.2, h.3));

    // Group rows by city and split into contiguous date spans
    let mut spans: Vec<CitySpan> = Vec::new();
    let mut current_city = String::new();
    let mut current_country = String::new();
    let mut current_dates: Vec<(NaiveDate, i64)> = Vec::new();

    let flush_city =
        |city: &str, country: &str, dates: &[(NaiveDate, i64)], out: &mut Vec<CitySpan>| {
            if dates.is_empty() {
                return;
            }
            // Split into contiguous spans (gap > 3 days = new span)
            let mut span_start = dates[0].0;
            let mut span_end = dates[0].0;
            let mut span_ids: Vec<i64> = vec![dates[0].1];

            for &(d, id) in &dates[1..] {
                if (d - span_end).num_days() > 3 {
                    out.push(CitySpan {
                        city: city.to_string(),
                        country: country.to_string(),
                        start: span_start,
                        end: span_end,
                        photo_ids: std::mem::take(&mut span_ids),
                    });
                    span_start = d;
                }
                span_end = d;
                span_ids.push(id);
            }
            out.push(CitySpan {
                city: city.to_string(),
                country: country.to_string(),
                start: span_start,
                end: span_end,
                photo_ids: span_ids,
            });
        };

    for (id, city, country, date_str) in &rows {
        let Some(date) = parse_date_prefix(date_str) else {
            continue;
        };

        if city != &current_city || country != &current_country {
            flush_city(&current_city, &current_country, &current_dates, &mut spans);
            current_city = city.clone();
            current_country = country.clone();
            current_dates.clear();
        }
        current_dates.push((date, *id));
    }
    flush_city(&current_city, &current_country, &current_dates, &mut spans);

    // Filter spans through the 5 gates
    let mut suggestions = Vec::new();
    for span in spans {
        let duration_days = (span.end - span.start).num_days() + 1;

        // Gate 1: >= 3 days. Two-day "trips" surfaced as suggestions
        // tend to be weekend errands, not the trips users want to
        // remember.
        if duration_days < 3 {
            continue;
        }
        // Gate 2: >= 15 photos. The old floor (8) flagged any short
        // outing with a photo per attraction; users called it noisy.
        if span.photo_ids.len() < 15 {
            continue;
        }
        // Gate 3: city in < 10% of weeks (rarity)
        let place = (span.city.clone(), span.country.clone());
        let cw = city_weeks.get(&place).copied().unwrap_or(0);
        if cw as f64 / total_weeks as f64 >= 0.10 {
            continue;
        }
        // Gate 4: not the home city and >= 50 km from home
        if home_place == Some((span.city.as_str(), span.country.as_str())) {
            continue;
        }
        if let Some((hlat, hlng)) = home_coords {
            if let Some(&(clat, clng)) = city_centroids.get(&place) {
                if haversine_km(hlat, hlng, clat, clng) < 50.0 {
                    continue;
                }
            }
        }
        // Gate 5: not > 60% already in albums
        let in_album = span
            .photo_ids
            .iter()
            .filter(|id| album_photo_set.contains(id))
            .count();
        if !span.photo_ids.is_empty() && in_album as f64 / span.photo_ids.len() as f64 > 0.60 {
            continue;
        }

        // Natural-prose title: "Trip to Paris  ·  Mar 3 – 7, 2024" for
        // longer trips, "Paris weekend  ·  Mar 3 – 4, 2024" for 2-day
        // weekends. The `·` separator gives a clean visual break
        // between the human-readable lead and the date suffix.
        let date_suffix =
            if span.start.year() == span.end.year() && span.start.month() == span.end.month() {
                format!(
                    "{} – {}, {}",
                    span.start.format("%b %-d"),
                    span.end.format("%-d"),
                    span.start.format("%Y"),
                )
            } else if span.start.year() == span.end.year() {
                format!(
                    "{} – {}",
                    span.start.format("%b %-d"),
                    span.end.format("%b %-d, %Y"),
                )
            } else {
                format!(
                    "{} – {}",
                    span.start.format("%b %-d, %Y"),
                    span.end.format("%b %-d, %Y"),
                )
            };
        let lead = if duration_days <= 2 {
            format!("{} weekend", span.city)
        } else {
            format!("Trip to {}", span.city)
        };
        let title = format!("{}  ·  {}", lead, date_suffix);

        let fp = compute_fingerprint(&span.photo_ids);
        let cover = pick_cover(conn, &span.photo_ids);

        suggestions.push(DetectedSuggestion {
            kind: "trip".to_string(),
            title,
            photo_ids: span.photo_ids,
            cover_photo_id: cover,
            fingerprint: fp,
        });
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Event detection
// ---------------------------------------------------------------------------

/// Detect event suggestions via a 4-hour sliding window.
/// An event is a burst of >= 8 photos separated by <= 4 hours from their
/// neighbours, with a signal check (faces or single-location).
pub fn detect_events(conn: &Connection, trip_photo_ids: &HashSet<i64>) -> Vec<DetectedSuggestion> {
    // Query all photos with date_taken, ordered chronologically
    let rows: Vec<(i64, String, Option<String>, Option<i64>)> = {
        let mut stmt = match conn.prepare(
            r#"SELECT p.id, p.date_taken, p.location_city,
                      (SELECT f.cluster_id FROM faces f WHERE f.photo_id = p.id AND f.cluster_id IS NOT NULL LIMIT 1)
               FROM photos p
               WHERE p.date_taken IS NOT NULL AND p.is_trashed = FALSE
               ORDER BY p.date_taken"#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .and_then(|iter| iter.collect::<rusqlite::Result<Vec<_>>>())
        .unwrap_or_else(|e| {
            tracing::warn!("album event detection skipped: failed reading event rows: {e}");
            Vec::new()
        })
    };

    if rows.is_empty() {
        return Vec::new();
    }

    // Photo IDs already in user albums
    let album_photo_set: HashSet<i64> = {
        match conn.prepare("SELECT DISTINCT photo_id FROM album_photos") {
            Ok(mut stmt) => stmt
                .query_map([], |row| row.get::<_, i64>(0))
                .and_then(|iter| iter.collect::<rusqlite::Result<HashSet<_>>>())
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "album event detection continuing without album membership filter: {e}"
                    );
                    HashSet::new()
                }),
            Err(_) => HashSet::new(),
        }
    };

    // Parse timestamps
    struct EventPhoto {
        id: i64,
        ts: i64, // unix epoch seconds
        city: Option<String>,
        cluster_id: Option<i64>,
    }

    let parsed: Vec<EventPhoto> = rows
        .into_iter()
        .filter_map(|(id, dt, city, cluster)| {
            let normalized = if dt.contains('T') {
                dt.replace('T', " ")
            } else {
                dt.clone()
            };
            let ts = chrono::NaiveDateTime::parse_from_str(
                &normalized[..normalized.len().min(19)],
                "%Y-%m-%d %H:%M:%S",
            )
            .ok()
            .map(|ndt| ndt.and_utc().timestamp())
            .or_else(|| {
                parse_date_prefix(&normalized)
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            })?;
            Some(EventPhoto {
                id,
                ts,
                city,
                cluster_id: cluster,
            })
        })
        .collect();

    // Split into windows at 4-hour gaps
    let four_hours = 4 * 3600;
    let mut windows: Vec<Vec<&EventPhoto>> = Vec::new();
    let mut current_window: Vec<&EventPhoto> = Vec::new();

    for photo in &parsed {
        if let Some(last) = current_window.last() {
            if photo.ts - last.ts > four_hours && !current_window.is_empty() {
                windows.push(std::mem::take(&mut current_window));
            }
        }
        current_window.push(photo);
    }
    if !current_window.is_empty() {
        windows.push(current_window);
    }

    let mut suggestions = Vec::new();

    for window in &windows {
        // Gate 1: >= 15 photos. Same noise-reduction reasoning as the
        // trip detector — short bursts at home aren't event-worthy.
        if window.len() < 15 {
            continue;
        }

        let ids: Vec<i64> = window.iter().map(|p| p.id).collect();

        // Gate 2: not > 70% in trips
        let in_trip = ids.iter().filter(|id| trip_photo_ids.contains(id)).count();
        if !ids.is_empty() && in_trip as f64 / ids.len() as f64 > 0.70 {
            continue;
        }

        // Gate 3: signal check — 2+ face clusters OR location spans < 3 days
        let cluster_ids: HashSet<i64> = window.iter().filter_map(|p| p.cluster_id).collect();
        let distinct_days: HashSet<i64> = window.iter().map(|p| p.ts / 86400).collect();
        let location_days: HashSet<String> = window.iter().filter_map(|p| p.city.clone()).collect();

        let has_face_signal = cluster_ids.len() >= 2;
        let has_location_signal = !location_days.is_empty() && distinct_days.len() <= 3;

        if !has_face_signal && !has_location_signal {
            continue;
        }

        // Gate 4: not > 60% already in albums
        let in_album = ids.iter().filter(|id| album_photo_set.contains(id)).count();
        if !ids.is_empty() && in_album as f64 / ids.len() as f64 > 0.60 {
            continue;
        }

        // Title: derive from location or date
        let primary_city = window
            .iter()
            .filter_map(|p| p.city.as_ref())
            .fold(HashMap::new(), |mut acc, c| {
                *acc.entry(c.clone()).or_insert(0usize) += 1;
                acc
            })
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(c, _)| c);

        let first_ts = window.first().map(|p| p.ts).unwrap_or(0);
        let date = chrono::DateTime::from_timestamp(first_ts, 0)
            .map(|dt| dt.format("%b %d, %Y").to_string())
            .unwrap_or_else(|| "Event".to_string());

        // "A day in Paris  ·  Mar 12, 2024" / "A day worth remembering · Mar 12, 2024"
        let title = if let Some(city) = primary_city {
            format!("A day in {}  ·  {}", city, date)
        } else {
            format!("A day worth remembering  ·  {}", date)
        };

        let fp = compute_fingerprint(&ids);
        let cover = pick_cover(conn, &ids);

        suggestions.push(DetectedSuggestion {
            kind: "event".to_string(),
            title,
            photo_ids: ids,
            cover_photo_id: cover,
            fingerprint: fp,
        });
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Gatherings detector — face-driven, no GPS required.
// ---------------------------------------------------------------------------

/// A "gathering" is a 1–2 day window with ≥ 8 photos that contain at
/// least 2 distinct named-or-unnamed face clusters. The intent is to
/// catch family weekends, parties, and outings on libraries that
/// have no location metadata at all (old phones, scanned DSLR exports).
///
/// We exclude photos already claimed by trip / event detectors so the
/// same weekend doesn't get suggested twice from different angles.
pub fn detect_gatherings(
    conn: &Connection,
    excluded_photo_ids: &HashSet<i64>,
) -> Vec<DetectedSuggestion> {
    // Pull every photo that has at least one face assigned to a cluster,
    // ordered by date. Group key is the day bucket (UTC date).
    let mut stmt = match conn.prepare(
        r#"
        SELECT DISTINCT
            p.id,
            CAST(strftime('%s', p.date_taken) AS INTEGER) AS ts,
            f.cluster_id
        FROM photos p
        JOIN faces  f ON f.photo_id = p.id
        WHERE p.is_trashed = FALSE
          AND p.date_taken IS NOT NULL
          AND f.cluster_id IS NOT NULL
        ORDER BY p.date_taken
        "#,
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    type Row = (i64, i64, i64); // (photo_id, ts, cluster_id)
    let rows: Vec<Row> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .and_then(|iter| iter.collect::<rusqlite::Result<Vec<_>>>())
        .unwrap_or_else(|e| {
            tracing::warn!("album people-gathering detection skipped: failed reading rows: {e}");
            Vec::new()
        });

    if rows.is_empty() {
        return Vec::new();
    }

    // Cluster names lookup — used to title the resulting album.
    let cluster_names: HashMap<i64, Option<String>> = {
        let mut s = match conn.prepare("SELECT id, name FROM face_clusters") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        s.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .and_then(|iter| iter.collect::<rusqlite::Result<HashMap<_, _>>>())
        .unwrap_or_else(|e| {
            tracing::warn!(
                "album people-gathering detection continuing without cluster names: {e}"
            );
            HashMap::new()
        })
    };

    // Sliding-window grouping by day bucket. Two consecutive days (or
    // a Sat→Sun) get fused into one gathering when they share clusters.
    let secs_per_day: i64 = 86_400;
    let max_span_days: i64 = 2;

    struct Window {
        start_ts: i64,
        end_ts: i64,
        photo_ids: Vec<i64>,
        cluster_ids: HashSet<i64>,
    }

    let mut windows: Vec<Window> = Vec::new();
    let mut cur: Option<Window> = None;
    let mut last_seen_photo: HashMap<i64, i64> = HashMap::new(); // photo_id → ts (dedup ts across cluster rows)

    for (photo_id, ts, cluster_id) in &rows {
        let ts = *ts;
        let pid = *photo_id;
        let cid = *cluster_id;
        let mut new_window = false;
        if let Some(c) = &cur {
            // Extend if within span.
            if (ts - c.start_ts) <= max_span_days * secs_per_day {
                // ok, extend
            } else {
                new_window = true;
            }
        } else {
            new_window = true;
        }
        if new_window {
            if let Some(w) = cur.take() {
                if !w.photo_ids.is_empty() {
                    windows.push(w);
                }
            }
            cur = Some(Window {
                start_ts: ts,
                end_ts: ts,
                photo_ids: Vec::new(),
                cluster_ids: HashSet::new(),
            });
        }
        let Some(w) = cur.as_mut() else {
            continue;
        };
        w.cluster_ids.insert(cid);
        w.end_ts = w.end_ts.max(ts);
        // Avoid double-pushing the same photo (it appears once per
        // cluster row from the JOIN).
        let prev_ts = last_seen_photo.get(&pid).copied();
        if prev_ts.is_none() {
            w.photo_ids.push(pid);
        }
        last_seen_photo.insert(pid, ts);
    }
    if let Some(w) = cur.take() {
        if !w.photo_ids.is_empty() {
            windows.push(w);
        }
    }

    let mut out = Vec::new();
    for mut w in windows {
        // Drop photos already used by other detectors.
        w.photo_ids.retain(|p| !excluded_photo_ids.contains(p));
        if w.photo_ids.len() < 8 || w.cluster_ids.len() < 2 {
            continue;
        }

        // Pick the 1-2 most-photographed clusters in the window for
        // the title. Counting requires a second pass over rows in the
        // window range — cheap, the window is small.
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for (pid, _, cid) in &rows {
            if w.photo_ids.contains(pid) {
                *counts.entry(*cid).or_insert(0) += 1;
            }
        }
        let mut sorted: Vec<(i64, usize)> = counts.into_iter().collect();
        sorted.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
        let top_names: Vec<String> = sorted
            .iter()
            .take(2)
            .filter_map(|(cid, _)| cluster_names.get(cid).and_then(|n| n.clone()))
            .collect();

        // Build a date suffix from the window's actual span.
        let start_dt = chrono::DateTime::<Utc>::from_timestamp(w.start_ts, 0);
        let end_dt = chrono::DateTime::<Utc>::from_timestamp(w.end_ts, 0);
        let date_part = match (start_dt, end_dt) {
            (Some(s), Some(e)) if s.date_naive() == e.date_naive() => {
                s.format("%b %-d, %Y").to_string()
            }
            (Some(s), Some(e)) => format!("{} – {}", s.format("%b %-d"), e.format("%b %-d, %Y")),
            _ => String::from("—"),
        };

        let title = if top_names.is_empty() {
            format!("Gathering · {}", date_part)
        } else if top_names.len() == 1 {
            format!("Time with {} · {}", top_names[0], date_part)
        } else {
            format!("{} & {} · {}", top_names[0], top_names[1], date_part)
        };

        let cover_photo_id = pick_cover(conn, &w.photo_ids);
        let fingerprint = compute_fingerprint(&w.photo_ids);

        out.push(DetectedSuggestion {
            kind: "gathering".to_string(),
            title,
            photo_ids: w.photo_ids,
            cover_photo_id,
            fingerprint,
        });
    }

    out
}

// ---------------------------------------------------------------------------
// Top-level pipeline
// ---------------------------------------------------------------------------

/// Run the full suggestion detection pipeline: trips then events.
/// Newly detected suggestions that don't match existing fingerprints are
/// persisted to the database.
#[allow(dead_code)]
pub fn detect_suggestions(
    conn: &Connection,
    home_city_override: Option<&str>,
) -> Vec<DetectedSuggestion> {
    detect_suggestions_with_diagnostics(conn, home_city_override).0
}

pub fn detect_suggestions_with_diagnostics(
    conn: &Connection,
    home_city_override: Option<&str>,
) -> (Vec<DetectedSuggestion>, SuggestionDiagnostics) {
    detect_suggestions_with_diagnostics_cancel(conn, home_city_override, None)
}

pub fn detect_suggestions_with_diagnostics_cancel(
    conn: &Connection,
    home_city_override: Option<&str>,
    cancel: Option<&AtomicBool>,
) -> (Vec<DetectedSuggestion>, SuggestionDiagnostics) {
    let mut diag = SuggestionDiagnostics {
        total_photos_with_date: conn
            .query_row(
                "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE AND date_taken IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0),
        photos_with_city: conn
            .query_row(
                "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE AND location_city IS NOT NULL AND location_city != ''",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0),
        photos_with_gps: conn
            .query_row(
                "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE AND gps_latitude IS NOT NULL AND gps_longitude IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0),
        ..SuggestionDiagnostics::default()
    };

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return (Vec::new(), diag);
    }

    let repo = AlbumSuggestionRepo::new(conn);
    let existing_fps: HashSet<String> = repo
        .get_all_fingerprints()
        .unwrap_or_default()
        .into_iter()
        .collect();

    if diag.total_photos_with_date < 20 {
        tracing::info!(
            "suggestions: insufficient dated photos ({})",
            diag.total_photos_with_date
        );
        return (Vec::new(), diag);
    }

    let home = detect_home_city(conn, home_city_override);
    diag.home_city = home.as_ref().map(|h| h.0.clone());
    let trips = detect_trips(conn, home.as_ref());
    diag.trip_candidates_passed = trips.len();

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return (Vec::new(), diag);
    }

    // Coarse gate diagnostics for trips.
    {
        let mut stmt = conn
            .prepare(
                r#"SELECT p.id, p.location_city, COALESCE(p.location_country,''), p.date_taken
                   FROM photos p
                   WHERE p.location_city IS NOT NULL
                     AND p.date_taken IS NOT NULL
                     AND p.is_trashed = FALSE
                   ORDER BY p.location_city, COALESCE(p.location_country,''), p.date_taken"#,
            )
            .ok();
        if let Some(ref mut stmt) = stmt {
            let rows: Vec<(i64, String, String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .and_then(|iter| iter.collect::<rusqlite::Result<Vec<_>>>())
                .unwrap_or_default();
            diag.trip_rows = rows.len();

            let total_weeks: i64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT strftime('%Y-%W', date_taken)) FROM photos WHERE date_taken IS NOT NULL AND is_trashed = FALSE",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(1)
                .max(1);
            let city_weeks: HashMap<(String, String), i64> = {
                match conn.prepare(
                    r#"SELECT location_city, COALESCE(location_country,''), COUNT(DISTINCT strftime('%Y-%W', date_taken))
                           FROM photos
                           WHERE location_city IS NOT NULL AND date_taken IS NOT NULL AND is_trashed = FALSE
                           GROUP BY location_city, COALESCE(location_country,'')"#,
                ) {
                    Ok(mut stmt) => stmt
                        .query_map([], |row| {
                            Ok((
                                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                                row.get::<_, i64>(2)?,
                            ))
                        })
                        .and_then(|iter| iter.collect::<rusqlite::Result<HashMap<_, _>>>())
                        .unwrap_or_default(),
                    Err(_) => HashMap::new(),
                }
            };

            let mut spans: Vec<(String, String, NaiveDate, NaiveDate, usize)> = Vec::new();
            let mut city = String::new();
            let mut country = String::new();
            let mut dates: Vec<NaiveDate> = Vec::new();
            let mut flush = |city: &str, country: &str, dates: &mut Vec<NaiveDate>| {
                if city.is_empty() || dates.is_empty() {
                    dates.clear();
                    return;
                }
                dates.sort();
                let mut start = dates[0];
                let mut end = dates[0];
                let mut count = 1usize;
                for d in dates.iter().skip(1).copied() {
                    if (d - end).num_days() > 3 {
                        spans.push((city.to_string(), country.to_string(), start, end, count));
                        start = d;
                        count = 0;
                    }
                    end = d;
                    count += 1;
                }
                spans.push((city.to_string(), country.to_string(), start, end, count));
                dates.clear();
            };
            for (_, c, co, ds) in rows {
                let Some(d) = parse_date_prefix(&ds) else {
                    continue;
                };
                if c != city || co != country {
                    flush(&city, &country, &mut dates);
                    city = c;
                    country = co;
                }
                dates.push(d);
            }
            flush(&city, &country, &mut dates);

            for (city, country, start, end, count) in spans {
                let duration_days = (end - start).num_days() + 1;
                if duration_days < 3 {
                    diag.trip_gate_duration_rejected += 1;
                    continue;
                }
                if count < 15 {
                    diag.trip_gate_photo_count_rejected += 1;
                    continue;
                }
                let cw = city_weeks.get(&(city, country)).copied().unwrap_or(0);
                if cw as f64 / total_weeks as f64 >= 0.10 {
                    diag.trip_gate_rarity_rejected += 1;
                    continue;
                }
            }
        }
    }

    // Collect all trip photo IDs for the event filter gate
    let trip_photo_ids: HashSet<i64> = trips
        .iter()
        .flat_map(|t| t.photo_ids.iter().copied())
        .collect();

    let events = detect_events(conn, &trip_photo_ids);
    diag.event_candidates_passed = events.len();

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return (Vec::new(), diag);
    }

    // Gatherings: face-driven, no GPS needed. Catches "weekend with
    // people" scenarios that trips and events miss on libraries
    // without location metadata.
    let already_used: HashSet<i64> = trips
        .iter()
        .chain(events.iter())
        .flat_map(|s| s.photo_ids.iter().copied())
        .collect();
    let gatherings = detect_gatherings(conn, &already_used);

    let mut all: Vec<DetectedSuggestion> = Vec::new();
    all.extend(trips);
    all.extend(events);
    all.extend(gatherings);

    // Persist only new suggestions (no matching fingerprint)
    let mut persisted = Vec::new();
    for s in all {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            break;
        }
        if existing_fps.contains(&s.fingerprint) {
            diag.skipped_existing_fingerprint += 1;
            continue;
        }
        match repo.insert(
            &s.kind,
            &s.title,
            &s.photo_ids,
            s.cover_photo_id,
            &s.fingerprint,
        ) {
            Ok(_id) => {
                tracing::info!("New {} suggestion: {}", s.kind, s.title);
                persisted.push(s);
                diag.persisted_new += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to insert suggestion: {}", e);
            }
        }
    }

    // Cleanup old non-pending records (> 180 days)
    let _ = repo.cleanup_old(180);

    tracing::info!(
        "suggestions diagnostics: dated={}, with_city={}, home_city={:?}, trip_passed={}, event_passed={}, persisted={}, skipped_fp={}",
        diag.total_photos_with_date,
        diag.photos_with_city,
        diag.home_city,
        diag.trip_candidates_passed,
        diag.event_candidates_passed,
        diag.persisted_new,
        diag.skipped_existing_fingerprint
    );

    (persisted, diag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::atomic::AtomicBool;

    fn create_trip_test_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                location_city TEXT,
                location_country TEXT,
                date_taken TEXT,
                gps_latitude REAL,
                gps_longitude REAL,
                is_trashed BOOLEAN DEFAULT FALSE
            );
            "#,
        )
        .unwrap();
    }

    #[test]
    fn detect_trips_tolerates_missing_album_photos_table() {
        let conn = Connection::open_in_memory().unwrap();
        create_trip_test_schema(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO photos
                (id, location_city, location_country, date_taken, gps_latitude, gps_longitude, is_trashed)
            VALUES
                (1, 'Goa', 'India', '2024-01-01T10:00:00Z', 15.2993, 74.1240, FALSE),
                (2, 'Goa', 'India', '2024-01-02T10:00:00Z', 15.2993, 74.1240, FALSE),
                (3, 'Goa', 'India', '2024-01-03T10:00:00Z', 15.2993, 74.1240, FALSE);
            "#,
        )
        .unwrap();

        let suggestions = detect_trips(&conn, None);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn detect_trips_does_not_merge_same_city_name_across_countries() {
        let conn = Connection::open_in_memory().unwrap();
        create_trip_test_schema(&conn);
        let mut id = 1;
        for country in ["Canada", "United Kingdom"] {
            for day in 1..=4 {
                for shot in 0..2 {
                    conn.execute(
                        r#"INSERT INTO photos
                           (id, location_city, location_country, date_taken, gps_latitude, gps_longitude, is_trashed)
                           VALUES (?1, 'London', ?2, ?3, 51.5, -0.1, FALSE)"#,
                        params![
                            id,
                            country,
                            format!("2024-01-{day:02}T10:00:{shot:02}Z")
                        ],
                    )
                    .unwrap();
                    id += 1;
                }
            }
        }
        for week in 1..=20 {
            let date = NaiveDate::from_ymd_opt(2023, 1, 1)
                .unwrap()
                .checked_add_days(chrono::Days::new((week - 1) * 7))
                .unwrap();
            conn.execute(
                "INSERT INTO photos (id, date_taken, is_trashed) VALUES (?1, ?2, FALSE)",
                params![id, format!("{date}T00:00:00Z")],
            )
            .unwrap();
            id += 1;
        }

        let suggestions = detect_trips(&conn, None);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggestion_detection_skips_short_malformed_dates() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                location_city TEXT,
                location_country TEXT,
                date_taken TEXT,
                gps_latitude REAL,
                gps_longitude REAL,
                is_trashed BOOLEAN DEFAULT FALSE
            );
            CREATE TABLE album_suggestions (
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                photo_ids_json TEXT NOT NULL,
                cover_photo_id INTEGER,
                fingerprint TEXT NOT NULL,
                status TEXT NOT NULL,
                seen_count INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .unwrap();
        for id in 1..=22 {
            conn.execute(
                r#"INSERT INTO photos
                   (id, location_city, location_country, date_taken, gps_latitude, gps_longitude, is_trashed)
                   VALUES (?1, 'Goa', 'India', 'bad', 15.2993, 74.1240, FALSE)"#,
                params![id],
            )
            .unwrap();
        }

        let (suggestions, _diag) = detect_suggestions_with_diagnostics_cancel(&conn, None, None);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn haversine_uses_shortest_path_across_date_line() {
        assert!(haversine_km(0.0, 179.9, 0.0, -179.9) < 25.0);
    }

    #[test]
    fn cancelled_detection_persists_no_suggestions() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                location_city TEXT,
                location_country TEXT,
                date_taken TEXT,
                gps_latitude REAL,
                gps_longitude REAL,
                is_trashed BOOLEAN DEFAULT FALSE
            );
            CREATE TABLE album_suggestions (
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                photo_ids_json TEXT NOT NULL,
                cover_photo_id INTEGER,
                fingerprint TEXT NOT NULL,
                status TEXT NOT NULL,
                seen_count INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO photos
                (id, location_city, location_country, date_taken, gps_latitude, gps_longitude, is_trashed)
            VALUES
                (1, 'Goa', 'India', '2024-01-01T10:00:00Z', 15.2993, 74.1240, FALSE),
                (2, 'Goa', 'India', '2024-01-02T10:00:00Z', 15.2993, 74.1240, FALSE);
            "#,
        )
        .unwrap();

        let cancel = AtomicBool::new(true);
        let (suggestions, _diag) =
            detect_suggestions_with_diagnostics_cancel(&conn, None, Some(&cancel));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM album_suggestions", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert!(suggestions.is_empty());
        assert_eq!(count, 0);
    }
}
