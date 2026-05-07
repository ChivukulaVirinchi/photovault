//! Event channel constants and shared payload types.
//!
//! Long-running ops emit to these named channels. The frontend uses
//! `@tauri-apps/api/event::listen` to subscribe. M1 defines the names
//! and shared `JobProgress` payload; M2 adds the typed payloads for
//! each topic and wires emission from job handlers.

use serde::Serialize;

pub const EV_SCAN_PROGRESS: &str = "scan:progress";
pub const EV_SCAN_COMPLETE: &str = "scan:complete";
pub const EV_FACES_PROGRESS: &str = "faces:progress";
pub const EV_FACES_COMPLETE: &str = "faces:complete";
pub const EV_DUPLICATES_PROGRESS: &str = "duplicates:progress";
pub const EV_DUPLICATES_COMPLETE: &str = "duplicates:complete";
pub const EV_BURSTS_PROGRESS: &str = "bursts:progress";
pub const EV_BURSTS_COMPLETE: &str = "bursts:complete";
pub const EV_DOCUMENTS_PROGRESS: &str = "documents:progress";
pub const EV_DOCUMENTS_COMPLETE: &str = "documents:complete";
pub const EV_THUMBNAILS_PROGRESS: &str = "thumbnails:progress";
pub const EV_GEOCODING_PROGRESS: &str = "geocoding:progress";
pub const EV_GEOCODING_COMPLETE: &str = "geocoding:complete";
pub const EV_ALBUM_SUGGESTIONS_PROGRESS: &str = "album_suggestions:progress";
pub const EV_ALBUM_SUGGESTIONS_COMPLETE: &str = "album_suggestions:complete";
pub const EV_ASSETS_PROGRESS: &str = "assets:progress";
pub const EV_UPDATE_DOWNLOAD_PROGRESS: &str = "update:download-progress";
pub const EV_UPDATE_INSTALLED: &str = "update:installed";
pub const EV_DRIVES_CHANGED: &str = "drives:changed";
pub const EV_LIBRARY_SCAN_RECOMMENDED: &str = "library:scan-recommended";

/// Generic progress payload used by most jobs that don't need
/// stage-specific fields.
#[derive(Debug, Serialize, Clone)]
pub struct JobProgress {
    pub job_id: String,
    pub stage: String,
    pub processed: u64,
    pub total: Option<u64>,
    pub elapsed_ms: u64,
    pub eta_ms: Option<u64>,
    pub message: Option<String>,
}
