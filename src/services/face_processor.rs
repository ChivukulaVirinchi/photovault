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
use crate::ml::{
    ClusterInput, FaceClusterer, FaceDetector, FaceEmbedder, FaceEmbedding, OnnxRuntime,
};
use crate::services::image_utils::apply_exif_orientation;

/// Progress information for face processing
#[derive(Debug, Clone)]
pub struct FaceProcessingProgress {
    pub processed: usize,
    pub total: usize,
    pub faces_found: usize,
    /// Wall-clock seconds since processing started.
    pub elapsed_secs: f64,
}

impl Default for FaceProcessingProgress {
    fn default() -> Self {
        Self {
            processed: 0,
            total: 0,
            faces_found: 0,
            elapsed_secs: 0.0,
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

struct ContextPropagationInput<'a> {
    drive_path: &'a Path,
    photo_id: i64,
    file_path: &'a str,
    target_ts: i64,
    target_brightness: f32,
    brightness_map: &'a HashMap<i64, f32>,
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
        resolver_weights: crate::ml::ResolverWeights,
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
            let clusters_created =
                Self::run_clustering(&face_repo, clustering_threshold, resolver_weights)?;
            return Ok(FaceProcessingResult {
                photos_processed: 0,
                faces_detected: 0,
                clusters_created,
            });
        }

        let pipeline_start = std::time::Instant::now();

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
        let num_workers = available_cpus.clamp(1, 6);
        let intra_threads = (available_cpus / num_workers).max(1);

        tracing::info!(
            "Face pipeline: {} workers, {} intra-threads per session ({} CPUs)",
            num_workers,
            intra_threads,
            available_cpus
        );

        // Build a custom rayon thread pool so we don't pollute the global pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_workers)
            .build()
            .map_err(|e| format!("Failed to create thread pool: {}", e))?;

        let processed_count = Arc::new(AtomicUsize::new(0));
        let faces_count = Arc::new(AtomicUsize::new(0));
        let rejected_small = Arc::new(AtomicUsize::new(0));
        let rejected_lowconf = Arc::new(AtomicUsize::new(0));
        let rejected_blurry = Arc::new(AtomicUsize::new(0));
        let cancel = cancel_flag
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

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
            let start_time = std::time::Instant::now();
            std::thread::spawn(move || {
                while !cancel.load(Ordering::Relaxed) {
                    let processed = processed_count.load(Ordering::Relaxed);
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.try_send(FaceProcessingProgress {
                            processed,
                            total,
                            faces_found: faces_count.load(Ordering::Relaxed),
                            elapsed_secs: start_time.elapsed().as_secs_f64(),
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

                    // Embed each detected face (after quality filter)
                    let mut face_inserts = Vec::new();
                    for face in &detected {
                        let (_bx, _by, bw, bh) = face.bbox;
                        if bw * bh < Self::MIN_FACE_AREA_PX2 {
                            rejected_small.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        if face.confidence < Self::MIN_FACE_CONFIDENCE {
                            rejected_lowconf.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

                        let aligned = match face.aligned_face.as_ref() {
                            Some(a) => a,
                            None => continue,
                        };

                        let sharpness = Self::laplacian_variance(aligned);
                        if sharpness < Self::MIN_LAPLACIAN_VAR {
                            rejected_blurry.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

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

        let rej_small = rejected_small.load(Ordering::Relaxed);
        let rej_lowconf = rejected_lowconf.load(Ordering::Relaxed);
        let rej_blurry = rejected_blurry.load(Ordering::Relaxed);
        if rej_small + rej_lowconf + rej_blurry > 0 {
            tracing::info!(
                "Quality filter: rejected {} small, {} low-confidence, {} blurry faces",
                rej_small,
                rej_lowconf,
                rej_blurry
            );
        }

        // ---- Stage 2: Batched DB Writes ----
        let mut total_faces = 0usize;
        let mut photos_processed = 0usize;
        let mut brightness_map: HashMap<i64, f32> = HashMap::new();

        // Batch in groups of 100 photos per transaction
        for chunk in results.chunks(100) {
            let tx = db
                .conn
                .unchecked_transaction()
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

            tx.commit()
                .map_err(|e| format!("Failed to commit batch: {}", e))?;
        }

        // ---- Stage 3: Contextual Identity Propagation ----
        // Uses precomputed brightness values (no image reloading)
        for result in &results {
            if result.had_error || !result.faces.is_empty() {
                continue; // Only propagate for photos with no detected faces
            }
            if let Some(target_ts) = result.taken_ts {
                let params = ContextPropagationInput {
                    drive_path,
                    photo_id: result.photo_id,
                    file_path: &result.file_path,
                    target_ts,
                    target_brightness: result.brightness,
                    brightness_map: &brightness_map,
                };
                let _ = Self::propagate_identity_from_context(&face_repo, &inferred_repo, &params);
            }
        }

        // Send final progress
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: photos_processed,
                total,
                faces_found: total_faces,
                elapsed_secs: pipeline_start.elapsed().as_secs_f64(),
            });
        }

        // ---- Stage 4: Clustering ----
        let clusters_created =
            Self::run_clustering(&face_repo, clustering_threshold, resolver_weights)?;

        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: photos_processed,
                total,
                faces_found: total_faces,
                elapsed_secs: pipeline_start.elapsed().as_secs_f64(),
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
        params: &ContextPropagationInput<'_>,
    ) -> Result<usize, String> {
        let folder_like = std::path::Path::new(params.file_path).parent().map(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() {
                "%".to_string()
            } else {
                format!("{}%", s)
            }
        });

        let candidates = face_repo
            .get_contextual_cluster_candidates(
                params.photo_id,
                folder_like.as_deref(),
                params.target_ts,
                Self::CONTEXT_WINDOW_SECS,
            )
            .map_err(|e| format!("Failed to query contextual candidates: {}", e))?;

        if candidates.is_empty() {
            return Ok(0);
        }

        let mut best_by_cluster: HashMap<i64, (i64, f32)> = HashMap::new();

        for (source_photo_id, cluster_id, source_ts, source_file_path) in candidates {
            let delta = (source_ts - params.target_ts).abs() as f32;
            let temporal_score = 1.0 - (delta / Self::CONTEXT_WINDOW_SECS as f32).clamp(0.0, 1.0);
            let mut confidence = 0.5 + (temporal_score * 0.4);

            // Use precomputed brightness if available, otherwise load from disk
            let source_brightness = params
                .brightness_map
                .get(&source_photo_id)
                .copied()
                .or_else(|| {
                    Self::load_average_brightness_from_relative(
                        params.drive_path,
                        &source_file_path,
                    )
                });

            if let Some(source_brightness) = source_brightness {
                // Smooth falloff instead of hard cutoff (Phase 2 fix included)
                let diff = (params.target_brightness - source_brightness).abs();
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
                .insert_inferred_identity(params.photo_id, cluster_id, source_photo_id, confidence)
                .is_ok()
            {
                inserted += 1;
            }
        }

        Ok(inserted)
    }

    /// Minimum bbox area (pixels^2) for a face to be worth embedding.
    /// 20x20 px = 400.
    const MIN_FACE_AREA_PX2: f32 = 400.0;

    /// Minimum detection confidence to accept a face for embedding, even if the
    /// detector's own threshold is looser.
    const MIN_FACE_CONFIDENCE: f32 = 0.3;

    /// Minimum Laplacian variance on the 112x112 aligned crop. Below this the
    /// face is too blurry to produce a reliable embedding.
    const MIN_LAPLACIAN_VAR: f32 = 10.0;

    /// Laplacian-of-gaussian-style blur measure on a 112x112 aligned crop.
    /// Higher = sharper. Very blurry faces score near 0.
    fn laplacian_variance(img: &image::RgbImage) -> f32 {
        let w = img.width() as i32;
        let h = img.height() as i32;
        if w < 3 || h < 3 {
            return 0.0;
        }

        let mut gray = vec![0.0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x as u32, y as u32).0;
                gray[(y * w + x) as usize] =
                    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            }
        }

        let mut sum = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut count = 0usize;
        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                let idx = (y * w + x) as usize;
                let center = gray[idx];
                let l = 4.0 * center
                    - gray[(y * w + (x - 1)) as usize]
                    - gray[(y * w + (x + 1)) as usize]
                    - gray[((y - 1) * w + x) as usize]
                    - gray[((y + 1) * w + x) as usize];
                sum += l;
                sum_sq += l * l;
                count += 1;
            }
        }

        let n = count as f32;
        let mean = sum / n;
        (sum_sq / n) - mean * mean
    }

    /// Build a resolver context for one face: co-occurrence counts between
    /// candidate clusters and the already-assigned clusters in this photo,
    /// plus temporal-neighbor assignments.
    fn build_resolver_context(
        face_repo: &FaceRepo,
        photo_id: i64,
        face_id: i64,
        hits: &[crate::ml::RetrievalHit],
    ) -> crate::ml::ResolverContext {
        const TEMPORAL_WINDOW_SECS: i64 = 60;

        let photo_other_clusters = face_repo
            .get_photo_other_clusters(photo_id, face_id)
            .unwrap_or_default();

        let mut cooccurrence_scores: std::collections::HashMap<i64, i64> =
            std::collections::HashMap::new();
        if !photo_other_clusters.is_empty() {
            for hit in hits {
                let mut total: i64 = 0;
                for other in &photo_other_clusters {
                    total += face_repo
                        .cooccurrence_count(hit.cluster_id, *other)
                        .unwrap_or(0);
                }
                if total > 0 {
                    cooccurrence_scores.insert(hit.cluster_id, total);
                }
            }
        }

        let temporal_neighbor_clusters: std::collections::HashSet<i64> = face_repo
            .temporal_neighbor_clusters(photo_id, TEMPORAL_WINDOW_SECS)
            .unwrap_or_default()
            .into_iter()
            .map(|(cid, _)| cid)
            .collect();

        crate::ml::ResolverContext {
            photo_other_clusters,
            cooccurrence_scores,
            temporal_neighbor_clusters,
        }
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
}

mod clustering;
