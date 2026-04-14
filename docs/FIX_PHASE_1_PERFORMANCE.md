# Phase 1: Performance Fixes (Ship Blockers)

## Problem

Face processing pipeline for 200-300 photos takes 4-5 hours. Target: minutes.
Scanning is also single-threaded and slower than necessary.

## Root Causes

| Bottleneck | File | Line(s) | Impact |
|-----------|------|---------|--------|
| Sequential photo loop | `services/face_processor.rs` | 145 | Photos processed one-at-a-time |
| No ONNX batching/parallelism | `ml/face_detector.rs` | 211 | Batch size hardcoded to 1 |
| Single-face embedding | `ml/face_embedder.rs` | 95, 123 | One face embedded at a time |
| Hardcoded 4 ONNX threads | `ml/runtime.rs` | 121 | Underuses CPU on modern systems |
| Per-face DB inserts | `services/face_processor.rs` | 221-229 | One INSERT per face in loop |
| Per-photo DB marks | `services/face_processor.rs` | 252 | One UPDATE per photo in loop |
| Image loaded twice | `services/face_processor.rs` | 182 vs 406 | Full decode repeated for brightness |
| Sequential scan pipeline | `services/scanner.rs` | whole file | Hash + EXIF + geocode per file, serial |

## Architecture: New Parallel Face Pipeline

### Current Flow (sequential)
```
for each photo:
    load image from disk          (~100-500ms)
    detect faces (ONNX, batch=1)  (~200-400ms)
    for each face:
        embed face (ONNX, batch=1)  (~50-100ms)
        INSERT face to DB             (~1-5ms)
    mark photo processed              (~1-5ms)
```

### New Flow (parallel detection, batched writes)
```
Stage 1 — Parallel Detection (rayon thread pool, N workers):
    Each worker has its own ONNX sessions (detector + embedder)
    for each photo (parallel):
        load image
        detect faces
        embed all detected faces
        compute brightness
        -> collect PhotoFaceResult

Stage 2 — Batched DB Writes (single thread):
    for each batch of ~100 results:
        BEGIN TRANSACTION
        INSERT all faces
        mark all photos processed
        COMMIT

Stage 3 — Clustering (unchanged):
    assign to existing galleries
    agglomerative clustering on remainder
```

### Why parallel sessions, not ONNX batching?

The SCRFD and ArcFace ONNX models as exported have batch dimension fixed to 1. Supporting dynamic batch requires re-exporting the models with dynamic axes. Running N independent ONNX sessions across rayon threads is simpler, more robust, and achieves similar throughput. Each session uses fewer intra-threads to avoid CPU oversubscription.

## Detailed Changes

### 1. `src/ml/runtime.rs` — Adaptive thread count

**Current** (line 121):
```rust
.with_intra_threads(4)?
```

**New:**
```rust
pub fn load_model<P: AsRef<Path>>(&self, path: P) -> ort::Result<Session> {
    self.load_model_with_threads(path, Self::default_intra_threads())
}

pub fn load_model_with_threads<P: AsRef<Path>>(
    &self, path: P, intra_threads: usize
) -> ort::Result<Session> {
    Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(intra_threads)?
        .commit_from_file(path)
}

fn default_intra_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4)
}
```

When called from the parallel pipeline, compute: `max(1, available_cpus / num_workers)`.

### 2. `src/ml/face_detector.rs` — Change `&mut self` to `&self`

In `ort 2.0.0-rc.11`, `Session::run` takes `&self`. The `&mut self` on detection methods is unnecessarily restrictive.

- Line 79: `pub fn detect(&mut self, ...)` -> `pub fn detect(&self, ...)`
- Line 113: `pub fn detect_adaptive(&mut self, ...)` -> `pub fn detect_adaptive(&self, ...)`
- Line 207: `fn run_inference(&mut self, ...)` -> `fn run_inference(&self, ...)`

### 3. `src/ml/face_embedder.rs` — Change `&mut self` to `&self`

- Line 81: `pub fn embed(&mut self, ...)` -> `pub fn embed(&self, ...)`
- Line 122: `fn run_inference(&mut self, ...)` -> `fn run_inference(&self, ...)`

### 4. `src/services/face_processor.rs` — Parallel pipeline (major rewrite)

#### New internal types:
```rust
struct PhotoFaceResult {
    photo_id: i64,
    file_path: String,
    faces: Vec<FaceInsert>,
    taken_ts: Option<i64>,
    brightness: f32,
}

struct FaceInsert {
    bbox: (f32, f32, f32, f32),  // normalized
    confidence: f32,
    embedding: FaceEmbedding,
    aligned_face: RgbImage,       // for saving crop later
}
```

#### New `process_photos` structure:

```rust
pub fn process_photos(...) -> Result<FaceProcessingResult> {
    let runtime = OnnxRuntime::init()?;
    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get().min(6))
        .unwrap_or(4);
    let intra_threads = std::cmp::max(1, available_cpus / num_workers);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_workers)
        .build()?;

    let processed_count = AtomicUsize::new(0);

    // Stage 1: Parallel detection + embedding
    let results: Vec<PhotoFaceResult> = pool.install(|| {
        unprocessed.par_iter().filter_map(|(photo_id, file_path, orientation, taken_ts)| {
            // Check cancellation
            if cancel_flag.load(Ordering::Relaxed) { return None; }

            // Thread-local ONNX sessions
            thread_local! {
                static DETECTOR: RefCell<Option<FaceDetector>> = RefCell::new(None);
                static EMBEDDER: RefCell<Option<FaceEmbedder>> = RefCell::new(None);
            }

            // Initialize on first use in this thread
            DETECTOR.with(|d| {
                if d.borrow().is_none() {
                    *d.borrow_mut() = Some(FaceDetector::new(
                        runtime.load_model_with_threads(&detector_path, intra_threads).ok()?,
                        confidence,
                    ));
                }
            });
            // ... same for EMBEDDER

            // Load image (once!)
            let image = load_and_orient(drive_path, file_path, orientation)?;
            let brightness = average_brightness(&image);

            // Detect
            let faces = DETECTOR.with(|d| d.borrow().as_ref()?.detect_adaptive(&image));

            // Embed each face
            let face_inserts: Vec<FaceInsert> = faces.iter().filter_map(|face| {
                let embedding = EMBEDDER.with(|e| e.borrow().as_ref()?.embed(&face.aligned));
                Some(FaceInsert { ... })
            }).collect();

            // Report progress
            processed_count.fetch_add(1, Ordering::Relaxed);

            Some(PhotoFaceResult { photo_id, file_path, faces: face_inserts, taken_ts, brightness })
        }).collect()
    });

    // Stage 2: Batched DB writes
    for chunk in results.chunks(100) {
        let tx = conn.unchecked_transaction()?;
        for result in chunk {
            for face in &result.faces {
                face_repo.insert_face_tx(&tx, ...);
                save_face_crop(&faces_dir, face_id, &face.aligned_face);
            }
            face_repo.mark_photo_processed_tx(&tx, result.photo_id);
        }
        tx.commit()?;
    }

    // Stage 3: Contextual identity propagation (uses precomputed brightness)
    // Pass brightness map instead of reloading images

    // Stage 4: Clustering (unchanged)
    let clusters = Self::run_clustering(&face_repo, threshold)?;

    Ok(FaceProcessingResult { ... })
}
```

#### Eliminate redundant image loading:
- Compute brightness at line 182 (right after loading) instead of line 406
- Pass `HashMap<i64, f32>` of photo_id -> brightness to `propagate_identity_from_context`
- Delete `load_average_brightness_from_relative()` function

#### Progress reporting with parallel workers:
- Use `AtomicUsize` counter incremented by each rayon thread
- Progress sender polls this counter every 250ms from a lightweight monitoring task
- Already have `progress_tx: Option<async_channel::Sender<FaceProcessingProgress>>`

### 5. `src/db/face_repo.rs` — Add transactional insert variants

Add `_tx` variants that accept a `&Transaction` instead of using `self.conn`:

```rust
pub fn insert_face_tx(&self, tx: &Transaction, ...) -> SqliteResult<i64> { ... }
pub fn mark_photo_processed_tx(&self, tx: &Transaction, photo_id: i64) -> SqliteResult<()> { ... }
```

These are used by the batched write stage. Existing non-tx methods remain for backward compat.

### 6. `src/services/scanner.rs` — Parallel hash + EXIF

```rust
pub fn scan_directory(...) -> Result<ScanResult> {
    // Phase 1: Collect candidate files (serial walkdir, fast)
    let mut candidates = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_entry(|e| ...) {
        // existing filtering: extension check, size check, skip patterns
        candidates.push((path, relative_path, file_size, mtime));
    }

    total_found = candidates.len();
    send_progress(total_found, 0);

    // Phase 2: Parallel hash + EXIF extraction (rayon)
    let processed: Vec<PhotoCandidate> = candidates
        .par_iter()
        .filter_map(|(path, rel, size, mtime)| {
            if cancel_flag.load(Relaxed) { return None; }
            let hash = calculate_hash(path).ok()?;
            let exif = ExifExtractor::extract(path);
            counter.fetch_add(1, Relaxed);
            Some(PhotoCandidate { rel, hash, exif, size, mtime })
        })
        .collect();

    // Phase 3: Serial geocoding + batched DB insert
    for chunk in processed.chunks(DB_BATCH_SIZE) {
        for item in chunk {
            if let Some(gps) = item.exif.gps() {
                geocode(gps); // ~1ms each, serial is fine
            }
        }
        repo.insert_batch(chunk)?;
    }
}
```

### 7. `Cargo.toml` — No new dependencies needed

`rayon` is already a dependency. `std::thread::available_parallelism()` is stable since Rust 1.59 (no new crate needed for CPU count).

## Order of Implementation

1. `&mut self` -> `&self` on detector/embedder (safe, no behavioral change)
2. Adaptive thread count in `runtime.rs`
3. Add `_tx` variants to `face_repo.rs`
4. Rewrite `face_processor.rs` with parallel pipeline
5. Parallelize `scanner.rs`
6. Integration test end-to-end

## Verification

| Test | Method | Target |
|------|--------|--------|
| Thread count | Unit test `default_intra_threads()` returns 1..=8 | Pass |
| Face pipeline speed | 200 photos end-to-end | < 5 minutes (from 4-5 hours) |
| Scanner speed | 1000 files directory scan | Near-linear speedup with cores |
| Correctness | Same test set, compare face count + cluster assignments before/after | Identical |
| Memory | Monitor RSS during 300 photo processing | < 2GB |
| Cancellation | Cancel mid-processing, verify partial results saved | Graceful stop |

## Expected Improvement

| Component | Before | After | Speedup |
|-----------|--------|-------|---------|
| Face detection | ~300ms/photo, sequential | ~300ms/photo, 6 parallel | ~6x |
| Face embedding | ~75ms/face, sequential | ~75ms/face, 6 parallel | ~6x |
| DB writes | ~5ms/face, per-face | ~0.05ms/face, batched | ~100x |
| Image loading | 2x per photo | 1x per photo | 2x |
| ONNX threads | 4 fixed | adaptive per-system | 1.5-2x |
| Scanning | sequential hash+EXIF | parallel hash+EXIF | ~4-6x |
| **Overall face pipeline** | **4-5 hours / 300 photos** | **2-5 minutes / 300 photos** | **~50-100x** |
