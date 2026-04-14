//! Inferred identity repository
//!
//! Stores contextual person links for photos where no direct face match exists.

use rusqlite::{params, Connection, Result as SqliteResult};

/// Repository for inferred identities.
pub struct InferredIdentityRepo<'a> {
    conn: &'a Connection,
}

impl<'a> InferredIdentityRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_inferred_identity(
        &self,
        photo_id: i64,
        cluster_id: i64,
        source_photo_id: i64,
        confidence: f32,
    ) -> SqliteResult<()> {
        self.conn.execute(
            r#"
            INSERT INTO photo_inferred_identities (photo_id, cluster_id, source_photo_id, confidence, is_inferred)
            VALUES (?1, ?2, ?3, ?4, TRUE)
            ON CONFLICT(photo_id, cluster_id) DO UPDATE SET
                source_photo_id = excluded.source_photo_id,
                confidence = excluded.confidence,
                is_inferred = TRUE,
                created_at = CURRENT_TIMESTAMP
            "#,
            params![photo_id, cluster_id, source_photo_id, confidence],
        )?;
        self.refresh_cluster_stats(cluster_id)?;
        Ok(())
    }

    fn refresh_cluster_stats(&self, cluster_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            r#"
            UPDATE face_clusters SET
                face_count = (SELECT COUNT(*) FROM faces WHERE cluster_id = ?1),
                photo_count = (
                    SELECT COUNT(DISTINCT photo_id)
                    FROM (
                        SELECT photo_id FROM faces WHERE cluster_id = ?1
                        UNION
                        SELECT photo_id FROM photo_inferred_identities WHERE cluster_id = ?1
                    )
                ),
                representative_face_id = (
                    SELECT id
                    FROM faces
                    WHERE cluster_id = ?1
                    ORDER BY confidence DESC
                    LIMIT 1
                ),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![cluster_id],
        )?;

        Ok(())
    }
}
