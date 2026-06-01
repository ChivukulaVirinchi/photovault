//! Albums + AI suggestions (read-only commands for M1).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::album_repo::AlbumRepo;
use smriti::db::album_suggestion_repo::AlbumSuggestionRepo;
use smriti::db::PhotoRepo;

use crate::dto::{AlbumDto, AlbumSuggestionDto, JobIdDto, PhotoSummaryDto};
use crate::events::{JobProgress, EV_ALBUM_SUGGESTIONS_COMPLETE, EV_ALBUM_SUGGESTIONS_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::thumbnail_upgrade::{upgrade_covers_to_medium, CoverInput};
use crate::{CommandError, CommandResult};

pub const FAVORITES_ALBUM_ID: i64 = -1;
const FAVORITES_ALBUM_NAME: &str = "Favourites";

fn is_reserved_album_name(name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    normalized == "favourites" || normalized == "favorites"
}

fn reject_virtual_album(field: &str) -> CommandError {
    CommandError::Validation {
        field: field.into(),
        reason: "Favourites is a smart album and cannot be modified".into(),
    }
}

fn fetch_album(repo: &AlbumRepo, id: i64) -> CommandResult<AlbumDto> {
    let all = repo.get_all()?;
    all.into_iter()
        .find(|a| a.id == id)
        .map(Into::into)
        .ok_or_else(|| CommandError::not_found("album", id))
}

fn favorites_album(conn: &rusqlite::Connection) -> CommandResult<Option<AlbumDto>> {
    let repo = PhotoRepo::new(conn);
    let Some((count, cover_photo_id, cover_thumbnail_path, start, end)) =
        repo.favorites_album_summary()?
    else {
        return Ok(None);
    };
    let now = chrono::Utc::now().to_rfc3339();
    Ok(Some(AlbumDto {
        id: FAVORITES_ALBUM_ID,
        name: FAVORITES_ALBUM_NAME.to_string(),
        photo_count: count,
        cover_photo_id,
        cover_thumbnail_path,
        date_range_start: start,
        date_range_end: end,
        created_at: now.clone(),
        updated_at: now,
        is_virtual: true,
    }))
}

/// Collects (album_index, file_path, file_hash, orientation) for every
/// album with a cover photo. Used to upgrade `cover_thumbnail_path`
/// from the stored Small variant to Medium before returning to the UI.
fn collect_album_cover_inputs(
    conn: &rusqlite::Connection,
    albums: &[AlbumDto],
) -> rusqlite::Result<Vec<CoverInput>> {
    if albums.is_empty() {
        return Ok(Vec::new());
    }
    let repo = smriti::db::PhotoRepo::new(conn);
    let mut out = Vec::with_capacity(albums.len());
    for (idx, a) in albums.iter().enumerate() {
        let Some(cover_id) = a.cover_photo_id else {
            continue;
        };
        if let Some(p) = repo.get_by_id(cover_id)? {
            out.push((idx, p.file_path, p.file_hash, p.orientation));
        }
    }
    Ok(out)
}

/// Same for AI suggestions, which carry their own cover_photo_id field.
fn collect_suggestion_cover_inputs(
    conn: &rusqlite::Connection,
    suggestions: &[AlbumSuggestionDto],
) -> rusqlite::Result<Vec<CoverInput>> {
    if suggestions.is_empty() {
        return Ok(Vec::new());
    }
    let repo = smriti::db::PhotoRepo::new(conn);
    let mut out = Vec::with_capacity(suggestions.len());
    for (idx, s) in suggestions.iter().enumerate() {
        let Some(cover_id) = s.cover_photo_id else {
            continue;
        };
        if let Some(p) = repo.get_by_id(cover_id)? {
            out.push((idx, p.file_path, p.file_hash, p.orientation));
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn albums_list(state: State<'_, AppState>) -> CommandResult<Vec<AlbumDto>> {
    let (mut albums, inputs, drive_root, thumbs) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        let db = lib.db.lock().await;
        let repo = AlbumRepo::new(&db.conn);
        let mut albums: Vec<AlbumDto> = repo.get_all()?.into_iter().map(Into::into).collect();
        if let Some(favorites) = favorites_album(&db.conn)? {
            albums.insert(0, favorites);
        }
        let inputs = collect_album_cover_inputs(&db.conn, &albums)?;
        (
            albums,
            inputs,
            lib.drive_root.clone(),
            lib.thumbnails.clone(),
        )
    };

    let upgrades = upgrade_covers_to_medium(thumbs, drive_root, inputs).await;
    for (idx, path) in upgrades {
        if let (Some(a), Some(p)) = (albums.get_mut(idx), path) {
            a.cover_thumbnail_path = Some(p);
        }
    }
    Ok(albums)
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
    let (mut album, inputs, drive_root, thumbs) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        let db = lib.db.lock().await;
        let album = if args.id == FAVORITES_ALBUM_ID {
            favorites_album(&db.conn)?.ok_or_else(|| CommandError::not_found("album", args.id))?
        } else {
            let repo = AlbumRepo::new(&db.conn);
            let all = repo.get_all()?;
            let album_model = all
                .into_iter()
                .find(|a| a.id == args.id)
                .ok_or_else(|| CommandError::not_found("album", args.id))?;
            album_model.into()
        };
        let inputs = collect_album_cover_inputs(&db.conn, std::slice::from_ref(&album))?;
        (
            album,
            inputs,
            lib.drive_root.clone(),
            lib.thumbnails.clone(),
        )
    };

    let upgrades = upgrade_covers_to_medium(thumbs, drive_root, inputs).await;
    if let Some((_, Some(p))) = upgrades.into_iter().next() {
        album.cover_thumbnail_path = Some(p);
    }
    Ok(album)
}

#[tauri::command]
pub async fn albums_suggestions_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<AlbumSuggestionDto>> {
    let (mut suggestions, inputs, drive_root, thumbs) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        let db = lib.db.lock().await;
        let repo = AlbumSuggestionRepo::new(&db.conn);
        let suggestions: Vec<AlbumSuggestionDto> =
            repo.get_pending()?.into_iter().map(Into::into).collect();
        let inputs = collect_suggestion_cover_inputs(&db.conn, &suggestions)?;
        (
            suggestions,
            inputs,
            lib.drive_root.clone(),
            lib.thumbnails.clone(),
        )
    };

    let upgrades = upgrade_covers_to_medium(thumbs, drive_root, inputs).await;
    for (idx, path) in upgrades {
        if let (Some(s), Some(p)) = (suggestions.get_mut(idx), path) {
            s.cover_thumbnail_path = Some(p);
        }
    }
    Ok(suggestions)
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

    let photo_repo = smriti::db::PhotoRepo::new(&db.conn);
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
    if is_reserved_album_name(&args.name) {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "reserved for the Favourites smart album".into(),
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
    if args.id == FAVORITES_ALBUM_ID {
        return Err(reject_virtual_album("id"));
    }
    if args.name.trim().is_empty() {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "must not be empty".into(),
        });
    }
    if is_reserved_album_name(&args.name) {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "reserved for the Favourites smart album".into(),
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
    if args.id == FAVORITES_ALBUM_ID {
        return Err(reject_virtual_album("id"));
    }
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
    if args.id == FAVORITES_ALBUM_ID {
        return Err(reject_virtual_album("id"));
    }
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
    if args.id == FAVORITES_ALBUM_ID {
        return Err(reject_virtual_album("id"));
    }
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
    if args.id == FAVORITES_ALBUM_ID {
        return Err(reject_virtual_album("id"));
    }
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

/// Suggestion-detection complete payload — same fields as the legacy
/// synchronous return value, now delivered via the
/// `album_suggestions:complete` event so callers can fire-and-forget.
#[derive(Debug, Serialize, Clone)]
pub struct AlbumSuggestionsCompleteDto {
    pub job_id: String,
    pub total_photos_with_date: i64,
    pub photos_with_city: i64,
    pub photos_with_gps: i64,
    pub home_city: Option<String>,
    pub trip_candidates_passed: usize,
    pub event_windows: usize,
    pub created: usize,
    pub elapsed_ms: u64,
    /// Pre-formatted summary the UI surfaces as a toast. Computing it
    /// in the backend means the frontend doesn't need a one-shot
    /// `listen()` subscription on the page (which races with the event
    /// arrival on fast detections); the global jobs store picks the
    /// message up via the standard `message` field on `JobProgress`-
    /// shaped payloads.
    pub message: String,
}

/// Kick off trip / event suggestion detection in the background.
///
/// Detection used to run synchronously inside the IPC handler and held
/// the DB lock for the full duration — on libraries with thousands of
/// dated photos this blocked the foreground for many seconds with no
/// feedback. Now: the handler returns a job_id immediately, the work
/// runs against a secondary SQLite connection, and progress + the
/// final diagnostics flow through the standard
/// `album_suggestions:progress|complete` event channels.
#[tauri::command]
pub async fn albums_suggestions_run_detection(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AlbumsSuggestionsRunDetectionArgs,
) -> CommandResult<JobIdDto> {
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };

    let job = jobs::start_job(&state, JobKind::AlbumSuggestions).await?;
    let job_id = job.id.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    let home_override = args.home_city_override.clone();

    tokio::task::spawn_blocking(move || {
        // Up-front "starting" tick so the UI's progress chip shows a
        // running job before the SQL work even begins.
        emit(
            &app_clone,
            EV_ALBUM_SUGGESTIONS_PROGRESS,
            JobProgress {
                job_id: job_id_clone.clone(),
                stage: "scanning".into(),
                processed: 0,
                total: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some("Scanning photo metadata…".into()),
            },
        );

        let db_path = smriti::db::db_path_for(&drive_root);
        let conn = match smriti::db::open_secondary(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("album suggestions: failed to open secondary db: {}", e);
                emit(
                    &app_clone,
                    EV_ALBUM_SUGGESTIONS_COMPLETE,
                    AlbumSuggestionsCompleteDto {
                        job_id: job_id_clone.clone(),
                        total_photos_with_date: 0,
                        photos_with_city: 0,
                        photos_with_gps: 0,
                        home_city: None,
                        trip_candidates_passed: 0,
                        event_windows: 0,
                        created: 0,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        message: format!("Couldn't open library: {}", e),
                    },
                );
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    let app_for_finish = app_clone.clone();
                    let job_id = job_id_clone.clone();
                    handle.spawn(async move {
                        let st: tauri::State<AppState> = app_for_finish.state();
                        jobs::finish_job(&st, &job_id).await;
                    });
                }
                return;
            }
        };

        emit(
            &app_clone,
            EV_ALBUM_SUGGESTIONS_PROGRESS,
            JobProgress {
                job_id: job_id_clone.clone(),
                stage: "detecting".into(),
                processed: 0,
                total: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some("Looking for trips and events…".into()),
            },
        );

        let (suggestions, diag) =
            smriti::services::album_suggestions::detect_suggestions_with_diagnostics(
                &conn,
                home_override.as_deref(),
            );

        // Friendly, context-aware summary so the user understands why
        // a run found nothing on a metadata-poor library instead of
        // staring at a silent indicator. Order matters — more specific
        // diagnoses first.
        let message = if !suggestions.is_empty() {
            format!(
                "{} suggestion{} ready to review.",
                suggestions.len(),
                if suggestions.len() == 1 { "" } else { "s" }
            )
        } else if diag.total_photos_with_date < 20 {
            // Trip detection needs at least ~20 dated photos to find
            // anything worth surfacing.
            "Need more dated photos for trip detection (about 20+).".to_string()
        } else if diag.photos_with_gps == 0 {
            // Honest answer for old / DSLR libraries: no GPS to work
            // with, and "Fill in place names" can't synthesise GPS
            // where none exists.
            "These photos have no GPS metadata, so trip detection can't run on this library."
                .to_string()
        } else if diag.photos_with_city == 0 {
            "Photos have GPS coordinates but no place names. Run ‘Fill in place names’ in Settings to unlock trip suggestions.".to_string()
        } else {
            "No new patterns this round.".to_string()
        };

        emit(
            &app_clone,
            EV_ALBUM_SUGGESTIONS_COMPLETE,
            AlbumSuggestionsCompleteDto {
                job_id: job_id_clone.clone(),
                total_photos_with_date: diag.total_photos_with_date,
                photos_with_city: diag.photos_with_city,
                photos_with_gps: diag.photos_with_gps,
                home_city: diag.home_city,
                trip_candidates_passed: diag.trip_candidates_passed,
                event_windows: diag.event_windows,
                created: suggestions.len(),
                elapsed_ms: started.elapsed().as_millis() as u64,
                message,
            },
        );

        // Bridge back to the async runtime to release the registry slot.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let app_for_finish = app_clone.clone();
            let job_id = job_id_clone.clone();
            handle.spawn(async move {
                let st: tauri::State<AppState> = app_for_finish.state();
                jobs::finish_job(&st, &job_id).await;
            });
        }
    });

    Ok(JobIdDto { job_id })
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
    if is_reserved_album_name(&name) {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "reserved for the Favourites smart album".into(),
        });
    }
    let album_id = album_repo.create(&name)?;
    let photo_ids = s.photo_ids();
    if !photo_ids.is_empty() {
        album_repo.add_photos(album_id, &photo_ids)?;
        album_repo.auto_pick_cover(album_id)?;
    }
    suggestion_repo.accept(args.id)?;
    fetch_album(&album_repo, album_id)
}

#[derive(Debug, Serialize)]
pub struct AlbumSuggestionsResetResult {
    pub dropped: u64,
}

/// Wipe the entire suggestions queue — pending AND dismissed. Lets a
/// user who reflexively dismissed everything start fresh after the
/// detector improves. The next "Detect" run repopulates from
/// scratch.
#[tauri::command]
pub async fn albums_suggestions_reset_all(
    state: State<'_, AppState>,
) -> CommandResult<AlbumSuggestionsResetResult> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let dropped = db.conn.execute("DELETE FROM album_suggestions", [])? as u64;
    Ok(AlbumSuggestionsResetResult { dropped })
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
