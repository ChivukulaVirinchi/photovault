//! System: asset health, app version (read-only).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::dto::{AppVersionDto, AssetHealthDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn system_asset_health() -> CommandResult<AssetHealthDto> {
    let h = photovault::bootstrap::asset_health();
    Ok(h.into())
}

#[tauri::command]
pub async fn system_app_version() -> CommandResult<AppVersionDto> {
    Ok(AppVersionDto {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// ---------- mutations / OS integration ----------

#[derive(Debug, Deserialize)]
pub struct SystemPhotoIdArgs {
    pub photo_id: i64,
}

#[tauri::command]
pub async fn system_open_in_explorer(
    state: State<'_, AppState>,
    args: SystemPhotoIdArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = photovault::db::PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.photo_id)?
        .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
    let abs = lib.drive_root.join(&photo.file_path);
    let parent = abs.parent().unwrap_or(&abs);
    open::that(parent).map_err(|e| CommandError::Io {
        message: e.to_string(),
    })?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct CopiedPathDto {
    pub path: String,
}

#[tauri::command]
pub async fn system_copy_path_to_clipboard(
    state: State<'_, AppState>,
    args: SystemPhotoIdArgs,
) -> CommandResult<CopiedPathDto> {
    // Frontend uses the @tauri-apps/plugin-clipboard-manager to actually
    // write — this command resolves the absolute path and returns it,
    // and the plugin handles the OS clipboard. Keeps clipboard-permission
    // surface narrowly scoped to the frontend layer.
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = photovault::db::PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.photo_id)?
        .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
    let abs = lib.drive_root.join(&photo.file_path);
    Ok(CopiedPathDto {
        path: abs.display().to_string(),
    })
}

#[derive(Debug, Serialize)]
pub struct UpdateStatusDto {
    pub current: String,
    pub latest: Option<String>,
    pub newer_available: bool,
    pub release_url: Option<String>,
    pub body: Option<String>,
}

#[tauri::command]
pub async fn system_updates_check() -> CommandResult<UpdateStatusDto> {
    let result = photovault::services::update_checker::check_for_updates()
        .await
        .map_err(|e| CommandError::Network {
            message: e.to_string(),
        })?;
    Ok(UpdateStatusDto {
        current: result.current.to_string(),
        latest: Some(result.latest.version.to_string()),
        newer_available: result.newer_available,
        release_url: Some(result.latest.html_url),
        body: Some(result.latest.body),
    })
}
