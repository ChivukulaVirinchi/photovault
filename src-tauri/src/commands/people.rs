//! People (face clusters) — read-only commands for M1.

use serde::Deserialize;
use tauri::State;

use photovault::db::face_repo::FaceRepo;

use crate::dto::{PersonDto, ReviewItemDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Default, Deserialize)]
pub struct PeopleListArgs {
    #[serde(default)]
    pub named_only: bool,
    pub min_photos: Option<i64>,
}

#[tauri::command]
pub async fn people_list(
    state: State<'_, AppState>,
    args: PeopleListArgs,
) -> CommandResult<Vec<PersonDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let mut clusters = repo.get_all_clusters()?;
    if args.named_only {
        clusters.retain(|c| c.name.is_some() && !c.name.as_deref().unwrap_or("").is_empty());
    }
    if let Some(min) = args.min_photos {
        clusters.retain(|c| c.photo_count >= min);
    }
    Ok(clusters.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct PeopleGetArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn people_get(
    state: State<'_, AppState>,
    args: PeopleGetArgs,
) -> CommandResult<PersonDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let clusters = repo.get_all_clusters()?;
    let c = clusters
        .into_iter()
        .find(|c| c.id == args.id)
        .ok_or_else(|| CommandError::not_found("person", args.id))?;
    Ok(c.into())
}

#[derive(Debug, Default, Deserialize)]
pub struct PeopleReviewQueueArgs {
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn people_review_queue(
    state: State<'_, AppState>,
    args: PeopleReviewQueueArgs,
) -> CommandResult<Vec<ReviewItemDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let limit = args.limit.unwrap_or(20).min(200) as usize;
    let items = repo.get_review_queue_items(limit)?;
    Ok(items.into_iter().map(Into::into).collect())
}
