//! Application configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: AppTheme,
    pub thumbnail_size: u32,
    pub face_detection_confidence: f32,
    pub face_clustering_threshold: f32,
    pub burst_time_window_seconds: i64,
    pub trash_auto_delete_days: u32,
    pub scan_hidden_folders: bool,
    pub date_format: DateFormat,
    pub remembered_drives: Vec<PathBuf>,
    pub window_width: u32,
    pub window_height: u32,
    pub sidebar_collapsed: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            thumbnail_size: 300,
            face_detection_confidence: 0.35,
            face_clustering_threshold: 0.45,
            burst_time_window_seconds: 3,
            trash_auto_delete_days: 30,
            scan_hidden_folders: false,
            date_format: DateFormat::Locale,
            remembered_drives: Vec::new(),
            window_width: 1400,
            window_height: 900,
            sidebar_collapsed: false,
        }
    }
}

impl AppConfig {
    /// Load config from disk. Falls back to defaults on any failure.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!("Failed to parse config {}: {}", path.display(), e);
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read config {}: {}", path.display(), e);
                Self::default()
            }
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
    }

    /// Config file path.
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("photovault")
            .join("config.json")
    }

    /// Add drive to remembered list.
    pub fn remember_drive(&mut self, path: PathBuf) {
        if !self.remembered_drives.contains(&path) {
            self.remembered_drives.push(path);
        }
    }

}

/// Theme setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    Dark,
    Light,
    System,
}

/// Date format setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    Locale,
    Iso,
    Us,
    Eu,
}
