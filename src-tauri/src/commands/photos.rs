//! Photos: listing, fetching, lookups.

use serde::{Deserialize, Serialize};
use tauri::State;

use smriti::db::album_repo::AlbumRepo;
use smriti::db::face_repo::FaceRepo;
use smriti::db::PhotoRepo;

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
            date_taken: p
                .date_taken
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
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
        &smriti::db::Database,
        Option<(Option<chrono::DateTime<chrono::Utc>>, i64)>,
        i64,
    ) -> Result<Vec<smriti::db::photo_repo::PhotoLite>, CommandError>,
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
            date_taken: p
                .date_taken
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
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

// ---------- on-demand thumbnail ----------
//
// The frontend calls this for cells whose `thumbnail_path` is null (not
// yet generated). It runs a synchronous generate on a blocking pool —
// the ThumbnailService's 8-permit limiter caps total concurrency across
// all in-flight requests.
//
// On success the relative path is returned AND written back to the DB,
// so subsequent list calls return it directly and we don't redo the
// generation work for photos already paged through.

#[derive(Debug, Serialize)]
pub struct ThumbnailResultDto {
    pub thumbnail_path: Option<String>,
}

#[tauri::command]
pub async fn photos_request_thumbnail(
    state: State<'_, AppState>,
    args: PhotosGetArgs,
) -> CommandResult<ThumbnailResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;

    // Pull what we need to feed the generator without holding the DB lock.
    let (file_path, file_hash, orientation, existing) = {
        let db = lib.db.lock().await;
        let repo = PhotoRepo::new(&db.conn);
        let p = repo
            .get_by_id(args.id)?
            .ok_or_else(|| CommandError::not_found("photo", args.id))?;
        (p.file_path, p.file_hash, p.orientation, p.thumbnail_path)
    };

    // Already generated — short-circuit.
    if let Some(rel) = existing {
        return Ok(ThumbnailResultDto {
            thumbnail_path: Some(rel),
        });
    }

    let abs = lib.drive_root.join(&file_path);
    let svc = lib.thumbnails.clone();
    let hash_for_thread = file_hash.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        svc.generate_thumbnail(
            &abs,
            &hash_for_thread,
            orientation,
            smriti::services::thumbnail::ThumbnailSize::Small,
        )
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?;

    let rel = match result {
        Ok(_) => relative_thumbnail_path(&file_hash),
        Err(e) => {
            tracing::debug!("thumbnail gen failed for photo_id={}: {}", args.id, e);
            return Ok(ThumbnailResultDto {
                thumbnail_path: None,
            });
        }
    };

    // Write the relative path back into the DB so future list calls
    // return it. Failures here are non-fatal — we still return the path.
    {
        let db = lib.db.lock().await;
        if let Err(e) = db.conn.execute(
            "UPDATE photos SET thumbnail_path = ?1 WHERE id = ?2",
            rusqlite::params![&rel, args.id],
        ) {
            tracing::warn!(
                "UPDATE thumbnail_path failed for photo_id={}: {}",
                args.id,
                e
            );
        }
    }

    Ok(ThumbnailResultDto {
        thumbnail_path: Some(rel),
    })
}

/// Compute the relative thumbnail path for a given file hash. Mirrors
/// the layout used by ThumbnailService::thumbnail_path so the path can
/// be resolved by the frontend's thumbUrl helper without an extra round
/// trip.
fn relative_thumbnail_path(file_hash: &str) -> String {
    let subdir = &file_hash[..2.min(file_hash.len())];
    format!(
        ".photovault/thumbnails/small/v2/{}/{}.jpg",
        subdir, file_hash
    )
}

// ---------- EXIF extras (tier 2) ----------
//
// Pulled at request time off the actual file rather than stored in DB.
// These fields are surfaced only in the photo-detail panel, so re-reading
// the EXIF on each open is acceptable — JPEG header parse is sub-1ms.

#[derive(Debug, Serialize)]
pub struct ExifExtrasDto {
    pub software: Option<String>,
    pub exposure_bias: Option<String>,
    pub modified_at: Option<String>,
    pub created_at: Option<String>,
}

#[tauri::command]
pub async fn photos_exif_extras(
    state: State<'_, AppState>,
    args: PhotosGetArgs,
) -> CommandResult<ExifExtrasDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let abs_path = {
        let db = lib.db.lock().await;
        let repo = PhotoRepo::new(&db.conn);
        let photo = repo
            .get_by_id(args.id)?
            .ok_or_else(|| CommandError::not_found("photo", args.id))?;
        lib.drive_root.join(&photo.file_path)
    };

    // Run the file IO + EXIF parse on a blocking pool to avoid blocking
    // the Tauri command dispatcher.
    let result = tauri::async_runtime::spawn_blocking(move || read_extras(&abs_path))
        .await
        .map_err(|e| CommandError::Internal {
            message: e.to_string(),
        })?;
    Ok(result)
}

fn read_extras(path: &std::path::Path) -> ExifExtrasDto {
    let mut software: Option<String> = None;
    let mut exposure_bias: Option<String> = None;

    if let Ok(file) = std::fs::File::open(path) {
        let mut reader = std::io::BufReader::new(&file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) {
            // Software tag — text. Strip surrounding quotes that the
            // display_value form puts on ASCII tags.
            if let Some(field) = exif.get_field(exif::Tag::Software, exif::In::PRIMARY) {
                let s = field.display_value().to_string();
                let clean = s.trim_matches('"').trim().to_string();
                if !clean.is_empty() {
                    software = Some(clean);
                }
            }
            // ExposureBiasValue — signed rational, format "+0.3 EV" / "-1.0 EV".
            if let Some(field) = exif.get_field(exif::Tag::ExposureBiasValue, exif::In::PRIMARY) {
                if let exif::Value::SRational(ref v) = field.value {
                    if let Some(r) = v.first() {
                        let f = r.to_f64();
                        let sign = if f > 0.0 { "+" } else { "" };
                        exposure_bias = Some(format!("{}{:.1} EV", sign, f));
                    }
                }
            }
        }
    }

    let (modified_at, created_at) = match std::fs::metadata(path) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string());
            let created = m
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string());
            (modified, created)
        }
        Err(_) => (None, None),
    };

    ExifExtrasDto {
        software,
        exposure_bias,
        modified_at,
        created_at,
    }
}
