//! Insights dashboard.

use serde::Deserialize;
use tauri::State;

use crate::dto::InsightsDto;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Default, Deserialize)]
pub struct InsightsComputeArgs {
    pub year: Option<i32>,
}

#[tauri::command]
pub async fn insights_compute(
    state: State<'_, AppState>,
    args: InsightsComputeArgs,
) -> CommandResult<InsightsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let data = photovault::services::insights::compute(&db.conn, args.year)?;
    Ok(data.into())
}

#[tauri::command]
pub async fn insights_invalidate(_state: State<'_, AppState>) -> CommandResult<()> {
    // No server-side cache today — handler exists so the frontend can
    // signal cache-stale events; future caching layer can hook here.
    Ok(())
}
