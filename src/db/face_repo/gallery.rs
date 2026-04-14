//! Gallery and stats management for FaceRepo.

use rusqlite::{params, Result as SqliteResult};

use crate::ml::FaceEmbedding;

use super::{FaceClusterRecord, FaceRepo};

impl<'a> FaceRepo<'a> {
    pub fn refresh_all_galleries(&self) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let mut ids = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT id FROM face_clusters")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            for row in rows {
                ids.push(row?);
            }
        }

        tx.execute("DELETE FROM person_gallery_embeddings", [])?;
        for cluster_id in ids {
            Self::refresh_gallery_tx(&tx, cluster_id)?;
        }
        tx.commit()
    }

    pub(crate) fn refresh_gallery_tx(
        tx: &rusqlite::Transaction<'_>,
        cluster_id: i64,
    ) -> SqliteResult<()> {
        // User-confirmed gallery members are sticky: never evicted by diversity
        // replacement. Auto-selected members are rebuilt from scratch each call.
        let mut sticky: Vec<(i64, FaceEmbedding)> = Vec::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT face_id, embedding
                FROM person_gallery_embeddings
                WHERE cluster_id = ?1 AND source = 'user_confirmed'
                "#,
            )?;
            let rows = stmt.query_map(params![cluster_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            for row in rows {
                let (face_id, bytes) = row?;
                if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                    sticky.push((face_id, emb));
                }
            }
        }

        tx.execute(
            "DELETE FROM person_gallery_embeddings WHERE cluster_id = ?1 AND source != 'user_confirmed'",
            params![cluster_id],
        )?;

        let sticky_ids: std::collections::HashSet<i64> =
            sticky.iter().map(|(id, _)| *id).collect();

        let mut stmt = tx.prepare(
            r#"
            SELECT id, embedding, confidence
            FROM faces
            WHERE cluster_id = ?1
            ORDER BY confidence DESC, id ASC
            "#,
        )?;

        let rows = stmt.query_map(params![cluster_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Option<f32>>(2)?.unwrap_or(0.0),
            ))
        })?;

        const MAX_GALLERY: usize = 30;
        const DIVERSITY_THRESHOLD: f32 = 0.70;

        // Auto members can't include faces already sticky.
        let mut auto: Vec<(i64, FaceEmbedding, f32)> = Vec::new();
        for row in rows {
            let (face_id, bytes, confidence) = row?;
            if sticky_ids.contains(&face_id) {
                continue;
            }
            let Some(emb) = FaceEmbedding::from_bytes(&bytes) else {
                tracing::warn!(
                    "Corrupted embedding in refresh_gallery for face_id={}: {} bytes",
                    face_id,
                    bytes.len()
                );
                continue;
            };

            // Seed with the top-N by confidence until we hit the cap.
            let total = sticky.len() + auto.len();
            if total < MAX_GALLERY {
                auto.push((face_id, emb, confidence));
                continue;
            }

            // Check diversity against both sticky and auto members.
            let mut min_similarity = 1.0f32;
            for (_, existing) in &sticky {
                let sim = emb.cosine_similarity(existing);
                if sim < min_similarity {
                    min_similarity = sim;
                }
            }
            for (_, existing, _) in &auto {
                let sim = emb.cosine_similarity(existing);
                if sim < min_similarity {
                    min_similarity = sim;
                }
            }

            if min_similarity < DIVERSITY_THRESHOLD {
                // Replace the most redundant *auto* entry (sticky ones are untouchable).
                let mut replace_idx = 0usize;
                let mut replace_score = 2.0f32;
                for (idx, (_, existing, _)) in auto.iter().enumerate() {
                    let mut avg = 0.0f32;
                    let mut cnt = 0.0f32;
                    for (_, other) in &sticky {
                        avg += existing.cosine_similarity(other);
                        cnt += 1.0;
                    }
                    for (j, (_, other, _)) in auto.iter().enumerate() {
                        if idx == j {
                            continue;
                        }
                        avg += existing.cosine_similarity(other);
                        cnt += 1.0;
                    }
                    if cnt > 0.0 {
                        avg /= cnt;
                    }
                    if avg < replace_score {
                        replace_score = avg;
                        replace_idx = idx;
                    }
                }
                if !auto.is_empty() {
                    auto[replace_idx] = (face_id, emb, confidence);
                }
            }
        }

        for (face_id, emb, quality_score) in auto {
            tx.execute(
                r#"
                INSERT INTO person_gallery_embeddings (cluster_id, face_id, embedding, quality_score, source)
                VALUES (?1, ?2, ?3, ?4, 'auto')
                "#,
                params![cluster_id, face_id, emb.to_bytes(), quality_score],
            )?;
        }

        Ok(())
    }

    pub(crate) fn refresh_cluster_stats_tx(
        tx: &rusqlite::Transaction<'_>,
        cluster_id: i64,
    ) -> SqliteResult<()> {
        tx.execute(
            r#"
            UPDATE face_clusters SET
                face_count = (
                    SELECT COUNT(*)
                    FROM faces f
                    JOIN photos p ON p.id = f.photo_id
                    WHERE f.cluster_id = ?1 AND p.is_trashed = FALSE
                ),
                photo_count = (
                    SELECT COUNT(DISTINCT photo_id)
                    FROM (
                        SELECT f.photo_id
                        FROM faces f
                        JOIN photos p ON p.id = f.photo_id
                        WHERE f.cluster_id = ?1 AND p.is_trashed = FALSE
                        UNION
                        SELECT pii.photo_id
                        FROM photo_inferred_identities pii
                        JOIN photos p ON p.id = pii.photo_id
                        WHERE pii.cluster_id = ?1 AND p.is_trashed = FALSE
                    )
                ),
                representative_face_id = (
                    SELECT f.id
                    FROM faces f
                    JOIN photos p ON p.id = f.photo_id
                    WHERE f.cluster_id = ?1 AND p.is_trashed = FALSE
                    ORDER BY f.confidence DESC
                    LIMIT 1
                ),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![cluster_id],
        )?;

        Ok(())
    }

    /// Recompute all cluster stats and prune empty clusters.
    pub fn normalize_cluster_stats(&self) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        let mut ids = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT id FROM face_clusters")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            for row in rows {
                ids.push(row?);
            }
        }

        for cluster_id in ids {
            Self::refresh_cluster_stats_tx(&tx, cluster_id)?;
        }

        tx.execute(
            "DELETE FROM face_clusters WHERE face_count <= 0 AND photo_count <= 0",
            [],
        )?;

        tx.commit()
    }

    /// Populate face thumbnail paths on cluster records.
    ///
    /// Call this after `get_all_clusters()` with the drive root path so
    /// each cluster's representative face crop can be resolved to an absolute path.
    pub fn populate_face_thumbnails(
        &self,
        clusters: &mut [FaceClusterRecord],
        drive_path: &std::path::Path,
    ) -> SqliteResult<()> {
        let faces_dir = drive_path.join(".photovault").join("faces");
        for cluster in clusters.iter_mut() {
            cluster.face_thumbnail_path = None;

            if let Some(face_id) = cluster.representative_face_id {
                let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                if crop_path.exists() {
                    cluster.face_thumbnail_path = Some(crop_path.to_string_lossy().to_string());
                    continue;
                }
            }

            let mut replacement_face_id: Option<i64> = None;
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id
                FROM faces
                WHERE cluster_id = ?1
                ORDER BY confidence DESC
                "#,
            )?;

            let mut rows = stmt.query(params![cluster.id])?;
            while let Some(row) = rows.next()? {
                let face_id: i64 = row.get(0)?;
                let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                if crop_path.exists() {
                    replacement_face_id = Some(face_id);
                    cluster.face_thumbnail_path = Some(crop_path.to_string_lossy().to_string());
                    break;
                }
            }

            drop(rows);
            drop(stmt);

            if let Some(face_id) = replacement_face_id {
                cluster.representative_face_id = Some(face_id);
                self.conn.execute(
                    "UPDATE face_clusters SET representative_face_id = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                    params![face_id, cluster.id],
                )?;
            }
        }

        Ok(())
    }
}
