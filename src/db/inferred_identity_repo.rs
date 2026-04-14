//! Inferred identity repository
//!
//! Stores contextual person links for photos where no direct face match exists.
#![allow(dead_code)]

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

    pub fn delete_for_photo(&self, photo_id: i64) -> SqliteResult<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT cluster_id FROM photo_inferred_identities WHERE photo_id = ?1",
        )?;
        let cluster_ids = stmt
            .query_map(params![photo_id], |row| row.get::<_, i64>(0))?
            .filter_map(|row| row.ok())
            .collect::<Vec<_>>();

        let deleted = self.conn.execute(
            "DELETE FROM photo_inferred_identities WHERE photo_id = ?1",
            params![photo_id],
        )?;

        for cluster_id in cluster_ids {
            self.refresh_cluster_stats(cluster_id)?;
        }

        Ok(deleted)
    }

    pub fn get_photo_ids_for_cluster(&self, cluster_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT photo_id FROM photo_inferred_identities WHERE cluster_id = ?1",
        )?;

        let rows = stmt.query_map(params![cluster_id], |row| row.get::<_, i64>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
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
