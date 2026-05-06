//! Settings (read-only get; update is M2).

use crate::dto::SettingsDto;
use crate::CommandResult;

#[tauri::command]
pub async fn settings_get() -> CommandResult<SettingsDto> {
    let cfg = photovault::config::AppConfig::load();
    Ok((&cfg).into())
}
