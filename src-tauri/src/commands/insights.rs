//! Insights dashboard.

use serde::Deserialize;
use tauri::State;

use crate::dto::InsightsDto;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Default, Deserialize)]
pub struct InsightsComputeArgs {
    pub year: Option<i32>,
}

#[tauri::command]
pub async fn insights_compute(
    state: State<'_, AppState>,
    args: InsightsComputeArgs,
) -> CommandResult<InsightsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let mut data = photovault::services::insights::compute(&db.conn, args.year)?;

    // Resolve the hero photo's thumbnail path (relative). Mirrors the
    // photo_request_thumbnail layout so the frontend's thumbUrl helper
    // resolves it without an extra round-trip.
    if let Some(hero_id) = data.hero_photo_id {
        let repo = photovault::db::PhotoRepo::new(&db.conn);
        if let Ok(Some(photo)) = repo.get_by_id(hero_id) {
            data.hero_thumbnail_path = photo.thumbnail_path;
        }
    }

    // Resolve face crop relative paths for top_people. The crop file
    // is at `<drive>/.photovault/faces/<face_id>.jpg`; we emit the
    // relative form so thumbUrl prepends drive_root.
    let faces_dir = lib.drive_root.join(".photovault").join("faces");
    for stat in data.top_people.iter_mut() {
        if let Some(face_id) = stat.face_id {
            let abs = faces_dir.join(format!("{}.jpg", face_id));
            if abs.exists() {
                stat.face_crop_path = Some(format!(".photovault/faces/{}.jpg", face_id));
            }
        }
    }

    Ok(data.into())
}

#[tauri::command]
pub async fn insights_invalidate(_state: State<'_, AppState>) -> CommandResult<()> {
    // No server-side cache today — handler exists so the frontend can
    // signal cache-stale events; future caching layer can hook here.
    Ok(())
}
