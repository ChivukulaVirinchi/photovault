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

/// Compute all insights data. Designed to run inside `spawn_blocking`.
pub fn compute(conn: &Connection, year: Option<i32>) -> SqliteResult<InsightsData> {
    let yc = year_clause(year);

    // 1. Total photos
    let total_photos: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE{}",
            yc
        ),
        [],
        |row| row.get(0),
    )?;

    // 2. Date range (MIN / MAX date_taken)
    let date_range_start: Option<String> = conn
        .query_row(
            &format!(
                "SELECT MIN(date_taken) FROM photos WHERE is_trashed = FALSE AND date_taken IS NOT NULL{}",
                yc
            ),
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let date_range_end: Option<String> = conn
        .query_row(
            &format!(
                "SELECT MAX(date_taken) FROM photos WHERE is_trashed = FALSE AND date_taken IS NOT NULL{}",
                yc
            ),
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    // 3. People count — distinct named clusters that have photos in scope
    let people_count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT fc.id)
             FROM face_clusters fc
             JOIN faces f ON f.cluster_id = fc.id
             JOIN photos p ON p.id = f.photo_id
             WHERE fc.name IS NOT NULL
               AND p.is_trashed = FALSE{}",
            yc.replace("date_taken", "p.date_taken")
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
            yc.replace("date_taken", "p.date_taken")
        ),
        [],
        |row| row.get(0),
    )?;

    // 5. Country + city counts
    let country_count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT location_country)
             FROM photos
             WHERE is_trashed = FALSE
               AND location_country IS NOT NULL
               AND location_country != ''{}",
            yc
        ),
        [],
        |row| row.get(0),
    )?;
    let city_count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT location_city)
             FROM photos
             WHERE is_trashed = FALSE
               AND location_city IS NOT NULL
               AND location_city != ''{}",
            yc
        ),
        [],
        |row| row.get(0),
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
                          p.date_taken DESC
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
            "SELECT CAST(strftime('%Y', MAX(date_taken)) AS INTEGER)
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
            "SELECT DATE(date_taken) AS d, COUNT(*) AS c
             FROM photos
             WHERE is_trashed = FALSE
               AND date_taken IS NOT NULL
               AND strftime('%Y', date_taken) = ?1
             GROUP BY d",
        )?;
        let rows = stmt.query_map(params![heatmap_year.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for r in rows {
            if let Ok((day, count)) = r {
                heatmap.insert(day, count);
            }
        }
    }

    // 8. Monthly counts (for the selected year if any, otherwise all time)
    let mut monthly_counts = [0i64; 12];
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT CAST(strftime('%m', date_taken) AS INTEGER) AS m, COUNT(*) AS c
             FROM photos
             WHERE is_trashed = FALSE
               AND date_taken IS NOT NULL{}
             GROUP BY m",
            yc
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i32>(0)?, row.get::<_, i64>(1)?))
        })?;
        for r in rows {
            if let Ok((month, count)) = r {
                if (1..=12).contains(&month) {
                    monthly_counts[(month - 1) as usize] = count;
                }
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
             LIMIT 5",
            yc.replace("date_taken", "p.date_taken")
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
        for r in rows {
            if let Ok(stat) = r {
                top_people.push(stat);
            }
        }
    }

    // 10. Top 5 locations (by city)
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
             LIMIT 5",
            yc
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(LocationStat {
                city: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                country: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                photo_count: row.get(2)?,
            })
        })?;
        for r in rows {
            if let Ok(stat) = r {
                top_locations.push(stat);
            }
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
        for r in rows {
            if let Ok(stat) = r {
                top_cameras.push(stat);
            }
        }
    }

    // 12. Available years (descending)
    let mut available_years = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT CAST(strftime('%Y', date_taken) AS INTEGER) AS y
             FROM photos
             WHERE is_trashed = FALSE AND date_taken IS NOT NULL
             ORDER BY y DESC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, i32>(0))?;
        for r in rows {
            if let Ok(y) = r {
                available_years.push(y);
            }
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
        hero_photo_id,
        hero_thumbnail_path: None, // resolved by the loader
        heatmap,
        heatmap_year,
        monthly_counts,
        top_people,
        top_locations,
        top_cameras,
        available_years,
    })
}

use chrono::Datelike;
