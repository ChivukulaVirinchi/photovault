//! Timeline photo stacks.

use serde::{Deserialize, Serialize};
use tauri::State;

use smriti::db::PhotoStackRepo;

use crate::dto::{stack_detail_dto, PhotoStackDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct StackIdArgs {
    pub id: i64,
}

#[derive(Debug, Deserialize)]
pub struct StackForPhotoArgs {
    pub photo_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct StackSetCoverArgs {
    pub stack_id: i64,
    pub photo_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct StackRemoveMemberArgs {
    pub stack_id: i64,
    pub photo_id: i64,
}

#[derive(Debug, Serialize)]
pub struct StackRefreshDto {
    pub stacks_found: u64,
}

#[derive(Debug, Serialize)]
pub struct StackCountResultDto {
    pub count: u64,
}

#[tauri::command]
pub async fn stacks_get(
    state: State<'_, AppState>,
    args: StackIdArgs,
) -> CommandResult<PhotoStackDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoStackRepo::new(&db.conn);
    let stack = repo
        .get_stack(args.id)?
        .ok_or_else(|| CommandError::not_found("photo_stack", args.id))?;
    let members = repo.get_members(args.id)?;
    Ok(stack_detail_dto(stack, members))
}

#[tauri::command]
pub async fn stacks_get_for_photo(
    state: State<'_, AppState>,
    args: StackForPhotoArgs,
) -> CommandResult<Option<PhotoStackDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoStackRepo::new(&db.conn);
    let Some(stack) = repo.get_stack_for_photo(args.photo_id)? else {
        return Ok(None);
    };
    let members = repo.get_members(stack.id)?;
    Ok(Some(stack_detail_dto(stack, members)))
}

#[tauri::command]
pub async fn stacks_set_cover(
    state: State<'_, AppState>,
    args: StackSetCoverArgs,
) -> CommandResult<PhotoStackDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoStackRepo::new(&db.conn);
    match repo.set_cover(args.stack_id, args.photo_id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("photo_stack_member", args.photo_id));
        }
        Err(e) => return Err(e.into()),
    }
    let stack = repo
        .get_stack(args.stack_id)?
        .ok_or_else(|| CommandError::not_found("photo_stack", args.stack_id))?;
    let members = repo.get_members(args.stack_id)?;
    Ok(stack_detail_dto(stack, members))
}

#[tauri::command]
pub async fn stacks_remove_member(
    state: State<'_, AppState>,
    args: StackRemoveMemberArgs,
) -> CommandResult<Option<PhotoStackDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoStackRepo::new(&db.conn);
    match repo.remove_member(args.stack_id, args.photo_id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("photo_stack_member", args.photo_id));
        }
        Err(e) => return Err(e.into()),
    }
    let Some(stack) = repo.get_stack(args.stack_id)? else {
        return Ok(None);
    };
    let members = repo.get_members(args.stack_id)?;
    Ok(Some(stack_detail_dto(stack, members)))
}

#[tauri::command]
pub async fn stacks_unstack(state: State<'_, AppState>, args: StackIdArgs) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    match PhotoStackRepo::new(&db.conn).unstack(args.id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("photo_stack", args.id));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[tauri::command]
pub async fn stacks_trash_others(
    state: State<'_, AppState>,
    args: StackIdArgs,
) -> CommandResult<StackCountResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoStackRepo::new(&db.conn);
    if repo.get_stack(args.id)?.is_none() {
        return Err(CommandError::not_found("photo_stack", args.id));
    }
    let to_trash = repo.photos_to_trash_except_cover(args.id)?;
    if to_trash.is_empty() {
        return Err(CommandError::Conflict {
            reason: "stack has no non-cover photos to trash".into(),
        });
    }
    let count = smriti::services::trash::TrashService::trash_photos(&db.conn, &to_trash)? as u64;
    repo.delete_stack(args.id)?;
    Ok(StackCountResultDto { count })
}

#[tauri::command]
pub async fn stacks_refresh(state: State<'_, AppState>) -> CommandResult<StackRefreshDto> {
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(smriti::services::PhotoStackService::refresh(
            &conn,
            &drive_root,
        )?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("stack refresh worker failed: {e}"),
    })??;
    Ok(StackRefreshDto {
        stacks_found: result.stacks_found as u64,
    })
}
