//! Library health diagnostics.

use tauri::State;

use crate::dto::LibraryHealthDto;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn health_compute(state: State<'_, AppState>) -> CommandResult<LibraryHealthDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let data = photovault::services::library_health::compute(&db.conn, &lib.drive_root)?;
    Ok(data.into())
}
