//! Unified search + recent searches (read-only).

use serde::Deserialize;
use tauri::State;

use photovault::db::recent_search_repo::RecentSearchRepo;
use photovault::services::search::SearchService;

use crate::dto::{RecentSearchDto, SearchResultsDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct SearchQueryArgs {
    pub q: String,
}

#[tauri::command]
pub async fn search_query(
    state: State<'_, AppState>,
    args: SearchQueryArgs,
) -> CommandResult<SearchResultsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let unified = SearchService::search_unified(&db.conn, &args.q)?;
    Ok(unified.into())
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchRecentListArgs {
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn search_recent_list(
    state: State<'_, AppState>,
    args: SearchRecentListArgs,
) -> CommandResult<Vec<RecentSearchDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = RecentSearchRepo::new(&db.conn);
    let limit = args.limit.unwrap_or(10).min(100) as i64;
    Ok(repo
        .get_recent(limit)?
        .into_iter()
        .map(Into::into)
        .collect())
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct SearchRecentRemoveArgs {
    pub q: String,
}

#[tauri::command]
pub async fn search_recent_remove(
    state: State<'_, AppState>,
    args: SearchRecentRemoveArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    RecentSearchRepo::new(&db.conn).remove(&args.q)?;
    Ok(())
}

#[tauri::command]
pub async fn search_recent_clear(state: State<'_, AppState>) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    RecentSearchRepo::new(&db.conn).clear()?;
    Ok(())
}
