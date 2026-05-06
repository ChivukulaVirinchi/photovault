//! Photos: listing, fetching, lookups.

use serde::Deserialize;
use tauri::State;

use photovault::db::album_repo::AlbumRepo;
use photovault::db::face_repo::FaceRepo;
use photovault::db::PhotoRepo;

use crate::dto::{AlbumDto, Page, PersonDto, PhotoDto, PhotoSummaryDto};
use crate::pagination::{self, Cursor};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

use super::cursor_for_lite;

#[derive(Debug, Deserialize)]
pub struct PhotosListArgs {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    #[serde(default)]
    pub include_trashed: bool,
}

#[tauri::command]
pub async fn photos_list(
    state: State<'_, AppState>,
    args: PhotosListArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let cursor_in = pagination::decode(args.cursor.as_deref())?;
    let limit = pagination::clamp_limit(args.limit) as i64;

    let db = lib.db.lock().await;
    let repo = PhotoRepo::new(&db.conn);
    let rows = repo.list_after(
        cursor_in.map(|c| (c.date_taken, c.id)),
        limit,
        args.include_trashed,
    )?;

    let has_more = rows.len() as i64 == limit;
    let next_cursor = rows.last().map(|p| pagination::encode(cursor_for_lite(p)));
    let total = if cursor_in.is_none() {
        Some(repo.count()? as u64)
    } else {
        None
    };
    let items = rows
        .into_iter()
        .map(|p| PhotoSummaryDto {
            id: p.id,
            thumbnail_path: p.thumbnail_path,
            date_taken: p.date_taken.map(|d| d.to_rfc3339()),
            width: p.width,
            height: p.height,
            orientation: p.orientation,
            is_trashed: p.is_trashed,
        })
        .collect();
    Ok(Page {
        items,
        next_cursor,
        has_more,
        total,
    })
}

#[derive(Debug, Deserialize)]
pub struct PhotosGetArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn photos_get(
    state: State<'_, AppState>,
    args: PhotosGetArgs,
) -> CommandResult<PhotoDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.id)?
        .ok_or_else(|| CommandError::not_found("photo", args.id))?;
    Ok(photo.into())
}

#[derive(Debug, Deserialize)]
pub struct PhotosGetManyArgs {
    pub ids: Vec<i64>,
}

#[tauri::command]
pub async fn photos_get_many(
    state: State<'_, AppState>,
    args: PhotosGetManyArgs,
) -> CommandResult<Vec<PhotoDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoRepo::new(&db.conn);
    let ids: Vec<i64> = args.ids.into_iter().take(500).collect();
    let photos = repo.get_by_ids(&ids)?;
    Ok(photos.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct PhotosListByAlbumArgs {
    pub album_id: i64,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn photos_list_by_album(
    state: State<'_, AppState>,
    args: PhotosListByAlbumArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    paged(state, args.cursor, args.limit, |db, cur, limit| {
        let repo = PhotoRepo::new(&db.conn);
        repo.list_after_by_album(args.album_id, cur, limit)
            .map_err(Into::into)
    })
    .await
}

#[derive(Debug, Deserialize)]
pub struct PhotosListByPersonArgs {
    pub person_id: i64,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn photos_list_by_person(
    state: State<'_, AppState>,
    args: PhotosListByPersonArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    paged(state, args.cursor, args.limit, |db, cur, limit| {
        let repo = PhotoRepo::new(&db.conn);
        repo.list_after_by_person(args.person_id, cur, limit)
            .map_err(Into::into)
    })
    .await
}

#[derive(Debug, Deserialize)]
pub struct PhotosListByDateArgs {
    pub start: String,
    pub end: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn photos_list_by_date(
    state: State<'_, AppState>,
    args: PhotosListByDateArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let start = args.start.clone();
    let end = args.end.clone();
    paged(state, args.cursor, args.limit, move |db, cur, limit| {
        let repo = PhotoRepo::new(&db.conn);
        repo.list_after_by_date(&start, &end, cur, limit)
            .map_err(Into::into)
    })
    .await
}

#[derive(Debug, Deserialize)]
pub struct PhotosListByPlaceArgs {
    pub city: Option<String>,
    pub country: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn photos_list_by_place(
    state: State<'_, AppState>,
    args: PhotosListByPlaceArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let city = args.city.clone();
    let country = args.country.clone();
    paged(state, args.cursor, args.limit, move |db, cur, limit| {
        let repo = PhotoRepo::new(&db.conn);
        repo.list_after_by_place(city.as_deref(), country.as_deref(), cur, limit)
            .map_err(Into::into)
    })
    .await
}

#[derive(Debug, Deserialize)]
pub struct PhotosPeopleInPhotoArgs {
    pub photo_id: i64,
}

#[tauri::command]
pub async fn photos_people_in_photo(
    state: State<'_, AppState>,
    args: PhotosPeopleInPhotoArgs,
) -> CommandResult<Vec<PersonDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let face_repo = FaceRepo::new(&db.conn);
    // get_people_for_photo returns Vec<(cluster_id, name)>. Build minimal PersonDtos
    // — a richer call (`people.get`) can fetch the rest if the UI needs it.
    let pairs = face_repo.get_people_for_photo(args.photo_id)?;
    Ok(pairs
        .into_iter()
        .map(|(id, name)| PersonDto {
            id,
            name: if name.is_empty() { None } else { Some(name) },
            photo_count: 0,
            representative_face_id: None,
            representative_thumbnail_path: None,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
pub struct PhotosAlbumsForPhotoArgs {
    pub photo_id: i64,
}

#[tauri::command]
pub async fn photos_albums_for_photo(
    state: State<'_, AppState>,
    args: PhotosAlbumsForPhotoArgs,
) -> CommandResult<Vec<AlbumDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let album_repo = AlbumRepo::new(&db.conn);
    let pairs = album_repo.get_albums_for_photo(args.photo_id)?;
    // Build minimal AlbumDtos (name + id only). For full info call `albums.get`.
    Ok(pairs
        .into_iter()
        .map(|(id, name)| AlbumDto {
            id,
            name,
            photo_count: 0,
            cover_photo_id: None,
            cover_thumbnail_path: None,
            date_range_start: None,
            date_range_end: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .collect())
}

// ---------- shared paged helper ----------

async fn paged<F>(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<u32>,
    f: F,
) -> CommandResult<Page<PhotoSummaryDto>>
where
    F: FnOnce(
        &photovault::db::Database,
        Option<(Option<chrono::DateTime<chrono::Utc>>, i64)>,
        i64,
    ) -> Result<Vec<photovault::db::photo_repo::PhotoLite>, CommandError>,
{
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let cursor_in = pagination::decode(cursor.as_deref())?;
    let limit_n = pagination::clamp_limit(limit) as i64;
    let db = lib.db.lock().await;
    let rows = f(&db, cursor_in.map(|c| (c.date_taken, c.id)), limit_n)?;

    let has_more = rows.len() as i64 == limit_n;
    let next_cursor: Option<String> = rows.last().map(|p| {
        pagination::encode(Cursor {
            date_taken: p.date_taken,
            id: p.id,
        })
    });
    let items = rows
        .into_iter()
        .map(|p| PhotoSummaryDto {
            id: p.id,
            thumbnail_path: p.thumbnail_path,
            date_taken: p.date_taken.map(|d| d.to_rfc3339()),
            width: p.width,
            height: p.height,
            orientation: p.orientation,
            is_trashed: p.is_trashed,
        })
        .collect();
    Ok(Page {
        items,
        next_cursor,
        has_more,
        total: None,
    })
}
