//! Event channel constants and shared payload types.
//!
//! Long-running ops emit to these named channels. The frontend uses
//! `@tauri-apps/api/event::listen` to subscribe. M1 defines the names
//! and shared `JobProgress` payload; M2 adds the typed payloads for
//! each topic and wires emission from job handlers.

use serde::Serialize;

pub const EV_SCAN_PROGRESS: &str = "scan:progress";
pub const EV_SCAN_COMPLETE: &str = "scan:complete";
pub const EV_METADATA_PROGRESS: &str = "metadata:progress";
pub const EV_METADATA_COMPLETE: &str = "metadata:complete";
pub const EV_THUMBNAIL_READY: &str = "thumbnail:ready";
pub const EV_FACES_PROGRESS: &str = "faces:progress";
pub const EV_FACES_COMPLETE: &str = "faces:complete";
pub const EV_DUPLICATES_PROGRESS: &str = "duplicates:progress";
pub const EV_DUPLICATES_COMPLETE: &str = "duplicates:complete";
pub const EV_BURSTS_PROGRESS: &str = "bursts:progress";
pub const EV_BURSTS_COMPLETE: &str = "bursts:complete";
pub const EV_THUMBNAILS_PROGRESS: &str = "thumbnails:progress";
pub const EV_THUMBNAILS_COMPLETE: &str = "thumbnails:complete";
pub const EV_GEOCODING_PROGRESS: &str = "geocoding:progress";
pub const EV_GEOCODING_COMPLETE: &str = "geocoding:complete";
pub const EV_ALBUM_SUGGESTIONS_PROGRESS: &str = "album_suggestions:progress";
pub const EV_ALBUM_SUGGESTIONS_COMPLETE: &str = "album_suggestions:complete";
pub const EV_ALBUM_EXPORT_PROGRESS: &str = "album_export:progress";
pub const EV_ALBUM_EXPORT_COMPLETE: &str = "album_export:complete";
pub const EV_SEMANTIC_PROGRESS: &str = "semantic:progress";
pub const EV_SEMANTIC_COMPLETE: &str = "semantic:complete";
pub const EV_ASSETS_PROGRESS: &str = "assets:progress";
pub const EV_ASSETS_COMPLETE: &str = "assets:complete";
pub const EV_TAKEOUT_PROGRESS: &str = "takeout:progress";
pub const EV_TAKEOUT_COMPLETE: &str = "takeout:complete";
pub const EV_ASSISTANT_ACTIVITY: &str = "assistant:activity";

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

#[derive(Debug, Serialize, Clone)]
pub struct AssistantActivityEvent {
    pub run_id: String,
    pub library_root: String,
    pub label: String,
}
