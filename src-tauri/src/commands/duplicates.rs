//! Duplicate groups (read-only listing).

use serde::Deserialize;
use tauri::State;

use photovault::db::duplicate_repo::DuplicateRepo;
use photovault::services::duplicate_detector::DuplicateDetector;

use crate::dto::{DuplicateGroupDto, DuplicateGroupSummaryDto, DuplicateMemberDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn duplicates_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<DuplicateGroupSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DuplicateRepo::new(&db.conn);
    Ok(repo.get_all_groups()?.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct DuplicatesGetGroupArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn duplicates_get_group(
    state: State<'_, AppState>,
    args: DuplicatesGetGroupArgs,
) -> CommandResult<DuplicateGroupDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DuplicateRepo::new(&db.conn);
    let members = repo.get_group_members(args.id)?;
    if members.is_empty() {
        return Err(CommandError::not_found("duplicate_group", args.id));
    }
    Ok(DuplicateGroupDto {
        id: args.id,
        members: members.into_iter().map(DuplicateMemberDto::from).collect(),
    })
}

#[derive(Debug, serde::Serialize)]
pub struct WastedSpaceDto {
    pub bytes: u64,
}

#[tauri::command]
pub async fn duplicates_wasted_space(state: State<'_, AppState>) -> CommandResult<WastedSpaceDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let bytes = DuplicateDetector::calculate_wasted_space(&db.conn)?;
    Ok(WastedSpaceDto { bytes })
}
