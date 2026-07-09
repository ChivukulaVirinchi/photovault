//! Library health diagnostics.

use tauri::State;

use smriti::db::{db_path_for, open_secondary};

use crate::dto::LibraryHealthDto;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn health_compute(state: State<'_, AppState>) -> CommandResult<LibraryHealthDto> {
    let (db_path, drive_root) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (db_path_for(&lib.drive_root), lib.drive_root.clone())
    };
    let data = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let snapshot = smriti::services::library_health::compute_snapshot(&conn)?;
        Ok::<_, CommandError>(smriti::services::library_health::finish_compute(
            snapshot,
            &drive_root,
        ))
    })
    .await
    .map_err(|e| CommandError::Io {
        message: format!("health worker failed: {e}"),
    })??;
    Ok(data.into())
}
