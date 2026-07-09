//! Gallery and stats management for FaceRepo.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, Result as SqliteResult};

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

        let sticky_ids: std::collections::HashSet<i64> = sticky.iter().map(|(id, _)| *id).collect();

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

    /// Return fallback face thumbnail candidates for the supplied clusters.
    ///
    /// The rows are ordered by cluster, then by face confidence descending.
    /// Callers can check the corresponding crop files outside the shared DB
    /// mutex and then persist any representative replacement in one batch.
    pub fn face_thumbnail_candidates(
        &self,
        cluster_ids: &[i64],
        max_per_cluster: usize,
    ) -> SqliteResult<HashMap<i64, Vec<i64>>> {
        if cluster_ids.is_empty() || max_per_cluster == 0 {
            return Ok(HashMap::new());
        }

        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut seen = HashSet::new();
        let mut unique_ids = Vec::new();
        for id in cluster_ids {
            if seen.insert(*id) {
                unique_ids.push(*id);
            }
        }

        for chunk in unique_ids.chunks(400) {
            let placeholders = (1..=chunk.len())
                .map(|idx| format!("?{}", idx))
                .collect::<Vec<_>>()
                .join(",");
            let limit_param = chunk.len() + 1;
            let sql = format!(
                r#"
                SELECT cluster_id, id
                FROM (
                    SELECT
                        faces.cluster_id,
                        faces.id,
                        ROW_NUMBER() OVER (
                            PARTITION BY faces.cluster_id
                            ORDER BY faces.confidence DESC, faces.id ASC
                        ) AS rn
                    FROM faces
                    JOIN photos p ON p.id = faces.photo_id
                    WHERE faces.cluster_id IN ({})
                      AND p.is_trashed = FALSE
                )
                WHERE rn <= ?{}
                ORDER BY cluster_id ASC, rn ASC
                "#,
                placeholders, limit_param
            );

            let mut values: Vec<rusqlite::types::Value> = chunk
                .iter()
                .map(|id| rusqlite::types::Value::from(*id))
                .collect();
            values.push(rusqlite::types::Value::from(max_per_cluster as i64));

            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (cluster_id, face_id) = row?;
                out.entry(cluster_id).or_default().push(face_id);
            }
        }

        Ok(out)
    }

    pub fn update_representative_faces(&self, updates: &[(i64, i64)]) -> SqliteResult<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE face_clusters SET representative_face_id = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            )?;
            for (cluster_id, face_id) in updates {
                stmt.execute(params![face_id, cluster_id])?;
            }
        }
        tx.commit()
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
    /// Call this after `get_all_clusters()` with the drive root path. The
    /// path written into each cluster is **relative to drive_root** (e.g.
    /// `.photovault/faces/42.jpg`) so it round-trips through the same
    /// frontend `thumbUrl()` helper as `photos.thumbnail_path`.
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
                    cluster.face_thumbnail_path =
                        Some(format!(".photovault/faces/{}.jpg", face_id));
                    continue;
                }
            }

            let mut replacement_face_id: Option<i64> = None;
            let mut stmt = self.conn.prepare(
                r#"
                SELECT f.id
                FROM faces f
                JOIN photos p ON p.id = f.photo_id
                WHERE f.cluster_id = ?1
                  AND p.is_trashed = FALSE
                ORDER BY f.confidence DESC
                "#,
            )?;

            let mut rows = stmt.query(params![cluster.id])?;
            while let Some(row) = rows.next()? {
                let face_id: i64 = row.get(0)?;
                let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                if crop_path.exists() {
                    replacement_face_id = Some(face_id);
                    cluster.face_thumbnail_path =
                        Some(format!(".photovault/faces/{}.jpg", face_id));
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

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};

    use super::*;
    use crate::db::create_schema;

    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        for id in 1..=3 {
            conn.execute(
                "INSERT INTO photos (id, file_path, file_name, file_hash, file_size)
                 VALUES (?1, ?2, ?3, ?4, 100)",
                params![
                    id,
                    format!("photos/{id}.jpg"),
                    format!("{id}.jpg"),
                    format!("hash-{id}")
                ],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO face_clusters (id, name, face_count, photo_count)
             VALUES (10, 'A', 2, 2), (20, 'B', 1, 1)",
            [],
        )
        .unwrap();
        for (id, cluster_id, confidence) in [(1, 10, 0.5), (2, 10, 0.9), (3, 20, 0.7)] {
            conn.execute(
                "INSERT INTO faces (
                    id, photo_id, bbox_x, bbox_y, bbox_width, bbox_height,
                    embedding, cluster_id, confidence, user_confirmed
                 )
                 VALUES (?1, ?1, 0.1, 0.1, 0.2, 0.2, ?2, ?3, ?4, 0)",
                params![id, vec![id as u8; 16], cluster_id, confidence],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn face_thumbnail_candidates_are_batched_and_confidence_ordered() {
        let conn = seeded_conn();
        let repo = FaceRepo::new(&conn);

        let candidates = repo.face_thumbnail_candidates(&[10, 20, 10], 2).unwrap();

        assert_eq!(candidates.get(&10).unwrap(), &vec![2, 1]);
        assert_eq!(candidates.get(&20).unwrap(), &vec![3]);
    }

    #[test]
    fn face_thumbnail_candidates_ignore_trashed_photos() {
        let conn = seeded_conn();
        conn.execute("UPDATE photos SET is_trashed = TRUE WHERE id = 2", [])
            .unwrap();
        let repo = FaceRepo::new(&conn);

        let candidates = repo.face_thumbnail_candidates(&[10], 2).unwrap();

        assert_eq!(candidates.get(&10).unwrap(), &vec![1]);
    }

    #[test]
    fn update_representative_faces_updates_all_rows_in_one_call() {
        let conn = seeded_conn();
        let repo = FaceRepo::new(&conn);

        repo.update_representative_faces(&[(10, 2), (20, 3)])
            .unwrap();

        let reps: Vec<(i64, i64)> = conn
            .prepare("SELECT id, representative_face_id FROM face_clusters ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(reps, vec![(10, 2), (20, 3)]);
    }
}
