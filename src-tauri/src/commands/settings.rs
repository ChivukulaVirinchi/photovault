//! Settings (read-only get; update is M2).

use serde::Deserialize;

use crate::dto::SettingsDto;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn settings_get() -> CommandResult<SettingsDto> {
    let cfg = smriti::config::AppConfig::load();
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
    pub show_timeline_stacks: Option<bool>,
    pub date_format: Option<String>,
    pub map_cache_limit_mb: Option<u32>,
    pub memories_enabled: Option<bool>,
    pub home_city_override: Option<Option<String>>,
    pub auto_update_check_enabled: Option<bool>,
    pub sidebar_collapsed: Option<bool>,
    /// Recently-opened drive paths (most-recent first). The frontend
    /// pushes the freshly-opened path on every `library.open` so the
    /// Welcome screen can list "Recent" libraries.
    pub remembered_drives: Option<Vec<String>>,
    pub thumbnail_cache_gb: Option<f64>,
    pub face_gpu_bridge_url: Option<Option<String>>,
    pub face_gpu_bridge_enabled: Option<bool>,
    pub assistant_enabled: Option<bool>,
    pub ai_features_enabled: Option<bool>,
    pub assistant_provider: Option<String>,
    pub assistant_base_url: Option<String>,
    pub assistant_model: Option<String>,
    pub assistant_api_key: Option<Option<String>>,
}

#[tauri::command]
pub async fn settings_update(args: SettingsUpdateArgs) -> CommandResult<SettingsDto> {
    use smriti::config::{AppTheme, DateFormat};
    let mut cfg = smriti::config::AppConfig::load();
    let assistant_enabled_was_patched = args.assistant_enabled.is_some();
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
    if let Some(v) = args.show_timeline_stacks {
        cfg.show_timeline_stacks = v;
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
    if let Some(v) = args.remembered_drives {
        cfg.remembered_drives = v.into_iter().map(std::path::PathBuf::from).collect();
    }
    if let Some(v) = args.thumbnail_cache_gb {
        cfg.thumbnail_cache_gb = v;
    }
    if let Some(v) = args.face_gpu_bridge_url {
        if let Some(ref url) = v {
            if !smriti::config::is_allowed_gpu_bridge_url(url) {
                return Err(CommandError::Validation {
                    field: "face_gpu_bridge_url".into(),
                    reason: "must use https:// or local http://".into(),
                });
            }
        }
        cfg.face_gpu_bridge_url = v;
    }
    if let Some(v) = args.face_gpu_bridge_enabled {
        cfg.face_gpu_bridge_enabled = v;
        if !v {
            cfg.face_gpu_bridge_url = None;
        }
    }
    if let Some(v) = args.assistant_enabled {
        cfg.assistant_enabled = v;
    }
    if let Some(v) = args.ai_features_enabled {
        apply_ai_feature_toggle(&mut cfg, v, assistant_enabled_was_patched);
    }
    if let Some(v) = args.assistant_provider {
        cfg.assistant_provider = match v.as_str() {
            "local" | "openai_compatible" => v,
            other => {
                return Err(CommandError::Validation {
                    field: "assistant_provider".into(),
                    reason: format!("unknown assistant provider {other}"),
                })
            }
        };
    }
    if let Some(v) = args.assistant_base_url {
        let trimmed = v.trim().trim_end_matches('/').to_string();
        let allowed = trimmed.starts_with("https://")
            || trimmed.starts_with("http://localhost")
            || trimmed.starts_with("http://127.0.0.1");
        if !allowed {
            return Err(CommandError::Validation {
                field: "assistant_base_url".into(),
                reason: "must use https:// or local http://".into(),
            });
        }
        cfg.assistant_base_url = trimmed;
    }
    if let Some(v) = args.assistant_model {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            return Err(CommandError::Validation {
                field: "assistant_model".into(),
                reason: "model is required".into(),
            });
        }
        cfg.assistant_model = trimmed;
    }
    if let Some(v) = args.assistant_api_key {
        cfg.assistant_api_key = v.and_then(|key| {
            let trimmed = key.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    }
    cfg.save().map_err(|e| CommandError::Io {
        message: e.to_string(),
    })?;
    Ok((&cfg).into())
}

fn apply_ai_feature_toggle(
    cfg: &mut smriti::config::AppConfig,
    ai_enabled: bool,
    assistant_enabled_was_patched: bool,
) {
    cfg.ai_features_enabled = ai_enabled;
    if !ai_enabled || !assistant_enabled_was_patched {
        cfg.assistant_enabled = false;
    }
}

#[cfg(test)]
mod tests {
    use smriti::config::AppConfig;

    #[test]
    fn reenabling_ai_keeps_assistant_off_by_default() {
        let mut cfg = AppConfig::default();

        super::apply_ai_feature_toggle(&mut cfg, false, false);
        assert!(!cfg.ai_features_enabled);
        assert!(!cfg.assistant_enabled);

        super::apply_ai_feature_toggle(&mut cfg, true, false);
        assert!(cfg.ai_features_enabled);
        assert!(!cfg.assistant_enabled);
    }

    #[test]
    fn explicit_assistant_patch_wins_when_ai_is_enabled() {
        let mut cfg = AppConfig {
            assistant_enabled: false,
            ..Default::default()
        };

        super::apply_ai_feature_toggle(&mut cfg, true, true);
        assert!(cfg.ai_features_enabled);
        assert!(!cfg.assistant_enabled);
    }
}
