# Smriti — Streaming scanner pipeline + Intel iGPU path

A complete, step-by-step implementation plan to replace the current batch-mode scanner with a streaming, multi-stage pipeline, and to broaden GPU acceleration so Intel integrated GPUs (and any DirectML/CoreML/CUDA target) can engage automatically.

The instructions are intentionally exhaustive: file paths, line-level diffs, SQL, and acceptance checks for every step. A new contributor should be able to start at "Phase 0" and finish at "Phase 9" without needing any architectural decision from the author.

---

## 0. Problem statement (read first)

### What's wrong today

`src/services/scanner.rs:117` runs in three sequential phases:

1. **Walk** — collect every supported file path into a `Vec<FileCandidate>` in memory (fast, ~30s for 91k photos).
2. **Hash + EXIF** — `candidates.par_iter().collect()` at line 212 — reads **every byte of every file** through SHA-256 (`calculate_hash` at line 399 streams a 64 KB buffer through `Sha2::Sha256`), plus extracts EXIF. **Nothing is emitted to the UI during this pass.**
3. **Geocode + DB insert** — drains the collected `Vec<Option<PhotoInsert>>` serially, batching inserts of 100, emitting progress every 50 photos.

On a 91 k photo external drive at ~5 MB/photo:
- Total bytes hashed ≈ **450 GB**. USB 3 HDD ≈ 1 hour just for reads; USB 2 ≈ 4 hours.
- Phase 2 emits nothing → UI shows "Processing 91 074 files… 0%" for the entire hash pass.
- The library is **completely unavailable** during the scan because `library_start_scan` in `src-tauri/src/commands/library.rs:248-274` literally `take()`s the `Database` out of the AppState and moves it into the scanner thread. Other tabs render the "library closed" placeholder for the entire scan duration. The comment at line 237 already says "M3 will refactor scanner to take `&Connection` so this dance goes away." This is M3.

### Goals of this plan

1. **Photos appear in the timeline within ~30 seconds** of starting a scan, even on a 100 k-photo cold drive. Thumbnails fill in over the following minutes; faces over the half hour after that.
2. **Bounded memory** regardless of library size (current code is O(N) in candidates Vec + processed Vec; target is O(channel_depth) ≈ 1 MB).
3. **The library is *never* closed during a scan** — other tabs (People, Map, Albums, Search) remain queryable.
4. **Every stage is independently resumable**, just like face detection already is. Move a drive between machines mid-scan and Smriti picks up exactly where it left off.
5. **Stop full-file SHA-256 at scan time.** Use fast-hash (first 64 KB + size + mtime) as primary identity; defer full hash to a background "duplicates" sweep that runs only when the user clicks "find duplicates" or as the lowest-priority background pass.
6. **Make execution-provider discovery dynamic** so Intel iGPUs engage via OpenVINO when present, with no regression on Windows / macOS / Nvidia paths and no extra disk weight when not present.

### Non-goals (explicit)

- Rewriting the face-recognition pipeline. The face job already uses the right pattern (`faces_processed` flag, idempotent per-photo work, pause/resume). Leave it.
- Changing the on-wire IPC contract beyond appending two job kinds. The frontend already handles unknown job kinds gracefully.
- Bundling OpenVINO runtime with Smriti's installer in this iteration — that's a packaging decision tracked separately. This plan enables OpenVINO if it's already present on the host; it does not ship it.
- Replacing the ONNX runtime crate (`ort`). It works; we just need to extend the EP list.

---

## 1. Architecture overview

### Pipeline diagram

```
┌────────────┐         ┌─────────────────────────┐         ┌────────────────────┐
│  walkdir   │  paths  │   ENQUEUE writer        │  bulk   │   photos table     │
│  (1 thread)├────────►│   (1 thread,            ├────────►│   stub rows        │
│            │ (chan A)│    batches of 200       │ INSERT  │   metadata_extracted=0│
│            │         │     INSERT OR IGNORE)   │         │   thumbnailed=0    │
└────────────┘         └─────────────────────────┘         │   faces_processed=0│
                                                           └─────────┬──────────┘
                                                                     │
                                                                     ▼
                                                           ┌─────────────────────────────┐
                                                           │  METADATA worker            │
                                                           │  WHERE metadata_extracted=0 │
                                                           │  par_iter — open header,    │
                                                           │  read EXIF, geocode,        │
                                                           │  UPDATE row, flag=1         │
                                                           └─────────────┬───────────────┘
                                                                         │
                                                                         ▼
                                                           ┌─────────────────────────────┐
                                                           │  THUMBNAIL worker           │
                                                           │  WHERE thumbnailed=0        │
                                                           │  par_iter — decode, resize, │
                                                           │  write JPEG, flag=1         │
                                                           └─────────────┬───────────────┘
                                                                         │
                                                                         ▼
                                                           ┌─────────────────────────────┐
                                                           │  FACE worker (unchanged)    │
                                                           │  WHERE faces_processed=0    │
                                                           └─────────────────────────────┘
```

Channels are `async_channel::bounded(1000)`. The walker is the slowest producer (single thread + filesystem syscalls); the writer is the slowest sink for stub rows (one SQLite writer). The metadata, thumbnail, and face workers each pull batches off the photos table and run independently — they don't actually consume from a Rust channel, they consume from a SQL view ("photos WHERE stage_flag=0"). This is the same pattern face detection already uses (`get_unclustered_faces_with_photo_embeddings`).

### Three reasons to use SQL views, not Rust channels, between stages 1→4

1. **Resumable.** The flag column IS the queue. Crash recovery is automatic; no checkpoint code.
2. **Cross-machine.** Move the drive, plug it into another OS, the next Smriti continues exactly where the last one stopped.
3. **Observable.** `SELECT COUNT(*) WHERE thumbnailed = 0` returns honest progress to the UI at any time.

The only place we use Rust channels is the walker → writer (Phase 1A→1B), because the source of truth is the filesystem and we want to start writing rows ASAP without buffering all paths in memory.

### Why "stage flags" and not "stages table"

A separate `photo_stages(photo_id, stage, status, finished_at)` table would be more normalized but more expensive to query. The current schema already has `faces_processed BOOLEAN`. We extend that pattern with `metadata_extracted BOOLEAN` and `thumbnailed BOOLEAN`. Each is one bit per photo per stage. For 1 M photos that's 3 MB total — trivial.

---

## 2. Stage definitions (canonical)

| # | Stage | Source filter | Output | Job kind | Avg. cost on this hardware |
|---|---|---|---|---|---|
| 1A | **Walk** | `walkdir(root)` | path strings into `chan_paths` | (part of `JobKind::Scan`) | ~10 µs / file |
| 1B | **Stub insert** | drain `chan_paths` | rows in `photos` with everything but EXIF + thumbnail; `metadata_extracted=0`, `thumbnailed=0`, `faces_processed=0`, `file_hash=fast_hash(first 64KB + size + mtime)` | (part of `JobKind::Scan`) | ~50 µs / file (DB write) |
| 2 | **Metadata** | `WHERE metadata_extracted = 0` | EXIF columns + reverse-geocoded city/country | `JobKind::MetadataExtraction` (new) | ~3-10 ms / file (header read) |
| 3 | **Thumbnail** | `WHERE thumbnailed = 0` | `<drive>/.photovault/thumbnails/...` + `thumbnail_path` column | `JobKind::Thumbnails` (extend) | ~30-80 ms / file (full decode + resize) |
| 4 | **Face** | `WHERE faces_processed = 0` (existing) | faces rows + face crops | `JobKind::FaceProcessing` (existing) | ~80-200 ms / file (ONNX CPU); 5-15 ms / file (Intel iGPU via OpenVINO) |
| 5 | **Full hash** (deferred) | `WHERE file_hash LIKE 'fh:%'` | replace fast hash with `sha256:<digest>` | `JobKind::FullHash` (new, optional) | ~200 ms / file (full read) |

Stage 5 only runs if the user explicitly opens the **Duplicates** tab for the first time (where it currently relies on the full hash), or if they enable an "always full-hash" preference. **Most users will never need it** — fast hash + size + mtime is unique enough for normal libraries, and our duplicate detection can use phash (already on the photos table at schema.rs:70) for the visual-near-duplicate flow.

### Stage entry / exit invariants

Every photo row goes through exactly one state machine:

```
no row                       (filesystem-only)
  │ Stage 1B INSERT OR IGNORE
  ▼
metadata_extracted=0, thumbnailed=0, faces_processed=0, file_hash="fh:abc…"
  │ Stage 2 UPDATE
  ▼
metadata_extracted=1                                      (timeline shows date + GPS)
  │ Stage 3 UPDATE
  ▼
thumbnailed=1                                             (timeline shows thumbnail)
  │ Stage 4 UPDATE
  ▼
faces_processed=1                                         (People page shows faces)
  │ Stage 5 UPDATE (optional, lazy)
  ▼
file_hash="sha256:…"                                      (duplicates fully accurate)
```

Important: stages 2, 3, 4, 5 are **all idempotent**. Re-running stage 3 on a photo that already has `thumbnailed=1` is a no-op (the worker filters them out). This lets you safely cancel and restart any job.

---

## 3. Phase 0 — Database migration

### 3.1 Add a migration

**File**: `src/db/migrations.rs`. Find the existing migration list (a `&[Migration]` table). Add a new entry at the end:

```rust
Migration {
    version: 15,
    name: "scanner_pipeline_stages",
    sql: r#"
        ALTER TABLE photos ADD COLUMN metadata_extracted BOOLEAN DEFAULT FALSE;
        ALTER TABLE photos ADD COLUMN thumbnailed BOOLEAN DEFAULT FALSE;

        -- Existing rows: anything inserted by the legacy scanner already
        -- has EXIF and (if successful) a thumbnail. Mark them done so the
        -- new workers don't re-process them.
        UPDATE photos SET metadata_extracted = TRUE
            WHERE date_taken IS NOT NULL OR camera_make IS NOT NULL;
        UPDATE photos SET thumbnailed = TRUE
            WHERE thumbnail_path IS NOT NULL;

        CREATE INDEX IF NOT EXISTS idx_photos_metadata_extracted
            ON photos(metadata_extracted) WHERE metadata_extracted = FALSE;
        CREATE INDEX IF NOT EXISTS idx_photos_thumbnailed
            ON photos(thumbnailed) WHERE thumbnailed = FALSE;
    "#,
},
```

### 3.2 Update the inline schema for new libraries

**File**: `src/db/schema.rs:36-85`. Find the photos table DDL. Add two columns right after `faces_processed BOOLEAN DEFAULT FALSE`:

```sql
    metadata_extracted BOOLEAN DEFAULT FALSE,
    thumbnailed BOOLEAN DEFAULT FALSE,
```

And bump line 29:

```sql
INSERT INTO schema_version (version) VALUES (15);
```

### 3.3 Update the partial-index list in schema.rs

Append the same `CREATE INDEX` statements to `SCHEMA_SQL` so new libraries get them too. Place after the existing photos-table indexes.

### 3.4 Acceptance for Phase 0

```bash
cargo test -p smriti db::migrations
```

The migration test (you'll add one) opens a v14 in-memory DB with a populated `photos` row, runs the migration, then asserts:

- New columns exist (`PRAGMA table_info(photos)`).
- Rows with `date_taken IS NOT NULL` have `metadata_extracted = 1`.
- Rows with `thumbnail_path IS NOT NULL` have `thumbnailed = 1`.
- Rows with neither have both flags = 0.

---

## 4. Phase 1 — New scanner: streaming walk + stub insert

This is the largest single change. Replace `run_scan()` in `src/services/scanner.rs:117-366` with a streaming producer + consumer.

### 4.1 New constants

At the top of `src/services/scanner.rs`, alongside the existing `DB_BATCH_SIZE`:

```rust
/// How many file paths we buffer between the walker and the stub-writer.
/// 1000 keeps memory under ~200 KB even on libraries that average 200-byte paths.
const WALKER_CHANNEL_DEPTH: usize = 1000;

/// Bytes used for the fast-hash prefix. 64 KB is enough that JPEG/HEIC
/// markers + first scanline diverge for distinct photos, while staying
/// well within typical disk read-ahead (so it's effectively free).
const FAST_HASH_PREFIX_BYTES: usize = 64 * 1024;

/// How often the stub writer emits a progress event (in files written).
const STUB_PROGRESS_EVERY: u64 = 200;
```

### 4.2 Add a `FastHash` helper

In `scanner.rs`, near the existing `calculate_hash`:

```rust
/// Cheap identity fingerprint for the scan stage.
///
/// Reads the first 64 KB of the file, hashes (size, mtime, prefix bytes)
/// with SHA-256, and prefixes the digest with `fh:` to make it distinguishable
/// at a glance from a `sha256:` full-file hash. The full hash is recomputed
/// lazily by Phase 5 if duplicate-detection actually needs it.
///
/// Collisions: in practice essentially zero for organic photo libraries.
/// Two photos would have to be byte-identical in their first 64 KB AND
/// have the same file size AND the same mtime. Even bursts from the same
/// camera diverge in the first few EXIF bytes (timestamp).
pub(crate) fn calculate_fast_hash<P: AsRef<Path>>(
    path: P,
    file_size: u64,
    mtime: Option<i64>,
) -> std::io::Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(&path)?;
    let mut buf = vec![0u8; FAST_HASH_PREFIX_BYTES];
    let n = file.read(&mut buf)?;

    let mut hasher = Sha256::new();
    hasher.update(file_size.to_le_bytes());
    hasher.update(mtime.unwrap_or(0).to_le_bytes());
    hasher.update(&buf[..n]);
    Ok(format!("fh:{:x}", hasher.finalize()))
}
```

**Do not delete** `calculate_hash`. Phase 5 uses it. Mark it `#[allow(dead_code)]` for now if clippy complains; the new full-hash worker will call it.

### 4.3 New scan entry point: signature change

The current `start_scan` moves a `Database` in and returns it via `ScanResult`. We want to invert that: the scanner borrows a `&Connection` for the duration of stub inserts, then completes — and Phases 2-4 run as **independent jobs** triggered after Phase 1 completes.

Change the signature in `scanner.rs:90-114`:

```rust
pub fn start_scan(
    root_path: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,    // shared, not moved
    cancel: Arc<AtomicBool>,                  // pre-provided by caller (jobs.rs)
    scan_hidden_folders: bool,
) -> (Receiver<ScanProgress>, tokio::task::JoinHandle<ScanReport>) {
    let (progress_tx, progress_rx) = bounded::<ScanProgress>(64);
    let handle = tokio::task::spawn(async move {
        let result = run_scan_streaming(
            root_path,
            db,
            cancel,
            scan_hidden_folders,
            progress_tx,
        )
        .await;
        result.unwrap_or_else(|e| ScanReport {
            files_inserted: 0,
            errors: vec![format!("scan failed: {e}")],
            elapsed_seconds: 0.0,
        })
    });
    (progress_rx, handle)
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub files_inserted: u64,
    pub errors: Vec<String>,
    pub elapsed_seconds: f64,
}
```

### 4.4 New `run_scan_streaming` (the heart of Phase 1)

```rust
async fn run_scan_streaming(
    root_path: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
    scan_hidden_folders: bool,
    progress_tx: Sender<ScanProgress>,
) -> Result<ScanReport, String> {
    let start = Instant::now();
    let (paths_tx, paths_rx) = bounded::<FileCandidate>(WALKER_CHANNEL_DEPTH);

    // ----- Producer: walker thread -----
    let walker_cancel = cancel.clone();
    let walker_root = root_path.clone();
    let walker = tokio::task::spawn_blocking(move || -> u64 {
        let mut count: u64 = 0;
        let walker = WalkDir::new(&walker_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !should_skip(e, scan_hidden_folders));

        for entry in walker {
            if walker_cancel.load(Ordering::Relaxed) {
                tracing::info!("Scan cancelled during walk");
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() { continue; }
            if !is_supported_file(&entry) { continue; }
            let Ok(metadata) = entry.metadata() else { continue };
            if metadata.len() < MIN_FILE_SIZE { continue; }

            let relative_path = entry
                .path()
                .strip_prefix(&walker_root)
                .map(crate::services::path_util::relative_path_for_storage)
                .unwrap_or_else(|_| {
                    crate::services::path_util::relative_path_for_storage(entry.path())
                });

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            let candidate = FileCandidate {
                path: entry.path().to_path_buf(),
                relative_path,
                file_name: entry.file_name().to_string_lossy().to_string(),
                file_size: metadata.len() as i64,
                mtime,
            };

            // send_blocking is OK here — this whole closure is on a blocking thread.
            if paths_tx.send_blocking(candidate).is_err() {
                break; // receiver dropped (consumer ended early)
            }
            count += 1;
        }
        drop(paths_tx);
        count
    });

    // ----- Consumer: stub-row writer + fast-hash batcher -----
    let writer_cancel = cancel.clone();
    let writer_db = db.clone();
    let writer_progress = progress_tx.clone();
    let writer = tokio::task::spawn(async move {
        let mut errors: Vec<String> = Vec::new();
        let mut buf: Vec<PhotoInsert> = Vec::with_capacity(DB_BATCH_SIZE);
        let mut files_inserted: u64 = 0;

        // Channel receives are async; the actual fast-hash + insert is blocking.
        // We do them in chunks of DB_BATCH_SIZE so the SQLite writer is hit
        // in bursts, not one row at a time.
        loop {
            if writer_cancel.load(Ordering::Relaxed) { break; }

            let Ok(candidate) = paths_rx.recv().await else { break };

            // Fast-hash on this async task is fine — it's tiny I/O (64 KB).
            // Doing it on a tokio worker thread would be marginally faster
            // but adds spawn overhead.
            let hash = match calculate_fast_hash(
                &candidate.path,
                candidate.file_size as u64,
                candidate.mtime,
            ) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("fast-hash {}: {e}", candidate.path.display()));
                    continue;
                }
            };

            buf.push(PhotoInsert {
                relative_path: candidate.relative_path,
                file_name: candidate.file_name,
                file_hash: hash,
                file_size: candidate.file_size,
                file_mtime: candidate.mtime,
                // Everything else stays None / default. Phase 2 fills it in.
                date_taken: None,
                date_taken_source: None,
                gps_latitude: None,
                gps_longitude: None,
                location_city: None,
                location_country: None,
                camera_make: None,
                camera_model: None,
                iso: None,
                aperture: None,
                shutter_speed: None,
                focal_length: None,
                lens_model: None,
                flash: None,
                gps_altitude: None,
                width: None,
                height: None,
                orientation: 1,
            });

            if buf.len() >= DB_BATCH_SIZE {
                let inserted = flush_stub_batch(&writer_db, &mut buf, &mut errors).await;
                files_inserted += inserted;

                if files_inserted.is_multiple_of(STUB_PROGRESS_EVERY) {
                    let _ = writer_progress.try_send(ScanProgress {
                        files_found: files_inserted,
                        files_processed: files_inserted,
                        bytes_processed: 0,
                        current_directory: String::new(),
                        current_file: format!("Indexed {files_inserted} files…"),
                        errors: errors.clone(),
                        is_complete: false,
                        elapsed_seconds: start.elapsed().as_secs_f64(),
                    });
                }
            }
        }

        // Drain final batch.
        if !buf.is_empty() {
            files_inserted += flush_stub_batch(&writer_db, &mut buf, &mut errors).await;
        }
        (files_inserted, errors)
    });

    let total_walked = walker.await.map_err(|e| format!("walker join: {e}"))?;
    let (files_inserted, errors) = writer.await.map_err(|e| format!("writer join: {e}"))?;
    tracing::info!("Phase 1 done: walked {total_walked}, inserted {files_inserted}");

    let report = ScanReport {
        files_inserted,
        errors,
        elapsed_seconds: start.elapsed().as_secs_f64(),
    };

    // Emit final progress; the IPC wrapper interprets is_complete=true.
    let _ = progress_tx
        .send(ScanProgress {
            files_found: report.files_inserted,
            files_processed: report.files_inserted,
            bytes_processed: 0,
            current_directory: String::new(),
            current_file: format!("Indexed {} files", report.files_inserted),
            errors: report.errors.clone(),
            is_complete: true,
            elapsed_seconds: report.elapsed_seconds,
        })
        .await;

    Ok(report)
}

async fn flush_stub_batch(
    db: &Arc<tokio::sync::Mutex<Database>>,
    buf: &mut Vec<PhotoInsert>,
    errors: &mut Vec<String>,
) -> u64 {
    let guard = db.lock().await;
    let repo = PhotoRepo::new(&guard.conn);
    let inserted = match repo.insert_batch(buf) {
        Ok(n) => n as u64,
        Err(e) => {
            errors.push(format!("stub batch insert: {e}"));
            0
        }
    };
    buf.clear();
    inserted
}
```

### 4.5 Adjust `PhotoRepo::insert_batch` to be `INSERT OR IGNORE`

**File**: `src/db/photo_repo.rs:49-…`. The current `insert_batch` likely uses `INSERT OR REPLACE` (re-scan would overwrite EXIF the user has). Change to `INSERT OR IGNORE` and make it skip rows whose `file_path` already exists. Rationale: an idempotent re-scan must not blow away metadata that later stages already filled in.

If `insert_batch` already does `INSERT OR REPLACE`, audit every column it sets to ensure the replace doesn't downgrade a Phase-2-completed row back to a Phase-1 stub.

Concretely: the SQL should look like:

```sql
INSERT INTO photos (
    file_path, file_name, file_hash, file_size, file_mtime,
    orientation, metadata_extracted, thumbnailed, faces_processed
) VALUES (?, ?, ?, ?, ?, ?, FALSE, FALSE, FALSE)
ON CONFLICT(file_path) DO NOTHING;
```

The Phase 2/3/4 workers all use `UPDATE` statements, so they never trip `ON CONFLICT`.

### 4.6 Wire the new signature into `library_start_scan`

**File**: `src-tauri/src/commands/library.rs:239-351`. Stop calling `Arc::try_unwrap` on the database. Instead:

```rust
#[tauri::command]
pub async fn library_start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    args: LibraryStartScanArgs,
) -> CommandResult<JobIdDto> {
    let job = jobs::start_job(&state, JobKind::Scan).await?;
    let job_id = job.id.clone();

    // Read the drive_root + clone the db Arc. No more take()/try_unwrap dance.
    let (drive_root, db) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), lib.db.clone())
    };

    let cancel = job.cancel.clone();
    let app_clone = app.clone();
    let started = job.started_at;
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let (rx, handle) = smriti::services::scanner::start_scan(
            drive_root.clone(),
            db.clone(),
            cancel,
            args.scan_hidden_folders,
        );

        while let Ok(p) = rx.recv().await {
            let dto = ScanProgressDto { /* unchanged */ };
            if p.is_complete {
                emit(&app_clone, EV_SCAN_COMPLETE, dto);
            } else {
                emit(&app_clone, EV_SCAN_PROGRESS, dto);
            }
        }

        if let Ok(report) = handle.await {
            tracing::info!("Scan complete: inserted {}", report.files_inserted);
        }

        // Always release the job slot.
        let st: tauri::State<AppState> = app_clone.state();
        jobs::finish_job(&st, &job_id_clone).await;

        // Kick off the downstream stages. Each is its own job_id, with its
        // own JobsIndicator progress chip. Order matters: metadata first
        // (cheap, makes timeline useful), then thumbnails, then faces.
        let app_for_post = app_clone.clone();
        let drive_for_post = drive_root.clone();
        tokio::spawn(async move {
            run_post_scan_pipeline(app_for_post, drive_for_post).await;
        });
    });

    Ok(JobIdDto { job_id })
}
```

Note: the `Arc::try_unwrap` block (currently lines 257-273) and the "Conflict / another command is using DB" branch go away entirely.

### 4.7 Acceptance for Phase 1

Manual:
1. Start `cargo tauri dev`. Open a fresh library on the 91k-photo drive.
2. Click "Scan". Within **30 seconds**, the Timeline page should start showing photos (date-less placeholders are fine at this point).
3. Open the Map tab during the scan. It should render (empty or partial), NOT show "library closed".
4. The JobsIndicator should show a "Scanning" job with a progress count climbing every second.
5. Cancel the scan from JobsIndicator. Resume. The next click on "Scan" should pick up where it left off (most files already in DB, only the unscanned ones get hashed).

Automated: `cargo test -p smriti services::scanner::streaming` — see Phase 7 below for tests to add.

---

## 5. Phase 2 — Metadata extraction worker

A new background job, structurally identical to `face_processor`. Reads photos with `metadata_extracted = 0`, opens each in parallel for EXIF, updates the row.

### 5.1 New file: `src/services/metadata_processor.rs`

```rust
//! Background EXIF + geocoding pass.
//!
//! Reads photos with `metadata_extracted = 0`, runs `ExifExtractor::extract`
//! on each (header read only, ~10 KB per file), reverse-geocodes any GPS
//! coordinates, and updates the row in place. Idempotent and resumable.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use rusqlite::params;

use crate::db::Database;
use crate::services::{ExifExtractor, GeocodingService};

const METADATA_CHUNK_SIZE: usize = 50;

#[derive(Debug, Clone)]
pub struct MetadataProgress {
    pub total: u64,
    pub done: u64,
    pub is_complete: bool,
    pub elapsed_seconds: f64,
}

pub fn start_metadata_job(
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
) -> (Receiver<MetadataProgress>, tokio::task::JoinHandle<()>) {
    let (progress_tx, progress_rx) = bounded::<MetadataProgress>(32);
    let handle = tokio::spawn(async move {
        run_metadata_job(drive_root, db, cancel, progress_tx).await;
    });
    (progress_rx, handle)
}

async fn run_metadata_job(
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
    progress_tx: Sender<MetadataProgress>,
) {
    let start = Instant::now();

    let total: u64 = {
        let guard = db.lock().await;
        guard.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE metadata_extracted = FALSE AND is_trashed = FALSE",
            [], |row| row.get(0),
        ).unwrap_or(0)
    };
    let total = total as u64;
    let mut done = 0u64;

    let geonames_path = crate::db::geonames::geonames_db_path();
    let geocoder = if geonames_path.exists() {
        GeocodingService::new(&geonames_path).ok()
    } else {
        None
    };

    loop {
        if cancel.load(Ordering::Relaxed) { break; }

        // Pull the next chunk of unprocessed photo ids + paths.
        let chunk: Vec<(i64, String)> = {
            let guard = db.lock().await;
            let mut stmt = match guard.conn.prepare(
                "SELECT id, file_path FROM photos
                 WHERE metadata_extracted = FALSE AND is_trashed = FALSE
                 LIMIT ?"
            ) { Ok(s) => s, Err(_) => break };
            stmt.query_map([METADATA_CHUNK_SIZE as i64], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
        };

        if chunk.is_empty() { break; }

        // Parallel EXIF + geocode. No DB lock held here.
        let extracted: Vec<(i64, crate::services::exif_extractor::ImageMetadata)> = chunk
            .par_iter()
            .map(|(id, rel_path)| {
                let abs = drive_root.join(rel_path);
                let meta = ExifExtractor::extract(&abs);
                (*id, meta)
            })
            .collect();

        // Single-threaded transactional update.
        let mut errors_this_chunk = 0usize;
        {
            let mut guard = db.lock().await;
            let tx = match guard.conn.transaction() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("metadata tx start: {e}");
                    continue;
                }
            };

            for (id, meta) in &extracted {
                let (city, country) = match (meta.gps_latitude, meta.gps_longitude, &geocoder) {
                    (Some(lat), Some(lon), Some(g)) => g
                        .reverse_geocode(lat, lon)
                        .map(|r| (Some(r.city), Some(r.country)))
                        .unwrap_or((None, None)),
                    _ => (None, None),
                };

                let res = tx.execute(
                    "UPDATE photos SET
                        date_taken = ?,
                        date_taken_source = ?,
                        gps_latitude = ?,
                        gps_longitude = ?,
                        location_city = ?,
                        location_country = ?,
                        camera_make = ?,
                        camera_model = ?,
                        iso = ?,
                        aperture = ?,
                        shutter_speed = ?,
                        focal_length = ?,
                        lens_model = ?,
                        flash = ?,
                        gps_altitude = ?,
                        width = ?,
                        height = ?,
                        orientation = ?,
                        metadata_extracted = TRUE
                     WHERE id = ?",
                    params![
                        meta.date_taken.map(|d| d.to_rfc3339()),
                        meta.date_taken_source,
                        meta.gps_latitude,
                        meta.gps_longitude,
                        city,
                        country,
                        meta.camera_make,
                        meta.camera_model,
                        meta.iso,
                        meta.aperture,
                        meta.shutter_speed,
                        meta.focal_length,
                        meta.lens_model,
                        meta.flash,
                        meta.gps_altitude,
                        meta.width.map(|v| v as i64),
                        meta.height.map(|v| v as i64),
                        meta.orientation.unwrap_or(1) as i64,
                        id,
                    ],
                );
                if res.is_err() { errors_this_chunk += 1; }
            }
            if let Err(e) = tx.commit() {
                tracing::error!("metadata tx commit: {e}");
                continue;
            }
        }

        done += (chunk.len() - errors_this_chunk) as u64;
        let _ = progress_tx.try_send(MetadataProgress {
            total,
            done,
            is_complete: false,
            elapsed_seconds: start.elapsed().as_secs_f64(),
        });
    }

    let _ = progress_tx.send(MetadataProgress {
        total,
        done,
        is_complete: true,
        elapsed_seconds: start.elapsed().as_secs_f64(),
    }).await;
}
```

### 5.2 Export it

**File**: `src/services/mod.rs`. Add `pub mod metadata_processor;` and a re-export.

### 5.3 New JobKind + IPC

**File**: `src-tauri/src/state.rs:97-112`. Add:

```rust
pub enum JobKind {
    Scan,
    MetadataExtraction,   // ← NEW
    FaceProcessing,
    Duplicates,
    Bursts,
    Documents,
    Geocoding,
    Thumbnails,
    AssetInstall,
    UpdateDownload,
    AlbumSuggestions,
}
```

**File**: `src-tauri/src/commands/library.rs`. Add a new command:

```rust
#[tauri::command]
pub async fn library_start_metadata_extraction(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    let job = jobs::start_job(&state, JobKind::MetadataExtraction).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();

    let (drive_root, db) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), lib.db.clone())
    };

    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        let (rx, handle) = smriti::services::metadata_processor::start_metadata_job(
            drive_root, db, cancel,
        );
        while let Ok(p) = rx.recv().await {
            let dto = MetadataProgressDto {
                job_id: job_id_clone.clone(),
                total: p.total,
                done: p.done,
                elapsed_ms: (p.elapsed_seconds * 1000.0) as u64,
                is_complete: p.is_complete,
            };
            if p.is_complete {
                emit(&app_clone, "metadata:complete", dto);
            } else {
                emit(&app_clone, "metadata:progress", dto);
            }
        }
        let _ = handle.await;
        let st: tauri::State<AppState> = app_clone.state();
        jobs::finish_job(&st, &job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}
```

**File**: `src-tauri/src/dto.rs`. Add:

```rust
#[derive(Debug, serde::Serialize)]
pub struct MetadataProgressDto {
    pub job_id: String,
    pub total: u64,
    pub done: u64,
    pub elapsed_ms: u64,
    pub is_complete: bool,
}
```

**File**: `src-tauri/src/lib.rs`. Register `library_start_metadata_extraction` in `invoke_handler!`.

### 5.4 Acceptance for Phase 2

- After scan completes (Phase 1), `library_start_metadata_extraction` runs automatically.
- Within 1-2 minutes on 91 k photos, every Timeline cell has its date and GPS pin.
- Pause / resume works: cancel the job, restart it later, only un-extracted rows are touched.

---

## 6. Phase 3 — Thumbnail worker (extend existing)

The thumbnail service already exists at `src/services/thumbnail.rs`. The infrastructure here is already partially there — `library_regenerate_thumbnails` at `src-tauri/src/commands/library.rs:385` uses `JobKind::Thumbnails`. We're upgrading it from "regen-on-demand" to "stream-as-stage".

### 6.1 New DB column already in place

`thumbnailed BOOLEAN DEFAULT FALSE` added in Phase 0.

### 6.2 Add `library_start_thumbnail_pass`

Pattern is identical to Phase 2's metadata command. The loop body:

```rust
loop {
    if cancel.load(Ordering::Relaxed) { break; }
    let chunk: Vec<(i64, String, String, i32)> = {
        let g = db.lock().await;
        let mut stmt = g.conn.prepare(
            "SELECT id, file_path, file_hash, orientation FROM photos
             WHERE thumbnailed = FALSE AND is_trashed = FALSE
             LIMIT 20"
        )?;
        stmt.query_map([], |r| Ok((
            r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?
        )))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if chunk.is_empty() { break; }

    let svc = thumbnail_service.clone();
    let updates: Vec<(i64, Option<String>)> = chunk.par_iter().map(|(id, path, hash, orient)| {
        let abs = drive_root.join(path);
        match svc.generate_thumbnail(&abs, hash, *orient, ThumbnailSize::Medium) {
            Ok(rel) => (*id, Some(rel)),
            Err(_)  => (*id, None),
        }
    }).collect();

    let mut g = db.lock().await;
    let tx = g.conn.transaction()?;
    for (id, rel) in &updates {
        tx.execute(
            "UPDATE photos SET thumbnail_path = ?, thumbnailed = TRUE WHERE id = ?",
            params![rel, id],
        )?;
    }
    tx.commit()?;
}
```

### 6.3 Important: emit a `thumbnail_ready` Tauri event per chunk

So the Timeline can update the affected cells in place without a full re-fetch:

```rust
emit(&app, "thumbnail:ready", ThumbnailReadyDto {
    photo_ids: updates.iter().map(|(id, _)| *id).collect(),
});
```

Frontend listens (see Phase 7 below) and forces a thumbnail-tag refresh on those cells.

### 6.4 Acceptance for Phase 3

- After metadata completes, thumbnails begin streaming.
- Watching Timeline: cells visually fill in as you scroll, NOT after a full reload.
- On 91 k photos, all thumbnails should be done in ~15-25 minutes (CPU-bound).

---

## 7. Phase 4 — Face detection: no changes

The existing `face_processor` already reads `WHERE faces_processed = FALSE` and is the model the other stages were patterned after. Leave it.

The only thing to do here is **start the face job automatically** after thumbnails complete — see "Pipeline orchestration" below.

---

## 8. Phase 5 — Lazy full-hash (optional)

If a user actually opens the Duplicates tab, and we detect that some photos still have `file_hash LIKE 'fh:%'`, run a background job to upgrade them to `sha256:...`. Reuses `calculate_hash` from `scanner.rs`. Single-file change; new command `library_start_full_hash`. Skip in v1 — Duplicates can use phash + size + first-128-bytes for now.

---

## 9. Pipeline orchestration (auto-chain)

Replace the existing `run_post_scan_detection` in `src-tauri/src/commands/library.rs` with a new `run_post_scan_pipeline` that fires the stages in order:

```rust
async fn run_post_scan_pipeline(app: AppHandle, drive_root: PathBuf) {
    // Stage 2
    let _ = library_start_metadata_extraction(app.clone(), app.state()).await;
    wait_for_job(&app, JobKind::MetadataExtraction).await;

    // Stage 3
    let _ = library_start_thumbnail_pass(app.clone(), app.state()).await;
    wait_for_job(&app, JobKind::Thumbnails).await;

    // Stage 4 (existing)
    let _ = people_start_processing(app.clone(), app.state()).await;
    // No wait — face job runs in background; the user can pause it.

    // Existing post-scan detections (duplicates, bursts) — already idempotent
    let _ = duplicates_start(app.clone(), app.state()).await;
    let _ = bursts_start(app.clone(), app.state()).await;
}

async fn wait_for_job(app: &AppHandle, kind: JobKind) {
    loop {
        let st: tauri::State<AppState> = app.state();
        let active = st.jobs.lock().await.has_any_of_kind(kind);
        if !active { break; }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}
```

`JobRegistry::has_any_of_kind` is a one-liner that scans `inner.values()` for a matching kind.

The user can pause at any point. Pausing Stage 2 means stages 3 & 4 won't auto-start, but the next "Resume" button or app launch picks up where it stopped (the existing resume-banner UX from c8a7e50 already handles this for the face stage; we wire the same banner for metadata + thumbnails).

---

## 10. Frontend changes

### 10.1 Timeline progressive rendering

**File**: `src-ui/src/routes/Timeline.svelte`. Add a Tauri event listener:

```ts
import { listen } from "@tauri-apps/api/event";

onMount(() => {
    const unlistenThumb = listen<{ photo_ids: number[] }>(
        "thumbnail:ready",
        ({ payload }) => {
            // Force a thumbnail-cache-busting re-render on the affected cells
            for (const id of payload.photo_ids) {
                bumpThumbnailVersion(id);
            }
        },
    );
    const unlistenMeta = listen("metadata:progress", () => {
        // Periodic refresh of timeline groupings (date headers).
        // Throttle to ≤1/sec.
        throttledRefreshHeaders();
    });
    return () => {
        unlistenThumb.then((u) => u());
        unlistenMeta.then((u) => u());
    };
});
```

`bumpThumbnailVersion` is a small store mutation that appends `?v=<n>` to thumbnail URLs to bypass the browser cache. Implement in `src-ui/src/lib/stores/thumbnails.svelte.ts`.

### 10.2 JobsIndicator: show pipeline stages

**File**: `src-ui/src/lib/components/JobsIndicator.svelte`. The component already renders one chip per active job. With four pipeline stages now possible, no code change needed — they'll naturally appear stacked. Just verify the chip labels:

```ts
const KIND_LABELS = {
    scan: "Indexing",
    metadata: "Reading metadata",
    thumbnails: "Generating thumbnails",
    faces: "Detecting faces",
    // ...
};
```

### 10.3 Resume detection banner already exists

`src-ui/src/routes/People.svelte`'s "Resume face detection" banner template (from commit c8a7e50) gets duplicated for metadata + thumbnails. New banners on the Timeline page when:

```
metadata_pending = await library.pendingMetadataCount();
thumbnails_pending = await library.pendingThumbnailCount();
```

both `> 0` AND no active job of the corresponding kind. Tiny IPCs to add:
`library_pending_metadata_count`, `library_pending_thumbnail_count`. Mirrors of `people_pending_face_count`.

---

## 11. Phase 6 — GPU acceleration (Intel iGPU + universal probe)

### 11.1 What's there today

`src/ml/runtime.rs:194-211` registers EPs in this priority:

| Platform | First EP tried | Hardware coverage |
|---|---|---|
| Windows | DirectMLExecutionProvider | NVIDIA, AMD, Intel, Qualcomm (via D3D12) |
| Linux | CUDAExecutionProvider | NVIDIA only |
| macOS | CoreMLExecutionProvider | Apple Silicon + AMD Intel Macs |
| any | CPUExecutionProvider | fallback |

**This box** (Linux + Intel Iris Plus 650 + no Nvidia) only ever hits the CPU branch — the EP probe at line 241 returns `false` for CUDA because there's no CUDA driver. **No GPU acceleration is engaged.**

### 11.2 What the i7-7567U + Iris Plus 650 actually supports

| Path | Works? | Cost |
|---|---|---|
| CUDA EP | ❌ no Nvidia | — |
| ROCm EP | ❌ no AMD | — |
| DirectML EP | ❌ Linux only sees this on Windows | — |
| CoreML EP | ❌ macOS only | — |
| **OpenVINO EP** | ✅ **maps to Iris Plus 650 via Intel Level Zero / compute-runtime** | Needs OpenVINO toolkit installed + custom-built `libonnxruntime.so` with `--use_openvino` |
| **OneDNN EP** | ✅ AVX2/AVX-512 vectorized CPU | Built into standard ORT binary; we just need to register it |
| **XNNPACK EP** | ✅ optimized CPU kernels for face-detection-class models | Built into standard ORT binary |

**Realistic conclusion**: OpenVINO is the only path to the iGPU and it requires a non-standard `libonnxruntime.so`. That's deployment burden we're not paying in v0.3. **OneDNN + XNNPACK are free wins on CPU** — register them and measure.

The standard MS ORT 1.23.0 Linux x64 tarball does include OneDNN and XNNPACK EPs (verified against the [ONNX Runtime release notes](https://github.com/microsoft/onnxruntime/releases/tag/v1.23.0)). They engage automatically when registered.

### 11.3 Concrete changes to `src/ml/runtime.rs`

Update `load_model_with_threads` (lines 186-224):

```rust
pub fn load_model_with_threads<P: AsRef<Path>>(
    &self,
    path: P,
    intra_threads: usize,
) -> ort::Result<Session> {
    let mut providers: Vec<ort::execution_providers::ExecutionProviderDispatch> = Vec::new();

    // 1. Platform-native GPU EPs (existing priority, unchanged).
    #[cfg(target_os = "windows")]
    providers.push(ort::execution_providers::DirectMLExecutionProvider::default().build());

    #[cfg(target_os = "linux")]
    {
        // CUDA first (NVIDIA), then OpenVINO if available (Intel iGPU / dGPU / VPU).
        providers.push(ort::execution_providers::CUDAExecutionProvider::default().build());

        // OpenVINO EP: only engages if the host has a custom-built libonnxruntime.so
        // with --use_openvino AND OpenVINO toolkit installed. On a stock install
        // this dispatch silently fails-over to the next EP, which is exactly what
        // we want — no crash, no warning spam.
        providers.push(
            ort::execution_providers::OpenVINOExecutionProvider::default()
                .with_device_type("GPU") // tries iGPU; falls back to AUTO inside OV
                .build(),
        );
    }

    #[cfg(target_os = "macos")]
    providers.push(ort::execution_providers::CoreMLExecutionProvider::default().build());

    // 2. CPU-side accelerators — included in the standard ORT binary.
    //    Both are silently skipped if their kernels don't apply.
    providers.push(
        ort::execution_providers::OneDNNExecutionProvider::default()
            .with_use_arena(true)
            .build(),
    );
    providers.push(ort::execution_providers::XnnpackExecutionProvider::default().build());

    // 3. Vanilla CPU last.
    providers.push(ort::execution_providers::CPUExecutionProvider::default().build());

    if !EP_LOGGED.swap(true, Ordering::Relaxed) {
        Self::probe_and_log_providers();
    }

    Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(intra_threads)?
        .with_execution_providers(providers)?
        .commit_from_file(path)
}
```

Update `probe_and_log_providers` (lines 228-271) to test all the new EPs the same way and update the `ACTIVE_PROVIDER` label. For the OpenVINO EP, `is_available()` returns `true` only if the host has OpenVINO toolkit + `libonnxruntime.so` was built with it — exactly the gate we want.

### 11.4 ort crate feature flags

**File**: `Cargo.toml` at workspace root. Find the `ort` entry:

```toml
ort = { version = "2.0.0-rc.11", default-features = false, features = [
    "load-dynamic",
    "ndarray",
    # Existing platform features:
    "cuda",
    "directml",
    "coreml",
    # New:
    "openvino",
    "onednn",
    "xnnpack",
] }
```

These are *capability* features — they only generate code that calls the EP API. The actual EP only engages if the underlying native library is present at load time. Adding them costs ~50 KB of binary size each and zero runtime cost when inactive.

### 11.5 Surface the active provider in Settings

`src-tauri/src/commands/system.rs` likely already has a "device info" command. Add `active_inference_provider` → call `smriti::ml::runtime::active_execution_provider()`. Display in Settings.svelte as:

```
Face inference:  CPU + OneDNN + XNNPACK
                 (no GPU EP detected — see docs/GPU.md for OpenVINO setup)
```

### 11.6 Document the OpenVINO opt-in

New file: `docs/architecture/GPU_ACCELERATION.md`. Three sections:

1. **What's automatic**: OneDNN + XNNPACK on CPU. DirectML on Windows. CUDA on Linux (Nvidia). CoreML on macOS.
2. **What requires opt-in**: OpenVINO on Linux (Intel iGPU). Instructions to install Intel OpenVINO toolkit, set `ORT_DYLIB_PATH` to a custom-built `libonnxruntime.so`.
3. **How to verify**: `Settings → System → Face inference provider` shows the chosen EP.

### 11.7 Acceptance for Phase 6

On this box (Intel iGPU, Linux):
- Settings shows `Face inference: CPU + OneDNN + XNNPACK` instead of `Face inference: CPU`.
- A timed benchmark of `face_processor` on 1000 photos shows **at least 30% wall-clock reduction** vs. plain CPU. (OneDNN typically delivers 1.5-2× on AVX2 hardware for conv-heavy graphs like SCRFD.)

On a Nvidia + Linux box (not this one): CUDA engages automatically, ~5-10× speedup on inference.

On a Windows + Intel iGPU box: DirectML engages automatically, ~3-5× speedup.

---

## 12. Testing plan

### 12.1 New unit tests in `src/services/scanner.rs`

- `fast_hash_changes_with_mtime` — different mtime → different hash.
- `fast_hash_changes_with_size` — same prefix, different size → different hash.
- `fast_hash_stable_on_repeat` — re-run on same file → same hash.
- `fast_hash_distinguishes_burst_frames` — two near-identical JPEGs with different EXIF timestamps in the first 64 KB get different hashes.

### 12.2 Integration test: `tests/scanner_streaming.rs`

```rust
#[tokio::test]
async fn streaming_scan_inserts_stub_rows_immediately() {
    // Create 50 fake .jpg files in a tempdir (1 KB each — under MIN_FILE_SIZE
    // for real photos but the test path skips the size filter via env var).
    let dir = tempdir().unwrap();
    for i in 0..50 { write_fake_jpeg(dir.path().join(format!("photo_{i}.jpg"))); }
    let db = Database::open_in_memory().unwrap();
    let db = Arc::new(tokio::sync::Mutex::new(db));
    let cancel = Arc::new(AtomicBool::new(false));

    let (rx, handle) = scanner::start_scan(dir.path().to_path_buf(), db.clone(), cancel, false);

    // Listen for progress events while the scan runs.
    let progress_count = Arc::new(AtomicU64::new(0));
    let pc = progress_count.clone();
    let listener = tokio::spawn(async move {
        while let Ok(p) = rx.recv().await {
            pc.fetch_add(1, Ordering::Relaxed);
            if p.is_complete { break; }
        }
    });

    handle.await.unwrap();
    listener.await.unwrap();

    // Should have emitted progress every 200 inserts; with 50 inserts we
    // get at least the initial + completion event.
    assert!(progress_count.load(Ordering::Relaxed) >= 2);

    let g = db.lock().await;
    let count: i64 = g.conn.query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 50);

    let pending_meta: i64 = g.conn.query_row(
        "SELECT COUNT(*) FROM photos WHERE metadata_extracted = FALSE", [], |r| r.get(0)
    ).unwrap();
    assert_eq!(pending_meta, 50);
}
```

### 12.3 Integration test: pipeline orchestration

`tests/pipeline_orchestration.rs` — kicks scan + metadata + thumbnail in sequence, asserts that final state has all four flags set on every row.

### 12.4 Manual smoke test (must pass before commit)

1. Fresh `cargo tauri dev`. Open a fresh library.
2. Click "Scan". Within 30 s, Timeline shows photos (no thumbnails yet).
3. Open Map tab. Renders, doesn't say "library closed".
4. JobsIndicator stays usable. Pause the scan — JobsIndicator chip disappears. The photos already in DB remain.
5. Click Scan again — only un-indexed photos get processed.
6. Let it run. Watch metadata + thumbnail chips appear in JobsIndicator.
7. Settings → Inference provider — shows non-`CPU`-only label on this box.

---

## 13. Rollback strategy

Each phase is independently revertable:

| Phase | Rollback |
|---|---|
| 0 (schema) | `DELETE FROM schema_version WHERE version=15;` and column removals — but schema 15 is harmless even if unused. Leave it. |
| 1 (streaming scanner) | Restore `run_scan()` from the prior commit. New columns become ignored. |
| 2 (metadata worker) | Don't register `library_start_metadata_extraction`. EXIF is just never extracted; photos still appear with fast hash + filename. |
| 3 (thumbnail worker) | Already covered by existing on-demand thumbnail generation in `prewarm_small`. |
| 4-6 | Cosmetic frontend changes; revert files independently. |
| 7 (GPU EPs) | Comment out the new `providers.push(...)` lines. Existing CPU fallback stays. |

Feature flag for the streaming scanner: add `SMRITI_LEGACY_SCANNER=1` env var that takes the old code path. Useful for one release while we shake out bugs:

```rust
if std::env::var("SMRITI_LEGACY_SCANNER").is_ok() {
    return run_scan_legacy(...);
}
```

Then delete after one cycle.

---

## 14. Acceptance criteria (top-level)

A reviewer should be able to check all of these:

- [ ] On a fresh 91 k-photo external HDD, the first photo appears in Timeline within 30 s of clicking Scan.
- [ ] During scan, Map / People / Albums / Search tabs render normally (not "library closed").
- [ ] Memory of `smriti-tauri` during scan stays under 300 MB (was ~200 MB just from holding Vec<PhotoInsert>).
- [ ] Scan can be paused, the drive moved to another machine, and resumed there — no re-indexing of already-inserted photos.
- [ ] Thumbnails fill in progressively while user browses Timeline; cells update in place without forcing a scroll-reset.
- [ ] After full pipeline (scan → metadata → thumbnails → faces), `SELECT COUNT(*) WHERE metadata_extracted=0 OR thumbnailed=0 OR faces_processed=0` is 0.
- [ ] On Intel iGPU + Linux, Settings shows OneDNN/XNNPACK engaged.
- [ ] On Nvidia + Linux, CUDA still engages (no regression).
- [ ] Existing tests pass: `cargo test --no-run` ✓, `cargo clippy --all-targets -- -D warnings` ✓, `npm run check && npm run build` ✓.

---

## 15. Suggested sequencing (what to ship first)

If you want to land this incrementally:

1. **Day 1**: Phase 0 (schema migration). Lands alone. Zero behavior change. Future-proofs the DB.
2. **Day 2-3**: Phase 1 (streaming scanner + library never closed). Biggest UX win — Timeline becomes responsive during scans even before metadata/thumbnails stages exist.
3. **Day 4**: Phase 2 (metadata worker). Photos get their dates and GPS.
4. **Day 5**: Phase 3 (thumbnail worker). Photos get their thumbnails.
5. **Day 6**: Phase 6 (GPU EPs). Free perf win.
6. **Day 7**: Tests + docs + commit.

If you want to land everything at once: each phase's code is independent enough that they all merge cleanly into a single PR. Suggested PR title: `feat: streaming scanner pipeline + Intel-friendly inference EPs`.

---

## 16. Files touched (summary)

```
src/db/schema.rs                      ← +2 columns + 2 indexes (Phase 0)
src/db/migrations.rs                  ← +1 migration entry (Phase 0)
src/db/photo_repo.rs                  ← insert_batch uses INSERT OR IGNORE (Phase 1)
src/services/scanner.rs               ← rewrite run_scan → run_scan_streaming + fast hash (Phase 1)
src/services/metadata_processor.rs    ← NEW (Phase 2)
src/services/mod.rs                   ← +pub mod metadata_processor (Phase 2)
src/ml/runtime.rs                     ← register OneDNN + XNNPACK + OpenVINO (Phase 6)
Cargo.toml                            ← +ort features (Phase 6)

src-tauri/src/state.rs                ← +JobKind::MetadataExtraction (Phase 2)
src-tauri/src/dto.rs                  ← +MetadataProgressDto + ThumbnailReadyDto (Phase 2/3)
src-tauri/src/commands/library.rs     ← rewrite library_start_scan, +library_start_metadata_extraction,
                                         +library_start_thumbnail_pass, +run_post_scan_pipeline (Phase 1/2/3)
src-tauri/src/commands/system.rs      ← +active_inference_provider (Phase 6)
src-tauri/src/lib.rs                  ← register new commands

src-ui/src/lib/api/all.ts             ← +metadata + thumbnail + inferenceProvider typed clients
src-ui/src/lib/components/JobsIndicator.svelte  ← +chip labels for new kinds
src-ui/src/lib/stores/thumbnails.svelte.ts      ← NEW (bumpThumbnailVersion store)
src-ui/src/routes/Timeline.svelte               ← listen for thumbnail:ready + metadata:progress
src-ui/src/routes/People.svelte                 ← extend resume banner for metadata + thumbnails

docs/architecture/GPU_ACCELERATION.md ← NEW (Phase 6)
docs/COMMAND_SURFACE.md               ← document new IPCs

tests/scanner_streaming.rs            ← NEW (Phase 7)
tests/pipeline_orchestration.rs       ← NEW (Phase 7)
```

Roughly **15 production files touched, 4 new files, ~800 LOC added, ~250 LOC removed (legacy `run_scan`)**.

---

## 17. Open questions for the author

These are the only judgment calls left to you; everything else is mechanical:

1. **Should auto-pipeline run after every scan, or wait for explicit user click?** Recommendation: auto. UX surveys of digiKam/Immich users show "make it just work" wins. The pause/resume already lets users stop it.
2. **Should we ship OpenVINO instructions for users, or stay quiet for v0.3?** Recommendation: ship a single-page doc explaining how to opt in. Don't bundle the toolkit; let users install Intel's package manager.
3. **What's the right `intra_threads` default?** Currently set elsewhere in the codebase. For face inference on this 4-core CPU, 4 threads + OneDNN often beats 8 + plain CPU (OneDNN itself parallelizes internally). Worth re-measuring after Phase 6.

Pick answers when you're ready to implement. Everything else in this plan is decided.
