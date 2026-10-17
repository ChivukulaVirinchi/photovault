//! Collage support: picking photos, face positions for smart cropping,
//! today's suggested collages, and saving the finished image.
//!
//! The collage itself is composed on a canvas in the UI and is never stored
//! in the library database; the only outputs are the file the user saves
//! and, when a suggestion is dismissed, a `memory_blocks` row.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::face_repo::FaceRepo;
use smriti::services::collage_picks::{self, PickOptions};

use crate::state::AppState;
use crate::thumbnail_upgrade::{upgrade_covers_to_medium, CoverInput};
use crate::{CommandError, CommandResult};

/// Collages hold at most nine photos; anything beyond is ignored.
const MAX_PHOTOS: usize = 9;
/// A 3000 px JPEG is a few MB. Refuse anything absurd from the webview.
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Folder collages are saved into, inside the user's Pictures folder.
const COLLAGE_FOLDER: &str = "Smriti Collages";

#[derive(Debug, Deserialize)]
pub struct CollageFacesArgs {
    pub photo_ids: Vec<i64>,
}

/// One face box, normalised to the photo as displayed (EXIF-rotated).
#[derive(Debug, Serialize)]
pub struct FaceBoxDto {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[tauri::command]
pub async fn collage_face_boxes(
    state: State<'_, AppState>,
    args: CollageFacesArgs,
) -> CommandResult<HashMap<i64, Vec<FaceBoxDto>>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let ids: Vec<i64> = args.photo_ids.into_iter().take(MAX_PHOTOS).collect();
    Ok(face_boxes(&db.conn, &ids)?)
}

fn face_boxes(
    conn: &rusqlite::Connection,
    ids: &[i64],
) -> rusqlite::Result<HashMap<i64, Vec<FaceBoxDto>>> {
    let mut out: HashMap<i64, Vec<FaceBoxDto>> = HashMap::new();
    for (photo_id, x, y, width, height) in FaceRepo::new(conn).get_face_boxes_for_photos(ids)? {
        out.entry(photo_id).or_default().push(FaceBoxDto {
            x,
            y,
            width,
            height,
        });
    }
    Ok(out)
}

/// Upper bound on ids sent from the webview for a pick.
const MAX_PICK_INPUT: usize = 20_000;
const FAVOURITES_ALBUM_ID: i64 = super::albums::FAVORITES_ALBUM_ID;

#[derive(Debug, Deserialize)]
pub struct CollagePickArgs {
    /// Pick from an album (`-1` is Favourites)...
    pub album_id: Option<i64>,
    /// ...or from these photos.
    #[serde(default)]
    pub photo_ids: Vec<i64>,
}

/// The best few photos of an album or a set, feature photo first.
#[tauri::command]
pub async fn collage_pick(
    state: State<'_, AppState>,
    args: CollagePickArgs,
) -> CommandResult<Vec<i64>> {
    let db_path = db_path(&state).await?;
    blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        let ids = match args.album_id {
            Some(FAVOURITES_ALBUM_ID) => collage_picks::favourite_ids(&conn)?,
            Some(album_id) => collage_picks::album_photo_ids(&conn, album_id)?,
            None => args.photo_ids.into_iter().take(MAX_PICK_INPUT).collect(),
        };
        Ok(collage_picks::pick_best(
            &conn,
            &ids,
            PickOptions::default(),
        )?)
    })
    .await
}

/// One photo of a suggested collage, with what the UI needs to lay it out
/// and draw it without further calls.
#[derive(Debug, Serialize)]
pub struct CollagePhotoDto {
    pub id: i64,
    /// Medium thumbnail when it could be made, otherwise the small one.
    pub thumbnail_path: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
    pub faces: Vec<FaceBoxDto>,
}

#[derive(Debug, Serialize)]
pub struct CollageSuggestionDto {
    /// Pass back to `collage_dismiss_suggestion`.
    pub key: String,
    pub eyebrow: String,
    pub title: String,
    /// Feature photo first.
    pub photos: Vec<CollagePhotoDto>,
}

/// Today's suggested collages for the Memories page. Computed on request
/// and never stored.
#[tauri::command]
pub async fn collage_suggestions(
    state: State<'_, AppState>,
) -> CommandResult<Vec<CollageSuggestionDto>> {
    if !smriti::config::AppConfig::load().memories_enabled {
        return Ok(Vec::new());
    }
    let (db_path, drive_root, thumbs) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            smriti::db::db_path_for(&lib.drive_root),
            lib.drive_root.clone(),
            lib.thumbnails.clone(),
        )
    };
    let (mut suggestions, inputs) = blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        let today = chrono::Local::now().date_naive();
        let memories = if smriti::services::memories::library_is_old_enough(&conn, today) {
            smriti::services::memories::generate_for_today(&conn, today)
                .map_err(|s| CommandError::internal(format!("memories: {s}")))?
        } else {
            Vec::new()
        };
        let picked = collage_picks::suggestions_for_today(&conn, today, &memories)?;

        let all_ids: Vec<i64> = picked
            .iter()
            .flat_map(|s| s.photo_ids.iter().copied())
            .collect();
        let photos: HashMap<i64, smriti::models::Photo> = smriti::db::PhotoRepo::new(&conn)
            .get_by_ids(&all_ids)?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        let mut faces = face_boxes(&conn, &all_ids)?;
        let mut inputs: Vec<CoverInput> = Vec::new();
        let mut out = Vec::new();
        for suggestion in picked {
            let chosen: Vec<&smriti::models::Photo> = suggestion
                .photo_ids
                .iter()
                .filter_map(|id| photos.get(id))
                .collect();
            if chosen.len() < collage_picks::MIN_AUTO_PHOTOS {
                continue;
            }
            let photos = chosen
                .into_iter()
                .map(|p| {
                    inputs.push((
                        inputs.len(),
                        p.file_path.clone(),
                        p.file_hash.clone(),
                        p.orientation,
                    ));
                    CollagePhotoDto {
                        id: p.id,
                        thumbnail_path: p.thumbnail_path.clone(),
                        width: p.width,
                        height: p.height,
                        orientation: p.orientation,
                        faces: faces.remove(&p.id).unwrap_or_default(),
                    }
                })
                .collect();
            out.push(CollageSuggestionDto {
                key: suggestion.source.key(),
                eyebrow: suggestion.source.eyebrow().into(),
                title: suggestion.title,
                photos,
            });
        }
        Ok((out, inputs))
    })
    .await?;

    // Cells are larger than grid tiles, so draw them from medium thumbnails.
    let medium = upgrade_covers_to_medium(thumbs, drive_root, inputs).await;
    let mut slots: Vec<&mut CollagePhotoDto> = suggestions
        .iter_mut()
        .flat_map(|s| s.photos.iter_mut())
        .collect();
    for (index, path) in medium {
        if let (Some(photo), Some(path)) = (slots.get_mut(index), path) {
            photo.thumbnail_path = Some(path);
        }
    }
    Ok(suggestions)
}

#[derive(Debug, Deserialize)]
pub struct CollageDismissArgs {
    pub key: String,
}

/// "Not this one": stop suggesting collages from this source.
#[tauri::command]
pub async fn collage_dismiss_suggestion(
    state: State<'_, AppState>,
    args: CollageDismissArgs,
) -> CommandResult<()> {
    let key = args.key.trim().to_string();
    if key.is_empty() || key.len() > 200 {
        return Err(CommandError::Validation {
            field: "key".into(),
            reason: "not a suggestion".into(),
        });
    }
    let db_path = db_path(&state).await?;
    blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok(collage_picks::dismiss(&conn, &key)?)
    })
    .await
}

async fn db_path(state: &State<'_, AppState>) -> CommandResult<PathBuf> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    Ok(smriti::db::db_path_for(&lib.drive_root))
}

/// Run database work off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| CommandError::internal(format!("collage worker failed: {e}")))?
}

/// `Pictures/Smriti Collages`, falling back to `~/Pictures` and then
/// Documents on systems without a configured Pictures folder.
fn collage_dir(app: &AppHandle) -> CommandResult<PathBuf> {
    let base = app
        .path()
        .picture_dir()
        .or_else(|_| app.path().home_dir().map(|home| home.join("Pictures")))
        .or_else(|_| app.path().document_dir())
        .map_err(|e| CommandError::Io {
            message: format!("Couldn't find a Pictures folder: {e}"),
        })?;
    Ok(base.join(COLLAGE_FOLDER))
}

#[derive(Debug, Serialize)]
pub struct CollageLocationDto {
    /// Full folder path, for tooltips.
    pub path: String,
    /// Short form for the UI, e.g. `Pictures › Smriti Collages`.
    pub label: String,
}

fn location(dir: &Path) -> CollageLocationDto {
    let parent = dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    CollageLocationDto {
        path: dir.to_string_lossy().into_owned(),
        label: if parent.is_empty() {
            COLLAGE_FOLDER.into()
        } else {
            format!("{parent} › {COLLAGE_FOLDER}")
        },
    }
}

/// Where Save will put collages, so the dialog can say so up front.
#[tauri::command]
pub fn collage_location(app: AppHandle) -> CommandResult<CollageLocationDto> {
    Ok(location(&collage_dir(&app)?))
}

#[derive(Debug, Serialize)]
pub struct CollageSavedDto {
    pub path: String,
    pub location: CollageLocationDto,
}

/// Save the rendered JPEG (the request body) into the collages folder.
///
/// The webview supplies only the image bytes; the folder and file name are
/// decided here, so the page cannot write anywhere else.
#[tauri::command]
pub async fn collage_save(
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> CommandResult<CollageSavedDto> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(CommandError::Validation {
            field: "body".into(),
            reason: "expected the collage image bytes".into(),
        });
    };
    if bytes.len() > MAX_IMAGE_BYTES || !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(CommandError::Validation {
            field: "body".into(),
            reason: "expected a JPEG image".into(),
        });
    }
    let bytes = bytes.clone();
    let dir = collage_dir(&app)?;
    let stem = format!(
        "Smriti collage {}",
        chrono::Local::now().format("%Y-%m-%d %H.%M.%S")
    );

    tauri::async_runtime::spawn_blocking(move || -> CommandResult<CollageSavedDto> {
        let io = |e: std::io::Error| CommandError::Io {
            message: format!("Couldn't save the collage: {e}"),
        };
        std::fs::create_dir_all(&dir).map_err(io)?;
        let path = unused_path(&dir, &stem);
        write_atomically(&path, &bytes).map_err(io)?;
        Ok(CollageSavedDto {
            path: path.to_string_lossy().into_owned(),
            location: location(&dir),
        })
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?
}

#[derive(Debug, Deserialize)]
pub struct CollageRevealArgs {
    pub path: String,
}

/// Select a saved collage in the file manager. Only files inside the
/// collages folder can be revealed.
#[tauri::command]
pub async fn collage_reveal(app: AppHandle, args: CollageRevealArgs) -> CommandResult<()> {
    let outside = || CommandError::Validation {
        field: "path".into(),
        reason: "not a saved collage".into(),
    };
    let dir = std::fs::canonicalize(collage_dir(&app)?).map_err(|_| outside())?;
    let file = std::fs::canonicalize(&args.path).map_err(|_| outside())?;
    if !file.starts_with(&dir) || !file.is_file() {
        return Err(outside());
    }
    super::system::select_in_file_manager(&file).map_err(|e| CommandError::Io {
        message: e.to_string(),
    })
}

/// `<stem>.jpg`, or `<stem> (2).jpg` and so on if that name is taken, so a
/// save never replaces an earlier collage.
fn unused_path(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.jpg"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).jpg")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

/// Write beside the target and rename over it, so a failed write never
/// leaves a half-written file where the user expects their collage.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent folder"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_saves_never_replace_earlier_collages() {
        let dir = tempfile::tempdir().unwrap();
        let first = unused_path(dir.path(), "c");
        assert_eq!(first, dir.path().join("c.jpg"));
        std::fs::write(&first, b"1").unwrap();
        assert_eq!(unused_path(dir.path(), "c"), dir.path().join("c (2).jpg"));
    }

    #[test]
    fn location_label_names_the_parent_folder() {
        let dir = PathBuf::from("C:/Users/me/Pictures").join(COLLAGE_FOLDER);
        assert_eq!(location(&dir).label, "Pictures › Smriti Collages");
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.jpg");
        std::fs::write(&path, b"old").unwrap();
        write_atomically(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
