//! Face processing pipeline
//!
//! Orchestrates the face detection -> embedding -> clustering workflow.
//! Uses rayon for parallel photo processing with thread-local ONNX sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rayon::prelude::*;

use crate::db::face_repo::FaceRepo;
use crate::db::Database;
use crate::db::InferredIdentityRepo;
use crate::ml::{ClusterInput, FaceClusterer, FaceDetector, FaceEmbedder, FaceEmbedding, OnnxRuntime};
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

/// Result of processing a single photo (collected from parallel workers)
struct PhotoFaceResult {
    photo_id: i64,
    file_path: String,
    faces: Vec<FaceInsert>,
    taken_ts: Option<i64>,
    brightness: f32,
    had_error: bool,
}

/// A face ready for DB insertion
struct FaceInsert {
    bbox_normalized: (f32, f32, f32, f32),
    confidence: f32,
    embedding: FaceEmbedding,
    aligned_face: image::RgbImage,
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
    /// Uses rayon for parallel detection/embedding with thread-local ONNX sessions,
    /// then batches DB writes for efficiency.
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
            let clusters_created = Self::run_clustering(&face_repo, clustering_threshold)?;
            return Ok(FaceProcessingResult {
                photos_processed: 0,
                faces_detected: 0,
                clusters_created,
            });
        }

        // Initialize ONNX Runtime
        let runtime = OnnxRuntime::init().map_err(|e| {
            format!(
                "Failed to init ONNX Runtime: {}. Install ONNX Runtime 1.23.x and set ORT_DYLIB_PATH, or place the runtime library in libs/onnxruntime/.",
                e
            )
        })?;

        let detector_path = model_dir.join("scrfd_10g_bnkps.onnx");
        let embedder_path = model_dir.join("glintr100.onnx");

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

        // Create faces directory
        let faces_dir = drive_path.join(".photovault").join("faces");
        if let Err(e) = std::fs::create_dir_all(&faces_dir) {
            tracing::warn!("Failed to create faces directory: {}", e);
        }

        tracing::info!("Face processing: {} photos to process", total);

        // Determine parallelism
        let available_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let num_workers = available_cpus.min(6).max(1);
        let intra_threads = (available_cpus / num_workers).max(1);

        tracing::info!(
            "Face pipeline: {} workers, {} intra-threads per session ({} CPUs)",
            num_workers, intra_threads, available_cpus
        );

        // Build a custom rayon thread pool so we don't pollute the global pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_workers)
            .build()
            .map_err(|e| format!("Failed to create thread pool: {}", e))?;

        let processed_count = Arc::new(AtomicUsize::new(0));
        let faces_count = Arc::new(AtomicUsize::new(0));
        let cancel = cancel_flag.clone().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

        // Shared paths for thread-local session init
        let detector_path = Arc::new(detector_path);
        let embedder_path = Arc::new(embedder_path);
        let drive_path_arc = Arc::new(drive_path.to_path_buf());

        // Spawn a lightweight progress reporter
        let progress_handle = {
            let progress_tx = progress_tx.clone();
            let processed_count = processed_count.clone();
            let faces_count = faces_count.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                while !cancel.load(Ordering::Relaxed) {
                    let processed = processed_count.load(Ordering::Relaxed);
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.try_send(FaceProcessingProgress {
                            processed,
                            total,
                            faces_found: faces_count.load(Ordering::Relaxed),
                        });
                    }
                    if processed >= total {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
        };

        // ---- Stage 1: Parallel Detection + Embedding ----
        let results: Vec<PhotoFaceResult> = pool.install(|| {
            unprocessed
                .par_iter()
                .map(|(photo_id, file_path, orientation, taken_ts)| {
                    // Check cancellation
                    if cancel.load(Ordering::Relaxed) {
                        return PhotoFaceResult {
                            photo_id: *photo_id,
                            file_path: file_path.clone(),
                            faces: Vec::new(),
                            taken_ts: *taken_ts,
                            brightness: 0.0,
                            had_error: true,
                        };
                    }

                    // Thread-local ONNX sessions
                    thread_local! {
                        static DETECTOR: std::cell::RefCell<Option<FaceDetector>> = const { std::cell::RefCell::new(None) };
                        static EMBEDDER: std::cell::RefCell<Option<FaceEmbedder>> = const { std::cell::RefCell::new(None) };
                    }

                    // Ensure sessions are initialized for this thread
                    let det_path = detector_path.clone();
                    let emb_path = embedder_path.clone();
                    DETECTOR.with(|d| {
                        if d.borrow().is_none() {
                            match FaceDetector::new_with_threads(&runtime, det_path.as_ref(), intra_threads) {
                                Ok(det) => {
                                    *d.borrow_mut() = Some(det.with_confidence_threshold(detector_confidence));
                                }
                                Err(e) => {
                                    tracing::error!("Failed to init detector in worker: {}", e);
                                }
                            }
                        }
                    });
                    EMBEDDER.with(|e| {
                        if e.borrow().is_none() {
                            match FaceEmbedder::new_with_threads(&runtime, emb_path.as_ref(), intra_threads) {
                                Ok(emb) => {
                                    *e.borrow_mut() = Some(emb);
                                }
                                Err(e_err) => {
                                    tracing::error!("Failed to init embedder in worker: {}", e_err);
                                }
                            }
                        }
                    });

                    // Load and orient image (once!)
                    let full_path = drive_path_arc.join(file_path);
                    let image = match image::open(&full_path) {
                        Ok(img) => {
                            let img = apply_exif_orientation(img, *orientation);
                            let max_dim = img.width().max(img.height());
                            if max_dim > 2048 {
                                img.resize(2048, 2048, image::imageops::FilterType::Triangle)
                            } else {
                                img
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Failed to open image {}: {}", file_path, e);
                            processed_count.fetch_add(1, Ordering::Relaxed);
                            return PhotoFaceResult {
                                photo_id: *photo_id,
                                file_path: file_path.clone(),
                                faces: Vec::new(),
                                taken_ts: *taken_ts,
                                brightness: 0.0,
                                had_error: true,
                            };
                        }
                    };

                    // Compute brightness once (eliminates redundant image reload)
                    let brightness = Self::average_brightness(&image);

                    // Detect faces
                    let detected = DETECTOR.with(|d| {
                        let mut borrow = d.borrow_mut();
                        match borrow.as_mut() {
                            Some(det) => det.detect_adaptive(&image),
                            None => Vec::new(),
                        }
                    });

                    if !detected.is_empty() {
                        tracing::info!(
                            "Photo: {} faces detected in {}",
                            detected.len(),
                            file_path
                        );
                    }

                    // Embed each detected face
                    let mut face_inserts = Vec::new();
                    for face in &detected {
                        if let Some(ref aligned) = face.aligned_face {
                            let embedding = EMBEDDER.with(|e| {
                                let mut borrow = e.borrow_mut();
                                borrow.as_mut().and_then(|emb| emb.embed(aligned))
                            });
                            if let Some(embedding) = embedding {
                                face_inserts.push(FaceInsert {
                                    bbox_normalized: face.bbox_normalized,
                                    confidence: face.confidence,
                                    embedding,
                                    aligned_face: aligned.clone(),
                                });
                            }
                        }
                    }

                    faces_count.fetch_add(face_inserts.len(), Ordering::Relaxed);
                    processed_count.fetch_add(1, Ordering::Relaxed);

                    PhotoFaceResult {
                        photo_id: *photo_id,
                        file_path: file_path.clone(),
                        faces: face_inserts,
                        taken_ts: *taken_ts,
                        brightness,
                        had_error: false,
                    }
                })
                .collect()
        });

        // Wait for progress reporter to finish
        let _ = progress_handle.join();

        // Check if cancelled partway through
        let was_cancelled = cancel.load(Ordering::Relaxed);

        // ---- Stage 2: Batched DB Writes ----
        let mut total_faces = 0usize;
        let mut photos_processed = 0usize;
        let mut brightness_map: HashMap<i64, f32> = HashMap::new();

        // Batch in groups of 100 photos per transaction
        for chunk in results.chunks(100) {
            let tx = db.conn.unchecked_transaction()
                .map_err(|e| format!("Failed to begin transaction: {}", e))?;

            for result in chunk {
                if result.had_error && result.faces.is_empty() {
                    // Mark errored/cancelled photos as processed so we don't retry
                    let _ = tx.execute(
                        "UPDATE photos SET faces_processed = TRUE WHERE id = ?1",
                        rusqlite::params![result.photo_id],
                    );
                    if !was_cancelled {
                        photos_processed += 1;
                    }
                    continue;
                }

                // Clear previous inferred identities
                let _ = tx.execute(
                    "DELETE FROM photo_inferred_identities WHERE photo_id = ?1",
                    rusqlite::params![result.photo_id],
                );

                // Insert each detected face
                for face in &result.faces {
                    match tx.execute(
                        r#"
                        INSERT INTO faces (
                            photo_id,
                            bbox_x, bbox_y, bbox_width, bbox_height,
                            confidence, embedding
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                        "#,
                        rusqlite::params![
                            result.photo_id,
                            face.bbox_normalized.0,
                            face.bbox_normalized.1,
                            face.bbox_normalized.2,
                            face.bbox_normalized.3,
                            face.confidence,
                            face.embedding.to_bytes(),
                        ],
                    ) {
                        Ok(_) => {
                            let face_id = tx.last_insert_rowid();
                            // Save face crop
                            let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                            if let Err(e) = Self::save_face_crop(&face.aligned_face, &crop_path) {
                                tracing::warn!("Failed to save face crop {}: {}", face_id, e);
                            }
                            total_faces += 1;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to insert face: {}", e);
                        }
                    }
                }

                // Mark photo as processed
                let _ = tx.execute(
                    "UPDATE photos SET faces_processed = TRUE WHERE id = ?1",
                    rusqlite::params![result.photo_id],
                );

                brightness_map.insert(result.photo_id, result.brightness);
                photos_processed += 1;
            }

            tx.commit().map_err(|e| format!("Failed to commit batch: {}", e))?;
        }

        // ---- Stage 3: Contextual Identity Propagation ----
        // Uses precomputed brightness values (no image reloading)
        for result in &results {
            if result.had_error || !result.faces.is_empty() {
                continue; // Only propagate for photos with no detected faces
            }
            if let Some(target_ts) = result.taken_ts {
                let _ = Self::propagate_identity_from_context(
                    &face_repo,
                    &inferred_repo,
                    drive_path,
                    result.photo_id,
                    &result.file_path,
                    target_ts,
                    result.brightness,
                    &brightness_map,
                );
            }
        }

        // Send final progress
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: photos_processed,
                total,
                faces_found: total_faces,
            });
        }

        // ---- Stage 4: Clustering ----
        let clusters_created = Self::run_clustering(&face_repo, clustering_threshold)?;

        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: photos_processed,
                total,
                faces_found: total_faces,
            });
        }

        tracing::info!(
            "Face processing complete: {} photos, {} faces, {} clusters",
            photos_processed,
            total_faces,
            clusters_created
        );

        Ok(FaceProcessingResult {
            photos_processed,
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
        target_brightness: f32,
        brightness_map: &HashMap<i64, f32>,
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

        let mut best_by_cluster: HashMap<i64, (i64, f32)> = HashMap::new();

        for (source_photo_id, cluster_id, source_ts, source_file_path) in candidates {
            let delta = (source_ts - target_ts).abs() as f32;
            let temporal_score = 1.0 - (delta / Self::CONTEXT_WINDOW_SECS as f32).clamp(0.0, 1.0);
            let mut confidence = 0.5 + (temporal_score * 0.4);

            // Use precomputed brightness if available, otherwise load from disk
            let source_brightness = brightness_map
                .get(&source_photo_id)
                .copied()
                .or_else(|| Self::load_average_brightness_from_relative(drive_path, &source_file_path));

            if let Some(source_brightness) = source_brightness {
                // Smooth falloff instead of hard cutoff (Phase 2 fix included)
                let diff = (target_brightness - source_brightness).abs();
                let brightness_bonus = 0.1 * (1.0 - (diff / 0.3).clamp(0.0, 1.0));
                confidence += brightness_bonus;
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
    /// 2) Run agglomerative clustering only on still-unclustered faces
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
        let resized = dynamic.resize_exact(80, 80, image::imageops::FilterType::Lanczos3);
        resized.save(path)
    }

    /// Regenerate missing face crop files from stored bounding box data.
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
                continue;
            }

            let full_path = drive_path.join(file_path);
            let img = match image::open(&full_path) {
                Ok(img) => apply_exif_orientation(img, *orientation),
                Err(_) => continue,
            };

            let (img_w, img_h) = (img.width() as f32, img.height() as f32);

            let px = (bbox_x * img_w) as u32;
            let py = (bbox_y * img_h) as u32;
            let pw = (bbox_w * img_w) as u32;
            let ph = (bbox_h * img_h) as u32;

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
