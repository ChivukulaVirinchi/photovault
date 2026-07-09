//! Albums + AI suggestions (read-only commands for M1).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::album_repo::AlbumRepo;
use smriti::db::album_suggestion_repo::AlbumSuggestionRepo;
use smriti::db::PhotoRepo;

use crate::dto::{AlbumDto, AlbumSuggestionDto, JobIdDto, PhotoSummaryDto};
use crate::events::{
    JobProgress, EV_ALBUM_EXPORT_COMPLETE, EV_ALBUM_EXPORT_PROGRESS, EV_ALBUM_SUGGESTIONS_COMPLETE,
    EV_ALBUM_SUGGESTIONS_PROGRESS,
};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::thumbnail_upgrade::{upgrade_covers_to_medium, CoverInput};
use crate::{CommandError, CommandResult};

pub const FAVORITES_ALBUM_ID: i64 = -1;
const FAVORITES_ALBUM_NAME: &str = "Favourites";
const MAX_BULK_PHOTO_IDS: usize = 10_000;

fn normalize_bulk_photo_ids(ids: Vec<i64>, field: &str) -> CommandResult<Vec<i64>> {
    if ids.len() > MAX_BULK_PHOTO_IDS {
        return Err(CommandError::Validation {
            field: field.into(),
            reason: format!("too many ids; maximum is {MAX_BULK_PHOTO_IDS}"),
        });
    }
    let mut seen = HashSet::new();
    Ok(ids
        .into_iter()
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect())
}

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
        photos_added: None,
        cover_photo_id,
        cover_thumbnail_path,
        date_range_start: start,
        date_range_end: end,
        created_at: now.clone(),
        updated_at: now,
        is_virtual: true,
        created_by: "user".into(),
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
        if let Some(p) = repo.get_by_id(cover_id)?.filter(|p| !p.is_trashed) {
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
        if let Some(p) = repo.get_by_id(cover_id)?.filter(|p| !p.is_trashed) {
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

#[derive(Debug, Deserialize)]
pub struct AlbumsExportArgs {
    pub album_id: i64,
    pub destination_dir: Option<String>,
    pub folder_name: Option<String>,
}

#[derive(Debug, Clone)]
struct AlbumExportItem {
    photo_id: i64,
    source_path: PathBuf,
    file_name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct AlbumExportCompleteDto {
    pub job_id: String,
    pub stage: String,
    pub processed: u64,
    pub total: Option<u64>,
    pub album_id: i64,
    pub folder_path: String,
    pub exported: u64,
    pub skipped_missing: u64,
    pub failed: u64,
    pub elapsed_ms: u64,
    pub message: String,
}

fn default_export_root(drive_root: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(profile)
                .join("Pictures")
                .join("Smriti Exports");
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Pictures").join("Smriti Exports");
        }
    }
    drive_root.join("Smriti Exports")
}

fn sanitize_export_folder_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.trim().chars() {
        let replacement =
            matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control();
        out.push(if replacement { '-' } else { ch });
    }
    let compact = out
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(['.', ' ', '-'])
        .to_string();
    if compact.is_empty() || is_windows_reserved_file_stem(&compact) {
        "Smriti Album".to_string()
    } else {
        compact.chars().take(120).collect()
    }
}

fn is_windows_reserved_file_stem(name: &str) -> bool {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .trim_end_matches('.')
        .to_ascii_lowercase();
    matches!(
        stem.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

fn unique_export_folder(root: &Path, preferred_name: &str) -> PathBuf {
    let base = sanitize_export_folder_name(preferred_name);
    let first = root.join(&base);
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let candidate = root.join(format!("{base} {n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    root.join(format!("{base} {}", chrono::Utc::now().timestamp()))
}

fn unique_file_path(folder: &Path, file_name: &str, reserved: &mut HashSet<String>) -> PathBuf {
    let fallback = "photo".to_string();
    let clean_name = sanitize_export_folder_name(file_name);
    let clean_name = if clean_name.is_empty() {
        fallback
    } else {
        clean_name
    };
    let path = Path::new(&clean_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("photo");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    for n in 1..10_000 {
        let name = if n == 1 {
            clean_name.clone()
        } else if ext.is_empty() {
            format!("{stem}-{n}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let key = name.to_lowercase();
        let candidate = folder.join(&name);
        if reserved.insert(key) && !candidate.exists() {
            return candidate;
        }
    }
    folder.join(format!("{stem}-{}", chrono::Utc::now().timestamp()))
}

fn export_message(exported: u64, skipped_missing: u64, failed: u64) -> String {
    if failed == 0 && skipped_missing == 0 {
        format!(
            "Exported {exported} {}",
            if exported == 1 { "item" } else { "items" }
        )
    } else {
        format!("Exported {exported}, skipped {skipped_missing} missing, failed {failed}")
    }
}

fn collect_export_items(
    conn: &rusqlite::Connection,
    album_id: i64,
    drive_root: &Path,
) -> CommandResult<Vec<AlbumExportItem>> {
    let mut stmt = if album_id == FAVORITES_ALBUM_ID {
        conn.prepare(
            "SELECT id, file_path, file_name
             FROM photos
             WHERE is_favorite = TRUE AND is_trashed = 0
             ORDER BY date_taken IS NULL ASC, date_taken ASC, id ASC",
        )?
    } else {
        conn.prepare(
            "SELECT p.id, p.file_path, p.file_name
             FROM album_photos ap
             JOIN photos p ON p.id = ap.photo_id
             WHERE ap.album_id = ?1 AND p.is_trashed = 0
             ORDER BY ap.added_at ASC, p.id ASC",
        )?
    };

    let rows = if album_id == FAVORITES_ALBUM_ID {
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(rusqlite::params![album_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };

    rows.into_iter()
        .map(|(photo_id, relative_path, file_name)| {
            let source_path =
                smriti::services::path_util::safe_join_relative(drive_root, &relative_path)
                    .map_err(|reason| CommandError::Validation {
                        field: "photo.file_path".into(),
                        reason,
                    })?;
            Ok(AlbumExportItem {
                photo_id,
                source_path,
                file_name,
            })
        })
        .collect()
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
pub async fn albums_photo_ids(
    state: State<'_, AppState>,
    args: AlbumsGetArgs,
) -> CommandResult<Vec<i64>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    if args.id == FAVORITES_ALBUM_ID {
        let mut stmt = db.conn.prepare(
            "SELECT id
               FROM photos
              WHERE is_favorite = TRUE AND is_trashed = 0
              ORDER BY date_taken IS NULL ASC, date_taken DESC, id DESC",
        )?;
        let ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        return Ok(ids);
    }
    let repo = AlbumRepo::new(&db.conn);
    fetch_album(&repo, args.id)?;
    Ok(repo.get_album_photo_ids(args.id)?)
}

#[tauri::command]
pub async fn albums_export(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AlbumsExportArgs,
) -> CommandResult<JobIdDto> {
    if state
        .jobs
        .lock()
        .await
        .has_any_of_kind(JobKind::AlbumExport)
    {
        return Err(CommandError::Conflict {
            reason: "an album export is already in progress".into(),
        });
    }

    let (album_name, items, export_root) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        let db = lib.db.lock().await;
        let album_name = if args.album_id == FAVORITES_ALBUM_ID {
            FAVORITES_ALBUM_NAME.to_string()
        } else {
            let repo = AlbumRepo::new(&db.conn);
            let album = repo
                .get_all()?
                .into_iter()
                .find(|a| a.id == args.album_id)
                .ok_or_else(|| CommandError::not_found("album", args.album_id))?;
            album.name
        };
        let items = collect_export_items(&db.conn, args.album_id, &lib.drive_root)?;
        let export_root = args
            .destination_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| default_export_root(&lib.drive_root));
        (album_name, items, export_root)
    };

    if items.is_empty() {
        return Err(CommandError::Validation {
            field: "album_id".into(),
            reason: "album has no exportable photos or videos".into(),
        });
    }

    let job = jobs::start_job(&state, JobKind::AlbumExport).await?;
    let job_id = job.id.clone();
    let started = job.started_at;
    let cancel = job.cancel.clone();

    let folder_name = args.folder_name.as_deref().unwrap_or(&album_name);
    if let Err(e) = std::fs::create_dir_all(&export_root) {
        jobs::finish_job(&state, &job_id).await;
        return Err(e.into());
    }
    let export_folder = unique_export_folder(&export_root, folder_name);
    if let Err(e) = std::fs::create_dir_all(&export_folder) {
        jobs::finish_job(&state, &job_id).await;
        return Err(e.into());
    }

    let total = items.len() as u64;
    let app_clone = app.clone();
    let app_for_finish = app.clone();
    let job_id_clone = job_id.clone();
    let export_folder_clone = export_folder.clone();
    let album_id = args.album_id;
    let finish_handle = tokio::runtime::Handle::current();

    emit(
        &app,
        EV_ALBUM_EXPORT_PROGRESS,
        JobProgress {
            job_id: job_id.clone(),
            stage: "copying".into(),
            processed: 0,
            total: Some(total),
            elapsed_ms: 0,
            eta_ms: None,
            message: Some(format!("Exporting to {}", export_folder.display())),
        },
    );

    tokio::task::spawn_blocking(move || {
        let mut reserved = HashSet::new();
        let mut exported = 0_u64;
        let mut skipped_missing = 0_u64;
        let mut failed = 0_u64;

        for (idx, item) in items.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let processed = idx as u64;
            emit(
                &app_clone,
                EV_ALBUM_EXPORT_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "copying".into(),
                    processed,
                    total: Some(total),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(item.file_name.clone()),
                },
            );

            if !item.source_path.exists() {
                skipped_missing += 1;
                continue;
            }
            let dest = unique_file_path(&export_folder_clone, &item.file_name, &mut reserved);
            match std::fs::copy(&item.source_path, &dest) {
                Ok(_) => exported += 1,
                Err(err) => {
                    failed += 1;
                    tracing::warn!(
                        "album export: failed to copy photo {} from {}: {}",
                        item.photo_id,
                        item.source_path.display(),
                        err
                    );
                }
            }
        }

        let message = if cancel.load(Ordering::Relaxed) {
            "Export cancelled".to_string()
        } else {
            export_message(exported, skipped_missing, failed)
        };
        let complete = AlbumExportCompleteDto {
            job_id: job_id_clone.clone(),
            stage: "complete".into(),
            processed: exported + skipped_missing + failed,
            total: Some(total),
            album_id,
            folder_path: export_folder_clone.display().to_string(),
            exported,
            skipped_missing,
            failed,
            elapsed_ms: started.elapsed().as_millis() as u64,
            message: message.clone(),
        };
        emit(&app_clone, EV_ALBUM_EXPORT_COMPLETE, complete);

        finish_handle.spawn(async move {
            let st: tauri::State<AppState> = app_for_finish.state();
            jobs::finish_job(&st, &job_id_clone).await;
        });
    });

    Ok(JobIdDto { job_id })
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

    let limit = args.limit.unwrap_or(60).clamp(1, 500) as usize;
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
    let name = args.name.trim();
    if name.is_empty() {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "must not be empty".into(),
        });
    }
    if is_reserved_album_name(name) {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "reserved for the Favourites smart album".into(),
        });
    }
    let photo_ids = normalize_bulk_photo_ids(args.photo_ids, "photo_ids")?;
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    let id = repo.create(name)?;
    let mut photos_added = 0;
    if !photo_ids.is_empty() {
        photos_added = repo.add_photos(id, &photo_ids)?;
        if photos_added > 0 {
            repo.auto_pick_cover(id)?;
        }
    }
    let mut album = fetch_album(&repo, id)?;
    album.photos_added = Some(photos_added as u64);
    Ok(album)
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
    let name = args.name.trim();
    if name.is_empty() {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "must not be empty".into(),
        });
    }
    if is_reserved_album_name(name) {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "reserved for the Favourites smart album".into(),
        });
    }
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    fetch_album(&repo, args.id)?;
    repo.rename(args.id, name)?;
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
    let repo = AlbumRepo::new(&db.conn);
    fetch_album(&repo, args.id)?;
    repo.delete(args.id)?;
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
    let photo_ids = normalize_bulk_photo_ids(args.photo_ids, "photo_ids")?;
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    fetch_album(&repo, args.id)?;
    let count = repo.add_photos(args.id, &photo_ids)?;
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
    let photo_ids = normalize_bulk_photo_ids(args.photo_ids, "photo_ids")?;
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = AlbumRepo::new(&db.conn);
    fetch_album(&repo, args.id)?;
    let count = repo.remove_photos(args.id, &photo_ids)?;
    Ok(AlbumsAddRemoveResult {
        count: count as u64,
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
    let cancel = job.cancel.clone();
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
                    EV_ALBUM_SUGGESTIONS_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "error".into(),
                        processed: 0,
                        total: Some(1),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("Couldn't open library: {}", e)),
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
            smriti::services::album_suggestions::detect_suggestions_with_diagnostics_cancel(
                &conn,
                home_override.as_deref(),
                Some(cancel.as_ref()),
            );

        // Friendly, context-aware summary so the user understands why
        // a run found nothing on a metadata-poor library instead of
        // staring at a silent indicator. Order matters — more specific
        // diagnoses first.
        let was_cancelled = cancel.load(std::sync::atomic::Ordering::Relaxed);
        let message = if was_cancelled {
            "Album suggestion detection cancelled.".to_string()
        } else if !suggestions.is_empty() {
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
                created: if was_cancelled { 0 } else { suggestions.len() },
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
    match suggestion_repo.accept(args.id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("album_suggestion", args.id));
        }
        Err(e) => return Err(e.into()),
    }
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
    match AlbumSuggestionRepo::new(&db.conn).dismiss(args.id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("album_suggestion", args.id));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_folder_name_is_filesystem_safe() {
        assert_eq!(
            sanitize_export_folder_name(r#" Goa: 2025 / Day * 1? "#),
            "Goa- 2025 - Day - 1"
        );
        assert_eq!(sanitize_export_folder_name("..."), "Smriti Album");
        assert_eq!(sanitize_export_folder_name(""), "Smriti Album");
        assert_eq!(sanitize_export_folder_name("CON"), "Smriti Album");
        assert_eq!(sanitize_export_folder_name("aux.jpg"), "Smriti Album");
    }

    #[test]
    fn bulk_photo_ids_are_positive_deduped_and_bounded() {
        assert_eq!(
            normalize_bulk_photo_ids(vec![3, -1, 3, 0, 4], "photo_ids").unwrap(),
            vec![3, 4]
        );
        let too_many = vec![1; MAX_BULK_PHOTO_IDS + 1];
        assert!(normalize_bulk_photo_ids(too_many, "photo_ids").is_err());
    }

    #[test]
    fn unique_export_folder_keeps_existing_exports() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("Goa")).unwrap();
        std::fs::create_dir(tmp.path().join("Goa 2")).unwrap();
        assert_eq!(
            unique_export_folder(tmp.path(), "Goa"),
            tmp.path().join("Goa 3")
        );
    }

    #[test]
    fn unique_file_path_preserves_extension_and_renames_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("IMG_0001.JPG"), b"old").unwrap();
        let mut reserved = HashSet::new();
        let first = unique_file_path(tmp.path(), "IMG_0001.JPG", &mut reserved);
        let second = unique_file_path(tmp.path(), "IMG_0001.JPG", &mut reserved);
        assert_eq!(first, tmp.path().join("IMG_0001-2.JPG"));
        assert_eq!(second, tmp.path().join("IMG_0001-3.JPG"));
    }
}
