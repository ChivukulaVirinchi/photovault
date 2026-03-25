//! Face database repository
//!
//! Handles all database operations for faces and face clusters.

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::ml::FaceEmbedding;

/// Face cluster record from database
#[derive(Debug, Clone)]
pub struct FaceClusterRecord {
    pub id: i64,
    pub name: Option<String>,
    pub representative_face_id: Option<i64>,
    pub face_count: i64,
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

    /// Delete all clusters and reset face cluster assignments.
    ///
    /// Call this before re-running clustering to avoid duplicate clusters.
    pub fn delete_all_clusters(&self) -> SqliteResult<()> {
        self.conn
            .execute("UPDATE faces SET cluster_id = NULL", [])?;
        self.conn.execute("DELETE FROM face_clusters", [])?;
        tracing::info!("Deleted all face clusters and reset face assignments");
        Ok(())
    }

    /// Create a new face cluster from a set of face IDs
    pub fn create_cluster(&self, face_ids: &[i64]) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO face_clusters (face_count)
            VALUES (?1)
            "#,
            params![face_ids.len() as i64],
        )?;

        let cluster_id = self.conn.last_insert_rowid();

        // Assign faces to this cluster
        for face_id in face_ids {
            self.conn.execute(
                "UPDATE faces SET cluster_id = ?1 WHERE id = ?2",
                params![cluster_id, face_id],
            )?;
        }

        // Set the representative face (highest confidence)
        self.conn.execute(
            r#"
            UPDATE face_clusters SET representative_face_id = (
                SELECT id FROM faces
                WHERE cluster_id = ?1
                ORDER BY confidence DESC
                LIMIT 1
            ) WHERE id = ?1
            "#,
            params![cluster_id],
        )?;

        Ok(cluster_id)
    }

    /// Get all clusters, ordered by face count descending
    pub fn get_all_clusters(&self) -> SqliteResult<Vec<FaceClusterRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, representative_face_id, face_count
            FROM face_clusters
            ORDER BY face_count DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FaceClusterRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                representative_face_id: row.get(2)?,
                face_count: row.get(3)?,
                face_thumbnail_path: None, // Set after query using drive path
            })
        })?;

        let mut clusters = Vec::new();
        for row in rows {
            clusters.push(row?);
        }

        Ok(clusters)
    }

    /// Populate face thumbnail paths on cluster records.
    ///
    /// Call this after `get_all_clusters()` with the drive root path so
    /// each cluster's representative face crop can be resolved to an absolute path.
    pub fn populate_face_thumbnails(
        clusters: &mut [FaceClusterRecord],
        drive_path: &std::path::Path,
    ) {
        let faces_dir = drive_path.join(".photovault").join("faces");
        for cluster in clusters.iter_mut() {
            if let Some(face_id) = cluster.representative_face_id {
                let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                if crop_path.exists() {
                    cluster.face_thumbnail_path = Some(crop_path.to_string_lossy().to_string());
                }
            }
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

    /// Merge source cluster into target cluster
    ///
    /// Moves all faces from source to target, updates counts, deletes source.
    pub fn merge_clusters(&self, source_id: i64, target_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Move all faces from source to target
        self.conn.execute(
            "UPDATE faces SET cluster_id = ?1 WHERE cluster_id = ?2",
            params![target_id, source_id],
        )?;

        // Update face count on target
        self.conn.execute(
            r#"
            UPDATE face_clusters SET
                face_count = (SELECT COUNT(*) FROM faces WHERE cluster_id = ?1),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![target_id],
        )?;

        // Delete source cluster
        self.conn.execute(
            "DELETE FROM face_clusters WHERE id = ?1",
            params![source_id],
        )?;

        tx.commit()
    }

    /// Get all photo IDs that contain faces from a given cluster
    pub fn get_photos_for_cluster(&self, cluster_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT photo_id FROM faces
            WHERE cluster_id = ?1
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

    /// Get all face IDs with bounding boxes and their photo file paths.
    /// Used for regenerating missing face crops.
    pub fn get_all_faces_with_paths(&self) -> SqliteResult<Vec<(i64, String, f32, f32, f32, f32)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.id, p.file_path, f.bbox_x, f.bbox_y, f.bbox_width, f.bbox_height
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
                row.get::<_, f32>(3)?,
                row.get::<_, f32>(4)?,
                row.get::<_, f32>(5)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }
}
