//! Library lifecycle (drives, current, resolve_path, detect_changes).
//!
//! M1 ships read-only commands only. `library.open`, `library.close`,
//! `library.start_scan`, `library.apply_changes` etc. land in M2.

use serde::{Deserialize, Serialize};
use tauri::State;

use photovault::services::drive_detector::DriveDetector;

use crate::dto::{DriveDto, IndexChangesDto, LibraryHandleDto};
use crate::state::AppState;
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
    // M1 stub: full reindexer integration lands in M2 alongside
    // `library.apply_changes`. For now, return zeros so the frontend
    // can call this and see "no changes detected".
    let _lib_guard = state.library.read().await;
    let _lib = _lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    Ok(IndexChangesDto::default())
}
