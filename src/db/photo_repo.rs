//! Photo database repository
//!
//! Handles all database operations for photos.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};

use crate::models::Photo;

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
                    camera_make, camera_model,
                    iso, aperture, shutter_speed, focal_length,
                    lens_model, flash, gps_altitude,
                    width, height, orientation
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7,
                    ?8, ?9,
                    ?10, ?11,
                    ?12, ?13, ?14, ?15,
                    ?16, ?17, ?18,
                    ?19, ?20, ?21
                )
                ON CONFLICT(file_path) DO UPDATE SET
                    file_hash = excluded.file_hash,
                    file_size = excluded.file_size,
                    file_mtime = excluded.file_mtime,
                    date_taken = excluded.date_taken,
                    date_taken_source = excluded.date_taken_source,
                    gps_latitude = excluded.gps_latitude,
                    gps_longitude = excluded.gps_longitude,
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
                is_trashed, trashed_at,
                indexed_at, updated_at
            FROM photos
            WHERE is_trashed = FALSE
            ORDER BY date_taken DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit, offset], Self::row_to_photo)?;

        let mut photos = Vec::new();
        for row in rows {
            photos.push(row?);
        }

        Ok(photos)
    }

    /// Convert a database row to a Photo struct
    fn row_to_photo(row: &rusqlite::Row) -> SqliteResult<Photo> {
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
            is_trashed: row.get(25)?,
            trashed_at: row
                .get::<_, Option<String>>(26)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            indexed_at: row
                .get::<_, String>(27)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            updated_at: row
                .get::<_, String>(28)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}
