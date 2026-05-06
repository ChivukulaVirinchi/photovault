//! Trash (read-only — list & stats. Trash/restore/delete are M2).

use serde::Deserialize;
use tauri::State;

use photovault::db::trash_repo::TrashRepo;
use photovault::services::trash::TrashService;

use crate::dto::{Page, TrashStatsDto, TrashedPhotoDto};
use crate::pagination;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Default, Deserialize)]
pub struct TrashListArgs {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn trash_list(
    state: State<'_, AppState>,
    args: TrashListArgs,
) -> CommandResult<Page<TrashedPhotoDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = TrashRepo::new(&db.conn);
    let all = repo.get_all()?;

    // Trash list is small (typically dozens to a few hundred). Use a
    // simple "skip until cursor seen, then take limit" — cursor encodes
    // the last seen photo_id only; use Cursor.id directly.
    let limit = pagination::clamp_limit(args.limit) as usize;
    let cursor = pagination::decode(args.cursor.as_deref())?;
    let start_idx = match cursor {
        Some(c) => all
            .iter()
            .position(|t| t.photo_id == c.id)
            .map_or(0, |i| i + 1),
        None => 0,
    };
    let slice: Vec<_> = all.iter().skip(start_idx).take(limit).cloned().collect();
    let has_more = start_idx + slice.len() < all.len();
    let next_cursor = slice.last().map(|t| {
        pagination::encode(crate::pagination::Cursor {
            date_taken: None,
            id: t.photo_id,
        })
    });
    let total = if cursor.is_none() {
        Some(all.len() as u64)
    } else {
        None
    };
    Ok(Page {
        items: slice.into_iter().map(Into::into).collect(),
        next_cursor,
        has_more,
        total,
    })
}

#[tauri::command]
pub async fn trash_stats(state: State<'_, AppState>) -> CommandResult<TrashStatsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    Ok(TrashService::get_stats(&db.conn)?.into())
}
