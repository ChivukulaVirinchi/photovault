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
    /// Whether the user's window was maximised when they last closed
    /// the app. Restored at startup by the Tauri shell.
    #[serde(default)]
    pub window_maximized: bool,
    pub sidebar_collapsed: bool,

    #[serde(default = "default_map_cache_limit_mb")]
    pub map_cache_limit_mb: u32,

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

    /// Optional user-specified home city for album suggestions.
    /// When set, the suggestion engine skips auto-detection of the home city.
    #[serde(default)]
    pub home_city_override: Option<String>,

    /// Last-viewed top-level view; restored on app launch.
    /// Stored as a string for forward compatibility across View enum changes.
    #[serde(default)]
    pub last_view: Option<String>,

    /// Opt-in: whether Smriti should query GitHub Releases once
    /// every 24 hours to see if a new version is available. Off by
    /// default — the first-run prompt flips it on if the user agrees.
    /// See PRIVACY.md.
    #[serde(default)]
    pub auto_update_check_enabled: bool,

    /// Unix timestamp of the last update check. Used to rate-limit
    /// the background subscription to at most one check per 24 h.
    #[serde(default)]
    pub last_update_check_at_unix: Option<i64>,

    /// True until the user accepts or declines the first-run update
    /// prompt. After that, toggling is done through Settings.
    #[serde(default = "default_show_first_run_update_prompt")]
    pub show_first_run_update_prompt: bool,

    /// Version of the EXIF date-extraction logic the user's library was
    /// last evaluated against. When the binary's `CURRENT_DATE_LOGIC_VERSION`
    /// is higher, the app auto-runs `RefreshPhotoDates` once on
    /// first launch with the upgraded binary so users don't keep
    /// seeing 2012 ghosts on photos shot in 2026 etc. After the
    /// refresh runs, this field is bumped to the new version.
    #[serde(default)]
    pub date_logic_version: u32,

    /// True once the user has been shown the "Place names off — run
    /// scripts/setup_assets.sh" toast. Prevents the same toast appearing
    /// on every drive-select.
    #[serde(default)]
    pub geonames_warning_seen: bool,

    /// Disk budget for the on-drive thumbnail cache, expressed in
    /// gigabytes. Drives the LRU eviction in `ThumbnailService`. Default
    /// 5 GB suits a mid-size phone library; small-disk users can dial
    /// down, big-library users up.
    #[serde(default = "default_thumbnail_cache_gb")]
    pub thumbnail_cache_gb: f64,

    /// Optional GPU bridge URL for face embedding (opt-in).
    #[serde(default)]
    pub face_gpu_bridge_url: Option<String>,

    /// Whether the GPU bridge is explicitly enabled by the user.
    #[serde(default)]
    pub face_gpu_bridge_enabled: bool,

    /// Version of config migrations applied. Bumped each time a default
    /// changes and existing installs need a one-time pick-up.
    #[serde(default)]
    pub config_version: u32,

    /// Face embedding model filename (relative to the models directory).
    /// "adaface_ir101_webface12m.onnx" (default, SOTA) or "glintr100.onnx" (legacy fallback).
    #[serde(default = "default_face_embedder_model")]
    pub face_embedder_model: String,
}

/// Bumped any time the EXIF date fallback chain changes. Stored in
/// `AppConfig::date_logic_version`; on launch, if the saved value is
/// less than this, the app re-extracts dates for every photo so the
/// new logic actually takes effect against existing rows.
///
/// Bump history:
///   0 — pre-PR6 chain (DateTimeOriginal -> DateTime -> filename -> mtime)
///   1 — PR6 chain (DateTimeOriginal -> DateTimeDigitized -> filename -> DateTime -> mtime)
/// V2 (2026-05-05): stale-EXIF defence. When `DateTime` modification
/// tag would be the source, sanity-check it against mtime — if they
/// disagree by ≥ 2 years, prefer mtime (handles phones whose firmware
/// clock reset to 2012-01-01 and now stamp every photo with that).
/// Also adds WhatsApp/Snapchat/generic filename patterns and tightens
/// the catch-all 8-digit regex so ID strings can't masquerade as dates.
pub const CURRENT_DATE_LOGIC_VERSION: u32 = 2;

fn default_show_first_run_update_prompt() -> bool {
    true
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

fn default_map_cache_limit_mb() -> u32 {
    500
}

fn default_thumbnail_cache_gb() -> f64 {
    5.0
}

fn default_face_embedder_model() -> String {
    "adaface_ir101_webface12m.onnx".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            thumbnail_size: 500,
            face_detection_confidence: 0.25,
            face_clustering_threshold: 0.30,
            burst_time_window_seconds: 3,
            trash_auto_delete_days: 30,
            scan_hidden_folders: false,
            date_format: DateFormat::Locale,
            remembered_drives: Vec::new(),
            window_width: 1600,
            window_height: 1000,
            window_maximized: false,
            sidebar_collapsed: false,
            map_cache_limit_mb: default_map_cache_limit_mb(),
            weight_cooccurrence: default_weight_cooccurrence(),
            weight_temporal: default_weight_temporal(),
            memories_enabled: default_memories_enabled(),
            home_city_override: None,
            last_view: None,
            auto_update_check_enabled: false,
            last_update_check_at_unix: None,
            show_first_run_update_prompt: true,
            date_logic_version: 0,
            geonames_warning_seen: false,
            thumbnail_cache_gb: default_thumbnail_cache_gb(),
            config_version: 2,
            face_embedder_model: default_face_embedder_model(),
            face_gpu_bridge_url: None,
            face_gpu_bridge_enabled: false,
        }
    }
}

impl AppConfig {
    /// Load config from disk. Falls back to defaults on any failure.
    pub fn load() -> Self {
        let path = Self::config_path();
        // If the new path is empty but a pre-rename config exists,
        // pick that up once so upgrading users don't lose their settings.
        let read_path = if path.exists() {
            path
        } else {
            let legacy = Self::legacy_config_path();
            if legacy.exists() {
                legacy
            } else {
                return Self::default();
            }
        };

        match std::fs::read_to_string(&read_path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut cfg) => {
                    cfg.validate();
                    cfg.migrate_config_defaults();
                    cfg
                }
                Err(e) => {
                    tracing::warn!("Failed to parse config {}: {}", read_path.display(), e);
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read config {}: {}", read_path.display(), e);
                Self::default()
            }
        }
    }

    /// Apply one-time config migrations when defaults change.
    /// Bumps `config_version` after each migration so it doesn't re-run.
    fn migrate_config_defaults(&mut self) {
        if self.config_version < 1 {
            // v1: tightened clustering threshold (0.42 → 0.28).
            // Only bump if the user never changed the old default.
            if (self.face_clustering_threshold - 0.42).abs() < 0.001 {
                self.face_clustering_threshold = 0.28;
            }
            self.config_version = 1;
        }
        if self.config_version < 2 {
            // v2: bump to 0.30 for AdaFace (tighter centroids).
            if (self.face_clustering_threshold - 0.28).abs() < 0.001 {
                self.face_clustering_threshold = 0.30;
            }
            self.config_version = 2;
        }
    }

    /// Clamp all values to valid ranges.
    fn validate(&mut self) {
        // Sanity-check GPU bridge URL if set
        if let Some(ref url) = self.face_gpu_bridge_url {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                tracing::warn!(
                    "Invalid face_gpu_bridge_url '{}' — must start with http:// or https://. Disabling.",
                    url
                );
                self.face_gpu_bridge_url = None;
                self.face_gpu_bridge_enabled = false;
            }
        }

        self.face_detection_confidence = self.face_detection_confidence.clamp(0.1, 0.95);
        self.face_clustering_threshold = self.face_clustering_threshold.clamp(0.1, 0.8);
        self.thumbnail_size = self.thumbnail_size.clamp(100, 1000);
        self.burst_time_window_seconds = self.burst_time_window_seconds.clamp(1, 30);
        self.trash_auto_delete_days = self.trash_auto_delete_days.clamp(1, 365);
        self.window_width = self.window_width.clamp(400, 7680);
        self.window_height = self.window_height.clamp(300, 4320);
        self.weight_cooccurrence = self.weight_cooccurrence.clamp(0.0, 2.0);
        self.weight_temporal = self.weight_temporal.clamp(0.0, 2.0);
        self.map_cache_limit_mb = self.map_cache_limit_mb.clamp(50, 10_000);
        self.thumbnail_cache_gb = self.thumbnail_cache_gb.clamp(0.5, 100.0);
        if self.face_embedder_model.is_empty() {
            self.face_embedder_model = default_face_embedder_model();
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
            .join("smriti")
            .join("config.json")
    }

    /// Pre-rename config path. Read-only fallback so users upgrading
    /// from PhotoVault keep their settings on first launch — once we
    /// save, the new path takes over.
    fn legacy_config_path() -> PathBuf {
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
