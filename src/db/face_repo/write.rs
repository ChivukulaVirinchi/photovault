//! Write/mutation methods for FaceRepo.

use rusqlite::{params, OptionalExtension, Result as SqliteResult};

use super::{FaceRepo, MergeReceipt};

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
    /// Its faces are set aside (`user_confirmed = -1`), so the next
    /// clustering pass doesn't quietly rebuild the same person.
    /// Inferred-identity links and gallery embeddings rooted at the
    /// cluster cascade away (FK ON DELETE CASCADE in the schema).
    pub fn delete_cluster(&self, cluster_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE faces SET cluster_id = NULL, user_confirmed = -1 WHERE cluster_id = ?1",
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

    /// "Not a face": set a detection aside for good. It leaves its person
    /// and clustering never picks it up again (`user_confirmed = -1`).
    pub fn ignore_face(&self, face_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let cluster_id: Option<i64> = tx
            .query_row(
                "SELECT cluster_id FROM faces WHERE id = ?1",
                params![face_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        tx.execute(
            "UPDATE faces SET cluster_id = NULL, user_confirmed = -1 WHERE id = ?1",
            params![face_id],
        )?;
        tx.execute(
            "DELETE FROM face_review_queue WHERE face_id = ?1",
            params![face_id],
        )?;
        if let Some(cluster_id) = cluster_id {
            Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
            Self::refresh_gallery_tx(&tx, cluster_id)?;
        }
        tx.commit()
    }

    /// Take a face out of review without deciding about it ("not sure").
    /// It keeps its person; review just stops asking.
    pub fn skip_review_face(&self, face_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            r#"INSERT INTO library_meta (key, value) VALUES ('people.review_skipped', json_array(?1))
               ON CONFLICT(key) DO UPDATE SET value = json_insert(value, '$[#]', ?1)"#,
            params![face_id],
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
    pub fn merge_clusters(
        &self,
        source_id: i64,
        target_id: i64,
    ) -> SqliteResult<Option<MergeReceipt>> {
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
            return Ok(None);
        }

        let tx = self.conn.unchecked_transaction()?;
        let (source_name, source_user_named): (Option<String>, bool) = tx.query_row(
            "SELECT name, COALESCE(is_user_named, 0) FROM face_clusters WHERE id = ?1",
            params![source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let face_ids: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM faces WHERE cluster_id = ?1")?;
            let rows = stmt.query_map(params![source_id], |row| row.get(0))?;
            rows.collect::<SqliteResult<_>>()?
        };
        let mut target_took_name = false;

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
                target_took_name = true;
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

        tx.commit()?;
        Ok(Some(MergeReceipt {
            source_id,
            target_id,
            source_name,
            source_user_named,
            face_ids,
            target_took_name,
        }))
    }

    /// Undo a merge: bring the source person back with their faces and
    /// remember the two are different people, so clustering never merges
    /// them again. Inferred photo links are rebuilt by the next face pass.
    pub fn unmerge_clusters(&self, receipt: &MergeReceipt) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO face_clusters (id, name, is_user_named) VALUES (?1, ?2, ?3)",
            params![
                receipt.source_id,
                receipt.source_name,
                receipt.source_user_named
            ],
        )?;
        {
            let mut stmt =
                tx.prepare("UPDATE faces SET cluster_id = ?1 WHERE id = ?2 AND cluster_id = ?3")?;
            for face_id in &receipt.face_ids {
                stmt.execute(params![receipt.source_id, face_id, receipt.target_id])?;
            }
        }
        if receipt.target_took_name {
            tx.execute(
                "UPDATE face_clusters SET name = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                params![receipt.target_id],
            )?;
        }
        let (a, b) = (
            receipt.source_id.min(receipt.target_id),
            receipt.source_id.max(receipt.target_id),
        );
        tx.execute(
            "INSERT OR IGNORE INTO cluster_cannot_merge (cluster_a_id, cluster_b_id) VALUES (?1, ?2)",
            params![a, b],
        )?;
        for cluster_id in [receipt.source_id, receipt.target_id] {
            Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
            Self::refresh_gallery_tx(&tx, cluster_id)?;
        }
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
