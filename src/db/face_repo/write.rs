//! Write/mutation methods for FaceRepo.

use rusqlite::{params, Result as SqliteResult};

use super::FaceRepo;

impl<'a> FaceRepo<'a> {
    /// Name a cluster (set the person's name)
    pub fn name_cluster(&self, cluster_id: i64, name: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE face_clusters SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, cluster_id],
        )?;
        Ok(())
    }

    /// Delete a face cluster — "this isn't a real person" gesture.
    ///
    /// Faces previously assigned to this cluster have their `cluster_id`
    /// set to NULL so a future re-clustering pass can pick them up
    /// again. Inferred-identity links and gallery embeddings rooted at
    /// the cluster cascade away (FK ON DELETE CASCADE in the schema).
    pub fn delete_cluster(&self, cluster_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE faces SET cluster_id = NULL WHERE cluster_id = ?1",
            params![cluster_id],
        )?;
        tx.execute(
            "DELETE FROM photo_inferred_identities WHERE cluster_id = ?1",
            params![cluster_id],
        )?;
        tx.execute(
            "DELETE FROM person_gallery_embeddings WHERE cluster_id = ?1",
            params![cluster_id],
        )?;
        tx.execute(
            "DELETE FROM face_clusters WHERE id = ?1",
            params![cluster_id],
        )?;
        tx.commit()
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
    /// Refuses if a cannot-merge constraint exists between the two clusters.
    pub fn merge_clusters(&self, source_id: i64, target_id: i64) -> SqliteResult<()> {
        let blocked: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(*) FROM cluster_cannot_merge
            WHERE (cluster_a_id = ?1 AND cluster_b_id = ?2)
               OR (cluster_a_id = ?2 AND cluster_b_id = ?1)
            "#,
            params![source_id, target_id],
            |row| row.get(0),
        )?;
        if blocked > 0 {
            tracing::warn!(
                "Refusing to merge clusters {} and {}: user marked them different",
                source_id,
                target_id
            );
            return Ok(());
        }

        let tx = self.conn.unchecked_transaction()?;

        // Preserve names: if the target is unnamed but the source has a name,
        // carry the name over so we never lose a user-assigned label.
        let target_name: Option<String> = tx
            .query_row(
                "SELECT name FROM face_clusters WHERE id = ?1",
                params![target_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        if target_name.is_none() {
            let source_name: Option<String> = tx
                .query_row(
                    "SELECT name FROM face_clusters WHERE id = ?1",
                    params![source_id],
                    |row| row.get(0),
                )
                .ok()
                .flatten();
            if let Some(name) = source_name {
                tx.execute(
                    "UPDATE face_clusters SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                    params![name, target_id],
                )?;
            }
        }

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

    /// Load all cannot-merge pairs (returns them in both directions for easy lookup).
    pub fn get_cannot_merge_map(
        &self,
    ) -> SqliteResult<std::collections::HashMap<i64, std::collections::HashSet<i64>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT cluster_a_id, cluster_b_id FROM cluster_cannot_merge")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;

        let mut map: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
            std::collections::HashMap::new();
        for row in rows {
            let (a, b) = row?;
            map.entry(a).or_default().insert(b);
            map.entry(b).or_default().insert(a);
        }
        Ok(map)
    }

    /// Confirm a face: set user_confirmed=1. Add to gallery if not already.
    pub fn confirm_face(&self, face_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "UPDATE faces SET user_confirmed = 1 WHERE id = ?1",
            params![face_id],
        )?;

        let (cluster_id, embedding_bytes, confidence): (i64, Vec<u8>, Option<f32>) = tx.query_row(
            "SELECT cluster_id, embedding, confidence FROM faces WHERE id = ?1",
            params![face_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        tx.execute(
            r#"
            INSERT INTO person_gallery_embeddings
                (cluster_id, face_id, embedding, quality_score, source)
            VALUES (?1, ?2, ?3, ?4, 'user_confirmed')
            ON CONFLICT(cluster_id, face_id) DO UPDATE SET
                source = 'user_confirmed',
                quality_score = excluded.quality_score
            "#,
            params![
                cluster_id,
                face_id,
                embedding_bytes,
                confidence.unwrap_or(0.0)
            ],
        )?;

        Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
        Self::refresh_gallery_tx(&tx, cluster_id)?;

        tx.commit()
    }

    /// Reject face to unknown: set cluster_id=NULL, user_confirmed=0. Write negative.
    pub fn reject_face_to_unknown(&self, face_id: i64, prev_cluster_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT OR IGNORE INTO face_negatives (face_id, not_cluster_id) VALUES (?1, ?2)",
            params![face_id, prev_cluster_id],
        )?;

        tx.execute(
            "UPDATE faces SET cluster_id = NULL, user_confirmed = 0 WHERE id = ?1",
            params![face_id],
        )?;

        Self::refresh_cluster_stats_tx(&tx, prev_cluster_id)?;
        Self::refresh_gallery_tx(&tx, prev_cluster_id)?;

        tx.commit()
    }

    /// Reassign a face to a different cluster. Marks user_confirmed=1, writes negative
    /// against the old cluster_id so it won't come back.
    pub fn reassign_face(
        &self,
        face_id: i64,
        new_cluster_id: i64,
        old_cluster_id: i64,
    ) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT OR IGNORE INTO face_negatives (face_id, not_cluster_id) VALUES (?1, ?2)",
            params![face_id, old_cluster_id],
        )?;

        tx.execute(
            "UPDATE faces SET cluster_id = ?1, user_confirmed = 1 WHERE id = ?2",
            params![new_cluster_id, face_id],
        )?;

        let embedding_bytes: Vec<u8> = tx.query_row(
            "SELECT embedding FROM faces WHERE id = ?1",
            params![face_id],
            |row| row.get(0),
        )?;
        let confidence: Option<f32> = tx
            .query_row(
                "SELECT confidence FROM faces WHERE id = ?1",
                params![face_id],
                |row| row.get(0),
            )
            .ok();

        tx.execute(
            r#"
            INSERT INTO person_gallery_embeddings
                (cluster_id, face_id, embedding, quality_score, source)
            VALUES (?1, ?2, ?3, ?4, 'user_confirmed')
            ON CONFLICT(cluster_id, face_id) DO UPDATE SET
                source = 'user_confirmed',
                quality_score = excluded.quality_score
            "#,
            params![
                new_cluster_id,
                face_id,
                embedding_bytes,
                confidence.unwrap_or(0.0)
            ],
        )?;

        Self::refresh_cluster_stats_tx(&tx, old_cluster_id)?;
        Self::refresh_gallery_tx(&tx, old_cluster_id)?;
        Self::refresh_cluster_stats_tx(&tx, new_cluster_id)?;
        Self::refresh_gallery_tx(&tx, new_cluster_id)?;

        tx.commit()
    }

    /// Confirm a currently-unassigned face into a cluster (used by K-similar flow).
    pub fn confirm_face_to_cluster(&self, face_id: i64, cluster_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "UPDATE faces SET cluster_id = ?1, user_confirmed = 1 WHERE id = ?2",
            params![cluster_id, face_id],
        )?;

        let embedding_bytes: Vec<u8> = tx.query_row(
            "SELECT embedding FROM faces WHERE id = ?1",
            params![face_id],
            |row| row.get(0),
        )?;
        let confidence: Option<f32> = tx
            .query_row(
                "SELECT confidence FROM faces WHERE id = ?1",
                params![face_id],
                |row| row.get(0),
            )
            .ok();

        tx.execute(
            r#"
            INSERT INTO person_gallery_embeddings
                (cluster_id, face_id, embedding, quality_score, source)
            VALUES (?1, ?2, ?3, ?4, 'user_confirmed')
            ON CONFLICT(cluster_id, face_id) DO UPDATE SET
                source = 'user_confirmed',
                quality_score = excluded.quality_score
            "#,
            params![
                cluster_id,
                face_id,
                embedding_bytes,
                confidence.unwrap_or(0.0)
            ],
        )?;

        Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
        Self::refresh_gallery_tx(&tx, cluster_id)?;

        tx.commit()
    }
}
