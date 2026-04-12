//! Document repository
//!
//! Queries and updates for document categorization and OCR metadata.

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::models::Photo;

pub struct DocumentRepo<'a> {
    conn: &'a Connection,
}

impl<'a> DocumentRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_non_photo_documents(&self, limit: i64, offset: i64) -> SqliteResult<Vec<Photo>> {
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
            WHERE is_trashed = FALSE AND content_category != 'photo'
            ORDER BY date_taken DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit, offset], crate::db::photo_repo::row_to_photo)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_documents_by_category(
        &self,
        category: &str,
        limit: i64,
        offset: i64,
    ) -> SqliteResult<Vec<Photo>> {
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
            WHERE is_trashed = FALSE AND content_category = ?1
            ORDER BY date_taken DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let rows = stmt.query_map(
            params![category, limit, offset],
            crate::db::photo_repo::row_to_photo,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn search_documents_fts(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> SqliteResult<Vec<Photo>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                p.id, p.file_path, p.file_name, p.file_hash, p.file_size,
                p.date_taken, p.date_taken_source,
                p.gps_latitude, p.gps_longitude,
                p.location_city, p.location_country,
                p.camera_make, p.camera_model,
                p.iso, p.aperture, p.shutter_speed, p.focal_length,
                p.lens_model, p.flash, p.gps_altitude,
                p.width, p.height, p.orientation,
                p.thumbnail_path, p.faces_processed,
                p.content_category, p.ocr_text, p.ocr_processed, p.ocr_confidence,
                p.is_trashed, p.trashed_at,
                p.indexed_at, p.updated_at
            FROM photos p
            JOIN photos_fts fts ON fts.rowid = p.id
            WHERE p.is_trashed = FALSE
                AND p.content_category != 'photo'
                AND photos_fts MATCH ?1
            ORDER BY p.date_taken DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let rows = stmt.query_map(
            params![query, limit, offset],
            crate::db::photo_repo::row_to_photo,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_content_category(&self, photo_id: i64, category: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE photos SET content_category = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![category, photo_id],
        )?;
        Ok(())
    }

    pub fn update_ocr_metadata(
        &self,
        photo_id: i64,
        text: Option<&str>,
        confidence: Option<f32>,
        processed: bool,
    ) -> SqliteResult<()> {
        self.conn.execute(
            r#"
            UPDATE photos
            SET ocr_text = ?1,
                ocr_confidence = ?2,
                ocr_processed = ?3,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?4
            "#,
            params![text, confidence, processed, photo_id],
        )?;
        Ok(())
    }

    pub fn get_unprocessed_for_document_analysis(
        &self,
        limit: i64,
    ) -> SqliteResult<Vec<(i64, String, i32)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, file_path, COALESCE(orientation, 1)
            FROM photos
            WHERE is_trashed = FALSE AND ocr_processed = FALSE
            ORDER BY date_taken DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
