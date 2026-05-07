//! Insights Dashboard — compute aggregate statistics from the photo library.
//!
//! `compute()` runs ~12 SQL queries against the photos/faces/albums tables
//! and returns an `InsightsData` struct with everything the view needs.

use std::collections::HashMap;

use rusqlite::{params, Connection, Result as SqliteResult};

/// A person stat row for the top-people section.
#[derive(Debug, Clone)]
pub struct PersonStat {
    pub cluster_id: i64,
    pub name: String,
    pub photo_count: i64,
    /// Representative face ID (used to locate the face crop thumbnail).
    pub face_id: Option<i64>,
    /// Resolved absolute path to the face crop image (populated post-query).
    pub face_crop_path: Option<String>,
}

/// A location stat row for the top-locations section.
#[derive(Debug, Clone)]
pub struct LocationStat {
    pub city: String,
    pub country: String,
    pub photo_count: i64,
}

/// A country stat row for the top-countries section.
#[derive(Debug, Clone)]
pub struct CountryStat {
    pub country: String,
    pub photo_count: i64,
}

/// A camera stat row for the camera-breakdown section.
#[derive(Debug, Clone)]
pub struct CameraStat {
    pub camera: String,
    pub photo_count: i64,
}

/// All data needed by the Insights dashboard view. Boxed in the message
/// because it is large.
#[derive(Debug, Clone)]
pub struct InsightsData {
    pub total_photos: i64,
    pub date_range_start: Option<String>,
    pub date_range_end: Option<String>,
    pub people_count: i64,
    pub album_count: i64,
    pub country_count: i64,
    pub city_count: i64,
    /// How many photos in scope have GPS coordinates. Lets the view tell
    /// "no GPS data yet" apart from "GPS data but no geocoder result".
    pub photos_with_gps: i64,
    pub hero_photo_id: Option<i64>,
    pub hero_thumbnail_path: Option<String>,
    /// Day-level photo counts for the heatmap: "YYYY-MM-DD" -> count.
    pub heatmap: HashMap<String, i64>,
    /// The year the heatmap covers.
    pub heatmap_year: i32,
    /// Monthly photo counts (index 0 = January, 11 = December).
    pub monthly_counts: [i64; 12],
    pub top_people: Vec<PersonStat>,
    pub top_locations: Vec<LocationStat>,
    pub top_countries: Vec<CountryStat>,
    pub top_cameras: Vec<CameraStat>,
    /// Distinct years present in the library (descending).
    pub available_years: Vec<i32>,
}

/// Build a year filter clause. When `year` is Some, returns
/// `" AND strftime('%Y', date_taken) = 'YYYY'"`. Otherwise empty.
fn year_clause(year: Option<i32>) -> String {
    match year {
        Some(y) => format!(" AND strftime('%Y', date_taken) = '{}'", y),
        None => String::new(),
    }
}

fn normalized_datetime_sql(column: &str) -> String {
    format!(
        "CASE WHEN instr({0}, 'T') > 0 THEN replace(substr({0}, 1, 19), 'T', ' ') ELSE substr({0}, 1, 19) END",
        column
    )
}

/// Compute all insights data. Designed to run inside `spawn_blocking`.
pub fn compute(conn: &Connection, year: Option<i32>) -> SqliteResult<InsightsData> {
    let _legacy = year_clause(year);
    let dt = normalized_datetime_sql("date_taken");
    let yc = match year {
        Some(y) => format!(" AND strftime('%Y', {}) = '{}'", dt, y),
        None => String::new(),
    };

    // 1+2. Total photos + min/max date — collapsed into a single
    // SELECT. Three round-trips → one. The aggregations all share
    // the same FROM / WHERE so SQLite can do them in one scan.
    let (total_photos, date_range_start, date_range_end): (i64, Option<String>, Option<String>) =
        conn.query_row(
            &format!(
                "SELECT COUNT(*),
                        MIN(CASE WHEN date_taken IS NOT NULL THEN {dt} END),
                        MAX(CASE WHEN date_taken IS NOT NULL THEN {dt} END)
                 FROM photos
                 WHERE is_trashed = FALSE{yc}",
                dt = dt,
                yc = yc
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

    // 3. People count — distinct named clusters that have photos in scope
    let people_count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT fc.id)
             FROM face_clusters fc
             JOIN faces f ON f.cluster_id = fc.id
             JOIN photos p ON p.id = f.photo_id
             WHERE fc.name IS NOT NULL
               AND p.is_trashed = FALSE{}",
            yc.replace(&dt, &normalized_datetime_sql("p.date_taken"))
        ),
        [],
        |row| row.get(0),
    )?;

    // 4. Album count — distinct albums that have photos in scope
    let album_count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT a.id)
             FROM albums a
             JOIN album_photos ap ON ap.album_id = a.id
             JOIN photos p ON p.id = ap.photo_id
             WHERE p.is_trashed = FALSE{}",
            yc.replace(&dt, &normalized_datetime_sql("p.date_taken"))
        ),
        [],
        |row| row.get(0),
    )?;

    // 5. Country + city counts + GPS-tagged count (one query, three aggs).
    let (country_count, city_count, photos_with_gps): (i64, i64, i64) = conn.query_row(
        &format!(
            "SELECT
                COUNT(DISTINCT CASE WHEN location_country IS NOT NULL AND location_country != ''
                                    THEN location_country END),
                COUNT(DISTINCT CASE WHEN location_city IS NOT NULL AND location_city != ''
                                    THEN location_city END),
                SUM(CASE WHEN gps_latitude IS NOT NULL AND gps_longitude IS NOT NULL THEN 1 ELSE 0 END)
             FROM photos
             WHERE is_trashed = FALSE{}",
            yc
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<i64>>(2)?.unwrap_or(0))),
    )?;

    // 6. Hero photo — prefer photos with faces, landscape orientation, newest
    let hero_photo_id: Option<i64> = conn
        .query_row(
            &format!(
                "SELECT p.id
                 FROM photos p
                 LEFT JOIN faces f ON f.photo_id = p.id
                 WHERE p.is_trashed = FALSE
                   AND p.thumbnail_path IS NOT NULL{}
                 GROUP BY p.id
                 ORDER BY (COUNT(f.id) > 0) DESC,
                          (COALESCE(p.width, 0) > COALESCE(p.height, 0)) DESC,
                           CASE WHEN instr(p.date_taken, 'T') > 0 THEN replace(substr(p.date_taken, 1, 19), 'T', ' ') ELSE substr(p.date_taken, 1, 19) END DESC
                  LIMIT 1",
                yc
            ),
            [],
            |row| row.get(0),
        )
        .ok();

    // Determine heatmap year: if a specific year is selected, use it;
    // otherwise use the most recent year with photos.
    let heatmap_year: i32 = if let Some(y) = year {
        y
    } else {
        conn.query_row(
            "SELECT CAST(strftime('%Y', MAX(CASE WHEN instr(date_taken, 'T') > 0 THEN replace(substr(date_taken, 1, 19), 'T', ' ') ELSE substr(date_taken, 1, 19) END)) AS INTEGER)
             FROM photos
             WHERE is_trashed = FALSE AND date_taken IS NOT NULL",
            [],
            |row| row.get::<_, Option<i32>>(0),
        )
        .ok()
        .flatten()
        .unwrap_or(chrono::Local::now().date_naive().year())
    };

    // 7. Heatmap day counts for the selected year
    let mut heatmap = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT DATE(CASE WHEN instr(date_taken, 'T') > 0 THEN replace(substr(date_taken, 1, 19), 'T', ' ') ELSE substr(date_taken, 1, 19) END) AS d, COUNT(*) AS c
             FROM photos
             WHERE is_trashed = FALSE
               AND date_taken IS NOT NULL
                AND strftime('%Y', CASE WHEN instr(date_taken, 'T') > 0 THEN replace(substr(date_taken, 1, 19), 'T', ' ') ELSE substr(date_taken, 1, 19) END) = ?1
              GROUP BY d",
        )?;
        let rows = stmt.query_map(params![heatmap_year.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for (day, count) in rows.flatten() {
            heatmap.insert(day, count);
        }
    }

    // 8. Monthly counts (for the selected year if any, otherwise all time)
    let mut monthly_counts = [0i64; 12];
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT CAST(strftime('%m', CASE WHEN instr(date_taken, 'T') > 0 THEN replace(substr(date_taken, 1, 19), 'T', ' ') ELSE substr(date_taken, 1, 19) END) AS INTEGER) AS m, COUNT(*) AS c
             FROM photos
             WHERE is_trashed = FALSE
               AND date_taken IS NOT NULL{}
             GROUP BY m",
            yc
        ))?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i64>(1)?)))?;
        for (month, count) in rows.flatten() {
            if (1..=12).contains(&month) {
                monthly_counts[(month - 1) as usize] = count;
            }
        }
    }

    // 9. Top 5 people (named clusters only, by photo count)
    let mut top_people = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT fc.id, fc.name, COUNT(DISTINCT f.photo_id) AS cnt, fc.representative_face_id
             FROM face_clusters fc
             JOIN faces f ON f.cluster_id = fc.id
             JOIN photos p ON p.id = f.photo_id
             WHERE fc.name IS NOT NULL
               AND p.is_trashed = FALSE{}
             GROUP BY fc.id
             ORDER BY cnt DESC
             LIMIT 10",
            yc.replace(&dt, &normalized_datetime_sql("p.date_taken"))
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(PersonStat {
                cluster_id: row.get(0)?,
                name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                photo_count: row.get(2)?,
                face_id: row.get(3)?,
                face_crop_path: None, // resolved later
            })
        })?;
        for stat in rows.flatten() {
            top_people.push(stat);
        }
    }

    // 10. Top cities (by photo count)
    let mut top_locations = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT location_city, location_country, COUNT(*) AS cnt
             FROM photos
             WHERE is_trashed = FALSE
               AND location_city IS NOT NULL
               AND location_city != ''{}
             GROUP BY location_city
             ORDER BY cnt DESC
             LIMIT 30",
            yc
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(LocationStat {
                city: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                country: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                photo_count: row.get(2)?,
            })
        })?;
        for stat in rows.flatten() {
            top_locations.push(stat);
        }
    }

    // 10b. Top countries (by photo count). Separate aggregation so a
    // country with photos spread across many cities still surfaces.
    let mut top_countries = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT location_country, COUNT(*) AS cnt
             FROM photos
             WHERE is_trashed = FALSE
               AND location_country IS NOT NULL
               AND location_country != ''{}
             GROUP BY location_country
             ORDER BY cnt DESC
             LIMIT 20",
            yc
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(CountryStat {
                country: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                photo_count: row.get(1)?,
            })
        })?;
        for stat in rows.flatten() {
            top_countries.push(stat);
        }
    }

    // 11. Top 3 cameras (by make||model, hidden if only 1 camera)
    let mut top_cameras = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT COALESCE(camera_make, '') || ' ' || COALESCE(camera_model, '') AS cam,
                    COUNT(*) AS cnt
             FROM photos
             WHERE is_trashed = FALSE
               AND (camera_make IS NOT NULL OR camera_model IS NOT NULL){}
             GROUP BY cam
             HAVING TRIM(cam) != ''
             ORDER BY cnt DESC
             LIMIT 3",
            yc
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(CameraStat {
                camera: row.get::<_, String>(0)?.trim().to_string(),
                photo_count: row.get(1)?,
            })
        })?;
        for stat in rows.flatten() {
            top_cameras.push(stat);
        }
    }

    // 12. Available years (descending)
    let mut available_years = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT CAST(strftime('%Y', CASE WHEN instr(date_taken, 'T') > 0 THEN replace(substr(date_taken, 1, 19), 'T', ' ') ELSE substr(date_taken, 1, 19) END) AS INTEGER) AS y
             FROM photos
             WHERE is_trashed = FALSE AND date_taken IS NOT NULL
             ORDER BY y DESC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, i32>(0))?;
        for y in rows.flatten() {
            available_years.push(y);
        }
    }

    Ok(InsightsData {
        total_photos,
        date_range_start,
        date_range_end,
        people_count,
        album_count,
        country_count,
        city_count,
        photos_with_gps,
        hero_photo_id,
        hero_thumbnail_path: None, // resolved by the loader
        heatmap,
        heatmap_year,
        monthly_counts,
        top_people,
        top_locations,
        top_countries,
        top_cameras,
        available_years,
    })
}

use chrono::Datelike;
