//! Photo database repository
//!
//! Handles all database operations for photos.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};

use crate::models::{ContentCategory, MediaType, Photo};

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
    pub media_type: MediaType,
    pub duration_ms: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub frame_rate: Option<f32>,
    pub bitrate: Option<i64>,
    pub has_audio: bool,
}

/// Photo repository for database operations
pub struct PhotoRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PhotoRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Batch insert stub rows (streaming scanner Phase 1B).
    ///
    /// Uses `INSERT OR IGNORE` so an idempotent re-scan never blows away
    /// metadata that Phase 2+ already filled in. Only sets the columns
    /// known at walk time; everything else (EXIF, thumbnail, geocoding)
    /// is updated later by the pipeline workers.
    pub fn insert_batch_stub(&self, photos: &[PhotoInsert]) -> SqliteResult<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0;

        for photo in photos {
            tx.execute(
                r#"
                INSERT INTO photos (
                    file_path, file_name, file_hash, file_size, file_mtime,
                    orientation, media_type, metadata_extracted, thumbnailed, faces_processed
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, FALSE, FALSE, FALSE)
                ON CONFLICT(file_path) DO NOTHING
                "#,
                params![
                    photo.relative_path,
                    photo.file_name,
                    photo.file_hash,
                    photo.file_size,
                    photo.file_mtime,
                    photo.orientation,
                    photo.media_type.as_str(),
                ],
            )?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
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
                    width, height, orientation,
                    media_type, duration_ms, video_codec, audio_codec,
                    frame_rate, bitrate, has_audio
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7,
                    ?8, ?9,
                    ?10, ?11,
                    ?12, ?13,
                    ?14, ?15, ?16, ?17,
                    ?18, ?19, ?20,
                    ?21, ?22, ?23,
                    ?24, ?25, ?26, ?27,
                    ?28, ?29, ?30
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
                    media_type = excluded.media_type,
                    duration_ms = excluded.duration_ms,
                    video_codec = excluded.video_codec,
                    audio_codec = excluded.audio_codec,
                    frame_rate = excluded.frame_rate,
                    bitrate = excluded.bitrate,
                    has_audio = excluded.has_audio,
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
                    photo.media_type.as_str(),
                    photo.duration_ms,
                    photo.video_codec,
                    photo.audio_codec,
                    photo.frame_rate,
                    photo.bitrate,
                    photo.has_audio,
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

    pub fn count_timeline_visible(&self, show_stacks: bool) -> SqliteResult<i64> {
        if !show_stacks {
            return self.count();
        }
        self.conn.query_row(
            r#"
            SELECT COUNT(*)
              FROM photos p
             WHERE p.is_trashed = FALSE
               AND NOT EXISTS (
                   SELECT 1
                     FROM photo_stack_members m
                     JOIN photo_stacks s ON s.id = m.stack_id
                    WHERE m.photo_id = p.id
                      AND s.dismissed = FALSE
                      AND p.id != s.cover_photo_id
               )
            "#,
            [],
            |row| row.get(0),
        )
    }

    /// Photos awaiting EXIF / geocoding extraction (Phase 2 of the
    /// streaming scanner). Drives the "Resume reading metadata" banner.
    pub fn count_pending_metadata(&self) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE metadata_extracted = FALSE AND is_trashed = FALSE",
            [],
            |row| row.get(0),
        )
    }

    /// Photos awaiting thumbnail generation (Phase 3).
    pub fn count_pending_thumbnails(&self) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE thumbnailed = FALSE AND is_trashed = FALSE AND media_type = 'photo'",
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
                media_type, duration_ms, video_codec, audio_codec,
                frame_rate, bitrate, has_audio,
                thumbnail_path, faces_processed,
                content_category, ocr_text, ocr_processed, ocr_confidence,
                is_favorite,
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
                    media_type, duration_ms, video_codec, audio_codec,
                    frame_rate, bitrate, has_audio,
                    thumbnail_path, faces_processed,
                    content_category, ocr_text, ocr_processed, ocr_confidence,
                    is_favorite,
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
                media_type, duration_ms, video_codec, audio_codec,
                frame_rate, bitrate, has_audio,
                thumbnail_path, faces_processed,
                content_category, ocr_text, ocr_processed, ocr_confidence,
                is_favorite,
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

    pub fn set_favorite(&self, id: i64, is_favorite: bool) -> SqliteResult<usize> {
        self.conn.execute(
            "UPDATE photos SET is_favorite = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![is_favorite, id],
        )
    }

    pub fn count_favorites(&self) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_favorite = TRUE AND is_trashed = FALSE",
            [],
            |row| row.get(0),
        )
    }

    pub fn favorites_album_summary(
        &self,
    ) -> SqliteResult<
        Option<(
            i64,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    > {
        self.conn.query_row(
            r#"
            SELECT
                COUNT(*),
                (
                    SELECT id
                    FROM photos
                    WHERE is_favorite = TRUE AND is_trashed = FALSE
                    ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                    LIMIT 1
                ),
                (
                    SELECT thumbnail_path
                    FROM photos
                    WHERE is_favorite = TRUE AND is_trashed = FALSE
                    ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                    LIMIT 1
                ),
                MIN(date_taken),
                MAX(date_taken)
            FROM photos
            WHERE is_favorite = TRUE AND is_trashed = FALSE
            "#,
            [],
            |row| {
                let count: i64 = row.get(0)?;
                if count == 0 {
                    Ok(None)
                } else {
                    Ok(Some((
                        count,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    )))
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::create_schema;
    use rusqlite::Connection;

    #[test]
    fn list_after_by_person_uses_valid_stack_aliases() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO face_clusters (id, face_count, photo_count) VALUES (7, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken, is_trashed)
             VALUES (1, 'img.jpg', 'img.jpg', 'hash', 12, '2024-01-01T00:00:00Z', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_width, bbox_height, embedding, cluster_id, confidence)
             VALUES (1, 0.0, 0.0, 0.1, 0.1, zeroblob(16), 7, 0.9)",
            [],
        )
        .unwrap();

        let rows = PhotoRepo::new(&conn)
            .list_after_by_person(7, None, 10)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 1);
        assert!(rows[0].stack_id.is_none());
    }

    #[test]
    fn favorites_summary_only_exists_for_visible_favorites() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken, thumbnail_path, is_favorite, is_trashed)
             VALUES
             (1, 'a.jpg', 'a.jpg', 'hash-a', 12, '2024-01-01T00:00:00Z', 'thumb-a.jpg', 1, 0),
             (2, 'b.jpg', 'b.jpg', 'hash-b', 12, '2024-02-01T00:00:00Z', 'thumb-b.jpg', 1, 1),
             (3, 'c.jpg', 'c.jpg', 'hash-c', 12, '2024-03-01T00:00:00Z', 'thumb-c.jpg', 0, 0)",
            [],
        )
        .unwrap();

        let repo = PhotoRepo::new(&conn);
        let summary = repo.favorites_album_summary().unwrap().unwrap();
        assert_eq!(summary.0, 1);
        assert_eq!(summary.1, Some(1));
        assert_eq!(summary.2.as_deref(), Some("thumb-a.jpg"));

        repo.set_favorite(1, false).unwrap();
        assert!(repo.favorites_album_summary().unwrap().is_none());
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
        media_type: row
            .get::<_, Option<String>>(23)?
            .map(|s| MediaType::from_db(&s))
            .unwrap_or_default(),
        duration_ms: row.get(24)?,
        video_codec: row.get(25)?,
        audio_codec: row.get(26)?,
        frame_rate: row.get(27)?,
        bitrate: row.get(28)?,
        has_audio: row.get::<_, Option<bool>>(29)?.unwrap_or(false),
        thumbnail_path: row.get(30)?,
        faces_processed: row.get(31)?,
        content_category: row
            .get::<_, Option<String>>(32)?
            .map(|s| ContentCategory::from_db(&s))
            .unwrap_or(ContentCategory::Photo),
        ocr_text: row.get(33)?,
        ocr_processed: row.get::<_, Option<bool>>(34)?.unwrap_or(false),
        ocr_confidence: row.get(35)?,
        is_favorite: row.get::<_, Option<bool>>(36)?.unwrap_or(false),
        is_trashed: row.get(37)?,
        trashed_at: row
            .get::<_, Option<String>>(38)?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        indexed_at: row
            .get::<_, String>(39)?
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now()),
        updated_at: row
            .get::<_, String>(40)?
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Cursor-based pagination methods used by the Tauri command surface.
///
/// A short descriptor for a photo that's enough to render a grid cell or
/// a map pin, without paying the cost of a full Photo materialisation.
///
/// Cursor-paginated reads use `(date_taken DESC, id DESC)` with explicit
/// `IS NULL` ordering so a stable cursor can be carried across pages.
#[derive(Debug, Clone)]
pub struct PhotoLite {
    pub id: i64,
    pub date_taken: Option<DateTime<Utc>>,
    pub thumbnail_path: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
    pub is_trashed: bool,
    pub is_favorite: bool,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub media_type: MediaType,
    pub duration_ms: Option<i64>,
    pub stack_id: Option<i64>,
    pub stack_kind: Option<String>,
    pub stack_member_count: Option<i64>,
    pub stack_cover_photo_id: Option<i64>,
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
        is_favorite: row.get::<_, Option<bool>>(7)?.unwrap_or(false),
        gps_latitude: row.get(8)?,
        gps_longitude: row.get(9)?,
        media_type: row
            .get::<_, Option<String>>(10)?
            .map(|s| MediaType::from_db(&s))
            .unwrap_or_default(),
        duration_ms: row.get(11)?,
        stack_id: row.get(12)?,
        stack_kind: row.get(13)?,
        stack_member_count: row.get(14)?,
        stack_cover_photo_id: row.get(15)?,
    })
}

const PHOTO_LITE_COLUMNS: &str = r#"
    id, date_taken, thumbnail_path,
    width, height, orientation, is_trashed, is_favorite,
    gps_latitude, gps_longitude,
    media_type, duration_ms,
    NULL AS stack_id, NULL AS stack_kind, NULL AS stack_member_count, NULL AS stack_cover_photo_id
"#;

const PHOTO_LITE_P_COLUMNS: &str = r#"
    p.id, p.date_taken, p.thumbnail_path,
    p.width, p.height, p.orientation, p.is_trashed, p.is_favorite,
    p.gps_latitude, p.gps_longitude,
    p.media_type, p.duration_ms,
    NULL AS stack_id, NULL AS stack_kind, NULL AS stack_member_count, NULL AS stack_cover_photo_id
"#;

const PHOTO_LITE_STACKED_COLUMNS: &str = r#"
    p.id, p.date_taken, p.thumbnail_path,
    p.width, p.height, p.orientation, p.is_trashed, p.is_favorite,
    p.gps_latitude, p.gps_longitude,
    p.media_type, p.duration_ms,
    s.id AS stack_id, s.kind AS stack_kind, COUNT(sm_all.photo_id) AS stack_member_count,
    s.cover_photo_id AS stack_cover_photo_id
"#;

impl<'a> PhotoRepo<'a> {
    /// Cursor-paginated timeline list. Cursor key is `(date_taken, id)`
    /// descending. NULL date_taken sorts last via SQLite's `IS NULL`.
    /// `limit` is clamped by the caller; this just trusts it.
    pub fn list_after(
        &self,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
        include_trashed: bool,
        show_stacks: bool,
    ) -> SqliteResult<Vec<PhotoLite>> {
        if show_stacks && !include_trashed {
            return self.list_after_stacked(cursor, limit);
        }
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

    fn list_after_stacked(
        &self,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let base = format!(
            r#"
            SELECT {cols}
              FROM photos p
              LEFT JOIN photo_stack_members sm ON sm.photo_id = p.id
              LEFT JOIN photo_stacks s ON s.id = sm.stack_id AND s.dismissed = FALSE
              LEFT JOIN photo_stack_members sm_all ON sm_all.stack_id = s.id
             WHERE p.is_trashed = 0
               AND (s.id IS NULL OR p.id = s.cover_photo_id)
            "#,
            cols = PHOTO_LITE_STACKED_COLUMNS
        );
        let group = " GROUP BY p.id, s.id";

        match cursor {
            Some((Some(d), id)) => {
                let sql = format!(
                    "{base}
                     AND ((p.date_taken IS NOT NULL AND
                           (p.date_taken < ?1 OR (p.date_taken = ?1 AND p.id < ?2)))
                          OR (p.date_taken IS NULL AND ?1 IS NULL AND p.id < ?2))
                     {group}
                     ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
                     LIMIT ?3"
                );
                let date_str = d.to_rfc3339();
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt.query_map(params![date_str, id, limit], row_to_photo_lite)?;
                rows.collect()
            }
            Some((None, id)) => {
                let sql = format!(
                    "{base}
                     AND p.date_taken IS NULL AND p.id < ?1
                     {group}
                     ORDER BY p.id DESC LIMIT ?2"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt.query_map(params![id, limit], row_to_photo_lite)?;
                rows.collect()
            }
            None => {
                let sql = format!(
                    "{base}
                     {group}
                     ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
                     LIMIT ?1"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt.query_map(params![limit], row_to_photo_lite)?;
                rows.collect()
            }
        }
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
            cols = PHOTO_LITE_P_COLUMNS
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

    /// Cursor-paginated favorite photos. This backs the virtual
    /// Favorites album without writing synthetic rows to `albums`.
    pub fn list_after_favorites(
        &self,
        cursor: Option<(Option<DateTime<Utc>>, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<PhotoLite>> {
        let base = format!(
            "SELECT {cols} FROM photos
             WHERE is_favorite = TRUE AND is_trashed = 0",
            cols = PHOTO_LITE_COLUMNS
        );

        match cursor {
            Some((Some(d), id)) => {
                let sql = format!(
                    "{base}
                     AND ((date_taken IS NOT NULL AND
                           (date_taken < ?1 OR (date_taken = ?1 AND id < ?2)))
                          OR (date_taken IS NULL AND ?1 IS NULL AND id < ?2))
                     ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                     LIMIT ?3"
                );
                let date_str = d.to_rfc3339();
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![date_str, id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            Some((None, id)) => {
                let sql = format!(
                    "{base} AND date_taken IS NULL AND id < ?1
                     ORDER BY id DESC LIMIT ?2"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![id, limit], row_to_photo_lite)?
                    .collect::<SqliteResult<Vec<_>>>()?;
                Ok(rows)
            }
            None => {
                let sql = format!(
                    "{base}
                     ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC
                     LIMIT ?1"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![limit], row_to_photo_lite)?
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
            cols = PHOTO_LITE_P_COLUMNS
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
