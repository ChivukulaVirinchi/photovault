//! Document repository
//!
//! Queries and updates for document categorization and OCR metadata.

use rusqlite::{params, params_from_iter, Connection, Result as SqliteResult};

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
                media_type, duration_ms, video_codec, audio_codec,
                frame_rate, bitrate, has_audio,
                thumbnail_path, faces_processed,
                content_category, ocr_text, ocr_processed, ocr_confidence,
                is_favorite,
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
                media_type, duration_ms, video_codec, audio_codec,
                frame_rate, bitrate, has_audio,
                thumbnail_path, faces_processed,
                content_category, ocr_text, ocr_processed, ocr_confidence,
                is_favorite,
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

    pub fn get_documents_by_categories(
        &self,
        categories: &[String],
        limit: i64,
        offset: i64,
    ) -> SqliteResult<Vec<Photo>> {
        if categories.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat_n("?", categories.len())
            .collect::<Vec<_>>()
            .join(", ");
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
            WHERE is_trashed = FALSE AND content_category IN ({placeholders})
            ORDER BY date_taken DESC
            LIMIT ? OFFSET ?
            "#
        );
        let params = categories.iter().map(|s| s as &dyn rusqlite::ToSql).chain([
            &limit as &dyn rusqlite::ToSql,
            &offset as &dyn rusqlite::ToSql,
        ]);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter(params),
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
        let Some(query) = fts_literal_query(query) else {
            return Ok(Vec::new());
        };
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
                p.media_type, p.duration_ms, p.video_codec, p.audio_codec,
                p.frame_rate, p.bitrate, p.has_audio,
                p.thumbnail_path, p.faces_processed,
                p.content_category, p.ocr_text, p.ocr_processed, p.ocr_confidence,
                p.is_favorite,
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

    pub fn update_content_category(&self, photo_id: i64, category: &str) -> SqliteResult<usize> {
        self.conn.execute(
            "UPDATE photos SET content_category = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![category, photo_id],
        )
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

fn fts_literal_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter_map(|term| {
            let cleaned = term.trim_matches(|ch: char| {
                !ch.is_alphanumeric() && ch != '\'' && ch != '-' && ch != '_'
            });
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("\"{}\"", cleaned.replace('"', "\"\"")))
            }
        })
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::{fts_literal_query, DocumentRepo};
    use crate::db::create_schema;
    use rusqlite::Connection;

    #[test]
    fn fts_literal_query_quotes_user_terms() {
        assert_eq!(
            fts_literal_query("invoice: (delhi) \"trip\""),
            Some("\"invoice\" \"delhi\" \"trip\"".to_string())
        );
    }

    #[test]
    fn fts_literal_query_ignores_operator_only_input() {
        assert_eq!(fts_literal_query("() OR *"), Some("\"OR\"".to_string()));
        assert_eq!(fts_literal_query("() *"), None);
    }

    #[test]
    fn category_list_pages_across_categories_in_one_order() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos
                (id, file_path, file_name, file_hash, file_size, date_taken, content_category, is_trashed)
             VALUES
                (1, 'a.jpg', 'a.jpg', 'hash-a', 10, '2026-01-01T00:00:00Z', 'document', 0),
                (2, 'b.jpg', 'b.jpg', 'hash-b', 10, '2026-01-03T00:00:00Z', 'receipt', 0),
                (3, 'c.jpg', 'c.jpg', 'hash-c', 10, '2026-01-02T00:00:00Z', 'document', 0)",
            [],
        )
        .unwrap();

        let repo = DocumentRepo::new(&conn);
        let categories = vec!["document".to_string(), "receipt".to_string()];
        let page = repo.get_documents_by_categories(&categories, 2, 1).unwrap();

        assert_eq!(page.iter().map(|p| p.id).collect::<Vec<_>>(), vec![3, 1]);
    }
}
