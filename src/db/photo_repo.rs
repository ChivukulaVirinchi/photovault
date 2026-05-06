//! Photo database repository
//!
//! Handles all database operations for photos.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};

use crate::models::{ContentCategory, Photo};

/// A discovered file ready for database insertion
#[derive(Debug, Clone)]
pub struct PhotoInsert {
    pub relative_path: String,
    pub file_name: String,
    pub file_hash: String,
    pub file_size: i64,
    pub file_mtime: Option<i64>,
    pub date_taken: Option<String>,
    pub date_taken_source: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub iso: Option<i32>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,
    pub lens_model: Option<String>,
    pub flash: Option<String>,
    pub gps_altitude: Option<f64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
}

/// Photo repository for database operations
pub struct PhotoRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PhotoRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Batch insert photos within a transaction
    pub fn insert_batch(&self, photos: &[PhotoInsert]) -> SqliteResult<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0;

        for photo in photos {
            tx.execute(
                r#"
                INSERT INTO photos (
                    file_path, file_name, file_hash, file_size, file_mtime,
                    date_taken, date_taken_source,
                    gps_latitude, gps_longitude,
                    location_city, location_country,
                    camera_make, camera_model,
                    iso, aperture, shutter_speed, focal_length,
                    lens_model, flash, gps_altitude,
                    width, height, orientation
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7,
                    ?8, ?9,
                    ?10, ?11,
                    ?12, ?13,
                    ?14, ?15, ?16, ?17,
                    ?18, ?19, ?20,
                    ?21, ?22, ?23
                )
                ON CONFLICT(file_path) DO UPDATE SET
                    file_hash = excluded.file_hash,
                    file_size = excluded.file_size,
                    file_mtime = excluded.file_mtime,
                    date_taken = excluded.date_taken,
                    date_taken_source = excluded.date_taken_source,
                    gps_latitude = excluded.gps_latitude,
                    gps_longitude = excluded.gps_longitude,
                    location_city = COALESCE(excluded.location_city, photos.location_city),
                    location_country = COALESCE(excluded.location_country, photos.location_country),
                    camera_make = excluded.camera_make,
                    camera_model = excluded.camera_model,
                    iso = excluded.iso,
                    aperture = excluded.aperture,
                    shutter_speed = excluded.shutter_speed,
                    focal_length = excluded.focal_length,
                    lens_model = excluded.lens_model,
                    flash = excluded.flash,
                    gps_altitude = excluded.gps_altitude,
                    width = excluded.width,
                    height = excluded.height,
                    orientation = excluded.orientation,
                    updated_at = CURRENT_TIMESTAMP
                "#,
                params![
                    photo.relative_path,
                    photo.file_name,
                    photo.file_hash,
                    photo.file_size,
                    photo.file_mtime,
                    photo.date_taken,
                    photo.date_taken_source,
                    photo.gps_latitude,
                    photo.gps_longitude,
                    photo.location_city,
                    photo.location_country,
                    photo.camera_make,
                    photo.camera_model,
                    photo.iso,
                    photo.aperture,
                    photo.shutter_speed,
                    photo.focal_length,
                    photo.lens_model,
                    photo.flash,
                    photo.gps_altitude,
                    photo.width,
                    photo.height,
                    photo.orientation,
                ],
            )?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    /// Get total photo count
    pub fn count(&self) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
            [],
            |row| row.get(0),
        )
    }

    /// Get all photos ordered by date
    pub fn get_all_by_date(&self, limit: i64, offset: i64) -> SqliteResult<Vec<Photo>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                id, file_path, file_name, file_hash, file_size,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                location_city, location_country,
                camera_make, camera_model,
                iso, aperture, shutter_speed, focal_length,
                lens_model, flash, gps_altitude,
                width, height, orientation,
                thumbnail_path, faces_processed,
                content_category, ocr_text, ocr_processed, ocr_confidence,
                is_trashed, trashed_at,
                indexed_at, updated_at
            FROM photos
            WHERE is_trashed = FALSE
            ORDER BY date_taken DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit, offset], row_to_photo)?;

        let mut photos = Vec::new();
        for row in rows {
            photos.push(row?);
        }

        Ok(photos)
    }

    /// Get photos by IDs ordered by date (descending).
    pub fn get_by_ids(&self, photo_ids: &[i64]) -> SqliteResult<Vec<Photo>> {
        if photo_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all = Vec::new();

        // Keep IN clause under SQLite variable limits.
        for chunk in photo_ids.chunks(900) {
            let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");

            let sql = format!(
                r#"
                SELECT
                    id, file_path, file_name, file_hash, file_size,
                    date_taken, date_taken_source,
                    gps_latitude, gps_longitude,
                    location_city, location_country,
                    camera_make, camera_model,
                    iso, aperture, shutter_speed, focal_length,
                    lens_model, flash, gps_altitude,
                    width, height, orientation,
                    thumbnail_path, faces_processed,
                    content_category, ocr_text, ocr_processed, ocr_confidence,
                    is_trashed, trashed_at,
                    indexed_at, updated_at
                FROM photos
                WHERE is_trashed = FALSE AND id IN ({})
                "#,
                placeholders
            );

            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(chunk.iter().copied()),
                row_to_photo,
            )?;
            for row in rows {
                all.push(row?);
            }
        }

        all.sort_by_key(|p| std::cmp::Reverse(p.date_taken));
        Ok(all)
    }

    /// Get a single photo by ID.
    pub fn get_by_id(&self, id: i64) -> SqliteResult<Option<Photo>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                id, file_path, file_name, file_hash, file_size,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                location_city, location_country,
                camera_make, camera_model,
                iso, aperture, shutter_speed, focal_length,
                lens_model, flash, gps_altitude,
                width, height, orientation,
                thumbnail_path, faces_processed,
                content_category, ocr_text, ocr_processed, ocr_confidence,
                is_trashed, trashed_at,
                indexed_at, updated_at
            FROM photos
            WHERE id = ?1
            "#,
        )?;

        match stmt.query_row(params![id], row_to_photo) {
            Ok(photo) => Ok(Some(photo)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Convert a database row to a Photo struct.
///
/// The selected columns must match the ordering used by `PhotoRepo` and document queries.
pub(crate) fn row_to_photo(row: &rusqlite::Row) -> SqliteResult<Photo> {
    Ok(Photo {
        id: row.get(0)?,
        file_path: row.get(1)?,
        file_name: row.get(2)?,
        file_hash: row.get(3)?,
        file_size: row.get(4)?,
        date_taken: row
            .get::<_, Option<String>>(5)?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        date_taken_source: row.get(6)?,
        gps_latitude: row.get(7)?,
        gps_longitude: row.get(8)?,
        location_city: row.get(9)?,
        location_country: row.get(10)?,
        camera_make: row.get(11)?,
        camera_model: row.get(12)?,
        iso: row.get(13)?,
        aperture: row.get(14)?,
        shutter_speed: row.get(15)?,
        focal_length: row.get(16)?,
        lens_model: row.get(17)?,
        flash: row.get(18)?,
        gps_altitude: row.get(19)?,
        width: row.get(20)?,
        height: row.get(21)?,
        orientation: row.get::<_, Option<i32>>(22)?.unwrap_or(1),
        thumbnail_path: row.get(23)?,
        faces_processed: row.get(24)?,
        content_category: row
            .get::<_, Option<String>>(25)?
            .map(|s| ContentCategory::from_db(&s))
            .unwrap_or(ContentCategory::Photo),
        ocr_text: row.get(26)?,
        ocr_processed: row.get::<_, Option<bool>>(27)?.unwrap_or(false),
        ocr_confidence: row.get(28)?,
        is_trashed: row.get(29)?,
        trashed_at: row
            .get::<_, Option<String>>(30)?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        indexed_at: row
            .get::<_, String>(31)?
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now()),
        updated_at: row
            .get::<_, String>(32)?
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Cursor-based pagination methods used by the Tauri command surface.
///
/// Older offset/limit methods (`get_all_by_date`, `get_by_ids`, ...) stay
/// in place for the iced UI; these additive variants tiebreak `(date_taken
/// DESC, id DESC)` with explicit `IS NULL` ordering so a stable cursor
/// can be carried across page boundaries.
///
/// `#[allow(dead_code)]` is needed during the iced↔Tauri coexistence:
/// the iced binary doesn't reach these, only `src-tauri/` does, and
/// clippy lints "unused" when checking the iced binary in isolation.
/// Drops naturally when iced is removed in M3.
///
/// A short descriptor for a photo that's enough to render a grid cell or a
/// map pin, without paying the cost of a full Photo materialisation.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PhotoLite {
    pub id: i64,
    pub date_taken: Option<DateTime<Utc>>,
    pub thumbnail_path: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
    pub is_trashed: bool,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
}

fn row_to_photo_lite(row: &rusqlite::Row) -> SqliteResult<PhotoLite> {
    Ok(PhotoLite {
        id: row.get(0)?,
        date_taken: row
            .get::<_, Option<String>>(1)?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        thumbnail_path: row.get(2)?,
        width: row.get(3)?,
        height: row.get(4)?,
        orientation: row.get::<_, Option<i32>>(5)?.unwrap_or(1),
        is_trashed: row.get(6)?,
        gps_latitude: row.get(7)?,
        gps_longitude: row.get(8)?,
    })
}

#[allow(dead_code)]
const PHOTO_LITE_COLUMNS: &str = r#"
    id, date_taken, thumbnail_path,
    width, height, orientation, is_trashed,
    gps_latitude, gps_longitude
"#;

#[allow(dead_code)]
impl<'a> PhotoRepo<'a> {
    /// Cursor-paginated timeline list. Cursor key is `(date_taken, id)`
    /// descending. NULL date_taken sorts last via SQLite's `IS NULL`.
    /// `limit` is clamped by the caller; this just trusts it.
    pub fn list_after(
        &self,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
        include_trashed: bool,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let trash_clause = if include_trashed {
            "1=1"
        } else {
            "is_trashed = 0"
        };

        let (sql, rows): (String, Vec<PhotoLite>) = match cursor {
            Some((Some(d), id)) => {
                let s = format!(
                    "SELECT {cols} FROM photos
                     WHERE {trash}
                       AND ((date_taken IS NOT NULL AND
                             (date_taken < ?1 OR (date_taken = ?1 AND id < ?2)))
                            OR (date_taken IS NULL AND ?1 IS NULL AND id < ?2))
                     ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                     LIMIT ?3",
                    cols = PHOTO_LITE_COLUMNS,
                    trash = trash_clause
                );
                let date_str = d.to_rfc3339();
                let mut stmt = self.conn.prepare(&s)?;
                let mapped = stmt.query_map(params![date_str, id, limit], row_to_photo_lite)?;
                let mut v = Vec::new();
                for r in mapped {
                    v.push(r?);
                }
                (s, v)
            }
            Some((None, id)) => {
                let s = format!(
                    "SELECT {cols} FROM photos
                     WHERE {trash} AND date_taken IS NULL AND id < ?1
                     ORDER BY id DESC LIMIT ?2",
                    cols = PHOTO_LITE_COLUMNS,
                    trash = trash_clause
                );
                let mut stmt = self.conn.prepare(&s)?;
                let mapped = stmt.query_map(params![id, limit], row_to_photo_lite)?;
                let mut v = Vec::new();
                for r in mapped {
                    v.push(r?);
                }
                (s, v)
            }
            None => {
                let s = format!(
                    "SELECT {cols} FROM photos
                     WHERE {trash}
                     ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                     LIMIT ?1",
                    cols = PHOTO_LITE_COLUMNS,
                    trash = trash_clause
                );
                let mut stmt = self.conn.prepare(&s)?;
                let mapped = stmt.query_map(params![limit], row_to_photo_lite)?;
                let mut v = Vec::new();
                for r in mapped {
                    v.push(r?);
                }
                (s, v)
            }
        };
        let _ = sql; // suppress unused if logging removed
        Ok(rows)
    }

    /// Cursor-paginated photos in a specific album, ordered by date.
    pub fn list_after_by_album(
        &self,
        album_id: i64,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let base = format!(
            "SELECT {cols} FROM photos p
             JOIN album_photos ap ON ap.photo_id = p.id
             WHERE ap.album_id = ?1 AND p.is_trashed = 0",
            cols = PHOTO_LITE_COLUMNS.replace("id,", "p.id,")
        );

        match cursor {
            Some((Some(d), id)) => {
                let sql = format!(
                    "{base}
                     AND ((p.date_taken IS NOT NULL AND
                           (p.date_taken < ?2 OR (p.date_taken = ?2 AND p.id < ?3)))
                          OR (p.date_taken IS NULL AND ?2 IS NULL AND p.id < ?3))
                     ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
                     LIMIT ?4"
                );
                let date_str = d.to_rfc3339();
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![album_id, date_str, id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            Some((None, id)) => {
                let sql = format!(
                    "{base} AND p.date_taken IS NULL AND p.id < ?2
                     ORDER BY p.id DESC LIMIT ?3"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![album_id, id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            None => {
                let sql = format!(
                    "{base}
                     ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
                     LIMIT ?2"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![album_id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
        }
    }

    /// Cursor-paginated photos featuring a person (face cluster).
    pub fn list_after_by_person(
        &self,
        cluster_id: i64,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let base = format!(
            "SELECT DISTINCT {cols} FROM photos p
             JOIN faces f ON f.photo_id = p.id
             WHERE f.cluster_id = ?1 AND p.is_trashed = 0",
            cols = PHOTO_LITE_COLUMNS.replace("id,", "p.id,")
        );

        match cursor {
            Some((Some(d), id)) => {
                let sql = format!(
                    "{base}
                     AND ((p.date_taken IS NOT NULL AND
                           (p.date_taken < ?2 OR (p.date_taken = ?2 AND p.id < ?3)))
                          OR (p.date_taken IS NULL AND ?2 IS NULL AND p.id < ?3))
                     ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
                     LIMIT ?4"
                );
                let date_str = d.to_rfc3339();
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![cluster_id, date_str, id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            Some((None, id)) => {
                let sql = format!(
                    "{base} AND p.date_taken IS NULL AND p.id < ?2
                     ORDER BY p.id DESC LIMIT ?3"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![cluster_id, id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            None => {
                let sql = format!(
                    "{base}
                     ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
                     LIMIT ?2"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![cluster_id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
        }
    }

    /// Cursor-paginated photos in a date range (inclusive).
    pub fn list_after_by_date(
        &self,
        start_iso: &str,
        end_iso: &str,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let base = format!(
            "SELECT {cols} FROM photos
             WHERE is_trashed = 0
               AND date_taken IS NOT NULL
               AND date_taken >= ?1 AND date_taken <= ?2",
            cols = PHOTO_LITE_COLUMNS
        );
        match cursor {
            Some((Some(d), id)) => {
                let sql = format!(
                    "{base}
                     AND (date_taken < ?3 OR (date_taken = ?3 AND id < ?4))
                     ORDER BY date_taken DESC, id DESC LIMIT ?5"
                );
                let date_str = d.to_rfc3339();
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(
                        params![start_iso, end_iso, date_str, id, limit],
                        row_to_photo_lite,
                    )?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            Some((None, _)) | None => {
                let sql = format!(
                    "{base}
                     ORDER BY date_taken DESC, id DESC LIMIT ?3"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![start_iso, end_iso, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
        }
    }

    /// Cursor-paginated photos in a place (city and/or country, both optional).
    pub fn list_after_by_place(
        &self,
        city: Option<&str>,
        country: Option<&str>,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let mut wheres = vec!["is_trashed = 0".to_string()];
        if city.is_some() {
            wheres.push("location_city = ?CITY".to_string());
        }
        if country.is_some() {
            wheres.push("location_country = ?COUNTRY".to_string());
        }
        let base_where = wheres.join(" AND ");
        let base = format!(
            "SELECT {cols} FROM photos WHERE {w}",
            cols = PHOTO_LITE_COLUMNS,
            w = base_where
        );

        // Build SQL by replacing named tokens with positional placeholders.
        let mut next = 1usize;
        let mut bind: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut sql = base;
        if let Some(c) = city {
            sql = sql.replacen("?CITY", &format!("?{}", next), 1);
            next += 1;
            bind.push(Box::new(c.to_string()));
        }
        if let Some(c) = country {
            sql = sql.replacen("?COUNTRY", &format!("?{}", next), 1);
            next += 1;
            bind.push(Box::new(c.to_string()));
        }

        match cursor {
            Some((Some(d), id)) => {
                sql.push_str(&format!(
                    " AND ((date_taken IS NOT NULL AND
                            (date_taken < ?{a} OR (date_taken = ?{a} AND id < ?{b})))
                           OR (date_taken IS NULL AND ?{a} IS NULL AND id < ?{b}))
                       ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                       LIMIT ?{c}",
                    a = next,
                    b = next + 1,
                    c = next + 2
                ));
                bind.push(Box::new(d.to_rfc3339()));
                bind.push(Box::new(id));
                bind.push(Box::new(limit));
            }
            Some((None, id)) => {
                sql.push_str(&format!(
                    " AND date_taken IS NULL AND id < ?{a}
                       ORDER BY id DESC LIMIT ?{b}",
                    a = next,
                    b = next + 1
                ));
                bind.push(Box::new(id));
                bind.push(Box::new(limit));
            }
            None => {
                sql.push_str(&format!(
                    " ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                       LIMIT ?{}",
                    next
                ));
                bind.push(Box::new(limit));
            }
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let bind_refs: Vec<&dyn rusqlite::ToSql> = bind.iter().map(|b| &**b).collect();
        let rows = stmt
            .query_map(rusqlite::params_from_iter(bind_refs), row_to_photo_lite)?
            .collect::<SqliteResult<Vec<_>>>()?;
        Ok(rows)
    }

    /// Photos within a lat/lng bounding box that have GPS coordinates.
    /// Used by `map.pins` for server-side aggregation.
    ///
    /// Bounds are inclusive on all sides. Returns a hard cap of `cap` rows.
    pub fn list_in_bounds(
        &self,
        north: f64,
        south: f64,
        east: f64,
        west: f64,
        cap: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        // Handle the antimeridian-crossing case (west > east) by splitting.
        let lng_clause = if west <= east {
            "gps_longitude >= ?3 AND gps_longitude <= ?4"
        } else {
            "(gps_longitude >= ?3 OR gps_longitude <= ?4)"
        };
        let sql = format!(
            "SELECT {cols} FROM photos
             WHERE is_trashed = 0
               AND gps_latitude IS NOT NULL
               AND gps_longitude IS NOT NULL
               AND gps_latitude >= ?2 AND gps_latitude <= ?1
               AND {lng}
             ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
             LIMIT ?5",
            cols = PHOTO_LITE_COLUMNS,
            lng = lng_clause,
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![north, south, west, east, cap], row_to_photo_lite)?
            .collect::<SqliteResult<Vec<_>>>()?;
        Ok(rows)
    }
}
