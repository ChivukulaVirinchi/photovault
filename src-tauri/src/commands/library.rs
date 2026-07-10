//! Library lifecycle (drives, current, resolve_path, detect_changes).
//!
//! M1 ships read-only commands only. `library.open`, `library.close`,
//! `library.start_scan`, `library.apply_changes` etc. land in M2.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use rusqlite::OpenFlags;
use smriti::db::Database;
use smriti::services::drive_detector::DriveDetector;
use smriti::services::semantic::{
    SemanticIndexCache, SemanticModelRunner, SemanticSearchService, SEMANTIC_TEXT_SEARCH_LIMIT,
};

use crate::dto::{
    DriveDto, ExcludedFolderDto, ExcludedFolderPreviewDto, IndexChangesDto, JobIdDto,
    LibraryHandleDto, MediaTypeDto, MetadataProgressDto, Page, PhotoSummaryDto, SchemaTooNewDto,
};
use crate::events::{
    EV_METADATA_COMPLETE, EV_METADATA_PROGRESS, EV_SCAN_COMPLETE, EV_SCAN_PROGRESS,
    EV_THUMBNAILS_COMPLETE, EV_THUMBNAILS_PROGRESS, EV_THUMBNAIL_READY,
};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind, OpenLibrary, UnsupportedLibrary};
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
    let (drive_root, db_path) = {
        let lib_guard = state.library.read().await;
        let Some(lib) = lib_guard.as_ref() else {
            let unsupported_guard = state.unsupported_library.read().await;
            return Ok(unsupported_guard.as_ref().map(unsupported_library_dto));
        };
        (
            lib.drive_root.display().to_string(),
            smriti::db::db_path_for(&lib.drive_root),
        )
    };
    let photo_count = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(smriti::db::PhotoRepo::new(&conn).count()?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("library current worker failed: {e}"),
    })??;
    Ok(Some(LibraryHandleDto {
        drive_root,
        photo_count,
        read_only: false,
        schema_too_new: None,
    }))
}

fn unsupported_library_dto(lib: &UnsupportedLibrary) -> LibraryHandleDto {
    LibraryHandleDto {
        drive_root: lib.drive_root.display().to_string(),
        photo_count: 0,
        read_only: true,
        schema_too_new: Some(SchemaTooNewDto {
            db_version: lib.db_version,
            max_supported: lib.max_supported,
        }),
    }
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
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);
    let changes = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        let reindexer = smriti::services::reindexer::Reindexer::new();
        Ok::<_, CommandError>(reindexer.detect_changes(&conn, &drive_root)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("change detection worker failed: {e}"),
    })??;
    Ok(IndexChangesDto {
        added: changes.added.len() as u64,
        removed: changes.removed.len() as u64,
        moved: changes.moved.len() as u64,
        modified: changes.modified.len() as u64,
    })
}

#[derive(Debug, Deserialize)]
pub struct LibraryExclusionPathArgs {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct LibraryExclusionRemoveArgs {
    pub relative_path: String,
}

#[tauri::command]
pub async fn library_exclusions_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<ExcludedFolderDto>> {
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        smriti::db::db_path_for(&lib.drive_root)
    };
    let exclusions = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(smriti::db::ExcludedFolderRepo::new(&conn).list()?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("library exclusions worker failed: {e}"),
    })??;
    Ok(exclusions.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn library_exclusions_preview(
    state: State<'_, AppState>,
    args: LibraryExclusionPathArgs,
) -> CommandResult<ExcludedFolderPreviewDto> {
    let (relative_path, db_path) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            validate_exclusion_path(&lib.drive_root, &args.path)?,
            smriti::db::db_path_for(&lib.drive_root),
        )
    };
    let relative_for_count = relative_path.clone();
    let indexed_count = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(
            smriti::db::ExcludedFolderRepo::new(&conn).count_indexed_under(&relative_for_count)?,
        )
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("library exclusion preview worker failed: {e}"),
    })??;
    Ok(ExcludedFolderPreviewDto {
        relative_path,
        indexed_count,
    })
}

#[tauri::command]
pub async fn library_exclusions_add(
    state: State<'_, AppState>,
    args: LibraryExclusionPathArgs,
) -> CommandResult<ExcludedFolderDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let relative_path = validate_exclusion_path(&lib.drive_root, &args.path)?;
    let db = lib.db.lock().await;
    let record =
        smriti::db::ExcludedFolderRepo::new(&db.conn).insert_and_remove_indexed(&relative_path)?;
    Ok(record.into())
}

#[tauri::command]
pub async fn library_exclusions_remove(
    state: State<'_, AppState>,
    args: LibraryExclusionRemoveArgs,
) -> CommandResult<()> {
    let relative_path = smriti::services::exclusions::normalize_stored_relative(
        &args.relative_path,
    )
    .map_err(|reason| CommandError::Validation {
        field: "relative_path".into(),
        reason,
    })?;
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let removed = smriti::db::ExcludedFolderRepo::new(&db.conn).remove(&relative_path)?;
    if !removed {
        return Err(CommandError::not_found("excluded_folder", relative_path));
    }
    Ok(())
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct LibraryOpenArgs {
    pub drive_path: String,
}

#[derive(Debug, Deserialize)]
pub struct CompatPhotosListArgs {
    pub offset: u32,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct LibraryOpenResult {
    pub drive_root: String,
    pub photo_count: i64,
    pub first_run: bool,
    pub read_only: bool,
    pub schema_too_new: Option<SchemaTooNewDto>,
}

#[tauri::command]
pub async fn library_open(
    state: State<'_, AppState>,
    args: LibraryOpenArgs,
) -> CommandResult<LibraryOpenResult> {
    let drive_root = PathBuf::from(&args.drive_path);
    validate_library_root(&drive_root, &args.drive_path)?;

    let drive_root_for_prepare = drive_root.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        prepare_library_database(drive_root_for_prepare)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("library open worker failed: {e}"),
    })?;

    let (database, needs_schema, photo_count) = match prepared {
        Ok(prepared) => prepared,
        Err(CommandError::SchemaTooNew {
            db_version,
            max_supported,
        }) => {
            state.jobs.lock().await.cancel_library_scoped();
            *state.library.write().await = None;
            *state.unsupported_library.write().await = Some(UnsupportedLibrary {
                drive_root: drive_root.clone(),
                db_version,
                max_supported,
            });
            state.assistant.lock().await.sessions.clear();
            return Ok(LibraryOpenResult {
                drive_root: drive_root.display().to_string(),
                photo_count: 0,
                first_run: false,
                read_only: true,
                schema_too_new: Some(SchemaTooNewDto {
                    db_version,
                    max_supported,
                }),
            });
        }
        Err(err) => return Err(err),
    };

    let open_library =
        OpenLibrary::new(drive_root.clone(), database).map_err(|e| CommandError::Io {
            message: e.to_string(),
        })?;
    spawn_semantic_warmup(
        drive_root.clone(),
        open_library.semantic_index.clone(),
        open_library.semantic_runner.clone(),
    );

    state.jobs.lock().await.cancel_library_scoped();
    *state.unsupported_library.write().await = None;
    let mut guard = state.library.write().await;
    *guard = Some(open_library);
    state.assistant.lock().await.sessions.clear();

    Ok(LibraryOpenResult {
        drive_root: drive_root.display().to_string(),
        photo_count,
        first_run: needs_schema,
        read_only: false,
        schema_too_new: None,
    })
}

#[tauri::command]
pub async fn library_compat_photos_list(
    state: State<'_, AppState>,
    args: CompatPhotosListArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let drive_root = {
        let unsupported_guard = state.unsupported_library.read().await;
        let lib = unsupported_guard
            .as_ref()
            .ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);
    let offset = i64::from(args.offset);
    let limit = i64::from(args.limit.unwrap_or(100).clamp(1, 200));
    let (items, total) = tauri::async_runtime::spawn_blocking(move || {
        compat_photos_list_from_db(&db_path, offset, limit)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("compat photo list worker failed: {e}"),
    })??;
    let row_count = items.len();
    Ok(Page {
        has_more: compat_offset_has_more(offset, row_count, total),
        next_cursor: None,
        items,
        total: Some(total),
    })
}

fn compat_offset_has_more(offset: i64, row_count: usize, total: u64) -> bool {
    (offset.max(0) as u64).saturating_add(row_count as u64) < total
}

fn compat_photos_list_from_db(
    db_path: &Path,
    offset: i64,
    limit: i64,
) -> CommandResult<(Vec<PhotoSummaryDto>, u64)> {
    let conn = rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    ensure_compat_photo_columns(&conn)?;
    let total: u64 = conn.query_row(
        "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
        [],
        |row| row.get::<_, i64>(0),
    )? as u64;
    let mut stmt = conn.prepare(
        r#"
        SELECT id, thumbnail_path, date_taken, width, height, orientation,
               media_type, duration_ms, is_favorite, is_trashed
          FROM photos
         WHERE is_trashed = FALSE
      ORDER BY date_taken IS NULL, date_taken DESC, id DESC
         LIMIT ?1 OFFSET ?2
        "#,
    )?;
    let rows = stmt.query_map([limit, offset], |row| {
        let media_type: String = row.get(6)?;
        Ok(PhotoSummaryDto {
            id: row.get(0)?,
            thumbnail_path: row.get(1)?,
            date_taken: row.get(2)?,
            width: row.get(3)?,
            height: row.get(4)?,
            orientation: row.get(5)?,
            media_type: if media_type == "video" {
                MediaTypeDto::Video
            } else {
                MediaTypeDto::Photo
            },
            duration_ms: row.get(7)?,
            is_favorite: row.get(8)?,
            is_trashed: row.get(9)?,
            stack: None,
        })
    })?;
    let items = rows.collect::<Result<Vec<_>, _>>()?;
    Ok((items, total))
}

fn ensure_compat_photo_columns(conn: &rusqlite::Connection) -> CommandResult<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(photos)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<Result<std::collections::HashSet<_>, _>>()?;
    for name in [
        "id",
        "thumbnail_path",
        "date_taken",
        "width",
        "height",
        "orientation",
        "media_type",
        "duration_ms",
        "is_favorite",
        "is_trashed",
    ] {
        if !columns.contains(name) {
            return Err(CommandError::Database {
                message: format!("unsupported library is missing photos.{name}"),
            });
        }
    }
    Ok(())
}

fn validate_library_root(drive_root: &Path, original_path: &str) -> CommandResult<()> {
    if !drive_root.exists() {
        return Err(CommandError::DriveNotMounted {
            path: original_path.into(),
        });
    }
    if !drive_root.is_dir() {
        return Err(CommandError::Validation {
            field: "drive_path".into(),
            reason: "path must be an existing folder".into(),
        });
    }
    Ok(())
}

fn prepare_library_database(drive_root: PathBuf) -> CommandResult<(Database, bool, i64)> {
    if let Some((db_version, max_supported)) = preflight_schema_too_new(&drive_root)? {
        return Err(CommandError::SchemaTooNew {
            db_version,
            max_supported,
        });
    }

    let database = Database::open_for_drive(&drive_root)?;
    let needs_schema = database.needs_schema()?;
    if needs_schema {
        smriti::db::create_schema(&database.conn)?;
    }
    smriti::db::migrations::run_migrations(&database.conn).map_err(|e| {
        if let Some(schema) = e.downcast_ref::<smriti::db::migrations::SchemaTooNewError>() {
            CommandError::SchemaTooNew {
                db_version: schema.db_version,
                max_supported: schema.max_supported,
            }
        } else {
            CommandError::Database {
                message: e.to_string(),
            }
        }
    })?;
    repair_thumbnail_paths(&database, &drive_root)?;

    let photo_count = smriti::db::PhotoRepo::new(&database.conn).count()?;
    Ok((database, needs_schema, photo_count))
}

fn preflight_schema_too_new(drive_root: &Path) -> CommandResult<Option<(i32, i32)>> {
    let db_path = smriti::db::db_path_for(drive_root);
    if !db_path.exists() {
        return Ok(None);
    }

    let conn = rusqlite::Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version = match smriti::db::migrations::get_schema_version(&conn) {
        Ok(version) => version,
        Err(rusqlite::Error::SqliteFailure(_, Some(message)))
            if message.contains("no such table") =>
        {
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };
    let max_supported = smriti::db::migrations::MAX_KNOWN_SCHEMA_VERSION;
    if version > max_supported {
        Ok(Some((version, max_supported)))
    } else {
        Ok(None)
    }
}

pub(crate) fn spawn_semantic_warmup(
    drive_root: PathBuf,
    semantic_index: Arc<std::sync::Mutex<SemanticIndexCache>>,
    semantic_runner: Arc<std::sync::Mutex<Option<SemanticModelRunner>>>,
) {
    tokio::task::spawn_blocking(move || {
        let db_path = smriti::db::db_path_for(&drive_root);
        let conn = match smriti::db::open_secondary(&db_path) {
            Ok(conn) => conn,
            Err(err) => {
                tracing::debug!("semantic warmup skipped: failed opening db: {}", err);
                return;
            }
        };
        let svc = SemanticSearchService::new(&drive_root);
        let status = match svc.status(&conn) {
            Ok(status) => status,
            Err(err) => {
                tracing::debug!("semantic warmup skipped: status failed: {}", err);
                return;
            }
        };
        if !status.assets_installed || !status.onnx_runtime_installed || status.indexed_photos == 0
        {
            return;
        }

        let mut runner_guard = match semantic_runner.lock() {
            Ok(runner) => runner,
            Err(_) => {
                tracing::debug!("semantic warmup skipped: model cache poisoned");
                return;
            }
        };
        if runner_guard.is_none() {
            match SemanticSearchService::model_runner() {
                Ok(runner) => *runner_guard = Some(runner),
                Err(err) => {
                    tracing::debug!("semantic warmup skipped: model unavailable: {}", err);
                    return;
                }
            }
        }
        let mut cache = match semantic_index.lock() {
            Ok(cache) => cache,
            Err(_) => {
                tracing::debug!("semantic warmup skipped: index cache poisoned");
                return;
            }
        };

        if let Some(runner) = runner_guard.as_mut() {
            if let Err(err) = svc.search_text_cached(
                &conn,
                &mut cache,
                runner,
                "photo",
                SEMANTIC_TEXT_SEARCH_LIMIT,
            ) {
                tracing::debug!("semantic warmup skipped: search cache failed: {}", err);
            }
        }
    });
}

#[tauri::command]
pub async fn library_close(state: State<'_, AppState>) -> CommandResult<()> {
    state.jobs.lock().await.cancel_library_scoped();
    *state.unsupported_library.write().await = None;
    let mut guard = state.library.write().await;
    if let Some(lib) = guard.take() {
        // Drop the Arc<Mutex<Database>>; once the last reference goes
        // away the Database's Drop impl triggers a passive WAL checkpoint.
        drop(lib);
    }
    state.assistant.lock().await.sessions.clear();
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
    let (drive_root, db) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), lib.db.clone())
    };
    let db_path = smriti::db::db_path_for(&drive_root);
    let mut changes = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        let reindexer = smriti::services::reindexer::Reindexer::new();
        Ok::<_, CommandError>(reindexer.detect_changes(&conn, &drive_root)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("change detection worker failed: {e}"),
    })??;
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
    let db = db.lock().await;
    let reindexer = smriti::services::reindexer::Reindexer::new();
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

    // Read the drive_root + clone the db Arc. No more take()/try_unwrap dance.
    let (drive_root, db, thumbnails) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            lib.drive_root.clone(),
            lib.db.clone(),
            lib.thumbnails.clone(),
        )
    };

    let job = jobs::start_job(&state, JobKind::Scan).await?;
    let job_id = job.id.clone();

    let cancel = job.cancel.clone();
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let cancel_after_scan = cancel.clone();
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

        if cancel_after_scan.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // Kick off the downstream pipeline in the background. Each
        // visible stage is its own job with its own progress chip.
        let app_for_post = app_clone.clone();
        let drive_for_post = drive_root.clone();
        let db_for_post = db.clone();
        let thumbnails_for_post = thumbnails.clone();
        tokio::spawn(async move {
            run_post_scan_pipeline(
                app_for_post,
                drive_for_post,
                db_for_post,
                thumbnails_for_post,
            )
            .await;
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
    let (drive_root, db, thumbnails) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            lib.drive_root.clone(),
            lib.db.clone(),
            lib.thumbnails.clone(),
        )
    };
    let job = jobs::start_job(&state, JobKind::Thumbnails).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();

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
            jobs::finish_job(&state, &job_id).await;
            return Err(e.into());
        }
    }

    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_thumbnail_pass(drive_root, db, thumbnails, cancel, app_clone, job_id_clone).await;
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
    let (drive_root, db) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), lib.db.clone())
    };

    let job = jobs::start_job(&state, JobKind::MetadataExtraction).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();

    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_metadata_stage(app_clone, job_id_clone, drive_root, db, cancel).await;
    });

    Ok(JobIdDto { job_id })
}

/// Re-read capture dates for every non-trashed media item. This clears
/// stored date fields and lets the normal metadata worker repopulate
/// them with the current parser, preserving the existing job/progress UI.
#[tauri::command]
pub async fn library_refresh_photo_dates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    let (drive_root, db) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), lib.db.clone())
    };
    let job = jobs::start_job(&state, JobKind::MetadataExtraction).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();

    {
        let guard = db.lock().await;
        if let Err(e) = guard.conn.execute(
            "UPDATE photos
             SET date_taken = NULL,
                 date_taken_source = NULL,
                 metadata_extracted = FALSE
             WHERE is_trashed = FALSE",
            [],
        ) {
            jobs::finish_job(&state, &job_id).await;
            return Err(e.into());
        }
    }

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
    let (drive_root, db, thumbnails) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            lib.drive_root.clone(),
            lib.db.clone(),
            lib.thumbnails.clone(),
        )
    };

    let job = jobs::start_job(&state, JobKind::Thumbnails).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();

    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_thumbnail_pass(drive_root, db, thumbnails, cancel, app_clone, job_id_clone).await;
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
            stage: p.stage,
            message: p.message,
        };
        if dto.stage.as_deref() == Some("error") {
            emit(&app, EV_METADATA_PROGRESS, dto);
        } else if dto.is_complete {
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
    svc: Arc<smriti::services::thumbnail::ThumbnailService>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    app: AppHandle,
    job_id: String,
) {
    use smriti::services::thumbnail::ThumbnailSize;

    let chunk_size: usize = 20;
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
    let mut hard_error: Option<String> = None;
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        let chunk: Vec<(i64, String, String, i32)> = {
            let guard = db.lock().await;
            match load_thumbnail_chunk(&guard.conn, &failed_ids, chunk_size) {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("thumbnail query failed: {e}");
                    hard_error = Some(format!("Thumbnail query failed: {e}"));
                    Vec::new()
                }
            }
        };

        if hard_error.is_some() || chunk.is_empty() {
            break;
        }

        let svc_for_chunk = svc.clone();
        let drive = drive_root.clone();
        let cancel_for_chunk = cancel.clone();
        let updates: Vec<(i64, String, Option<String>)> =
            match tauri::async_runtime::spawn_blocking(move || {
                let mut updates = Vec::with_capacity(chunk.len());
                for (id, path, hash, orient) in chunk {
                    if cancel_for_chunk.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let abs = match smriti::services::path_util::safe_join_relative(&drive, &path) {
                        Ok(abs) => abs,
                        Err(e) => {
                            tracing::debug!(
                                "thumbnail pass skipped unsafe path for photo_id={id}: {e}"
                            );
                            updates.push((id, hash, None));
                            continue;
                        }
                    };
                    let rel = match svc_for_chunk.generate_thumbnail_background(
                        &abs,
                        &hash,
                        orient,
                        ThumbnailSize::Medium,
                    ) {
                        Ok(_) => Some(relative_thumbnail_path(&hash)),
                        Err(e) => {
                            tracing::debug!("thumbnail pass failed for photo_id={id}: {e}");
                            None
                        }
                    };
                    updates.push((id, hash, rel));
                }
                updates
            })
            .await
            {
                Ok(updates) => updates,
                Err(e) => {
                    tracing::error!("thumbnail worker panicked: {e}");
                    hard_error = Some(format!("Thumbnail worker failed: {e}"));
                    break;
                }
            };

        let mut ready_photo_ids = Vec::with_capacity(updates.len());

        {
            let mut guard = db.lock().await;
            let tx = match guard.conn.transaction() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("thumbnail tx: {e}");
                    hard_error = Some(format!("Thumbnail database transaction failed: {e}"));
                    break;
                }
            };
            for (id, hash, rel) in &updates {
                if let Some(rel) = rel {
                    match tx.execute(
                        "UPDATE photos SET thumbnail_path = ?1, thumbnailed = TRUE WHERE id = ?2 AND file_hash = ?3",
                        rusqlite::params![rel, id, hash],
                    ) {
                        Ok(affected) => {
                            if affected > 0 {
                                ready_photo_ids.push(*id);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("thumbnail update failed for photo_id={id}: {e}");
                        }
                    }
                } else {
                    failed_ids.insert(*id);
                }
            }
            if let Err(e) = tx.commit() {
                tracing::error!("thumbnail tx commit: {e}");
                hard_error = Some(format!("Thumbnail database commit failed: {e}"));
                break;
            }
        }

        if !ready_photo_ids.is_empty() {
            emit(
                &app,
                EV_THUMBNAIL_READY,
                crate::dto::ThumbnailReadyDto {
                    photo_ids: ready_photo_ids,
                },
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

    if let Some(message) = hard_error {
        emit(
            &app,
            EV_THUMBNAILS_PROGRESS,
            crate::events::JobProgress {
                job_id: job_id.clone(),
                stage: "error".into(),
                processed: total_done,
                total: total_pending,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some(message),
            },
        );
    } else {
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
    }

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

fn load_thumbnail_chunk(
    conn: &rusqlite::Connection,
    failed_ids: &std::collections::HashSet<i64>,
    limit: usize,
) -> rusqlite::Result<Vec<(i64, String, String, i32)>> {
    let mut sql = String::from(
        "SELECT id, file_path, file_hash, orientation FROM photos \
         WHERE thumbnailed = FALSE AND is_trashed = FALSE AND media_type = 'photo'",
    );
    let mut params: Vec<i64> = Vec::with_capacity(failed_ids.len() + 1);
    if !failed_ids.is_empty() {
        sql.push_str(" AND id NOT IN (");
        for idx in 0..failed_ids.len() {
            if idx > 0 {
                sql.push_str(", ");
            }
            sql.push('?');
        }
        sql.push(')');
        params.extend(failed_ids.iter().copied());
    }
    sql.push_str(" ORDER BY id ASC LIMIT ?");
    params.push(limit as i64);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    rows.collect()
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
        let usable = (stored == expected || stored == legacy) && drive_root.join(&stored).exists();
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

fn validate_exclusion_path(drive_root: &std::path::Path, input: &str) -> CommandResult<String> {
    let raw = PathBuf::from(input.trim());
    let selected = if raw.is_absolute() {
        raw
    } else {
        drive_root.join(raw)
    };
    if !selected.exists() {
        return Err(CommandError::Validation {
            field: "path".into(),
            reason: "folder does not exist".into(),
        });
    }
    if !selected.is_dir() {
        return Err(CommandError::Validation {
            field: "path".into(),
            reason: "path must be a folder".into(),
        });
    }

    let root = drive_root.canonicalize()?;
    let selected = selected.canonicalize()?;
    let relative = selected
        .strip_prefix(&root)
        .map_err(|_| CommandError::Validation {
            field: "path".into(),
            reason: "folder must be inside the current library".into(),
        })?;

    let relative_path = smriti::services::path_util::relative_path_for_storage(relative);
    let relative_path = smriti::services::exclusions::normalize_stored_relative(&relative_path)
        .map_err(|reason| CommandError::Validation {
            field: "path".into(),
            reason,
        })?;

    let first_component = relative_path.split('/').next().unwrap_or_default();
    if first_component.eq_ignore_ascii_case(smriti::db::LIBRARY_METADATA_DIR) {
        return Err(CommandError::Validation {
            field: "path".into(),
            reason: "Smriti's library metadata folder is always managed internally".into(),
        });
    }

    Ok(relative_path)
}

/// Auto-chain after a Scan completes: metadata, full-library thumbnail
/// prewarm, then existing duplicates/bursts detection. Face detection
/// is deliberately NOT auto-chained — it's heavy enough that users
/// should opt in via the People page.
async fn run_post_scan_pipeline(
    app: AppHandle,
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    thumbnails: Arc<smriti::services::thumbnail::ThumbnailService>,
) {
    // Stage 2: metadata extraction.
    {
        if !library_is_still_open(&app, &drive_root).await {
            tracing::info!("post-scan: skipped metadata because library changed");
            return;
        }
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

    // Stage 3: prewarm durable thumbnails for the whole library. This
    // uses the background-priority generator, so visible viewport
    // requests still take permits first. Without this pass, jumping deep
    // into a large cold library can require scroll-time generation for
    // every visible cell.
    {
        if !library_is_still_open(&app, &drive_root).await {
            tracing::info!("post-scan: skipped thumbnail prewarm because library changed");
            return;
        }
        let state: tauri::State<AppState> = app.state();
        if !state.jobs.lock().await.has_any_of_kind(JobKind::Thumbnails) {
            let pending = {
                let guard = db.lock().await;
                smriti::db::photo_repo::PhotoRepo::new(&guard.conn)
                    .count_pending_thumbnails()
                    .unwrap_or(0)
            };
            if pending > 0 {
                match jobs::start_job(&state, JobKind::Thumbnails).await {
                    Ok(job) => {
                        let cancel = job.cancel.clone();
                        let job_id = job.id.clone();
                        let app_for_thumbs = app.clone();
                        let drive_for_thumbs = drive_root.clone();
                        let db_for_thumbs = db.clone();
                        let thumbnails_for_job = thumbnails.clone();
                        tokio::spawn(async move {
                            run_thumbnail_pass(
                                drive_for_thumbs,
                                db_for_thumbs,
                                thumbnails_for_job,
                                cancel,
                                app_for_thumbs,
                                job_id,
                            )
                            .await;
                        });
                    }
                    Err(e) => tracing::warn!("post-scan thumbnail prewarm skipped: {e}"),
                }
            }
        }
    }

    // Existing post-scan detections (duplicates, bursts) — already idempotent.
    run_post_scan_detection(app, drive_root).await;
}

/// Run duplicate + burst detection passes after a scan completes. Both
/// passes are idempotent and persist their groups, so the Bursts and
/// Duplicates tabs reflect the fresh library state without the user
/// having to manually click "Scan" inside each tab.
async fn run_post_scan_detection(app: AppHandle, drive_root: PathBuf) {
    if !library_is_still_open(&app, &drive_root).await {
        tracing::info!("post-scan: skipped detection because library changed");
        return;
    }

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
        let exact = match smriti::services::duplicate_detector::DuplicateDetector::find_duplicates(
            &conn,
            &drive_for_dups,
        ) {
            Ok(groups) => groups,
            Err(e) => {
                tracing::error!("post-scan dups: exact pass failed: {}", e);
                return 0u64;
            }
        };
        let exclude_ids: std::collections::HashSet<i64> = exact
            .iter()
            .flat_map(|g| g.photo_ids.iter().copied())
            .collect();
        let perc =
            match smriti::services::duplicate_detector::DuplicateDetector::find_perceptual_duplicates(
                    &conn,
                    &drive_for_dups,
                    &exclude_ids,
                ) {
                Ok(groups) => groups,
                Err(e) => {
                    tracing::error!("post-scan dups: perceptual pass failed: {}", e);
                    return 0u64;
                }
            };
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
        if let Err(e) = repo.sync_duplicate_groups(&to_persist) {
            tracing::error!("post-scan dups: persist failed: {}", e);
            return 0u64;
        }
        (exact.len() + perc.len()) as u64
    })
    .await
    .unwrap_or(0);
    tracing::info!("post-scan: {} duplicate groups", dups);

    if !library_is_still_open(&app, &drive_root).await {
        tracing::info!("post-scan: skipped bursts because library changed");
        return;
    }

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
        let groups = match detector.find_bursts(&conn, Some(&drive_for_bursts), Some(&thumbs_root))
        {
            Ok(groups) => groups,
            Err(e) => {
                tracing::error!("post-scan bursts: scan failed: {}", e);
                return 0u64;
            }
        };
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
        if let Err(e) = repo.sync_burst_groups(&triples) {
            tracing::error!("post-scan bursts: persist failed: {}", e);
            return 0u64;
        }
        groups.len() as u64
    })
    .await
    .unwrap_or(0);
    tracing::info!("post-scan: {} burst groups", bursts);
}

async fn library_is_still_open(app: &AppHandle, drive_root: &Path) -> bool {
    let state: tauri::State<AppState> = app.state();
    let guard = state.library.read().await;
    guard
        .as_ref()
        .is_some_and(|lib| lib.drive_root.as_path() == drive_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn thumbnail_test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                orientation INTEGER NOT NULL DEFAULT 1,
                thumbnailed BOOLEAN NOT NULL DEFAULT FALSE,
                is_trashed BOOLEAN NOT NULL DEFAULT FALSE,
                media_type TEXT NOT NULL DEFAULT 'photo'
            );
            INSERT INTO photos (id, file_path, file_hash, orientation, thumbnailed, is_trashed, media_type)
            VALUES
                (1, 'one.jpg', 'aa111', 1, FALSE, FALSE, 'photo'),
                (2, 'two.jpg', 'bb222', 1, FALSE, FALSE, 'photo'),
                (3, 'done.jpg', 'cc333', 1, TRUE, FALSE, 'photo'),
                (4, 'trashed.jpg', 'dd444', 1, FALSE, TRUE, 'photo'),
                (5, 'video.mp4', 'ee555', 1, FALSE, FALSE, 'video');
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn thumbnail_chunk_skips_current_run_failures_without_hiding_later_pending_rows() {
        let conn = thumbnail_test_conn();
        let mut failed = HashSet::new();
        failed.insert(1);

        let rows = load_thumbnail_chunk(&conn, &failed, 20).unwrap();

        assert_eq!(rows, vec![(2, "two.jpg".into(), "bb222".into(), 1)]);
    }

    #[test]
    fn validate_library_root_rejects_files() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-a-library-root");
        std::fs::write(&file, b"not a folder").unwrap();

        let err = validate_library_root(&file, file.to_str().unwrap()).unwrap_err();

        assert!(matches!(err, CommandError::Validation { field, .. } if field == "drive_path"));
    }

    #[test]
    fn compat_photos_list_reads_stable_columns_only() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("photovault.db");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            smriti::db::create_schema(&conn).unwrap();
            conn.execute(
                "INSERT INTO photos
                    (id, file_path, file_name, file_hash, file_size, date_taken,
                     thumbnail_path, width, height, media_type, is_trashed)
                 VALUES
                    (1, 'old.jpg', 'old.jpg', 'h1', 10, '2026-01-01T00:00:00',
                     '.photovault/thumbs/old.jpg', 400, 300, 'photo', FALSE),
                    (2, 'new.mp4', 'new.mp4', 'h2', 20, '2026-01-02T00:00:00',
                     '.photovault/thumbs/new.jpg', 800, 600, 'video', FALSE),
                    (3, 'trash.jpg', 'trash.jpg', 'h3', 30, '2026-01-03T00:00:00',
                     '.photovault/thumbs/trash.jpg', 100, 100, 'photo', TRUE)",
                [],
            )
            .unwrap();
        }

        let (items, total) = compat_photos_list_from_db(&db_path, 0, 10).unwrap();

        assert_eq!(total, 2);
        assert_eq!(items.iter().map(|p| p.id).collect::<Vec<_>>(), vec![2, 1]);
        assert!(items.iter().all(|p| p.stack.is_none()));
        assert!(matches!(items[0].media_type, MediaTypeDto::Video));
    }

    #[test]
    fn compat_offset_has_more_uses_total_count() {
        assert!(compat_offset_has_more(0, 100, 101));
        assert!(!compat_offset_has_more(100, 100, 200));
        assert!(!compat_offset_has_more(200, 0, 200));
    }

    #[test]
    fn preflight_schema_too_new_reads_existing_db_without_normal_open() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = smriti::db::db_path_for(temp.path());
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO schema_version (version) VALUES (999);
            "#,
        )
        .unwrap();
        drop(conn);

        let detected = preflight_schema_too_new(temp.path()).unwrap();

        assert_eq!(
            detected,
            Some((999, smriti::db::migrations::MAX_KNOWN_SCHEMA_VERSION)),
        );
    }
}
