//! Library lifecycle (drives, current, resolve_path, detect_changes).
//!
//! M1 ships read-only commands only. `library.open`, `library.close`,
//! `library.start_scan`, `library.apply_changes` etc. land in M2.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use photovault::db::Database;
use photovault::services::drive_detector::DriveDetector;

use crate::dto::{DriveDto, IndexChangesDto, JobIdDto, LibraryHandleDto};
use crate::events::{EV_SCAN_COMPLETE, EV_SCAN_PROGRESS, EV_THUMBNAILS_PROGRESS};
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
    let repo = photovault::db::PhotoRepo::new(&db.conn);
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
    let repo = photovault::db::PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.photo_id)?
        .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
    let abs = lib.drive_root.join(&photo.file_path);
    Ok(ResolvedPath {
        absolute_path: abs.display().to_string(),
    })
}

#[tauri::command]
pub async fn library_detect_changes(state: State<'_, AppState>) -> CommandResult<IndexChangesDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let reindexer = photovault::services::reindexer::Reindexer::new();
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
        photovault::db::create_schema(&database.conn)?;
    }
    photovault::db::migrations::run_migrations(&database.conn).map_err(|e| {
        CommandError::Database {
            message: e.to_string(),
        }
    })?;

    let photo_count = {
        let repo = photovault::db::PhotoRepo::new(&database.conn);
        repo.count()?
    };

    let mut guard = state.library.write().await;
    *guard = Some(
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
    let reindexer = photovault::services::reindexer::Reindexer::new();
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
/// Caveat: the photovault scanner takes ownership of the Database
/// (moves it into a spawn_blocking thread). We work around that by
/// extracting the database from AppState, running the scan, then
/// putting it back. While the scan runs the library is "borrowed" —
/// other commands see `LibraryClosed` for the scan duration. M3 will
/// refactor scanner to take `&Connection` so this dance goes away.
#[tauri::command]
pub async fn library_start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    args: LibraryStartScanArgs,
) -> CommandResult<JobIdDto> {
    let job = jobs::start_job(&state, JobKind::Scan).await?;
    let job_id = job.id.clone();

    // Extract Database, leaving library temporarily empty.
    let (drive_root, database) = {
        let mut guard = state.library.write().await;
        let lib = guard.take().ok_or(CommandError::LibraryClosed)?;
        let drive_root = lib.drive_root.clone();
        let thumbnails = lib.thumbnails.clone();
        // Try to take exclusive ownership of the DB — only possible if
        // no other handler is currently holding it. The Arc must have a
        // refcount of 1.
        let db_arc = lib.db;
        let db_mutex = match Arc::try_unwrap(db_arc) {
            Ok(m) => m,
            Err(arc) => {
                // Put it back; another handler is mid-operation.
                *guard = Some(OpenLibrary {
                    drive_root: drive_root.clone(),
                    db: arc,
                    thumbnails,
                });
                jobs::finish_job(&state, &job_id).await;
                return Err(CommandError::Conflict {
                    reason: "another command is currently using the database".into(),
                });
            }
        };
        let db = db_mutex.into_inner();
        (drive_root, db)
    };

    let cancel = job.cancel.clone();
    let app_clone = app.clone();
    let started = job.started_at;
    let drive_root_clone = drive_root.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        // Bridge the photovault scanner channel → Tauri events.
        let (rx, scanner_cancel, handle) = photovault::services::scanner::start_scan(
            drive_root_clone.clone(),
            database,
            args.scan_hidden_folders,
        );

        // Forward our unified cancel into scanner's cancel.
        let cancel_propagator = {
            let cancel = cancel.clone();
            let scanner_cancel = scanner_cancel.clone();
            tokio::spawn(async move {
                while !cancel.load(Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                scanner_cancel.store(true, Ordering::Relaxed);
            })
        };

        // Forward progress.
        while let Ok(p) = rx.recv().await {
            let dto = ScanProgressDto {
                job_id: job_id_clone.clone(),
                files_found: p.files_found,
                files_processed: p.files_processed,
                bytes_processed: p.bytes_processed,
                current_file: p.current_file,
                elapsed_ms: started.elapsed().as_millis() as u64,
                is_complete: p.is_complete,
                error_count: p.errors.len(),
            };
            if p.is_complete {
                emit(&app_clone, EV_SCAN_COMPLETE, dto);
            } else {
                emit(&app_clone, EV_SCAN_PROGRESS, dto);
            }
        }
        cancel_propagator.abort();

        // Re-attach the database (returned via ScanResult). Use AppHandle's
        // state accessor — `*const AppState` would not be Send across the
        // tokio task boundary.
        if let Ok(result) = handle.await {
            let st: tauri::State<AppState> = app_clone.state();
            let mut guard = st.library.write().await;
            match OpenLibrary::new(drive_root_clone, result.database) {
                Ok(lib) => *guard = Some(lib),
                Err(e) => {
                    tracing::error!("re-attaching library after scan failed: {}", e);
                }
            }
            drop(guard);
            jobs::finish_job(&st, &job_id_clone).await;
        }
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
    let job = jobs::start_job(&state, JobKind::Thumbnails).await?;
    let job_id = job.id.clone();
    let app_clone = app.clone();
    let started = job.started_at;
    let job_id_for_evt = job_id.clone();

    // For now: emit a "started" then a "complete" event without per-photo
    // progress; the real work is wired into the existing rotated-data
    // regen service in M3. The frontend can already treat this as a
    // long-running op.
    tokio::spawn(async move {
        emit(
            &app_clone,
            EV_THUMBNAILS_PROGRESS,
            crate::events::JobProgress {
                job_id: job_id_for_evt.clone(),
                stage: "regenerate".into(),
                processed: 0,
                total: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some("regen scaffolding — wired in M3".into()),
            },
        );
    });

    Ok(JobIdDto { job_id })
}

// Re-export Arc for the scan job's try_unwrap dance.
use std::sync::Arc;
