//! Albums + AI suggestions (read-only commands for M1).

use serde::{Deserialize, Serialize};
use tauri::State;

use photovault::db::album_repo::AlbumRepo;
use photovault::db::album_suggestion_repo::AlbumSuggestionRepo;

use crate::dto::{AlbumDto, AlbumSuggestionDto, PhotoSummaryDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

fn fetch_album(repo: &AlbumRepo, id: i64) -> CommandResult<AlbumDto> {
    let all = repo.get_all()?;
    all.into_iter()
        .find(|a| a.id == id)
        .map(Into::into)
        .ok_or_else(|| CommandError::not_found("album", id))
}

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

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct AlbumsCreateArgs {
    pub name: String,
    #[serde(default)]
    pub photo_ids: Vec<i64>,
}

#[tauri::command]
pub async fn albums_create(
    state: State<'_, AppState>,
    args: AlbumsCreateArgs,
) -> CommandResult<AlbumDto> {
    if args.name.trim().is_empty() {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "must not be empty".into(),
        });
    }
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    let id = repo.create(&args.name)?;
    if !args.photo_ids.is_empty() {
        repo.add_photos(id, &args.photo_ids)?;
        repo.auto_pick_cover(id)?;
    }
    fetch_album(&repo, id)
}

#[derive(Debug, Deserialize)]
pub struct AlbumsRenameArgs {
    pub id: i64,
    pub name: String,
}

#[tauri::command]
pub async fn albums_rename(
    state: State<'_, AppState>,
    args: AlbumsRenameArgs,
) -> CommandResult<AlbumDto> {
    if args.name.trim().is_empty() {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "must not be empty".into(),
        });
    }
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    repo.rename(args.id, &args.name)?;
    fetch_album(&repo, args.id)
}

#[derive(Debug, Deserialize)]
pub struct AlbumsDeleteArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn albums_delete(
    state: State<'_, AppState>,
    args: AlbumsDeleteArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    AlbumRepo::new(&db.conn).delete(args.id)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct AlbumsAddPhotosArgs {
    pub id: i64,
    pub photo_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct AlbumsAddRemoveResult {
    pub count: u64,
}

#[tauri::command]
pub async fn albums_add_photos(
    state: State<'_, AppState>,
    args: AlbumsAddPhotosArgs,
) -> CommandResult<AlbumsAddRemoveResult> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let count = AlbumRepo::new(&db.conn).add_photos(args.id, &args.photo_ids)?;
    Ok(AlbumsAddRemoveResult {
        count: count as u64,
    })
}

#[derive(Debug, Deserialize)]
pub struct AlbumsRemovePhotosArgs {
    pub id: i64,
    pub photo_ids: Vec<i64>,
}

#[tauri::command]
pub async fn albums_remove_photos(
    state: State<'_, AppState>,
    args: AlbumsRemovePhotosArgs,
) -> CommandResult<AlbumsAddRemoveResult> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    AlbumRepo::new(&db.conn).remove_photos(args.id, &args.photo_ids)?;
    Ok(AlbumsAddRemoveResult {
        count: args.photo_ids.len() as u64,
    })
}

#[derive(Debug, Deserialize)]
pub struct AlbumsAutoPickCoverArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn albums_auto_pick_cover(
    state: State<'_, AppState>,
    args: AlbumsAutoPickCoverArgs,
) -> CommandResult<AlbumDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    repo.auto_pick_cover(args.id)?;
    fetch_album(&repo, args.id)
}

// ---------- album suggestions: mutations ----------

#[derive(Debug, Default, Deserialize)]
pub struct AlbumsSuggestionsRunDetectionArgs {
    pub home_city_override: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SuggestionDiagnosticsDto {
    pub total_photos_with_date: i64,
    pub photos_with_city: i64,
    pub home_city: Option<String>,
    pub trip_candidates_passed: usize,
    pub event_windows: usize,
    pub created: usize,
}

#[tauri::command]
pub async fn albums_suggestions_run_detection(
    state: State<'_, AppState>,
    args: AlbumsSuggestionsRunDetectionArgs,
) -> CommandResult<SuggestionDiagnosticsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let (suggestions, diag) =
        photovault::services::album_suggestions::detect_suggestions_with_diagnostics(
            &db.conn,
            args.home_city_override.as_deref(),
        );
    Ok(SuggestionDiagnosticsDto {
        total_photos_with_date: diag.total_photos_with_date,
        photos_with_city: diag.photos_with_city,
        home_city: diag.home_city,
        trip_candidates_passed: diag.trip_candidates_passed,
        event_windows: diag.event_windows,
        created: suggestions.len(),
    })
}

#[derive(Debug, Deserialize)]
pub struct AlbumsSuggestionsAcceptArgs {
    pub id: i64,
    pub name: Option<String>,
}

#[tauri::command]
pub async fn albums_suggestions_accept(
    state: State<'_, AppState>,
    args: AlbumsSuggestionsAcceptArgs,
) -> CommandResult<AlbumDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let suggestion_repo = AlbumSuggestionRepo::new(&db.conn);

    let s = suggestion_repo
        .get_pending()?
        .into_iter()
        .find(|s| s.id == args.id)
        .ok_or_else(|| CommandError::not_found("album_suggestion", args.id))?;

    let album_repo = AlbumRepo::new(&db.conn);
    let name = args.name.unwrap_or(s.title.clone());
    let album_id = album_repo.create(&name)?;
    let photo_ids = s.photo_ids();
    if !photo_ids.is_empty() {
        album_repo.add_photos(album_id, &photo_ids)?;
        album_repo.auto_pick_cover(album_id)?;
    }
    suggestion_repo.accept(args.id)?;
    fetch_album(&album_repo, album_id)
}

#[derive(Debug, Deserialize)]
pub struct AlbumsSuggestionsDismissArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn albums_suggestions_dismiss(
    state: State<'_, AppState>,
    args: AlbumsSuggestionsDismissArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    AlbumSuggestionRepo::new(&db.conn).dismiss(args.id)?;
    Ok(())
}
