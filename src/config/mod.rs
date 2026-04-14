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

    /// Weight applied to co-occurrence prior when multiple candidate
    /// clusters exist for an unassigned face. 0 disables the signal.
    #[serde(default = "default_weight_cooccurrence")]
    pub weight_cooccurrence: f32,

    /// Weight applied to temporal-neighbor prior (±60 s, same cluster).
    #[serde(default = "default_weight_temporal")]
    pub weight_temporal: f32,

    /// Whether the Memories feature (N years ago today, seasonal recaps) is on.
    #[serde(default = "default_memories_enabled")]
    pub memories_enabled: bool,
}

fn default_weight_cooccurrence() -> f32 {
    0.30
}

fn default_weight_temporal() -> f32 {
    0.50
}

fn default_memories_enabled() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            thumbnail_size: 500,
            face_detection_confidence: 0.25,
            face_clustering_threshold: 0.42,
            burst_time_window_seconds: 3,
            trash_auto_delete_days: 30,
            scan_hidden_folders: false,
            date_format: DateFormat::Locale,
            remembered_drives: Vec::new(),
            window_width: 1400,
            window_height: 900,
            sidebar_collapsed: false,
            weight_cooccurrence: default_weight_cooccurrence(),
            weight_temporal: default_weight_temporal(),
            memories_enabled: default_memories_enabled(),
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
                Ok(mut cfg) => {
                    cfg.validate();
                    cfg
                }
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

    /// Clamp all values to valid ranges.
    fn validate(&mut self) {
        self.face_detection_confidence = self.face_detection_confidence.clamp(0.1, 0.95);
        self.face_clustering_threshold = self.face_clustering_threshold.clamp(0.1, 0.8);
        self.thumbnail_size = self.thumbnail_size.clamp(100, 1000);
        self.burst_time_window_seconds = self.burst_time_window_seconds.max(1).min(30);
        self.trash_auto_delete_days = self.trash_auto_delete_days.max(1).min(365);
        self.window_width = self.window_width.clamp(400, 7680);
        self.window_height = self.window_height.clamp(300, 4320);
        self.weight_cooccurrence = self.weight_cooccurrence.clamp(0.0, 2.0);
        self.weight_temporal = self.weight_temporal.clamp(0.0, 2.0);
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
