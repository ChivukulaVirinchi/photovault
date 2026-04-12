//! Face database repository
//!
//! Handles all database operations for faces and face clusters.

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::ml::FaceEmbedding;

#[derive(Debug, Clone)]
pub struct GalleryEmbedding {
    pub cluster_id: i64,
    pub face_id: i64,
    pub embedding: FaceEmbedding,
}

/// Face cluster record from database
#[derive(Debug, Clone)]
pub struct FaceClusterRecord {
    pub id: i64,
    pub name: Option<String>,
    pub representative_face_id: Option<i64>,
    pub face_count: i64,
    pub photo_count: i64,
    /// Path to the representative face thumbnail (computed, not stored in DB)
    pub face_thumbnail_path: Option<String>,
}

/// Face database repository
pub struct FaceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> FaceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

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
            if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                faces.push((id, emb));
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
            if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                faces.push((id, photo_id, emb));
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
            if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                faces.push((id, emb));
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
            if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                faces.push((id, photo_id, emb));
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
            if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                grouped.entry(cluster_id).or_default().push(emb);
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

            let centroid = FaceEmbedding::new(ndarray::Array1::from_vec(sum));
            centroids.push((cluster_id, centroid));
        }

        Ok(centroids)
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
            if let Some(embedding) = FaceEmbedding::from_bytes(&bytes) {
                result.push(GalleryEmbedding {
                    cluster_id,
                    face_id,
                    embedding,
                });
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

    /// Name a cluster (set the person's name)
    pub fn name_cluster(&self, cluster_id: i64, name: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE face_clusters SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, cluster_id],
        )?;
        Ok(())
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

    fn refresh_cluster_stats_tx(
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

    fn refresh_gallery_tx(tx: &rusqlite::Transaction<'_>, cluster_id: i64) -> SqliteResult<()> {
        tx.execute(
            "DELETE FROM person_gallery_embeddings WHERE cluster_id = ?1",
            params![cluster_id],
        )?;

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

        let mut selected: Vec<(i64, FaceEmbedding, f32)> = Vec::new();
        for row in rows {
            let (face_id, bytes, confidence) = row?;
            let Some(emb) = FaceEmbedding::from_bytes(&bytes) else {
                continue;
            };

            if selected.len() < 5 {
                selected.push((face_id, emb, confidence));
                continue;
            }

            let mut min_similarity = 1.0f32;
            for (_, existing, _) in &selected {
                let sim = emb.cosine_similarity(existing);
                if sim < min_similarity {
                    min_similarity = sim;
                }
            }

            if min_similarity < 0.80 {
                // Replace most redundant current item to preserve diversity.
                let mut replace_idx = 0usize;
                let mut replace_score = 2.0f32;
                for (idx, (_, existing, _)) in selected.iter().enumerate() {
                    let mut avg = 0.0f32;
                    let mut cnt = 0.0f32;
                    for (j, (_, other, _)) in selected.iter().enumerate() {
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
                selected[replace_idx] = (face_id, emb, confidence);
            }
        }

        for (face_id, emb, quality_score) in selected {
            tx.execute(
                r#"
                INSERT INTO person_gallery_embeddings (cluster_id, face_id, embedding, quality_score)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![cluster_id, face_id, emb.to_bytes(), quality_score],
            )?;
        }

        Ok(())
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
}
