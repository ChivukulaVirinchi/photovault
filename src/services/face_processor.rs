//! Face processing pipeline
//!
//! Orchestrates the face detection -> embedding -> clustering workflow.
//! Designed to run as a background task without blocking the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::face_repo::FaceRepo;
use crate::db::Database;
use crate::db::InferredIdentityRepo;
use crate::ml::{ClusterInput, FaceClusterer, FaceDetector, FaceEmbedder, OnnxRuntime};
use crate::services::image_utils::apply_exif_orientation;

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
    const CONTEXT_WINDOW_SECS: i64 = 60;
    const CONTEXT_MIN_CONFIDENCE: f32 = 0.5;

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
        clustering_threshold: f32,
        progress_tx: Option<async_channel::Sender<FaceProcessingProgress>>,
        cancel_flag: Option<Arc<AtomicBool>>,
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
            .get_unprocessed_photos_with_context()
            .map_err(|e| format!("Failed to get unprocessed photos: {}", e))?;

        let inferred_repo = InferredIdentityRepo::new(&db.conn);

        let total = unprocessed.len();
        if total == 0 {
            // No photos to process; still run clustering on any unclustered faces
            let clusters_created = Self::run_clustering(&face_repo, clustering_threshold)?;
            return Ok(FaceProcessingResult {
                photos_processed: 0,
                faces_detected: 0,
                clusters_created,
            });
        }

        // Initialize ONNX Runtime and load models
        let runtime = OnnxRuntime::init().map_err(|e| {
            format!(
                "Failed to init ONNX Runtime: {}. Install ONNX Runtime 1.23.x and set ORT_DYLIB_PATH, or place the runtime library in libs/onnxruntime/.",
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
        for (idx, (photo_id, file_path, orientation, taken_ts)) in unprocessed.iter().enumerate() {
            // Check for cancellation
            if let Some(ref flag) = cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    tracing::info!("Face processing cancelled at photo {}/{}", idx, total);
                    // Still run clustering on whatever faces we've found so far
                    let clusters_created = Self::run_clustering(&face_repo, clustering_threshold)?;
                    return Ok(FaceProcessingResult {
                        photos_processed: idx,
                        faces_detected: total_faces,
                        clusters_created,
                    });
                }
            }

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

            // Load image and pre-resize for faster inference
            // SCRFD input is 640x640 but larger images help detect smaller faces
            let full_path = drive_path.join(file_path);
            let image = match image::open(&full_path) {
                Ok(img) => {
                    let img = apply_exif_orientation(img, *orientation);
                    let (w, h) = (img.width(), img.height());
                    let max_dim = w.max(h);
                    if max_dim > 2048 {
                        img.resize(2048, 2048, image::imageops::FilterType::Triangle)
                    } else {
                        img
                    }
                }
                Err(e) => {
                    tracing::debug!("Failed to open image {}: {}", file_path, e);
                    // Mark as processed so we don't retry
                    let _ = face_repo.mark_photo_processed(*photo_id);
                    continue;
                }
            };

            // Detect faces
            let faces = detector.detect_adaptive(&image);

            // Clear any previous inferred identities for this photo before writing fresh results.
            let _ = inferred_repo.delete_for_photo(*photo_id);

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
                                if let Err(e) = Self::save_face_crop(aligned, &crop_path) {
                                    tracing::warn!(
                                        "Failed to save face crop for face {} ({}): {}",
                                        face_id,
                                        crop_path.display(),
                                        e
                                    );
                                }
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

            if faces.is_empty() {
                if let Some(target_ts) = taken_ts {
                    let _ = Self::propagate_identity_from_context(
                        &face_repo,
                        &inferred_repo,
                        drive_path,
                        *photo_id,
                        file_path,
                        *target_ts,
                        &image,
                    );
                }
            }
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
        let clusters_created = Self::run_clustering(&face_repo, clustering_threshold)?;

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

    fn propagate_identity_from_context(
        face_repo: &FaceRepo,
        inferred_repo: &InferredIdentityRepo,
        drive_path: &Path,
        photo_id: i64,
        file_path: &str,
        target_ts: i64,
        target_image: &image::DynamicImage,
    ) -> Result<usize, String> {
        let folder_like = std::path::Path::new(file_path).parent().map(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() {
                "%".to_string()
            } else {
                format!("{}%", s)
            }
        });

        let candidates = face_repo
            .get_contextual_cluster_candidates(
                photo_id,
                folder_like.as_deref(),
                target_ts,
                Self::CONTEXT_WINDOW_SECS,
            )
            .map_err(|e| format!("Failed to query contextual candidates: {}", e))?;

        if candidates.is_empty() {
            return Ok(0);
        }

        let target_brightness = Self::average_brightness(target_image);
        let mut best_by_cluster: HashMap<i64, (i64, f32)> = HashMap::new();

        for (source_photo_id, cluster_id, source_ts, source_file_path) in candidates {
            let delta = (source_ts - target_ts).abs() as f32;
            let temporal_score = 1.0 - (delta / Self::CONTEXT_WINDOW_SECS as f32).clamp(0.0, 1.0);
            let mut confidence = 0.5 + (temporal_score * 0.4);

            // Lightweight visual consistency: compare average brightness from current image
            // and source image if available. This reduces mistaken links across dissimilar scenes.
            if let Some(source_brightness) =
                Self::load_average_brightness_from_relative(drive_path, &source_file_path)
            {
                let diff = (target_brightness - source_brightness).abs();
                if diff < 0.12 {
                    confidence += 0.1;
                }
            }

            confidence = confidence.clamp(0.0, 1.0);

            if confidence < Self::CONTEXT_MIN_CONFIDENCE {
                continue;
            }

            match best_by_cluster.get(&cluster_id) {
                Some((_, existing)) if *existing >= confidence => {}
                _ => {
                    best_by_cluster.insert(cluster_id, (source_photo_id, confidence));
                }
            }
        }

        let mut inserted = 0usize;
        for (cluster_id, (source_photo_id, confidence)) in best_by_cluster {
            if inferred_repo
                .insert_inferred_identity(photo_id, cluster_id, source_photo_id, confidence)
                .is_ok()
            {
                inserted += 1;
            }
        }

        Ok(inserted)
    }

    fn average_brightness(image: &image::DynamicImage) -> f32 {
        let small = image
            .resize(64, 64, image::imageops::FilterType::Triangle)
            .to_rgb8();
        let mut total = 0.0f32;
        let mut count = 0.0f32;

        for p in small.pixels() {
            let [r, g, b] = p.0;
            total += (0.2126 * (r as f32) + 0.7152 * (g as f32) + 0.0722 * (b as f32)) / 255.0;
            count += 1.0;
        }

        if count == 0.0 {
            0.0
        } else {
            total / count
        }
    }

    fn load_average_brightness_from_relative(
        drive_path: &Path,
        relative_path: &str,
    ) -> Option<f32> {
        let path = drive_path.join(relative_path);
        let image = image::open(path).ok()?;
        Some(Self::average_brightness(&image))
    }

    /// Run incremental clustering in two stages:
    /// 1) Assign unclustered faces to existing clusters (high-confidence match)
    /// 2) Run DBSCAN only on still-unclustered faces to form new clusters
    fn run_clustering(face_repo: &FaceRepo, clustering_threshold: f32) -> Result<usize, String> {
        let mut assigned_to_existing = 0usize;
        let strict_max_distance = clustering_threshold.min(0.35).clamp(0.15, 0.6);

        // Stage A: assign unclustered faces to existing person gallery entries.
        let galleries = face_repo
            .get_gallery_embeddings()
            .map_err(|e| format!("Failed to load person galleries: {}", e))?;
        let cluster_photo_rows = face_repo
            .get_cluster_photo_ids()
            .map_err(|e| format!("Failed to load cluster-photo map: {}", e))?;

        let mut cluster_photo_ids: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
            std::collections::HashMap::new();
        for (cluster_id, photo_id) in cluster_photo_rows {
            cluster_photo_ids
                .entry(cluster_id)
                .or_default()
                .insert(photo_id);
        }

        let mut gallery_by_cluster: std::collections::HashMap<i64, Vec<crate::ml::FaceEmbedding>> =
            std::collections::HashMap::new();
        for g in galleries {
            gallery_by_cluster
                .entry(g.cluster_id)
                .or_default()
                .push(g.embedding);
        }

        let unclustered = face_repo
            .get_unclustered_faces_with_photo_embeddings()
            .map_err(|e| format!("Failed to get unclustered faces: {}", e))?;

        for (face_id, photo_id, embedding) in &unclustered {
            let mut best: Option<(i64, f32)> = None;
            for (cluster_id, gallery) in &gallery_by_cluster {
                if cluster_photo_ids
                    .get(cluster_id)
                    .map(|set| set.contains(photo_id))
                    .unwrap_or(false)
                {
                    // Conflict: cannot put two faces from same photo in one person cluster.
                    continue;
                }

                for sample in gallery {
                    let distance = 1.0 - embedding.cosine_similarity(sample);
                    if distance > strict_max_distance {
                        continue;
                    }
                    match best {
                        Some((_, best_dist)) if distance >= best_dist => {}
                        _ => best = Some((*cluster_id, distance)),
                    }
                }
            }

            if let Some((cluster_id, _)) = best {
                face_repo
                    .assign_face_to_cluster(*face_id, cluster_id)
                    .map_err(|e| format!("Failed to assign face to cluster: {}", e))?;
                cluster_photo_ids
                    .entry(cluster_id)
                    .or_default()
                    .insert(*photo_id);
                assigned_to_existing += 1;
            }
        }

        // Stage B: complete-link agglomerative clustering on unresolved faces.
        let unresolved = face_repo
            .get_unclustered_faces_with_photo_embeddings()
            .map_err(|e| format!("Failed to reload unresolved faces: {}", e))?;

        if unresolved.is_empty() {
            face_repo
                .refresh_all_galleries()
                .map_err(|e| format!("Failed to refresh galleries: {}", e))?;
            tracing::info!(
                "Agglomerative clustering: assigned {} faces to existing galleries; no unresolved faces left",
                assigned_to_existing
            );
            return Ok(0);
        }

        let inputs: Vec<ClusterInput> = unresolved
            .iter()
            .map(|(face_id, photo_id, emb)| ClusterInput {
                face_id: *face_id,
                photo_id: *photo_id,
                embedding: emb.clone(),
            })
            .collect();

        let clusterer = FaceClusterer::new().with_max_distance(strict_max_distance);
        let assignments = clusterer.cluster(&inputs);

        let mut cluster_groups: HashMap<i32, Vec<i64>> = HashMap::new();
        for (face_id, cluster_id) in assignments {
            if cluster_id >= 0 {
                cluster_groups.entry(cluster_id).or_default().push(face_id);
            }
        }

        let mut clusters_created = 0usize;
        for face_ids in cluster_groups.values() {
            if face_ids.len() >= 2 {
                face_repo
                    .create_cluster(face_ids)
                    .map_err(|e| format!("Failed to create cluster: {}", e))?;
                clusters_created += 1;
            }
        }

        face_repo
            .refresh_all_galleries()
            .map_err(|e| format!("Failed to refresh galleries: {}", e))?;

        tracing::info!(
            "Agglomerative clustering: assigned {} faces, created {} new clusters from {} unresolved faces",
            assigned_to_existing,
            clusters_created,
            unresolved.len()
        );

        Ok(clusters_created)
    }

    /// Save a face crop image to disk as JPEG.
    fn save_face_crop(
        aligned_face: &image::RgbImage,
        path: &Path,
    ) -> Result<(), image::ImageError> {
        let dynamic = image::DynamicImage::ImageRgb8(aligned_face.clone());
        // Resize to 80x80 for thumbnail display (the aligned face is 112x112)
        let resized = dynamic.resize_exact(80, 80, image::imageops::FilterType::Lanczos3);
        resized.save(path)
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
        for (face_id, file_path, orientation, bbox_x, bbox_y, bbox_w, bbox_h) in &all_faces {
            let crop_path = faces_dir.join(format!("{}.jpg", face_id));
            if crop_path.exists() {
                continue; // Already has crop
            }

            let full_path = drive_path.join(file_path);
            let img = match image::open(&full_path) {
                Ok(img) => apply_exif_orientation(img, *orientation),
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
