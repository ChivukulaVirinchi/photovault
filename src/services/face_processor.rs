//! Face processing pipeline
//!
//! Orchestrates the face detection -> embedding -> clustering workflow.
//! Uses rayon for parallel photo processing with thread-local ONNX sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use rayon::prelude::*;

use crate::db::face_repo::FaceRepo;
use crate::db::Database;
use crate::db::InferredIdentityRepo;
use crate::ml::{
    ClusterInput, FaceClusterer, FaceDetector, FaceEmbedder, FaceEmbedding, OnnxRuntime,
};
use crate::services::image_utils::apply_exif_orientation;

/// Coarse progress phase. The UI uses this to keep the bar moving past
/// the per-photo loop into clustering, so we never sit at "x/x" while
/// the post-processing tail runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceProcessingStage {
    /// Per-photo detect + embed loop. `processed` counts photos.
    Detecting,
    /// Stage 3 propagation + Stage 4 clustering — runs once after the
    /// per-photo loop finishes. `processed` is capped at 95% of `total`
    /// so the UI shows a "wrapping up" state instead of staying at 100%.
    Finishing,
    /// Pipeline complete. `processed == total`.
    Done,
}

/// Progress information for face processing
#[derive(Debug, Clone)]
pub struct FaceProcessingProgress {
    pub processed: usize,
    pub total: usize,
    pub faces_found: usize,
    /// Wall-clock seconds since processing started.
    pub elapsed_secs: f64,
    pub stage: FaceProcessingStage,
    /// Number of streaming flushes the writer thread has committed so
    /// far. The UI watches this and refreshes the People view when it
    /// increments — that's how new clusters appear during a long run
    /// instead of only at the end.
    pub chunks_flushed: u32,
}

impl Default for FaceProcessingProgress {
    fn default() -> Self {
        Self {
            processed: 0,
            total: 0,
            faces_found: 0,
            elapsed_secs: 0.0,
            stage: FaceProcessingStage::Detecting,
            chunks_flushed: 0,
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

/// Lightweight summary kept around after a result has been handed off
/// to the writer thread. Stage 3 (contextual identity propagation)
/// runs over these — it doesn't need the (large) face crops or
/// embeddings, just enough to look up neighbors.
struct ProcessedPhotoSummary {
    photo_id: i64,
    file_path: String,
    taken_ts: Option<i64>,
    brightness: f32,
    had_error: bool,
    has_faces: bool,
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
        // Stage flag: 0=Detecting, 1=Finishing, 2=Done. The reporter
        // thread reads this to decide whether to clamp `processed` to
        // 95% of total — that's how the bar keeps moving during the
        // clustering tail instead of stalling at 100%.
        let stage_flag = Arc::new(AtomicU8::new(0));
        // Bumped by the streaming writer after every chunk commit. The
        // UI uses this signal to refresh the People view mid-run so
        // newly-detected faces show up before the whole pipeline ends.
        let chunks_flushed = Arc::new(AtomicUsize::new(0));

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
            let stage_flag = stage_flag.clone();
            let chunks_flushed_atomic = chunks_flushed.clone();
            let start_time = std::time::Instant::now();
            std::thread::spawn(move || {
                loop {
                    let stage_raw = stage_flag.load(Ordering::Relaxed);
                    let processed_raw = processed_count.load(Ordering::Relaxed);
                    let stage = match stage_raw {
                        0 => FaceProcessingStage::Detecting,
                        1 => FaceProcessingStage::Finishing,
                        _ => FaceProcessingStage::Done,
                    };
                    // Cap at 95% during Finishing so the UI shows a
                    // moving bar with a "wrapping up" hint rather than
                    // a frozen 100%.
                    let processed = match stage {
                        FaceProcessingStage::Finishing if total > 0 => {
                            let cap = ((total as f64) * 0.95).floor() as usize;
                            processed_raw.min(cap.max(1))
                        }
                        _ => processed_raw,
                    };
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.try_send(FaceProcessingProgress {
                            processed,
                            total,
                            faces_found: faces_count.load(Ordering::Relaxed),
                            elapsed_secs: start_time.elapsed().as_secs_f64(),
                            stage,
                            chunks_flushed: chunks_flushed_atomic.load(Ordering::Relaxed) as u32,
                        });
                    }
                    // Exit only when the pipeline is fully done OR cancelled.
                    if matches!(stage, FaceProcessingStage::Done) || cancel.load(Ordering::Relaxed)
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
        };

        // ---- Streaming writer thread ----
        // Owns its own DB connection (rusqlite Connection isn't Send,
        // so we can't share `db.conn` here). Drains chunks of
        // PhotoFaceResult from the channel below, transactionally
        // writes faces + flips faces_processed, and bumps the
        // chunks_flushed atomic so the UI can refresh mid-run.
        const FLUSH_CHUNK: usize = 25;
        let (chunk_tx, chunk_rx) = mpsc::sync_channel::<Vec<PhotoFaceResult>>(4);
        let writer_handle: std::thread::JoinHandle<Result<(usize, usize), String>> = {
            let drive_path_buf = drive_path.to_path_buf();
            let faces_dir_buf = faces_dir.clone();
            let chunks_flushed = chunks_flushed.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || -> Result<(usize, usize), String> {
                let writer_db = Database::open_for_drive(&drive_path_buf)
                    .map_err(|e| format!("Failed to open writer DB: {}", e))?;
                let mut total_faces = 0usize;
                let mut photos_processed = 0usize;
                while let Ok(chunk) = chunk_rx.recv() {
                    let was_cancelled = cancel.load(Ordering::Relaxed);
                    let (faces_added, photos_added) =
                        flush_result_chunk(&writer_db, &faces_dir_buf, &chunk, was_cancelled)?;
                    total_faces += faces_added;
                    photos_processed += photos_added;
                    chunks_flushed.fetch_add(1, Ordering::Relaxed);
                }
                Ok((total_faces, photos_processed))
            })
        };

        // ---- Stage 1: Parallel Detection + Embedding (streaming) ----
        let summaries: Arc<Mutex<Vec<ProcessedPhotoSummary>>> = Arc::new(Mutex::new(Vec::new()));
        let buffer: Arc<Mutex<Vec<PhotoFaceResult>>> =
            Arc::new(Mutex::new(Vec::with_capacity(FLUSH_CHUNK)));
        let chunk_tx = Arc::new(chunk_tx);
        pool.install(|| {
            unprocessed
                .par_iter()
                .for_each(|(photo_id, file_path, orientation, taken_ts)| {
                    // Skip entirely on cancellation — don't mark this
                    // photo processed, so a future run will retry it.
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let result: PhotoFaceResult = (|| -> PhotoFaceResult {

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

                    // Cancel check: post-decode is a natural breakpoint
                    // before the (~200 ms) detector inference fires.
                    if cancel.load(Ordering::Relaxed) {
                        return PhotoFaceResult {
                            photo_id: *photo_id,
                            file_path: file_path.clone(),
                            faces: Vec::new(),
                            taken_ts: *taken_ts,
                            brightness,
                            had_error: true,
                        };
                    }

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

                    // Cancel check: post-detect, before per-face embeds.
                    if cancel.load(Ordering::Relaxed) {
                        return PhotoFaceResult {
                            photo_id: *photo_id,
                            file_path: file_path.clone(),
                            faces: Vec::new(),
                            taken_ts: *taken_ts,
                            brightness,
                            had_error: true,
                        };
                    }

                    // Embed each detected face (after quality filter)
                    let mut face_inserts = Vec::new();
                    for face in &detected {
                        // Cancel between embedding calls — group photos
                        // can have many faces and embedding is the
                        // single most expensive per-face step.
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
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
                    })();

                    // Record summary BEFORE handing the result off — the
                    // writer thread takes ownership of `result`, but
                    // Stage 3 still needs the photo metadata.
                    summaries.lock().unwrap().push(ProcessedPhotoSummary {
                        photo_id: result.photo_id,
                        file_path: result.file_path.clone(),
                        taken_ts: result.taken_ts,
                        brightness: result.brightness,
                        had_error: result.had_error,
                        has_faces: !result.faces.is_empty(),
                    });

                    // Push into the streaming buffer. When it hits
                    // FLUSH_CHUNK, ship the chunk to the writer thread
                    // so the user sees faces appear mid-run AND the
                    // photos are persisted as faces_processed=TRUE.
                    let mut buf = buffer.lock().unwrap();
                    buf.push(result);
                    if buf.len() >= FLUSH_CHUNK {
                        let chunk = std::mem::replace(&mut *buf, Vec::with_capacity(FLUSH_CHUNK));
                        drop(buf);
                        // send blocks if writer is behind — that's fine,
                        // it's natural backpressure that prevents memory
                        // blow-up on a fast detector / slow disk.
                        let _ = chunk_tx.send(chunk);
                    }
                });
        });

        // Final flush of any sub-FLUSH_CHUNK leftover photos.
        {
            let leftover = std::mem::take(&mut *buffer.lock().unwrap());
            if !leftover.is_empty() {
                let _ = chunk_tx.send(leftover);
            }
        }
        // Drop the only remaining sender — once the writer drains the
        // last chunk, recv() returns Err and the thread exits.
        drop(chunk_tx);

        // Wait for the writer to finish committing.
        let (mut total_faces, mut photos_processed) = match writer_handle.join() {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("Face writer thread panicked".to_string()),
        };
        // The writer used `mut` because Stage 3 below still adds via
        // brightness lookups; but for now the only mutation here is to
        // refresh `total_faces`/`photos_processed` for the toast text.
        // Suppress the unused-mut lint if the compiler flags it.
        let _ = (&mut total_faces, &mut photos_processed);

        // Per-photo loop is done. Move into the post-processing tail
        // (DB writes + propagation + clustering). The progress reporter
        // keeps running and emits `Finishing` events with a 95% cap so
        // the UI shows continued activity rather than stalling at x/x.
        stage_flag.store(1, Ordering::Relaxed);

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

        // ---- Stage 3: Contextual Identity Propagation ----
        // Build a brightness map from successful summaries so propagation
        // can compare lighting between target and neighbor photos
        // without reopening images. Errored photos are excluded — their
        // brightness was never measured.
        let summaries_vec = std::mem::take(&mut *summaries.lock().unwrap());
        let brightness_map: HashMap<i64, f32> = summaries_vec
            .iter()
            .filter(|s| !s.had_error)
            .map(|s| (s.photo_id, s.brightness))
            .collect();

        for summary in &summaries_vec {
            if summary.had_error || summary.has_faces {
                continue; // Only propagate for photos with no detected faces
            }
            if let Some(target_ts) = summary.taken_ts {
                let params = ContextPropagationInput {
                    drive_path,
                    photo_id: summary.photo_id,
                    file_path: &summary.file_path,
                    target_ts,
                    target_brightness: summary.brightness,
                    brightness_map: &brightness_map,
                };
                let _ = Self::propagate_identity_from_context(&face_repo, &inferred_repo, &params);
            }
        }
        let _ = was_cancelled;

        // Mid-tail progress nudge so the UI sees an immediate "Finishing"
        // tick once Stages 2-3 wrap up — the reporter would catch this on
        // the next 250 ms cycle anyway, but an explicit send eliminates
        // the visible gap before clustering kicks off.
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: photos_processed,
                total,
                faces_found: total_faces,
                elapsed_secs: pipeline_start.elapsed().as_secs_f64(),
                stage: FaceProcessingStage::Finishing,
                chunks_flushed: chunks_flushed.load(Ordering::Relaxed) as u32,
            });
        }

        // ---- Stage 4: Clustering ----
        let clusters_created =
            Self::run_clustering(&face_repo, clustering_threshold, resolver_weights)?;

        // Pipeline complete: flip stage so the reporter exits its loop
        // and emit one final 100% Done tick for the UI.
        stage_flag.store(2, Ordering::Relaxed);
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(FaceProcessingProgress {
                processed: photos_processed,
                total,
                faces_found: total_faces,
                elapsed_secs: pipeline_start.elapsed().as_secs_f64(),
                stage: FaceProcessingStage::Done,
                chunks_flushed: chunks_flushed.load(Ordering::Relaxed) as u32,
            });
        }

        // Join the reporter now that we've signalled Done.
        let _ = progress_handle.join();

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
        match image::open(&path) {
            Ok(image) => Some(Self::average_brightness(&image)),
            Err(e) => {
                tracing::trace!(
                    "context-brightness lookup failed for {}: {}",
                    path.display(),
                    e
                );
                None
            }
        }
    }
}

/// Commit one streamed chunk of per-photo results: insert detected
/// faces, mark photos `faces_processed=TRUE`, save face crops to disk.
/// On cancellation, photos with `had_error && faces.is_empty()` are
/// left as `faces_processed=FALSE` so a later run will retry them.
///
/// Returns `(faces_added, photos_added)` for stats only — durable state
/// is the SQL writes.
fn flush_result_chunk(
    db: &Database,
    faces_dir: &Path,
    chunk: &[PhotoFaceResult],
    was_cancelled: bool,
) -> Result<(usize, usize), String> {
    let mut faces_added = 0usize;
    let mut photos_added = 0usize;

    let tx = db
        .conn
        .unchecked_transaction()
        .map_err(|e| format!("Failed to begin chunk transaction: {}", e))?;

    for result in chunk {
        if result.had_error && result.faces.is_empty() {
            // Mark errored photos processed only when we're not in the
            // middle of a cancel — otherwise leave them queued for next run.
            if !was_cancelled {
                let _ = tx.execute(
                    "UPDATE photos SET faces_processed = TRUE WHERE id = ?1",
                    rusqlite::params![result.photo_id],
                );
                photos_added += 1;
            }
            continue;
        }

        let _ = tx.execute(
            "DELETE FROM photo_inferred_identities WHERE photo_id = ?1",
            rusqlite::params![result.photo_id],
        );

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
                    let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                    if let Err(e) = FaceProcessor::save_face_crop(&face.aligned_face, &crop_path) {
                        tracing::warn!("Failed to save face crop {}: {}", face_id, e);
                    }
                    faces_added += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to insert face: {}", e);
                }
            }
        }

        let _ = tx.execute(
            "UPDATE photos SET faces_processed = TRUE WHERE id = ?1",
            rusqlite::params![result.photo_id],
        );
        photos_added += 1;
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit chunk: {}", e))?;
    Ok((faces_added, photos_added))
}

mod clustering;
