//! Face processing pipeline
//!
//! Orchestrates the face detection -> embedding -> clustering workflow.
//! Designed to run as a background task without blocking the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::db::face_repo::FaceRepo;
use crate::db::Database;
use crate::ml::{FaceClusterer, FaceDetector, FaceEmbedder, OnnxRuntime};

/// Progress information for face processing
#[derive(Debug, Clone)]
pub struct FaceProcessingProgress {
    pub processed: usize,
    pub total: usize,
    pub faces_found: usize,
}

impl Default for FaceProcessingProgress {
    fn default() -> Self {
        Self {
            processed: 0,
            total: 0,
            faces_found: 0,
        }
    }
}

/// Result of a face processing run
#[derive(Debug, Clone)]
pub struct FaceProcessingResult {
    pub photos_processed: usize,
    pub faces_detected: usize,
    pub clusters_created: usize,
}

/// Face processing pipeline
///
/// Call `process_photos` to run the full detect -> embed -> cluster pipeline.
pub struct FaceProcessor;

impl FaceProcessor {
    /// Run the full face processing pipeline on unprocessed photos.
    ///
    /// This is designed to be called from `spawn_blocking` as it does
    /// heavy CPU work (ML inference) and blocking DB operations.
    ///
    /// # Arguments
    /// * `drive_path` - Root drive path
    /// * `model_dir` - Directory containing ONNX model files
    /// * `progress_tx` - Channel to send progress updates
    pub fn process_photos(
        drive_path: &Path,
        model_dir: &Path,
        detector_confidence: f32,
        progress_tx: Option<async_channel::Sender<FaceProcessingProgress>>,
    ) -> Result<FaceProcessingResult, String> {
        // Open database
        let db = Database::open_for_drive(drive_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        let face_repo = FaceRepo::new(&db.conn);

        // Reset processing flags if a prior run marked photos as processed
        // but didn't actually detect any faces (e.g., model loading failed)
        let _ = face_repo.reset_if_no_faces();

        // Get unprocessed photos
        let unprocessed = face_repo
            .get_unprocessed_photo_ids()
            .map_err(|e| format!("Failed to get unprocessed photos: {}", e))?;

        let total = unprocessed.len();
        if total == 0 {
            // No photos to process; still run clustering on any unclustered faces
            let clusters_created = Self::run_clustering(&face_repo)?;
            return Ok(FaceProcessingResult {
                photos_processed: 0,
                faces_detected: 0,
                clusters_created,
            });
        }

        // Initialize ONNX Runtime and load models
        let runtime = OnnxRuntime::init().map_err(|e| {
            format!(
                "Failed to init ONNX Runtime: {}. Install ONNX Runtime 1.23.x and set ORT_DYLIB_PATH, or place libonnxruntime.so in libs/onnxruntime/.",
                e
            )
        })?;

        let detector_path = model_dir.join("scrfd_10g_bnkps.onnx");
        let embedder_path = model_dir.join("glintr100.onnx");

        // Check if model files exist
        if !detector_path.exists() {
            return Err(format!(
                "Face detection model not found at: {}. \
                 Please download SCRFD model to this location.",
                detector_path.display()
            ));
        }
        if !embedder_path.exists() {
            return Err(format!(
                "Face embedding model not found at: {}. \
                 Please download ArcFace model to this location.",
                embedder_path.display()
            ));
        }

        let mut detector = FaceDetector::new(&runtime, &detector_path)
            .map_err(|e| format!("Failed to load face detector: {}", e))?;
        detector = detector.with_confidence_threshold(detector_confidence);

        let mut embedder = FaceEmbedder::new(&runtime, &embedder_path)
            .map_err(|e| format!("Failed to load face embedder: {}", e))?;

        tracing::info!("Face processing: {} photos to process", total);

        let mut total_faces = 0usize;
        let mut last_progress_emit = Instant::now()
            .checked_sub(Duration::from_millis(250))
            .unwrap_or_else(Instant::now);

        // Create faces directory for storing cropped face thumbnails
        let faces_dir = drive_path.join(".photovault").join("faces");
        if let Err(e) = std::fs::create_dir_all(&faces_dir) {
            tracing::warn!("Failed to create faces directory: {}", e);
        }

        // Phase 1: Detect faces and generate embeddings
        for (idx, (photo_id, file_path)) in unprocessed.iter().enumerate() {
            // Send progress
            if let Some(ref tx) = progress_tx {
                let now = Instant::now();
                if now.duration_since(last_progress_emit) >= Duration::from_millis(250)
                    || idx + 1 == total
                {
                    if tx
                        .try_send(FaceProcessingProgress {
                            processed: idx,
                            total,
                            faces_found: total_faces,
                        })
                        .is_ok()
                    {
                        last_progress_emit = now;
                    }
                }
            }

            // Load image
            let full_path = drive_path.join(file_path);
            let image = match image::open(&full_path) {
                Ok(img) => img,
                Err(e) => {
                    tracing::debug!("Failed to open image {}: {}", file_path, e);
                    // Mark as processed so we don't retry
                    let _ = face_repo.mark_photo_processed(*photo_id);
                    continue;
                }
            };

            // Detect faces
            let faces = detector.detect(&image);

            if !faces.is_empty() {
                tracing::info!(
                    "Photo {}/{}: {} faces detected in {}",
                    idx + 1,
                    total,
                    faces.len(),
                    file_path
                );
            }

            // Generate embeddings and store in DB
            for face in &faces {
                if let Some(ref aligned) = face.aligned_face {
                    if let Some(embedding) = embedder.embed(aligned) {
                        match face_repo.insert_face(
                            *photo_id,
                            face.bbox_normalized.0,
                            face.bbox_normalized.1,
                            face.bbox_normalized.2,
                            face.bbox_normalized.3,
                            face.confidence,
                            &embedding,
                        ) {
                            Ok(face_id) => {
                                // Save face crop to disk for display in People view
                                let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                                Self::save_face_crop(aligned, &crop_path);
                                total_faces += 1;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to insert face: {}", e);
                            }
                        }
                    }
                }
            }

            // Mark photo as processed
            let _ = face_repo.mark_photo_processed(*photo_id);
        }

        // Send clustering phase progress
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: total,
                total,
                faces_found: total_faces,
            });
        }

        // Phase 2: Cluster faces
        let clusters_created = Self::run_clustering(&face_repo)?;

        // Send completion
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: total,
                total,
                faces_found: total_faces,
            });
        }

        tracing::info!(
            "Face processing complete: {} photos, {} faces, {} clusters",
            total,
            total_faces,
            clusters_created
        );

        Ok(FaceProcessingResult {
            photos_processed: total,
            faces_detected: total_faces,
            clusters_created,
        })
    }

    /// Run DBSCAN clustering on all face embeddings and create/update clusters.
    ///
    /// Clears all existing clusters first to avoid duplicates when re-run.
    fn run_clustering(face_repo: &FaceRepo) -> Result<usize, String> {
        let all_faces = face_repo
            .get_all_faces_with_embeddings()
            .map_err(|e| format!("Failed to get faces for clustering: {}", e))?;

        if all_faces.is_empty() {
            return Ok(0);
        }

        // Clear existing clusters before re-clustering to avoid duplicates
        face_repo
            .delete_all_clusters()
            .map_err(|e| format!("Failed to clear existing clusters: {}", e))?;

        let clusterer = FaceClusterer::new();
        let assignments = clusterer.cluster(&all_faces);

        // Group face IDs by cluster
        let mut cluster_groups: HashMap<i32, Vec<i64>> = HashMap::new();
        for (face_id, cluster_id) in &assignments {
            if *cluster_id >= 0 {
                cluster_groups
                    .entry(*cluster_id)
                    .or_default()
                    .push(*face_id);
            }
        }

        // Create clusters in database
        let mut clusters_created = 0;
        for (_cluster_label, face_ids) in &cluster_groups {
            if face_ids.len() >= 2 {
                let _ = face_repo.create_cluster(face_ids);
                clusters_created += 1;
            }
        }

        Ok(clusters_created)
    }

    /// Save a face crop image to disk as JPEG.
    fn save_face_crop(aligned_face: &image::RgbImage, path: &Path) {
        let dynamic = image::DynamicImage::ImageRgb8(aligned_face.clone());
        // Resize to 80x80 for thumbnail display (the aligned face is 112x112)
        let resized = dynamic.resize_exact(80, 80, image::imageops::FilterType::Lanczos3);
        if let Err(e) = resized.save(path) {
            tracing::debug!("Failed to save face crop to {}: {}", path.display(), e);
        }
    }

    /// Regenerate missing face crop files from stored bounding box data.
    ///
    /// This handles the case where faces were detected before the crop-saving
    /// code was added. It reads original images, crops using the stored bbox
    /// coordinates, and saves 80x80 JPEG thumbnails.
    pub fn regenerate_missing_crops(drive_path: &Path) -> Result<usize, String> {
        let db = Database::open_for_drive(drive_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        let face_repo = FaceRepo::new(&db.conn);

        let faces_dir = Self::faces_dir(drive_path);
        if let Err(e) = std::fs::create_dir_all(&faces_dir) {
            return Err(format!("Failed to create faces directory: {}", e));
        }

        let all_faces = face_repo
            .get_all_faces_with_paths()
            .map_err(|e| format!("Failed to get faces: {}", e))?;

        let mut regenerated = 0usize;
        for (face_id, file_path, bbox_x, bbox_y, bbox_w, bbox_h) in &all_faces {
            let crop_path = faces_dir.join(format!("{}.jpg", face_id));
            if crop_path.exists() {
                continue; // Already has crop
            }

            let full_path = drive_path.join(file_path);
            let img = match image::open(&full_path) {
                Ok(img) => img,
                Err(_) => continue,
            };

            let (img_w, img_h) = (img.width() as f32, img.height() as f32);

            // Convert normalized bbox back to pixel coordinates
            let px = (bbox_x * img_w) as u32;
            let py = (bbox_y * img_h) as u32;
            let pw = (bbox_w * img_w) as u32;
            let ph = (bbox_h * img_h) as u32;

            // Expand crop area slightly for context (20% padding)
            let pad_x = (pw as f32 * 0.2) as u32;
            let pad_y = (ph as f32 * 0.2) as u32;
            let crop_x = px.saturating_sub(pad_x);
            let crop_y = py.saturating_sub(pad_y);
            let crop_w = (pw + 2 * pad_x).min(img.width() - crop_x);
            let crop_h = (ph + 2 * pad_y).min(img.height() - crop_y);

            if crop_w == 0 || crop_h == 0 {
                continue;
            }

            let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h);
            let resized = cropped.resize_exact(80, 80, image::imageops::FilterType::Lanczos3);
            if resized.save(&crop_path).is_ok() {
                regenerated += 1;
            }
        }

        if regenerated > 0 {
            tracing::info!("Regenerated {} missing face crop thumbnails", regenerated);
        }

        Ok(regenerated)
    }

    /// Get the face crops directory for a drive.
    pub fn faces_dir(drive_path: &Path) -> PathBuf {
        drive_path.join(".photovault").join("faces")
    }
}
