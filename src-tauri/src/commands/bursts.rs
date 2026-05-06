//! Burst groups (read-only listing).

use serde::Deserialize;
use tauri::State;

use photovault::db::burst_repo::BurstRepo;

use crate::dto::{BurstGroupDto, BurstGroupSummaryDto, BurstMemberDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn bursts_list(state: State<'_, AppState>) -> CommandResult<Vec<BurstGroupSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    Ok(repo.get_all_groups()?.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct BurstsGetGroupArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn bursts_get_group(
    state: State<'_, AppState>,
    args: BurstsGetGroupArgs,
) -> CommandResult<BurstGroupDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    let members = repo.get_group_members(args.id)?;
    if members.is_empty() {
        return Err(CommandError::not_found("burst_group", args.id));
    }
    Ok(BurstGroupDto {
        id: args.id,
        members: members.into_iter().map(BurstMemberDto::from).collect(),
    })
}
