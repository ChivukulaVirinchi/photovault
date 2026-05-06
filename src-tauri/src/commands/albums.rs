//! Albums + AI suggestions (read-only commands for M1).

use serde::Deserialize;
use tauri::State;

use photovault::db::album_repo::AlbumRepo;
use photovault::db::album_suggestion_repo::AlbumSuggestionRepo;

use crate::dto::{AlbumDto, AlbumSuggestionDto, PhotoSummaryDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn albums_list(state: State<'_, AppState>) -> CommandResult<Vec<AlbumDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    Ok(repo.get_all()?.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct AlbumsGetArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn albums_get(
    state: State<'_, AppState>,
    args: AlbumsGetArgs,
) -> CommandResult<AlbumDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    let all = repo.get_all()?;
    let album = all
        .into_iter()
        .find(|a| a.id == args.id)
        .ok_or_else(|| CommandError::not_found("album", args.id))?;
    Ok(album.into())
}

#[tauri::command]
pub async fn albums_suggestions_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<AlbumSuggestionDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumSuggestionRepo::new(&db.conn);
    Ok(repo.get_pending()?.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct AlbumsSuggestionsPreviewArgs {
    pub id: i64,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn albums_suggestions_preview(
    state: State<'_, AppState>,
    args: AlbumsSuggestionsPreviewArgs,
) -> CommandResult<Vec<PhotoSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let suggestion_repo = AlbumSuggestionRepo::new(&db.conn);
    let pending = suggestion_repo.get_pending()?;
    let s = pending
        .into_iter()
        .find(|s| s.id == args.id)
        .ok_or_else(|| CommandError::not_found("album_suggestion", args.id))?;

    let limit = args.limit.unwrap_or(60) as usize;
    let mut ids = s.photo_ids();
    ids.truncate(limit);

    let photo_repo = photovault::db::PhotoRepo::new(&db.conn);
    let photos = photo_repo.get_by_ids(&ids)?;
    Ok(photos.iter().map(PhotoSummaryDto::from).collect())
}
