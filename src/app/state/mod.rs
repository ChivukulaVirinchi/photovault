//! Application state types and helpers

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::collections::HashSet;

use async_channel::Receiver;
use iced::Task;

use crate::config::AppConfig;
use crate::db::{
    BurstGroupMemberRecord, BurstGroupRecord, Database,
    DuplicateGroupMemberRecord, DuplicateGroupRecord,
    FaceClusterRecord, TrashedPhotoRecord,
};
use crate::models::{ContentCategory, Photo};
use crate::services::{
    DriveDetector, DriveInfo,
    FaceProcessingProgress,
    IndexChanges, OcrProgress, ScanProgress,
    ThumbnailService, ThumbnailSize, TrashStats,
};

use super::messages::Message;

mod loaders;
mod thumbnails;

pub(crate) const THUMBNAIL_DB_FLUSH_BATCH: usize = 64;

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Scanning,
    Timeline,
    People,
    ClusterDetail,
    FaceReview,
    Duplicates,
    DuplicateDetail,
    Bursts,
    BurstDetail,
    Search,
    Documents,
    Cull,
    Trash,
    Settings,
    PhotoDetail,
}

/// Interactive face review deck state.
#[derive(Debug, Clone)]
pub struct FaceReviewState {
    pub queue: Vec<crate::db::ReviewItem>,
    pub current_index: usize,
    pub confirmed: usize,
    pub rejected: usize,
    pub skipped: usize,
    /// Queue IDs resolved in this session, latest last, for undo.
    pub undo_stack: Vec<(i64, ReviewDecision)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Same,
    Different,
    Skip,
}

impl FaceReviewState {
    pub fn new(queue: Vec<crate::db::ReviewItem>) -> Self {
        Self {
            queue,
            current_index: 0,
            confirmed: 0,
            rejected: 0,
            skipped: 0,
            undo_stack: Vec::new(),
        }
    }

    pub fn current(&self) -> Option<&crate::db::ReviewItem> {
        self.queue.get(self.current_index)
    }

    pub fn advance(&mut self) {
        if self.current_index + 1 <= self.queue.len() {
            self.current_index += 1;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.current_index >= self.queue.len()
    }
}

/// Active scanning state
pub struct ScanState {
    pub progress: ScanProgress,
    pub progress_receiver: Receiver<ScanProgress>,
    pub cancel_flag: Arc<AtomicBool>,
}

/// Application state
pub struct PhotoVault {
    /// Current active view
    pub(crate) current_view: View,

    /// Detected drives
    pub(crate) drives: Vec<DriveInfo>,

    /// Currently selected drive path
    pub(crate) selected_drive: Option<PathBuf>,

    /// Database connection (if a drive is selected and not scanning)
    pub(crate) database: Option<Database>,

    /// Active scanning state
    pub(crate) scan_state: Option<ScanState>,

    /// Photo count after indexing
    pub(crate) photo_count: i64,

    // --- Phase 3 additions ---
    /// All loaded photos (from DB, ordered by date DESC)
    pub(crate) photos: Vec<Photo>,

    /// Currently selected photo index (for detail view)
    pub(crate) selected_photo_index: Option<usize>,

    /// Thumbnail service (created when a drive is selected)
    pub(crate) thumbnail_service: Option<ThumbnailService>,

    /// Whether we're currently generating thumbnails in the background
    pub(crate) thumbnail_generation_active: bool,

    /// Queue of photos still needing thumbnail generation (photo_id, file_path, file_hash, orientation)
    /// Processed in batches to avoid overwhelming the system.
    pub(crate) thumbnail_queue: Vec<(i64, String, String, i32)>,

    /// Cursor for incremental thumbnail scheduling over full photo list.
    pub(crate) thumbnail_scan_cursor: usize,

    /// Monotonic generation token used to invalidate stale thumbnail tasks.
    pub(crate) thumbnail_generation_epoch: u64,

    /// Current window width in pixels (for responsive grid columns)
    pub(crate) window_width: f32,

    // --- Phase 4 additions ---
    /// Face clusters loaded from database
    pub(crate) face_clusters: Vec<FaceClusterRecord>,

    /// Whether face processing is running in the background
    pub(crate) face_processing_active: bool,

    /// Current face processing progress
    pub(crate) face_processing_progress: Option<FaceProcessingProgress>,

    /// Live progress channel receiver for face processing.
    pub(crate) face_progress_receiver: Option<Receiver<FaceProcessingProgress>>,

    /// Last face-processing error shown in People view.
    pub(crate) face_processing_error: Option<String>,

    /// Cluster ID currently being name-edited
    pub(crate) editing_cluster_id: Option<i64>,

    /// Current edit text for cluster name
    pub(crate) edit_cluster_name: String,

    /// Currently selected cluster ID (for detail view)
    pub(crate) selected_cluster_id: Option<i64>,

    /// Photos belonging to the currently selected cluster
    pub(crate) cluster_photos: Vec<Photo>,

    /// Whether merge mode is active in People view
    pub(crate) merge_mode_active: bool,

    /// Selected cluster IDs for merging
    pub(crate) merge_selected_clusters: Vec<i64>,

    /// Previous view before opening photo detail (for proper back navigation)
    pub(crate) previous_view: Option<View>,

    // --- Phase 5 additions ---
    /// Duplicate groups loaded from database
    pub(crate) duplicate_groups: Vec<DuplicateGroupRecord>,

    /// Total wasted space from duplicates (bytes)
    pub(crate) duplicate_wasted_space: u64,

    /// Currently selected duplicate group (for detail view)
    pub(crate) selected_duplicate_group: Option<DuplicateGroupRecord>,

    /// Members of the currently selected duplicate group
    pub(crate) selected_duplicate_members: Vec<DuplicateGroupMemberRecord>,

    /// Whether duplicate detection is currently running
    pub(crate) duplicate_detection_running: bool,

    /// Duplicate overview summaries keyed by group id: (recoverable_bytes, preview_photo_id)
    pub(crate) duplicate_overview: Vec<(i64, u64, Option<i64>)>,

    /// Burst groups loaded from database
    pub(crate) burst_groups: Vec<BurstGroupRecord>,

    /// Number of photos that could be saved across all bursts
    pub(crate) burst_saveable_count: usize,

    /// Currently selected burst group (for detail view)
    pub(crate) selected_burst_group: Option<BurstGroupRecord>,

    /// Members of the currently selected burst group
    pub(crate) selected_burst_members: Vec<BurstGroupMemberRecord>,

    /// Whether burst detection is currently running
    pub(crate) burst_detection_running: bool,

    /// Burst overview previews keyed by group id: (group_id, preview_photo_ids)
    pub(crate) burst_overview_previews: Vec<(i64, Vec<i64>)>,

    // --- Phase 6 additions ---
    /// Current search input text
    pub(crate) search_query: String,

    /// Search suggestion chips
    pub(crate) search_suggestions: Vec<String>,

    /// Grouped search results
    pub(crate) search_results: Option<Vec<crate::services::SearchResultGroup>>,

    /// Flat list of photo IDs from latest search results (for cull mode)
    pub(crate) search_result_photo_ids: Vec<i64>,

    /// Search loading state
    pub(crate) search_loading: bool,

    /// Quick cull session state
    pub(crate) cull_state: Option<crate::views::CullState>,

    /// Face review deck state
    pub(crate) face_review_state: Option<FaceReviewState>,

    /// Outstanding review queue size (for sidebar badge)
    pub(crate) face_review_pending: i64,

    /// Whether cull finish confirmation is pending
    pub(crate) cull_confirm_pending: bool,

    /// Previous view before entering cull mode
    pub(crate) cull_return_view: Option<View>,

    /// Trashed items list
    pub(crate) trash_items: Vec<TrashedPhotoRecord>,

    /// Trash statistics
    pub(crate) trash_stats: TrashStats,

    /// Selected trashed photo IDs for bulk restore
    pub(crate) selected_trash_ids: std::collections::HashSet<i64>,

    /// Pending confirmation for empty trash action
    pub(crate) confirm_empty_trash: bool,

    /// Pending per-photo permanent deletion confirmation
    pub(crate) confirm_delete_photo_id: Option<i64>,

    // --- Phase 7 additions ---
    /// Application configuration
    pub(crate) config: AppConfig,

    /// Last detected index changes
    pub(crate) pending_index_changes: Option<IndexChanges>,

    /// Whether to trigger face processing after the current background scan completes.
    pub(crate) run_face_processing_after_scan: bool,

    /// Geocoding progress if running
    pub(crate) geocoding_progress: Option<(usize, usize)>,

    /// Whether rotated-data regeneration is running
    pub(crate) rotated_data_regen_active: bool,

    /// Cancel flag for face processing
    pub(crate) face_cancel_flag: Option<Arc<AtomicBool>>,

    /// Whether ML models are available (checked once at startup)
    pub(crate) ml_available: bool,

    /// People names detected in the currently viewed photo
    pub(crate) current_photo_people: Vec<String>,

    /// Number of faces detected in the currently viewed photo
    pub(crate) current_photo_face_count: usize,

    /// Current rotation offset in photo detail (0, 90, 180, 270)
    pub(crate) photo_rotation: i32,

    /// Pre-decoded display image for photo detail (rotated thumbnail, fast to manipulate)
    pub(crate) current_display_image: Option<image::DynamicImage>,

    /// Whether the metadata panel is shown in photo detail view
    pub(crate) show_metadata_panel: bool,

    /// Selected photo IDs in timeline multi-select mode
    pub(crate) selected_timeline_photo_ids: HashSet<i64>,

    /// Hovered photo in timeline grid (for hover-only selection affordance)
    pub(crate) hovered_timeline_photo_id: Option<i64>,

    /// Hovered day key in timeline header (for day-level selection affordance)
    pub(crate) hovered_timeline_day_key: Option<String>,

    /// Documents view data (non-photo categorized items)
    pub(crate) documents: Vec<Photo>,

    /// Documents search query (FTS text)
    pub(crate) documents_query: String,

    /// Category filter in documents view
    pub(crate) documents_filter: Option<ContentCategory>,

    /// OCR/document analysis running
    pub(crate) document_analysis_active: bool,

    /// OCR/document analysis progress receiver
    pub(crate) ocr_progress_receiver: Option<Receiver<OcrProgress>>,

    /// OCR/document analysis progress
    pub(crate) ocr_progress: Option<OcrProgress>,

    /// OCR processing cancel flag
    pub(crate) ocr_cancel_flag: Option<Arc<AtomicBool>>,

}

impl PhotoVault {
    pub(crate) fn merge_detected_and_remembered_drives(&self, detected: Vec<DriveInfo>) -> Vec<DriveInfo> {
        let mut merged = detected;
        let mut seen = std::collections::HashSet::new();

        for d in &merged {
            seen.insert(d.path.clone());
        }

        for remembered in &self.config.remembered_drives {
            if seen.contains(remembered) {
                continue;
            }
            if let Some(info) = DriveDetector::inspect_path(remembered.clone()) {
                seen.insert(info.path.clone());
                merged.push(info);
            }
        }

        merged
    }

    pub(crate) fn configured_thumbnail_size(&self) -> ThumbnailSize {
        // Use Medium for crisp grid thumbnails (500px, 85% quality, Lanczos3).
        ThumbnailSize::Medium
    }

    /// Create new application instance
    pub fn new() -> (Self, Task<Message>) {
        let config = AppConfig::load();
        let app = Self {
            current_view: View::Welcome,
            drives: Vec::new(),
            selected_drive: None,
            database: None,
            scan_state: None,
            photo_count: 0,
            // Phase 3
            photos: Vec::new(),
            selected_photo_index: None,
            thumbnail_service: None,
            thumbnail_generation_active: false,
            thumbnail_queue: Vec::new(),
            thumbnail_scan_cursor: 0,
            thumbnail_generation_epoch: 0,
            window_width: 1280.0, // sensible default until first resize event
            // Phase 4
            face_clusters: Vec::new(),
            face_processing_active: false,
            face_processing_progress: None,
            face_progress_receiver: None,
            face_processing_error: None,
            editing_cluster_id: None,
            edit_cluster_name: String::new(),
            selected_cluster_id: None,
            cluster_photos: Vec::new(),
            merge_mode_active: false,
            merge_selected_clusters: Vec::new(),
            previous_view: None,
            // Phase 5
            duplicate_groups: Vec::new(),
            duplicate_wasted_space: 0,
            selected_duplicate_group: None,
            selected_duplicate_members: Vec::new(),
            duplicate_detection_running: false,
            duplicate_overview: Vec::new(),
            burst_groups: Vec::new(),
            burst_saveable_count: 0,
            selected_burst_group: None,
            selected_burst_members: Vec::new(),
            burst_detection_running: false,
            burst_overview_previews: Vec::new(),
            // Phase 6
            search_query: String::new(),
            search_suggestions: Vec::new(),
            search_results: None,
            search_result_photo_ids: Vec::new(),
            search_loading: false,
            cull_state: None,
            face_review_state: None,
            face_review_pending: 0,
            cull_confirm_pending: false,
            cull_return_view: None,
            trash_items: Vec::new(),
            trash_stats: TrashStats::default(),
            selected_trash_ids: std::collections::HashSet::new(),
            confirm_empty_trash: false,
            confirm_delete_photo_id: None,
            // Phase 7
            config,
            pending_index_changes: None,
            run_face_processing_after_scan: false,
            geocoding_progress: None,
            rotated_data_regen_active: false,
            // Phase 8: Production readiness
            face_cancel_flag: None,
            ml_available: crate::bootstrap::has_face_models(),
            current_photo_people: Vec::new(),
            current_photo_face_count: 0,
            photo_rotation: 0,
            current_display_image: None,
            show_metadata_panel: true,
            selected_timeline_photo_ids: HashSet::new(),
            hovered_timeline_photo_id: None,
            hovered_timeline_day_key: None,
            documents: Vec::new(),
            documents_query: String::new(),
            documents_filter: None,
            document_analysis_active: false,
            ocr_progress_receiver: None,
            ocr_progress: None,
            ocr_cancel_flag: None,
        };

        // Detect drives on startup
        let task = Task::perform(
            async { DriveDetector::detect() },
            Message::DrivesDetected,
        );

        (app, task)
    }

    /// Maximum number of thumbnails to process per batch.
    pub(crate) const THUMBNAIL_BATCH_SIZE: usize = 8;
    /// Number of photos inspected per scheduling pass.
    pub(crate) const THUMBNAIL_SCAN_CHUNK: usize = 64;
    /// Max number of queued thumbnail jobs to keep in memory.
    pub(crate) const THUMBNAIL_QUEUE_TARGET: usize = 48;
    /// Number of currently-visible rows to prioritize.
    pub(crate) const THUMBNAIL_VISIBLE_ROWS: usize = 10;
    /// Number of prefetch rows just beyond visible rows.
    pub(crate) const THUMBNAIL_PREFETCH_ROWS: usize = 6;

    pub(crate) fn timeline_columns_for_width(width: f32) -> usize {
        let available_width = (width - 200.0 - 32.0).max(168.0);
        (available_width / 168.0).floor().max(2.0) as usize
    }

    pub(crate) fn photo_detail_navigation_list(&self) -> &[Photo] {
        if self.previous_view == Some(View::ClusterDetail) && !self.cluster_photos.is_empty() {
            &self.cluster_photos
        } else if self.previous_view == Some(View::Documents) && !self.documents.is_empty() {
            &self.documents
        } else {
            &self.photos
        }
    }
}
