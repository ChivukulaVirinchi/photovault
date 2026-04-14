//! Write/mutation methods for FaceRepo.

use rusqlite::{params, Result as SqliteResult};

use crate::ml::FaceEmbedding;

use super::FaceRepo;

impl<'a> FaceRepo<'a> {
    /// Insert a detected face with its embedding
    pub fn insert_face(
        &self,
        photo_id: i64,
        bbox_x: f32,
        bbox_y: f32,
        bbox_width: f32,
        bbox_height: f32,
        confidence: f32,
        embedding: &FaceEmbedding,
    ) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO faces (
                photo_id,
                bbox_x, bbox_y, bbox_width, bbox_height,
                confidence, embedding
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                photo_id,
                bbox_x,
                bbox_y,
                bbox_width,
                bbox_height,
                confidence,
                embedding.to_bytes(),
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Mark a photo as face-processed (even if no faces found)
    pub fn mark_photo_processed(&self, photo_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE photos SET faces_processed = TRUE WHERE id = ?1",
            params![photo_id],
        )?;
        Ok(())
    }

    /// Reset faces_processed flags when no faces were actually detected.
    ///
    /// This handles the case where a prior run marked all photos as processed
    /// but failed to actually detect faces (e.g., model loading error).
    pub fn reset_if_no_faces(&self) -> SqliteResult<usize> {
        let face_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM faces", [], |row| row.get(0))?;
        let processed_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE faces_processed = TRUE",
            [],
            |row| row.get(0),
        )?;

        if face_count == 0 && processed_count > 0 {
            let reset = self.conn.execute(
                "UPDATE photos SET faces_processed = FALSE WHERE faces_processed = TRUE",
                [],
            )?;
            tracing::info!(
                "Reset faces_processed flag on {} photos (no faces were actually detected)",
                reset
            );
            Ok(reset)
        } else {
            Ok(0)
        }
    }

    /// Name a cluster (set the person's name)
    pub fn name_cluster(&self, cluster_id: i64, name: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE face_clusters SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, cluster_id],
        )?;
        Ok(())
    }

    /// Assign an existing face to an existing cluster and update cluster metadata.
    pub fn assign_face_to_cluster(&self, face_id: i64, cluster_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "UPDATE faces SET cluster_id = ?1 WHERE id = ?2",
            params![cluster_id, face_id],
        )?;

        Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
        Self::refresh_gallery_tx(&tx, cluster_id)?;

        tx.commit()
    }

    /// Delete all clusters and reset face cluster assignments.
    ///
    /// Call this before re-running clustering to avoid duplicate clusters.
    pub fn delete_all_clusters(&self) -> SqliteResult<()> {
        self.conn
            .execute("UPDATE faces SET cluster_id = NULL", [])?;
        self.conn
            .execute("DELETE FROM person_gallery_embeddings", [])?;
        self.conn.execute("DELETE FROM face_clusters", [])?;
        tracing::info!("Deleted all face clusters and reset face assignments");
        Ok(())
    }

    /// Create a new face cluster from a set of face IDs
    pub fn create_cluster(&self, face_ids: &[i64]) -> SqliteResult<i64> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            r#"
            INSERT INTO face_clusters (face_count, photo_count)
            VALUES (0, 0)
            "#,
            [],
        )?;

        let cluster_id = tx.last_insert_rowid();

        // Assign faces to this cluster
        for face_id in face_ids {
            tx.execute(
                "UPDATE faces SET cluster_id = ?1 WHERE id = ?2",
                params![cluster_id, face_id],
            )?;
        }

        Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
        Self::refresh_gallery_tx(&tx, cluster_id)?;

        tx.commit()?;

        Ok(cluster_id)
    }

    /// Merge source cluster into target cluster
    ///
    /// Moves all faces from source to target, updates counts, deletes source.
    pub fn merge_clusters(&self, source_id: i64, target_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Move all faces from source to target
        tx.execute(
            "UPDATE faces SET cluster_id = ?1 WHERE cluster_id = ?2",
            params![target_id, source_id],
        )?;

        // Move inferred identities from source to target (dedupe on unique constraint).
        tx.execute(
            r#"
            INSERT OR IGNORE INTO photo_inferred_identities (photo_id, cluster_id, source_photo_id, confidence)
            SELECT photo_id, ?1, source_photo_id, confidence
            FROM photo_inferred_identities
            WHERE cluster_id = ?2
            "#,
            params![target_id, source_id],
        )?;

        Self::refresh_cluster_stats_tx(&tx, target_id)?;
        Self::refresh_gallery_tx(&tx, target_id)?;

        // Delete source cluster
        tx.execute(
            "DELETE FROM face_clusters WHERE id = ?1",
            params![source_id],
        )?;

        tx.commit()
    }
}
