//! Unified search + recent searches (read-only).

use serde::Deserialize;
use tauri::State;

use smriti::db::recent_search_repo::RecentSearchRepo;
use smriti::db::{db_path_for, open_secondary};
use smriti::services::search::SearchService;
use smriti::services::semantic::{
    relevant_text_search_candidates, SemanticSearchService, SEMANTIC_TEXT_SEARCH_LIMIT,
};

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
    let (db_path, drive_root, semantic_index, semantic_runner) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            db_path_for(&lib.drive_root),
            lib.drive_root.clone(),
            lib.semantic_index.clone(),
            lib.semantic_runner.clone(),
        )
    };
    let q = args.q;
    let unified = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let semantic_ids =
            semantic_photo_ids(&conn, &drive_root, &semantic_index, &semantic_runner, &q)?;
        Ok::<_, CommandError>(SearchService::search_unified_with_semantic(
            &conn,
            &q,
            semantic_ids,
        )?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("search worker failed: {e}"),
    })??;
    Ok(unified.into())
}

fn should_try_semantic(q: &str) -> bool {
    let trimmed = q.trim();
    trimmed.len() >= 3 && trimmed.chars().any(char::is_alphabetic)
}

fn semantic_photo_ids(
    conn: &rusqlite::Connection,
    drive_root: &std::path::Path,
    semantic_index: &std::sync::Arc<
        std::sync::Mutex<smriti::services::semantic::SemanticIndexCache>,
    >,
    semantic_runner: &std::sync::Arc<
        std::sync::Mutex<Option<smriti::services::semantic::SemanticModelRunner>>,
    >,
    q: &str,
) -> Result<Vec<i64>, CommandError> {
    if !should_try_semantic(q) {
        return Ok(Vec::new());
    }

    let svc = SemanticSearchService::new(drive_root);
    let ready = matches!(
        svc.status(conn),
        Ok(status)
            if status.assets_installed
                && status.onnx_runtime_installed
                && status.indexed_photos > 0
    );
    if !ready {
        return Ok(Vec::new());
    }

    let vector = {
        let mut runner_guard = semantic_runner
            .lock()
            .map_err(|_| CommandError::internal("semantic model cache poisoned"))?;
        if runner_guard.is_none() {
            match SemanticSearchService::model_runner() {
                Ok(runner) => *runner_guard = Some(runner),
                Err(err) => {
                    tracing::debug!("semantic model unavailable: {}", err);
                    return Ok(Vec::new());
                }
            }
        }
        let Some(runner) = runner_guard.as_mut() else {
            tracing::debug!("semantic runner missing after initialization");
            return Ok(Vec::new());
        };
        match runner.embed_text(q) {
            Ok(vector) => vector,
            Err(err) => {
                tracing::debug!("semantic text embedding skipped: {}", err);
                return Ok(Vec::new());
            }
        }
    };

    let candidates = {
        let mut cache = semantic_index
            .lock()
            .map_err(|_| CommandError::internal("semantic index cache poisoned"))?;
        match svc.search_vector_cached(conn, &mut cache, &vector, SEMANTIC_TEXT_SEARCH_LIMIT) {
            Ok(candidates) => candidates,
            Err(err) => {
                tracing::debug!("semantic search skipped: {}", err);
                return Ok(Vec::new());
            }
        }
    };

    Ok(relevant_text_search_candidates(candidates)
        .into_iter()
        .map(|c| c.photo_id)
        .collect())
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
    let limit = args.limit.unwrap_or(10).clamp(1, 100) as i64;
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
