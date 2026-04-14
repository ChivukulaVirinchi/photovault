//! Read/query methods for FaceRepo.

use rusqlite::{params, Result as SqliteResult};

use crate::ml::FaceEmbedding;

use super::{FaceClusterRecord, FaceRepo, GalleryEmbedding};

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
}
