//! Unified search + recent searches (read-only).

use serde::Deserialize;
use tauri::State;

use smriti::db::recent_search_repo::RecentSearchRepo;
use smriti::services::search::SearchService;
use smriti::services::semantic::SemanticSearchService;

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
    let semantic_ids = if should_try_semantic(&args.q) {
        let svc = SemanticSearchService::new(&lib.drive_root);
        match svc.status(&db.conn) {
            Ok(status)
                if status.assets_installed
                    && status.onnx_runtime_installed
                    && status.indexed_photos > 0 =>
            {
                let mut cache = match lib.semantic_index.lock() {
                    Ok(cache) => cache,
                    Err(_) => return Err(CommandError::internal("semantic index cache poisoned")),
                };
                let mut runner_guard = match lib.semantic_runner.lock() {
                    Ok(runner) => runner,
                    Err(_) => return Err(CommandError::internal("semantic model cache poisoned")),
                };
                if runner_guard.is_none() {
                    match SemanticSearchService::model_runner() {
                        Ok(runner) => *runner_guard = Some(runner),
                        Err(err) => {
                            tracing::debug!("semantic model unavailable: {}", err);
                            return Ok(SearchService::search_unified(&db.conn, &args.q)?.into());
                        }
                    }
                }
                let runner = runner_guard
                    .as_mut()
                    .expect("semantic runner initialized above");
                match svc.search_text_cached(&db.conn, &mut cache, runner, &args.q, 250) {
                    Ok(candidates) => candidates.into_iter().map(|c| c.photo_id).collect(),
                    Err(err) => {
                        tracing::debug!("semantic search skipped: {}", err);
                        Vec::new()
                    }
                }
            }
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let unified = SearchService::search_unified_with_semantic(&db.conn, &args.q, semantic_ids)?;
    Ok(unified.into())
}

fn should_try_semantic(q: &str) -> bool {
    let trimmed = q.trim();
    trimmed.len() >= 3 && trimmed.chars().any(char::is_alphabetic)
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
