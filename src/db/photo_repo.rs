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

    /// Insert a new photo with its metadata (upsert on conflict)
    pub fn insert(&self, photo: &PhotoInsert) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO photos (
                file_path, file_name, file_hash, file_size, file_mtime,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                camera_make, camera_model,
                width, height, orientation
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7,
                ?8, ?9,
                ?10, ?11,
                ?12, ?13, ?14
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
                photo.width,
                photo.height,
                photo.orientation,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
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
                    width, height, orientation
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7,
                    ?8, ?9,
                    ?10, ?11,
                    ?12, ?13, ?14
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

    /// Get photos for a specific date range
    pub fn get_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> SqliteResult<Vec<Photo>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                id, file_path, file_name, file_hash, file_size,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                location_city, location_country,
                camera_make, camera_model,
                width, height, orientation,
                thumbnail_path, faces_processed,
                is_trashed, trashed_at,
                indexed_at, updated_at
            FROM photos
            WHERE date_taken BETWEEN ?1 AND ?2
              AND is_trashed = FALSE
            ORDER BY date_taken DESC
            "#,
        )?;

        let rows = stmt.query_map(
            params![start.to_rfc3339(), end.to_rfc3339()],
            Self::row_to_photo,
        )?;

        let mut photos = Vec::new();
        for row in rows {
            photos.push(row?);
        }

        Ok(photos)
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

    /// Get photos grouped by date (for timeline)
    pub fn get_dates_with_counts(&self) -> SqliteResult<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                DATE(date_taken) as photo_date,
                COUNT(*) as photo_count
            FROM photos
            WHERE date_taken IS NOT NULL
              AND is_trashed = FALSE
            GROUP BY DATE(date_taken)
            ORDER BY photo_date DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut dates = Vec::new();
        for row in rows {
            dates.push(row?);
        }

        Ok(dates)
    }

    /// Check if a file hash already exists (for duplicate detection)
    pub fn hash_exists(&self, hash: &str) -> SqliteResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE file_hash = ?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
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
            width: row.get(13)?,
            height: row.get(14)?,
            orientation: row.get::<_, Option<i32>>(15)?.unwrap_or(1),
            thumbnail_path: row.get(16)?,
            faces_processed: row.get(17)?,
            is_trashed: row.get(18)?,
            trashed_at: row
                .get::<_, Option<String>>(19)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            indexed_at: row
                .get::<_, String>(20)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            updated_at: row
                .get::<_, String>(21)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_schema;

    #[test]
    fn test_insert_and_count() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let repo = PhotoRepo::new(&conn);

        let photo = PhotoInsert {
            relative_path: "test/photo.jpg".to_string(),
            file_name: "photo.jpg".to_string(),
            file_hash: "abc123".to_string(),
            file_size: 15000,
            file_mtime: Some(1234567890),
            date_taken: Some("2019-03-15T14:30:22+00:00".to_string()),
            date_taken_source: Some("exif".to_string()),
            gps_latitude: Some(40.7128),
            gps_longitude: Some(-74.0060),
            camera_make: Some("Canon".to_string()),
            camera_model: Some("EOS R5".to_string()),
            width: Some(6000),
            height: Some(4000),
            orientation: 1,
        };

        let id = repo.insert(&photo).unwrap();
        assert!(id > 0);

        let count = repo.count().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_upsert_on_conflict() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let repo = PhotoRepo::new(&conn);

        let photo = PhotoInsert {
            relative_path: "test/photo.jpg".to_string(),
            file_name: "photo.jpg".to_string(),
            file_hash: "abc123".to_string(),
            file_size: 15000,
            file_mtime: Some(1234567890),
            date_taken: None,
            date_taken_source: None,
            gps_latitude: None,
            gps_longitude: None,
            camera_make: None,
            camera_model: None,
            width: None,
            height: None,
            orientation: 1,
        };

        repo.insert(&photo).unwrap();
        repo.insert(&photo).unwrap(); // Should not fail

        let count = repo.count().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_hash_exists() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let repo = PhotoRepo::new(&conn);

        assert!(!repo.hash_exists("abc123").unwrap());

        let photo = PhotoInsert {
            relative_path: "test/photo.jpg".to_string(),
            file_name: "photo.jpg".to_string(),
            file_hash: "abc123".to_string(),
            file_size: 15000,
            file_mtime: None,
            date_taken: None,
            date_taken_source: None,
            gps_latitude: None,
            gps_longitude: None,
            camera_make: None,
            camera_model: None,
            width: None,
            height: None,
            orientation: 1,
        };

        repo.insert(&photo).unwrap();
        assert!(repo.hash_exists("abc123").unwrap());
    }
}
