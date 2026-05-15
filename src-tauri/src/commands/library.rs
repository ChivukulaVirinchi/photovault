//! Library lifecycle (drives, current, resolve_path, detect_changes).
//!
//! M1 ships read-only commands only. `library.open`, `library.close`,
//! `library.start_scan`, `library.apply_changes` etc. land in M2.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::Database;
use smriti::services::drive_detector::DriveDetector;

use crate::dto::{DriveDto, IndexChangesDto, JobIdDto, LibraryHandleDto, MetadataProgressDto};
use crate::events::{
    EV_METADATA_COMPLETE, EV_METADATA_PROGRESS, EV_SCAN_COMPLETE, EV_SCAN_PROGRESS,
    EV_THUMBNAILS_COMPLETE, EV_THUMBNAILS_PROGRESS, EV_THUMBNAIL_READY,
};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind, OpenLibrary};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn library_list_drives() -> CommandResult<Vec<DriveDto>> {
    Ok(DriveDetector::detect()
        .into_iter()
        .map(Into::into)
        .collect())
}

#[tauri::command]
pub async fn library_current(
    state: State<'_, AppState>,
) -> CommandResult<Option<LibraryHandleDto>> {
    let lib_guard = state.library.read().await;
    let Some(lib) = lib_guard.as_ref() else {
        return Ok(None);
    };
    let db = lib.db.lock().await;
    let repo = smriti::db::PhotoRepo::new(&db.conn);
    let photo_count = repo.count()?;
    Ok(Some(LibraryHandleDto {
        drive_root: lib.drive_root.display().to_string(),
        photo_count,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ResolvePathArgs {
    pub photo_id: i64,
}

#[derive(Debug, Serialize)]
pub struct ResolvedPath {
    pub absolute_path: String,
}

#[tauri::command]
pub async fn library_resolve_path(
    state: State<'_, AppState>,
    args: ResolvePathArgs,
) -> CommandResult<ResolvedPath> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = smriti::db::PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.photo_id)?
        .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
    let abs = smriti::services::path_util::safe_join_relative(&lib.drive_root, &photo.file_path)
        .map_err(|e| CommandError::Validation {
            field: "photo.file_path".into(),
            reason: e,
        })?;
    Ok(ResolvedPath {
        absolute_path: abs.display().to_string(),
    })
}

#[tauri::command]
pub async fn library_detect_changes(state: State<'_, AppState>) -> CommandResult<IndexChangesDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let reindexer = smriti::services::reindexer::Reindexer::new();
    let changes = reindexer.detect_changes(&db.conn, &lib.drive_root)?;
    Ok(IndexChangesDto {
        added: changes.added.len() as u64,
        removed: changes.removed.len() as u64,
        moved: changes.moved.len() as u64,
        modified: changes.modified.len() as u64,
    })
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct LibraryOpenArgs {
    pub drive_path: String,
}

#[derive(Debug, Serialize)]
pub struct LibraryOpenResult {
    pub drive_root: String,
    pub photo_count: i64,
    pub first_run: bool,
}

#[tauri::command]
pub async fn library_open(
    state: State<'_, AppState>,
    args: LibraryOpenArgs,
) -> CommandResult<LibraryOpenResult> {
    let drive_root = PathBuf::from(&args.drive_path);
    if !drive_root.exists() {
        return Err(CommandError::DriveNotMounted {
            path: args.drive_path,
        });
    }

    let database = Database::open_for_drive(&drive_root)?;
    let needs_schema = database.needs_schema()?;
    if needs_schema {
        smriti::db::create_schema(&database.conn)?;
    }
    smriti::db::migrations::run_migrations(&database.conn).map_err(|e| CommandError::Database {
        message: e.to_string(),
    })?;
    repair_thumbnail_paths(&database, &drive_root)?;

    let photo_count = {
        let repo = smriti::db::PhotoRepo::new(&database.conn);
        repo.count()?
    };

    let mut guard = state.library.write().await;
    *guard =
        Some(
            OpenLibrary::new(drive_root.clone(), database).map_err(|e| CommandError::Io {
                message: e.to_string(),
            })?,
        );

    Ok(LibraryOpenResult {
        drive_root: drive_root.display().to_string(),
        photo_count,
        first_run: needs_schema,
    })
}

#[tauri::command]
pub async fn library_close(state: State<'_, AppState>) -> CommandResult<()> {
    let mut guard = state.library.write().await;
    if let Some(lib) = guard.take() {
        // Drop the Arc<Mutex<Database>>; once the last reference goes
        // away the Database's Drop impl triggers a passive WAL checkpoint.
        drop(lib);
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryApplyChangesArgs {
    #[serde(default = "yes")]
    pub added: bool,
    #[serde(default = "yes")]
    pub removed: bool,
    #[serde(default = "yes")]
    pub moved: bool,
    #[serde(default = "yes")]
    pub modified: bool,
}
fn yes() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ApplyResultDto {
    pub new_files: usize,
    pub moves_applied: usize,
    pub removals_applied: usize,
    pub updates_applied: usize,
}

#[tauri::command]
pub async fn library_apply_changes(
    state: State<'_, AppState>,
    args: LibraryApplyChangesArgs,
) -> CommandResult<ApplyResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let reindexer = smriti::services::reindexer::Reindexer::new();
    let mut changes = reindexer.detect_changes(&db.conn, &lib.drive_root)?;
    if !args.added {
        changes.added.clear();
    }
    if !args.removed {
        changes.removed.clear();
    }
    if !args.moved {
        changes.moved.clear();
    }
    if !args.modified {
        changes.modified.clear();
    }
    let r = reindexer.apply_changes(&db.conn, &changes)?;
    Ok(ApplyResultDto {
        new_files: r.new_files,
        moves_applied: r.moves_applied,
        removals_applied: r.removals_applied,
        updates_applied: r.updates_applied,
    })
}

// ---------- jobs ----------

#[derive(Debug, Default, Deserialize)]
pub struct LibraryStartScanArgs {
    #[serde(default)]
    pub scan_hidden_folders: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ScanProgressDto {
    pub job_id: String,
    pub files_found: u64,
    pub files_processed: u64,
    pub bytes_processed: u64,
    pub current_file: String,
    pub elapsed_ms: u64,
    pub is_complete: bool,
    pub error_count: usize,
}

/// Start a scan job. Returns the job_id immediately; progress streams on
/// `scan:progress`, completion on `scan:complete`.
///
/// The scanner borrows the database via `Arc<Mutex<Database>>` so the
/// library stays open and queryable (Timeline, Map, etc.) during the
/// scan. Cancellation is unified through the job registry's flag.
#[tauri::command]
pub async fn library_start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    args: LibraryStartScanArgs,
) -> CommandResult<JobIdDto> {
    // Refuse a second Scan if one is already mid-run. Old code prevented
    // this implicitly via the try_unwrap dance on the Database Arc;
    // since we now share the Arc, we have to be explicit.
    if state.jobs.lock().await.has_any_of_kind(JobKind::Scan) {
        return Err(CommandError::Conflict {
            reason: "a scan is already in progress".into(),
        });
    }

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
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let (rx, handle) = smriti::services::scanner::start_scan(
            drive_root.clone(),
            db.clone(),
            cancel,
            args.scan_hidden_folders,
        );

        // Forward progress.
        while let Ok(p) = rx.recv().await {
            let dto = ScanProgressDto {
                job_id: job_id_clone.clone(),
                files_found: p.files_found,
                files_processed: p.files_processed,
                bytes_processed: p.bytes_processed,
                current_file: p.current_file,
                elapsed_ms: (p.elapsed_seconds * 1000.0) as u64,
                is_complete: p.is_complete,
                error_count: p.errors.len(),
            };
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

        // Kick off the downstream pipeline (metadata + lightweight detections).
        // in the background. Each stage is its own job with its own
        // progress chip.
        let app_for_post = app_clone.clone();
        let drive_for_post = drive_root.clone();
        let db_for_post = db.clone();
        tokio::spawn(async move {
            run_post_scan_pipeline(app_for_post, drive_for_post, db_for_post).await;
        });
    });

    Ok(JobIdDto { job_id })
}

#[derive(Debug, Deserialize)]
pub struct CancelJobArgs {
    pub job_id: String,
}

#[tauri::command]
pub async fn library_cancel_scan(
    state: State<'_, AppState>,
    args: CancelJobArgs,
) -> CommandResult<()> {
    state.jobs.lock().await.cancel(&args.job_id);
    Ok(())
}

/// Generic job-cancel handler used by the global JobsIndicator's X
/// button — works for any job_id regardless of domain. Engine workers
/// that respect the cancel flag (face_processor, scanner, geocoding
/// backfill, etc.) abort at the next checkpoint; engines that don't
/// will run to completion but the registry slot is still released.
#[tauri::command]
pub async fn jobs_cancel(state: State<'_, AppState>, args: CancelJobArgs) -> CommandResult<()> {
    state.jobs.lock().await.cancel(&args.job_id);
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryRegenerateThumbnailsArgs {
    #[allow(dead_code)] // photo_ids targeted regen lands in M3 — for now we
    // always regenerate everything missing.
    pub photo_ids: Option<Vec<i64>>,
}

#[tauri::command]
pub async fn library_regenerate_thumbnails(
    app: AppHandle,
    state: State<'_, AppState>,
    _args: LibraryRegenerateThumbnailsArgs,
) -> CommandResult<JobIdDto> {
    if state.jobs.lock().await.has_any_of_kind(JobKind::Thumbnails) {
        return Err(CommandError::Conflict {
            reason: "thumbnail generation is already in progress".into(),
        });
    }
    let job = jobs::start_job(&state, JobKind::Thumbnails).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();
    let (drive_root, db) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), lib.db.clone())
    };

    // Full regenerate semantics: wipe every photo's thumbnail_path +
    // thumbnailed flag so `run_thumbnail_pass` (which selects rows
    // with thumbnailed = FALSE) reprocesses the entire library at the
    // current ThumbnailSize. Without this, the pass would skip rows
    // that already have a stored path — even if those rows point at
    // smaller legacy thumbnails the user wants to upgrade. The
    // existing JPEG files on disk are overwritten in place by the
    // generator, so no separate cleanup is needed.
    {
        let guard = db.lock().await;
        if let Err(e) = guard.conn.execute(
            "UPDATE photos SET thumbnail_path = NULL, thumbnailed = FALSE WHERE is_trashed = FALSE",
            [],
        ) {
            tracing::error!("regen wipe failed: {e}");
        }
    }

    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_thumbnail_pass(drive_root, db, cancel, app_clone, job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}

/// Public IPC: start the metadata extraction pass on demand (Resume
/// banner on Timeline). Refuses if one is already in flight.
#[tauri::command]
pub async fn library_start_metadata_extraction(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    if state
        .jobs
        .lock()
        .await
        .has_any_of_kind(JobKind::MetadataExtraction)
    {
        return Err(CommandError::Conflict {
            reason: "metadata extraction is already in progress".into(),
        });
    }
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
        run_metadata_stage(app_clone, job_id_clone, drive_root, db, cancel).await;
    });

    Ok(JobIdDto { job_id })
}

/// Public IPC: start the thumbnail generation pass on demand. Refuses
/// if one is already in flight.
#[tauri::command]
pub async fn library_start_thumbnail_pass(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    if state.jobs.lock().await.has_any_of_kind(JobKind::Thumbnails) {
        return Err(CommandError::Conflict {
            reason: "thumbnail generation is already in progress".into(),
        });
    }
    let job = jobs::start_job(&state, JobKind::Thumbnails).await?;
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
        run_thumbnail_pass(drive_root, db, cancel, app_clone, job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}

/// Read-only count of photos awaiting EXIF / geocoding. Drives the
/// "Resume reading metadata" banner on Timeline.
#[tauri::command]
pub async fn library_pending_metadata_count(
    state: State<'_, AppState>,
) -> CommandResult<crate::dto::PendingCountDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let pending_photos =
        smriti::db::photo_repo::PhotoRepo::new(&db.conn).count_pending_metadata()?;
    Ok(crate::dto::PendingCountDto { pending_photos })
}

/// Read-only count of photos awaiting thumbnail generation. Drives the
/// "Resume generating thumbnails" banner on Timeline.
#[tauri::command]
pub async fn library_pending_thumbnail_count(
    state: State<'_, AppState>,
) -> CommandResult<crate::dto::PendingCountDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let pending_photos =
        smriti::db::photo_repo::PhotoRepo::new(&db.conn).count_pending_thumbnails()?;
    Ok(crate::dto::PendingCountDto { pending_photos })
}

/// Shared body for "run metadata extraction now". Forwards engine
/// progress events as Tauri events and releases the job slot when
/// done. Used by both `library_start_metadata_extraction` (explicit
/// Resume click) and the auto post-scan pipeline.
async fn run_metadata_stage(
    app: AppHandle,
    job_id: String,
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<smriti::db::Database>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let (rx, handle) =
        smriti::services::metadata_processor::start_metadata_job(drive_root, db, cancel);
    while let Ok(p) = rx.recv().await {
        let dto = MetadataProgressDto {
            job_id: job_id.clone(),
            total: p.total,
            done: p.done,
            elapsed_ms: (p.elapsed_seconds * 1000.0) as u64,
            is_complete: p.is_complete,
        };
        if p.is_complete {
            emit(&app, EV_METADATA_COMPLETE, dto);
        } else {
            emit(&app, EV_METADATA_PROGRESS, dto);
        }
    }
    let _ = handle.await;
    let st: tauri::State<AppState> = app.state();
    jobs::finish_job(&st, &job_id).await;
}

async fn run_thumbnail_pass(
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<smriti::db::Database>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    app: AppHandle,
    job_id: String,
) {
    use smriti::services::thumbnail::{ThumbnailService, ThumbnailSize};

    let chunk_size: usize = 20;
    let svc = match ThumbnailService::new(
        &drive_root,
        smriti::config::AppConfig::load().thumbnail_cache_gb,
    ) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("thumbnail pass: failed to create service: {e}");
            let st: tauri::State<AppState> = app.state();
            jobs::finish_job(&st, &job_id).await;
            return;
        }
    };

    let total_pending: Option<u64> = {
        let guard = db.lock().await;
        smriti::db::photo_repo::PhotoRepo::new(&guard.conn)
            .count_pending_thumbnails()
            .ok()
            .map(|n| n.max(0) as u64)
    };
    let started = std::time::Instant::now();
    let mut total_done: u64 = 0;
    let mut failed_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        let chunk: Vec<(i64, String, String, i32)> = {
            let guard = db.lock().await;
            let mut stmt = match guard.conn.prepare(
                "SELECT id, file_path, file_hash, orientation FROM photos \
                 WHERE thumbnailed = FALSE AND is_trashed = FALSE \
                 LIMIT ?",
            ) {
                Ok(s) => s,
                Err(_) => break,
            };
            stmt.query_map([(chunk_size * 4) as i64], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _, _, _)| !failed_ids.contains(id))
            .take(chunk_size)
            .collect()
        };

        if chunk.is_empty() {
            break;
        }

        let svc = svc.clone();
        let drive = drive_root.clone();
        let updates: Vec<(i64, Option<String>)> = chunk
            .iter()
            .map(|(id, path, hash, orient)| {
                let abs = drive.join(path);
                match svc.generate_thumbnail(&abs, hash, *orient, ThumbnailSize::Medium) {
                    Ok(_) => (*id, Some(relative_thumbnail_path(hash))),
                    Err(_) => (*id, None),
                }
            })
            .collect();

        let photo_ids: Vec<i64> = updates
            .iter()
            .filter_map(|(id, rel)| if rel.is_some() { Some(*id) } else { None })
            .collect();

        {
            let mut guard = db.lock().await;
            let tx = match guard.conn.transaction() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("thumbnail tx: {e}");
                    continue;
                }
            };
            for (id, rel) in &updates {
                if let Some(rel) = rel {
                    let _ = tx.execute(
                        "UPDATE photos SET thumbnail_path = ?1, thumbnailed = TRUE WHERE id = ?2",
                        rusqlite::params![rel, id],
                    );
                } else {
                    failed_ids.insert(*id);
                }
            }
            if let Err(e) = tx.commit() {
                tracing::error!("thumbnail tx commit: {e}");
                continue;
            }
        }

        if !photo_ids.is_empty() {
            emit(
                &app,
                EV_THUMBNAIL_READY,
                crate::dto::ThumbnailReadyDto { photo_ids },
            );
        }

        total_done += updates.len() as u64;
        emit(
            &app,
            EV_THUMBNAILS_PROGRESS,
            crate::events::JobProgress {
                job_id: job_id.clone(),
                stage: "generate".into(),
                processed: total_done,
                total: total_pending,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: estimate_eta_ms(total_done, total_pending, started.elapsed()),
                message: None,
            },
        );
    }

    // Emit a completion event so the JobsIndicator clears the chip
    // (the store dismisses ~2.5s after `:complete`). Without this the
    // thumbnail row lingers as "running" forever even after the worker
    // exits.
    emit(
        &app,
        EV_THUMBNAILS_COMPLETE,
        crate::events::JobProgress {
            job_id: job_id.clone(),
            stage: "generate".into(),
            processed: total_done,
            total: total_pending,
            elapsed_ms: started.elapsed().as_millis() as u64,
            eta_ms: None,
            message: None,
        },
    );

    let st: tauri::State<AppState> = app.state();
    jobs::finish_job(&st, &job_id).await;
}

fn relative_thumbnail_path(file_hash: &str) -> String {
    let subdir = &file_hash[..2.min(file_hash.len())];
    format!(
        ".photovault/thumbnails/medium/v2/{}/{}.jpg",
        subdir, file_hash
    )
}

fn legacy_thumbnail_path(file_hash: &str) -> String {
    let subdir = &file_hash[..2.min(file_hash.len())];
    format!(
        ".photovault/thumbnails/small/v2/{}/{}.jpg",
        subdir, file_hash
    )
}

fn repair_thumbnail_paths(database: &Database, drive_root: &std::path::Path) -> CommandResult<()> {
    let mut stmt = database.conn.prepare(
        "SELECT id, file_hash, thumbnail_path FROM photos WHERE thumbnail_path IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut repaired = 0usize;
    for (id, hash, stored) in rows {
        // A stored path is usable if it matches either the current
        // (medium) layout or the legacy (small) layout AND the file
        // exists on disk. Anything else (orphaned row pointing at a
        // missing file) gets cleared so the thumbnail pass regenerates.
        let expected = relative_thumbnail_path(&hash);
        let legacy = legacy_thumbnail_path(&hash);
        let usable = (stored == expected || stored == legacy)
            && drive_root.join(&stored).exists();
        if usable {
            continue;
        }
        database.conn.execute(
            "UPDATE photos SET thumbnail_path = NULL, thumbnailed = FALSE WHERE id = ?1",
            rusqlite::params![id],
        )?;
        repaired += 1;
    }

    if repaired > 0 {
        tracing::info!("Cleared {} stale thumbnail_path rows", repaired);
    }
    Ok(())
}

fn estimate_eta_ms(
    processed: u64,
    total: Option<u64>,
    elapsed: std::time::Duration,
) -> Option<u64> {
    let total = total?;
    if processed == 0 || processed >= total {
        return None;
    }
    let elapsed_ms = elapsed.as_millis() as u64;
    if elapsed_ms < 1_000 {
        return None;
    }
    Some((elapsed_ms / processed).saturating_mul(total - processed))
}

use std::sync::Arc;

/// Auto-chain after a Scan completes: metadata → existing
/// duplicates/bursts detection. Face detection is deliberately NOT
/// auto-chained — it's heavy enough that users should opt in via the
/// People page. Thumbnails are viewport-driven from the UI, so we do
/// not run a full-library thumbnail pass here.
async fn run_post_scan_pipeline(
    app: AppHandle,
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
) {
    // Stage 2: metadata extraction.
    {
        let state: tauri::State<AppState> = app.state();
        // Skip if a user-initiated metadata job is already running. The
        // single `WHERE metadata_extracted = FALSE` worker would otherwise
        // race itself; idempotent but wasteful.
        if !state
            .jobs
            .lock()
            .await
            .has_any_of_kind(JobKind::MetadataExtraction)
        {
            if let Ok(job) = jobs::start_job(&state, JobKind::MetadataExtraction).await {
                let cancel = job.cancel.clone();
                let job_id = job.id.clone();
                run_metadata_stage(app.clone(), job_id, drive_root.clone(), db.clone(), cancel)
                    .await;
            }
        }
    }

    // Face detection is intentionally NOT auto-chained — users start it
    // explicitly via the People page so the heavy ML pass isn't running
    // in the background by surprise.

    // Existing post-scan detections (duplicates, bursts) — already idempotent.
    run_post_scan_detection(app, drive_root).await;
}

/// Run duplicate + burst detection passes after a scan completes. Both
/// passes are idempotent and persist their groups, so the Bursts and
/// Duplicates tabs reflect the fresh library state without the user
/// having to manually click "Scan" inside each tab.
async fn run_post_scan_detection(_app: AppHandle, drive_root: PathBuf) {
    // Open a secondary connection so we don't compete with foreground
    // photos_list / albums queries for the shared Arc<Mutex<Database>>.
    // SQLite WAL handles the concurrent reader/writer.
    let db_path = smriti::db::db_path_for(&drive_root);

    let drive_for_dups = drive_root.clone();
    let db_path_for_dups = db_path.clone();
    let dups = tokio::task::spawn_blocking(move || {
        let conn = match smriti::db::open_secondary(&db_path_for_dups) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("post-scan dups: open secondary DB failed: {}", e);
                return 0u64;
            }
        };
        let exact = smriti::services::duplicate_detector::DuplicateDetector::find_duplicates(&conn)
            .unwrap_or_default();
        let exclude_ids: std::collections::HashSet<i64> = exact
            .iter()
            .flat_map(|g| g.photo_ids.iter().copied())
            .collect();
        let perc =
            smriti::services::duplicate_detector::DuplicateDetector::find_perceptual_duplicates(
                &conn,
                &drive_for_dups,
                &exclude_ids,
            )
            .unwrap_or_default();
        let mut to_persist: Vec<(String, Vec<i64>, Option<i64>, &'static str)> =
            Vec::with_capacity(exact.len() + perc.len());
        for g in exact.iter().chain(perc.iter()) {
            to_persist.push((
                g.hash.clone(),
                g.photo_ids.clone(),
                g.suggested_keep_id,
                g.duplicate_type,
            ));
        }
        let repo = smriti::db::duplicate_repo::DuplicateRepo::new(&conn);
        let _ = repo.sync_duplicate_groups(&to_persist);
        (exact.len() + perc.len()) as u64
    })
    .await
    .unwrap_or(0);
    tracing::info!("post-scan: {} duplicate groups", dups);

    let drive_for_bursts = drive_root.clone();
    let db_path_for_bursts = db_path.clone();
    let bursts = tokio::task::spawn_blocking(move || {
        let conn = match smriti::db::open_secondary(&db_path_for_bursts) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("post-scan bursts: open secondary DB failed: {}", e);
                return 0u64;
            }
        };
        let cfg = smriti::config::AppConfig::load();
        let burst_cfg = smriti::services::burst_detector::BurstConfig {
            max_gap_seconds: cfg.burst_time_window_seconds,
            ..Default::default()
        };
        let detector = smriti::services::burst_detector::BurstDetector::new(burst_cfg);
        let thumbs_root = drive_for_bursts.join(".photovault/thumbnails/small/v2");
        let groups = detector
            .find_bursts(&conn, Some(&drive_for_bursts), Some(&thumbs_root))
            .unwrap_or_default();
        let triples: Vec<(String, String, Vec<i64>)> = groups
            .iter()
            .map(|g| {
                (
                    g.start_time.to_rfc3339(),
                    g.end_time.to_rfc3339(),
                    g.photo_ids.clone(),
                )
            })
            .collect();
        let repo = smriti::db::burst_repo::BurstRepo::new(&conn);
        let _ = repo.sync_burst_groups(&triples);
        groups.len() as u64
    })
    .await
    .unwrap_or(0);
    tracing::info!("post-scan: {} burst groups", bursts);
}
