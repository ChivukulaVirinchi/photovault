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
use crate::state::{AppState, SemanticQueryPageCache};
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct SearchQueryArgs {
    pub q: String,
    /// Offset into the stable (date/rank-ordered) photo result list.
    /// Combined with `limit` this pages through matches that exceed a
    /// single response.
    pub offset: Option<u32>,
    /// Page size. Defaults to 200, capped at the service's 20,000-row
    /// product ceiling; the frontend requests small batches.
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn search_query(
    state: State<'_, AppState>,
    args: SearchQueryArgs,
) -> CommandResult<SearchResultsDto> {
    let (db_path, drive_root, semantic_index, semantic_runner, semantic_query) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            db_path_for(&lib.drive_root),
            lib.drive_root.clone(),
            lib.semantic_index.clone(),
            lib.semantic_runner.clone(),
            lib.semantic_queries.clone(),
        )
    };
    let q = args.q;
    let offset = args.offset.unwrap_or(0) as usize;
    let limit = args
        .limit
        .map(|l| l as usize)
        .unwrap_or(200)
        .clamp(1, SearchService::UNIFIED_SEARCH_MAX_PHOTOS);
    let unified = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let semantic_ids = semantic_photo_ids(
            &conn,
            &drive_root,
            &semantic_index,
            &semantic_runner,
            &semantic_query,
            &q,
        )?;
        Ok::<_, CommandError>(SearchService::search_unified_page(
            &conn,
            &q,
            semantic_ids,
            offset,
            limit,
        )?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("search worker failed: {e}"),
    })??;
    Ok(unified.results.into())
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
    semantic_query: &std::sync::Arc<std::sync::Mutex<SemanticQueryPageCache>>,
    q: &str,
) -> Result<Vec<i64>, CommandError> {
    if !should_try_semantic(q) {
        return Ok(Vec::new());
    }

    let revision = conn
        .query_row(
            "SELECT revision FROM semantic_revision WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);

    // Every page must use the SAME semantic candidate list, or the photo
    // ordering changes between pages and the user sees duplicates and gaps
    // while scrolling. So: reuse the snapshot whenever it is available, and
    // when it is not, recompute it here rather than falling back to an empty
    // list.
    //
    // Returning `unwrap_or_default()` on a cache miss was actively wrong.
    // An empty candidate list makes `has_structured_filters()` false for the
    // semantic branch, so page 2+ silently switched to date ordering while
    // page 1 had been semantically ranked — a different result set, not a
    // slower one. A miss is normal: the cache holds 8 entries, and the
    // warmup thread holds the runner mutex for the first seconds after
    // startup.
    if let Ok(mut cache) = semantic_query.try_lock() {
        if let Some(cached) = cache.get(q, revision) {
            return Ok(cached);
        }
    }

    // Only a *successful* computation is worth remembering. Caching the
    // transient failures below would permanently pin a metadata-only result
    // to a query the user typed while the model was still loading.
    //
    // The cost of recomputing when the snapshot is unavailable is one text
    // embedding plus a vector search — acceptable, and it only happens on a
    // cache miss.

    let svc = SemanticSearchService::new(drive_root);
    let ready = matches!(
        svc.status(conn),
        Ok(status)
            if status.assets_installed
                && status.onnx_runtime_installed
                && status.indexed_photos > 0
    );
    if !ready {
        // Not an error: semantic search is unavailable, so the caller falls
        // back to metadata/text. Do not cache — readiness can change.
        return Ok(Vec::new());
    }

    let vector = {
        // The startup warmup owns this mutex while loading ONNX. Search must
        // remain instant during that work and fall back to metadata/text.
        let Ok(mut runner_guard) = semantic_runner.try_lock() else {
            return Ok(Vec::new());
        };
        let Some(runner) = runner_guard.as_mut() else {
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
        let Ok(mut cache) = semantic_index.try_lock() else {
            return Ok(Vec::new());
        };
        match svc.search_vector_cached(conn, &mut cache, &vector, SEMANTIC_TEXT_SEARCH_LIMIT) {
            Ok(candidates) => candidates,
            Err(err) => {
                tracing::debug!("semantic search skipped: {}", err);
                return Ok(Vec::new());
            }
        }
    };

    let photo_ids = relevant_text_search_candidates(candidates)
        .into_iter()
        .map(|c| c.photo_id)
        .collect::<Vec<_>>();

    if let Ok(mut cache) = semantic_query.try_lock() {
        cache.remember(q.to_owned(), revision, photo_ids.clone());
    }
    Ok(photo_ids)
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
