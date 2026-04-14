//! Read/query methods for FaceRepo.
//!
//! Several methods here are kept as part of the repo's public surface even
//! when nothing in the binary calls them today: they're cheap to maintain
//! and likely to be used by future features.
#![allow(dead_code)]

use rusqlite::{params, Result as SqliteResult};

use crate::ml::FaceEmbedding;

use super::{FaceClusterRecord, FaceRepo, GalleryEmbedding, ReviewItem};

impl<'a> FaceRepo<'a> {
    /// Get all faces with embeddings (for clustering)
    pub fn get_all_faces_with_embeddings(&self) -> SqliteResult<Vec<(i64, FaceEmbedding)>> {
        let mut stmt = self.conn.prepare("SELECT id, embedding FROM faces")?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            Ok((id, bytes))
        })?;

        let mut faces = Vec::new();
        for row in rows {
            let (id, bytes) = row?;
            match FaceEmbedding::from_bytes(&bytes) {
                Some(emb) => faces.push((id, emb)),
                None => tracing::warn!("Corrupted face embedding for face_id={}: {} bytes", id, bytes.len()),
            }
        }

        Ok(faces)
    }

    /// Get all faces with photo_id and embeddings.
    pub fn get_all_faces_with_photo_embeddings(
        &self,
    ) -> SqliteResult<Vec<(i64, i64, FaceEmbedding)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, photo_id, embedding FROM faces")?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let photo_id: i64 = row.get(1)?;
            let bytes: Vec<u8> = row.get(2)?;
            Ok((id, photo_id, bytes))
        })?;

        let mut faces = Vec::new();
        for row in rows {
            let (id, photo_id, bytes) = row?;
            match FaceEmbedding::from_bytes(&bytes) {
                Some(emb) => faces.push((id, photo_id, emb)),
                None => tracing::warn!("Corrupted face embedding for face_id={}: {} bytes", id, bytes.len()),
            }
        }

        Ok(faces)
    }

    /// Get unclustered faces with embeddings.
    pub fn get_unclustered_faces_with_embeddings(&self) -> SqliteResult<Vec<(i64, FaceEmbedding)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, embedding FROM faces WHERE cluster_id IS NULL")?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            Ok((id, bytes))
        })?;

        let mut faces = Vec::new();
        for row in rows {
            let (id, bytes) = row?;
            match FaceEmbedding::from_bytes(&bytes) {
                Some(emb) => faces.push((id, emb)),
                None => tracing::warn!("Corrupted face embedding for face_id={}: {} bytes", id, bytes.len()),
            }
        }

        Ok(faces)
    }

    /// Get unclustered faces with photo_id and embeddings.
    pub fn get_unclustered_faces_with_photo_embeddings(
        &self,
    ) -> SqliteResult<Vec<(i64, i64, FaceEmbedding)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, photo_id, embedding FROM faces WHERE cluster_id IS NULL")?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let photo_id: i64 = row.get(1)?;
            let bytes: Vec<u8> = row.get(2)?;
            Ok((id, photo_id, bytes))
        })?;

        let mut faces = Vec::new();
        for row in rows {
            let (id, photo_id, bytes) = row?;
            match FaceEmbedding::from_bytes(&bytes) {
                Some(emb) => faces.push((id, photo_id, emb)),
                None => tracing::warn!("Corrupted face embedding for face_id={}: {} bytes", id, bytes.len()),
            }
        }

        Ok(faces)
    }

    /// Get centroid embedding for each existing cluster.
    pub fn get_cluster_centroids(&self) -> SqliteResult<Vec<(i64, FaceEmbedding)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT cluster_id, embedding
            FROM faces
            WHERE cluster_id IS NOT NULL
            ORDER BY cluster_id
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let cluster_id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            Ok((cluster_id, bytes))
        })?;

        let mut grouped: std::collections::HashMap<i64, Vec<FaceEmbedding>> =
            std::collections::HashMap::new();
        for row in rows {
            let (cluster_id, bytes) = row?;
            match FaceEmbedding::from_bytes(&bytes) {
                Some(emb) => { grouped.entry(cluster_id).or_default().push(emb); }
                None => tracing::warn!("Corrupted face embedding in cluster_id={}: {} bytes", cluster_id, bytes.len()),
            }
        }

        let mut centroids = Vec::new();
        for (cluster_id, embeddings) in grouped {
            if embeddings.is_empty() {
                continue;
            }

            let mut sum = vec![0.0f32; 512];
            for emb in &embeddings {
                for (i, value) in emb.vector.iter().enumerate() {
                    sum[i] += *value;
                }
            }

            let count = embeddings.len() as f32;
            for value in &mut sum {
                *value /= count;
            }

            let norm: f32 = sum.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for value in &mut sum {
                    *value /= norm;
                }
            }

            let centroid = FaceEmbedding::new(ndarray::Array1::from_vec(sum));
            centroids.push((cluster_id, centroid));
        }

        Ok(centroids)
    }

    /// Get all clusters, ordered by photo count descending
    pub fn get_all_clusters(&self) -> SqliteResult<Vec<FaceClusterRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, representative_face_id, face_count, photo_count
            FROM face_clusters
            ORDER BY photo_count DESC, face_count DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FaceClusterRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                representative_face_id: row.get(2)?,
                face_count: row.get(3)?,
                photo_count: row.get(4)?,
                face_thumbnail_path: None, // Set after query using drive path
            })
        })?;

        let mut clusters = Vec::new();
        for row in rows {
            clusters.push(row?);
        }

        Ok(clusters)
    }

    pub fn get_gallery_embeddings(&self) -> SqliteResult<Vec<GalleryEmbedding>> {
        let mut stmt = self.conn.prepare(
            "SELECT cluster_id, face_id, embedding FROM person_gallery_embeddings ORDER BY cluster_id",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (cluster_id, face_id, bytes) = row?;
            match FaceEmbedding::from_bytes(&bytes) {
                Some(embedding) => {
                    result.push(GalleryEmbedding {
                        cluster_id,
                        face_id,
                        embedding,
                    });
                }
                None => tracing::warn!("Corrupted gallery embedding for cluster_id={}, face_id={}: {} bytes", cluster_id, face_id, bytes.len()),
            }
        }

        Ok(result)
    }

    pub fn get_cluster_photo_ids(&self) -> SqliteResult<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT cluster_id, photo_id FROM faces WHERE cluster_id IS NOT NULL",
        )?;

        let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get person names for a photo (faces detected in this photo with cluster names)
    pub fn get_person_names_for_photo(&self, photo_id: i64) -> SqliteResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT name FROM (
                SELECT fc.name AS name
                FROM faces f
                JOIN face_clusters fc ON f.cluster_id = fc.id
                WHERE f.photo_id = ?1

                UNION

                SELECT fc.name AS name
                FROM photo_inferred_identities pii
                JOIN face_clusters fc ON pii.cluster_id = fc.id
                WHERE pii.photo_id = ?1
            )
            WHERE name IS NOT NULL AND name != ''
            ORDER BY name
            "#,
        )?;

        let names = stmt
            .query_map(params![photo_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(names)
    }

    /// Get all photo IDs that contain faces from a given cluster
    pub fn get_photos_for_cluster(&self, cluster_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT photo_id FROM (
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
            "#,
        )?;

        let rows = stmt.query_map(params![cluster_id], |row| row.get(0))?;

        let mut photo_ids = Vec::new();
        for row in rows {
            photo_ids.push(row?);
        }

        Ok(photo_ids)
    }

    /// Get all face IDs with bounding boxes and their photo file paths.
    /// Used for regenerating missing face crops.
    pub fn get_all_faces_with_paths(
        &self,
    ) -> SqliteResult<Vec<(i64, String, i32, f32, f32, f32, f32)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.id, p.file_path, COALESCE(p.orientation, 1), f.bbox_x, f.bbox_y, f.bbox_width, f.bbox_height
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, f32>(3)?,
                row.get::<_, f32>(4)?,
                row.get::<_, f32>(5)?,
                row.get::<_, f32>(6)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    /// Find contextual candidate identities from nearby photos in the same folder.
    pub fn get_contextual_cluster_candidates(
        &self,
        photo_id: i64,
        folder_prefix_like: Option<&str>,
        target_ts: i64,
        window_secs: i64,
    ) -> SqliteResult<Vec<(i64, i64, i64, String)>> {
        let sql_with_folder = r#"
            SELECT DISTINCT
                p.id,
                f.cluster_id,
                CAST(strftime('%s', p.date_taken) AS INTEGER) AS source_ts,
                p.file_path
            FROM photos p
            JOIN faces f ON f.photo_id = p.id
            WHERE p.id != ?1
                AND p.is_trashed = FALSE
                AND p.date_taken IS NOT NULL
                AND f.cluster_id IS NOT NULL
                AND p.file_path LIKE ?2
                AND ABS(CAST(strftime('%s', p.date_taken) AS INTEGER) - ?3) <= ?4
        "#;

        let sql_without_folder = r#"
            SELECT DISTINCT
                p.id,
                f.cluster_id,
                CAST(strftime('%s', p.date_taken) AS INTEGER) AS source_ts,
                p.file_path
            FROM photos p
            JOIN faces f ON f.photo_id = p.id
            WHERE p.id != ?1
                AND p.is_trashed = FALSE
                AND p.date_taken IS NOT NULL
                AND f.cluster_id IS NOT NULL
                AND ABS(CAST(strftime('%s', p.date_taken) AS INTEGER) - ?2) <= ?3
        "#;

        let mut result = Vec::new();

        if let Some(folder_like) = folder_prefix_like {
            let mut stmt = self.conn.prepare(sql_with_folder)?;
            let rows = stmt.query_map(
                params![photo_id, folder_like, target_ts, window_secs],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            for row in rows {
                result.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(sql_without_folder)?;
            let rows = stmt.query_map(params![photo_id, target_ts, window_secs], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                result.push(row?);
            }
        }

        Ok(result)
    }

    /// Get photos that need face processing (not yet processed)
    pub fn get_unprocessed_photo_ids(&self) -> SqliteResult<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, file_path FROM photos
            WHERE faces_processed = FALSE AND is_trashed = FALSE
            ORDER BY date_taken DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    /// Get unprocessed photos with optional timestamp for contextual identity linking.
    pub fn get_unprocessed_photos_with_context(
        &self,
    ) -> SqliteResult<Vec<(i64, String, i32, Option<i64>)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                id,
                file_path,
                COALESCE(orientation, 1) AS orientation,
                CASE
                    WHEN date_taken IS NOT NULL THEN CAST(strftime('%s', date_taken) AS INTEGER)
                    ELSE NULL
                END AS taken_ts
            FROM photos
            WHERE faces_processed = FALSE AND is_trashed = FALSE
            ORDER BY date_taken DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Count of unresolved entries in the face review queue.
    pub fn review_queue_size(&self) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM face_review_queue WHERE resolved_at IS NULL",
            [],
            |row| row.get(0),
        )
    }

    /// Pull the top-N most informative unresolved review items.
    ///
    /// Ordered by ambiguity ascending (closer top-2 scores first = higher
    /// information gain) then by candidate cluster size descending (resolving
    /// ambiguity on big clusters propagates to more photos).
    pub fn get_review_queue_items(&self, limit: usize) -> SqliteResult<Vec<ReviewItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                q.id, q.face_id, f.photo_id,
                q.candidate_cluster_id, c.name, c.face_count,
                q.score, q.ambiguity
            FROM face_review_queue q
            JOIN faces f ON f.id = q.face_id
            JOIN face_clusters c ON c.id = q.candidate_cluster_id
            WHERE q.resolved_at IS NULL
            ORDER BY COALESCE(q.ambiguity, 1.0) ASC, c.face_count DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ReviewItem {
                queue_id: row.get(0)?,
                face_id: row.get(1)?,
                face_photo_id: row.get(2)?,
                candidate_cluster_id: row.get(3)?,
                candidate_cluster_name: row.get(4)?,
                candidate_cluster_size: row.get(5)?,
                candidate_sample_face_ids: Vec::new(),
                score: row.get(6)?,
                ambiguity: row.get(7)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        drop(stmt);

        // Populate candidate sample face_ids per item (up to 4 best from the cluster).
        let mut sample_stmt = self.conn.prepare(
            r#"
            SELECT id
            FROM faces
            WHERE cluster_id = ?1
            ORDER BY confidence DESC, id ASC
            LIMIT 4
            "#,
        )?;
        for item in items.iter_mut() {
            let ids = sample_stmt
                .query_map(params![item.candidate_cluster_id], |row| row.get::<_, i64>(0))?;
            let mut collected = Vec::new();
            for id in ids {
                collected.push(id?);
            }
            item.candidate_sample_face_ids = collected;
        }

        Ok(items)
    }

    /// Cluster IDs of the already-assigned faces in the given photo (other
    /// than the face we're resolving). Used as the "context" in co-occurrence.
    pub fn get_photo_other_clusters(
        &self,
        photo_id: i64,
        exclude_face_id: i64,
    ) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT cluster_id FROM faces
             WHERE photo_id = ?1 AND id != ?2 AND cluster_id IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![photo_id, exclude_face_id], |row| {
            row.get::<_, i64>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// How many photos contain both cluster_a and cluster_b.
    ///
    /// Used as the co-occurrence strength between two clusters. High values
    /// mean the two people appear together frequently and should bias
    /// retrieval toward whichever matches the rest of the photo's cast.
    pub fn cooccurrence_count(&self, cluster_a: i64, cluster_b: i64) -> SqliteResult<i64> {
        if cluster_a == cluster_b {
            return Ok(0);
        }
        self.conn.query_row(
            r#"
            SELECT COUNT(*) FROM (
                SELECT photo_id FROM faces WHERE cluster_id = ?1
                INTERSECT
                SELECT photo_id FROM faces WHERE cluster_id = ?2
            )
            "#,
            params![cluster_a, cluster_b],
            |row| row.get(0),
        )
    }

    /// Clusters assigned to faces within a time window around `date_taken`,
    /// excluding the photo itself. Returns (cluster_id, delta_seconds).
    ///
    /// Used for temporal-chain propagation: if nearby-in-time photos contain
    /// high-confidence assignments for cluster X, an ambiguous face in our
    /// photo should be biased toward X.
    pub fn temporal_neighbor_clusters(
        &self,
        photo_id: i64,
        window_secs: i64,
    ) -> SqliteResult<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            r#"
            WITH base AS (
                SELECT date_taken AS t
                FROM photos
                WHERE id = ?1 AND date_taken IS NOT NULL
            )
            SELECT DISTINCT f.cluster_id,
                CAST(ABS(
                    strftime('%s', p.date_taken) - strftime('%s', (SELECT t FROM base))
                ) AS INTEGER) AS delta_sec
            FROM faces f
            JOIN photos p ON p.id = f.photo_id
            WHERE f.cluster_id IS NOT NULL
              AND p.id != ?1
              AND p.is_trashed = FALSE
              AND p.date_taken IS NOT NULL
              AND ABS(
                  strftime('%s', p.date_taken) - strftime('%s', (SELECT t FROM base))
              ) <= ?2
            "#,
        )?;
        let rows = stmt.query_map(params![photo_id, window_secs], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
