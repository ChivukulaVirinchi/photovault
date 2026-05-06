//! System: asset health, app version (read-only).

use crate::dto::{AppVersionDto, AssetHealthDto};
use crate::CommandResult;

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
