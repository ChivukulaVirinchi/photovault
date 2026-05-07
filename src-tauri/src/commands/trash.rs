//! Trash (read-only — list & stats. Trash/restore/delete are M2).

use serde::Deserialize;
use tauri::State;

use smriti::db::trash_repo::TrashRepo;
use smriti::services::trash::TrashService;

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

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct TrashPhotoIdsArgs {
    pub photo_ids: Vec<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct TrashCountDto {
    pub count: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct TrashDeleteResultDto {
    pub files_deleted: u64,
    pub db_records_deleted: u64,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn trash_trash_photos(
    state: State<'_, AppState>,
    args: TrashPhotoIdsArgs,
) -> CommandResult<TrashCountDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let n = TrashService::trash_photos(&db.conn, &args.photo_ids)? as u64;
    Ok(TrashCountDto { count: n })
}

#[tauri::command]
pub async fn trash_restore(
    state: State<'_, AppState>,
    args: TrashPhotoIdsArgs,
) -> CommandResult<TrashCountDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let n = TrashService::restore_photos(&db.conn, &args.photo_ids)? as u64;
    Ok(TrashCountDto { count: n })
}

#[tauri::command]
pub async fn trash_permanent_delete(
    state: State<'_, AppState>,
    args: TrashPhotoIdsArgs,
) -> CommandResult<TrashDeleteResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let r = TrashService::permanent_delete(&db.conn, &args.photo_ids, &lib.drive_root)?;
    Ok(TrashDeleteResultDto {
        files_deleted: r.files_deleted as u64,
        db_records_deleted: r.db_records_deleted as u64,
        errors: r.errors,
    })
}

#[tauri::command]
pub async fn trash_empty(state: State<'_, AppState>) -> CommandResult<TrashDeleteResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let r = TrashService::empty_trash(&db.conn, &lib.drive_root)?;
    Ok(TrashDeleteResultDto {
        files_deleted: r.files_deleted as u64,
        db_records_deleted: r.db_records_deleted as u64,
        errors: r.errors,
    })
}
