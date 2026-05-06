//! Settings (read-only get; update is M2).

use serde::Deserialize;

use crate::dto::SettingsDto;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn settings_get() -> CommandResult<SettingsDto> {
    let cfg = photovault::config::AppConfig::load();
    Ok((&cfg).into())
}

/// Partial settings patch — every field optional. Unknown fields ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SettingsUpdateArgs {
    pub theme: Option<String>,
    pub thumbnail_size: Option<u32>,
    pub face_detection_confidence: Option<f32>,
    pub face_clustering_threshold: Option<f32>,
    pub burst_time_window_seconds: Option<i64>,
    pub trash_auto_delete_days: Option<u32>,
    pub scan_hidden_folders: Option<bool>,
    pub date_format: Option<String>,
    pub map_cache_limit_mb: Option<u32>,
    pub memories_enabled: Option<bool>,
    pub home_city_override: Option<Option<String>>,
    pub auto_update_check_enabled: Option<bool>,
    pub sidebar_collapsed: Option<bool>,
}

#[tauri::command]
pub async fn settings_update(args: SettingsUpdateArgs) -> CommandResult<SettingsDto> {
    use photovault::config::{AppTheme, DateFormat};
    let mut cfg = photovault::config::AppConfig::load();
    if let Some(t) = args.theme {
        cfg.theme = match t.as_str() {
            "dark" => AppTheme::Dark,
            "light" => AppTheme::Light,
            "system" => AppTheme::System,
            other => {
                return Err(CommandError::Validation {
                    field: "theme".into(),
                    reason: format!("unknown theme {other}"),
                })
            }
        };
    }
    if let Some(v) = args.thumbnail_size {
        cfg.thumbnail_size = v;
    }
    if let Some(v) = args.face_detection_confidence {
        cfg.face_detection_confidence = v;
    }
    if let Some(v) = args.face_clustering_threshold {
        cfg.face_clustering_threshold = v;
    }
    if let Some(v) = args.burst_time_window_seconds {
        cfg.burst_time_window_seconds = v;
    }
    if let Some(v) = args.trash_auto_delete_days {
        cfg.trash_auto_delete_days = v;
    }
    if let Some(v) = args.scan_hidden_folders {
        cfg.scan_hidden_folders = v;
    }
    if let Some(d) = args.date_format {
        cfg.date_format = match d.as_str() {
            "locale" => DateFormat::Locale,
            "iso" => DateFormat::Iso,
            "us" => DateFormat::Us,
            "eu" => DateFormat::Eu,
            other => {
                return Err(CommandError::Validation {
                    field: "date_format".into(),
                    reason: format!("unknown date format {other}"),
                })
            }
        };
    }
    if let Some(v) = args.map_cache_limit_mb {
        cfg.map_cache_limit_mb = v;
    }
    if let Some(v) = args.memories_enabled {
        cfg.memories_enabled = v;
    }
    if let Some(v) = args.home_city_override {
        cfg.home_city_override = v;
    }
    if let Some(v) = args.auto_update_check_enabled {
        cfg.auto_update_check_enabled = v;
    }
    if let Some(v) = args.sidebar_collapsed {
        cfg.sidebar_collapsed = v;
    }
    cfg.save().map_err(|e| CommandError::Io {
        message: e.to_string(),
    })?;
    Ok((&cfg).into())
}
