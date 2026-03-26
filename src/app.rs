//! Main application state and logic

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_channel::Receiver;
use iced::keyboard;
use iced::widget::{column, container, row, text};
use iced::{event, Element, Length, Subscription, Task};

use crate::components::{ScanProgressView, Sidebar};
use crate::config::{AppConfig, AppTheme, DateFormat};
use crate::db::{
    create_schema, BurstGroupMemberRecord, BurstGroupRecord, BurstRepo, Database,
    DuplicateGroupMemberRecord, DuplicateGroupRecord, DuplicateRepo, FaceClusterRecord, FaceRepo,
    PhotoRepo, TrashRepo, TrashedPhotoRecord, migrations,
};
use crate::models::Photo;
use crate::services::{
    BurstConfig, BurstDetector, DriveDetector, DriveInfo, DuplicateDetector,
    FaceProcessingProgress, FaceProcessingResult, FaceProcessor, ScanProgress, SearchService,
    ThumbnailService, ThumbnailSize, TrashService, TrashStats, Reindexer, IndexChanges,
    ApplyResult, GeocodingService,
};
use tokio::task::JoinSet;
use crate::theme::colors::{Backgrounds, Border, Text};
use crate::views::{
    BurstsView, CullState, CullView, DuplicatesView, PeopleView, PhotoDetailView, SearchView,
    SettingsView, TimelineView, TrashView, WelcomeView,
};

const THUMBNAIL_DB_FLUSH_BATCH: usize = 64;

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Scanning,
    Timeline,
    People,
    ClusterDetail,
    Duplicates,
    DuplicateDetail,
    Bursts,
    BurstDetail,
    Search,
    Cull,
    Trash,
    Settings,
    PhotoDetail,
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
    current_view: View,

    /// Detected drives
    drives: Vec<DriveInfo>,

    /// Currently selected drive path
    selected_drive: Option<PathBuf>,

    /// Database connection (if a drive is selected and not scanning)
    database: Option<Database>,

    /// Active scanning state
    scan_state: Option<ScanState>,

    /// Photo count after indexing
    photo_count: i64,

    // --- Phase 3 additions ---
    /// All loaded photos (from DB, ordered by date DESC)
    photos: Vec<Photo>,

    /// Currently selected photo index (for detail view)
    selected_photo_index: Option<usize>,

    /// Thumbnail service (created when a drive is selected)
    thumbnail_service: Option<ThumbnailService>,

    /// Whether we're currently generating thumbnails in the background
    thumbnail_generation_active: bool,

    /// Queue of photos still needing thumbnail generation (photo_id, file_path, file_hash)
    /// Processed in batches to avoid overwhelming the system.
    thumbnail_queue: Vec<(i64, String, String)>,

    /// Cursor for incremental thumbnail scheduling over full photo list.
    thumbnail_scan_cursor: usize,

    /// Monotonic generation token used to invalidate stale thumbnail tasks.
    thumbnail_generation_epoch: u64,

    /// Current window width in pixels (for responsive grid columns)
    window_width: f32,

    // --- Phase 4 additions ---
    /// Face clusters loaded from database
    face_clusters: Vec<FaceClusterRecord>,

    /// Whether face processing is running in the background
    face_processing_active: bool,

    /// Current face processing progress
    face_processing_progress: Option<FaceProcessingProgress>,

    /// Live progress channel receiver for face processing.
    face_progress_receiver: Option<Receiver<FaceProcessingProgress>>,

    /// Last face-processing error shown in People view.
    face_processing_error: Option<String>,

    /// Cluster ID currently being name-edited
    editing_cluster_id: Option<i64>,

    /// Current edit text for cluster name
    edit_cluster_name: String,

    /// Currently selected cluster ID (for detail view)
    selected_cluster_id: Option<i64>,

    /// Photos belonging to the currently selected cluster
    cluster_photos: Vec<Photo>,

    /// Whether merge mode is active in People view
    merge_mode_active: bool,

    /// Selected cluster IDs for merging
    merge_selected_clusters: Vec<i64>,

    /// Previous view before opening photo detail (for proper back navigation)
    previous_view: Option<View>,

    // --- Phase 5 additions ---
    /// Duplicate groups loaded from database
    duplicate_groups: Vec<DuplicateGroupRecord>,

    /// Total wasted space from duplicates (bytes)
    duplicate_wasted_space: u64,

    /// Currently selected duplicate group (for detail view)
    selected_duplicate_group: Option<DuplicateGroupRecord>,

    /// Members of the currently selected duplicate group
    selected_duplicate_members: Vec<DuplicateGroupMemberRecord>,

    /// Whether duplicate detection is currently running
    duplicate_detection_running: bool,

    /// Duplicate overview summaries keyed by group id: (recoverable_bytes, preview_photo_id)
    duplicate_overview: Vec<(i64, u64, Option<i64>)>,

    /// Burst groups loaded from database
    burst_groups: Vec<BurstGroupRecord>,

    /// Number of photos that could be saved across all bursts
    burst_saveable_count: usize,

    /// Currently selected burst group (for detail view)
    selected_burst_group: Option<BurstGroupRecord>,

    /// Members of the currently selected burst group
    selected_burst_members: Vec<BurstGroupMemberRecord>,

    /// Whether burst detection is currently running
    burst_detection_running: bool,

    /// Burst overview previews keyed by group id: (group_id, preview_photo_ids)
    burst_overview_previews: Vec<(i64, Vec<i64>)>,

    // --- Phase 6 additions ---
    /// Current search input text
    search_query: String,

    /// Search suggestion chips
    search_suggestions: Vec<String>,

    /// Grouped search results
    search_results: Option<Vec<crate::services::SearchResultGroup>>,

    /// Flat list of photo IDs from latest search results (for cull mode)
    search_result_photo_ids: Vec<i64>,

    /// Search loading state
    search_loading: bool,

    /// Quick cull session state
    cull_state: Option<CullState>,

    /// Whether cull finish confirmation is pending
    cull_confirm_pending: bool,

    /// Previous view before entering cull mode
    cull_return_view: Option<View>,

    /// Trashed items list
    trash_items: Vec<TrashedPhotoRecord>,

    /// Trash statistics
    trash_stats: TrashStats,

    /// Selected trashed photo IDs for bulk restore
    selected_trash_ids: std::collections::HashSet<i64>,

    /// Pending confirmation for empty trash action
    confirm_empty_trash: bool,

    /// Pending per-photo permanent deletion confirmation
    confirm_delete_photo_id: Option<i64>,

    // --- Phase 7 additions ---
    /// Application configuration
    config: AppConfig,

    /// Last detected index changes
    pending_index_changes: Option<IndexChanges>,

    /// Geocoding progress if running
    geocoding_progress: Option<(usize, usize)>,

    /// Cancel flag for face processing
    face_cancel_flag: Option<Arc<AtomicBool>>,

    /// Whether ML models are available (checked once at startup)
    ml_available: bool,

    /// People names detected in the currently viewed photo
    current_photo_people: Vec<String>,

    /// Current rotation offset in photo detail (0, 90, 180, 270)
    photo_rotation: i32,

    /// Path to the rotated image temp file (if rotation applied)
    rotated_image_path: Option<std::path::PathBuf>,
}

/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Navigate to a different view
    NavigateTo(View),

    /// Select a drive to index
    SelectDrive(PathBuf),

    /// Open folder browser dialog
    BrowseForFolder,

    /// Folder selected from browser
    FolderSelected(Option<PathBuf>),

    /// Drives detected
    DrivesDetected(Vec<DriveInfo>),

    /// Start scanning the selected drive
    StartScan,

    /// Poll scan channels (from subscription tick)
    PollScanChannels,

    /// Cancel ongoing scan
    CancelScan,

    /// Scan finished -- database returned from scanner thread
    ScanFinished(ScanResult),

    /// Scan complete -- user clicked "Continue"
    ScanComplete,

    // --- Phase 3 additions ---
    /// Photos loaded from database
    PhotosLoaded(Vec<Photo>),

    /// Select a photo to view in detail
    SelectPhoto(i64),

    /// Close photo detail view
    ClosePhotoDetail,

    /// Navigate to previous photo
    PreviousPhoto,

    /// Navigate to next photo
    NextPhoto,

    /// Batch of thumbnails ready
    ThumbnailBatchReady(u64, Vec<(i64, PathBuf)>),

    /// DB write for a thumbnail batch completed; triggers the next batch
    ThumbnailBatchSaved(u64),

    /// Continue background thumbnail scheduling in small chunks.
    ContinueThumbnailScheduling(u64),

    /// Keyboard event
    KeyPressed(keyboard::Key),

    /// No-op message (used as callback when we don't need the result)
    NoOp,

    // --- Phase 4: Face processing ---
    /// Start face processing pipeline
    ProcessFaces,

    /// Face processing completed
    FaceProcessingComplete(Result<FaceProcessingResult, String>),

    /// Run clustering on existing face embeddings
    RunClustering,

    /// Clustering completed
    ClusteringComplete(Result<usize, String>),

    /// Face clusters loaded from database
    FaceClustersLoaded(Vec<FaceClusterRecord>),

    /// Select a face cluster to view
    SelectCluster(i64),

    /// Go back from cluster detail to People view
    BackToPeople,

    /// Start editing a cluster name
    StartEditClusterName(i64),

    /// Cluster name text changed
    EditClusterName(i64, String),

    /// Save the edited cluster name
    SaveClusterName(i64),

    /// Toggle merge mode on/off
    ToggleMergeMode,

    /// Toggle a cluster's selection for merging
    ToggleMergeSelect(i64),

    /// Execute merge of all selected clusters
    MergeSelectedClusters,

    // --- Phase 5: Duplicate & Burst Detection ---
    /// Run duplicate detection
    RunDuplicateDetection,

    /// Duplicate detection completed
    DuplicateDetectionComplete(Vec<DuplicateGroupRecord>, u64, Vec<(i64, u64, Option<i64>)>),

    /// Open a specific duplicate group for review
    OpenDuplicateGroup(i64),

    /// Close duplicate detail view
    CloseDuplicateDetail,

    /// Set which photo to keep in a duplicate group
    SetKeepDuplicate(i64, i64),

    /// Keep the suggested photo and trash the rest
    KeepSuggestedDuplicate(i64),

    /// Trash non-suggested duplicates in a group
    TrashNonSuggestedDuplicates(i64),

    /// Dismiss a duplicate group without trashing
    DismissDuplicateGroup(i64),

    /// Run burst detection
    RunBurstDetection,

    /// Burst detection completed
    BurstDetectionComplete(Vec<BurstGroupRecord>, usize, Vec<(i64, Vec<i64>)>),

    /// Open a burst group for review
    OpenBurstGroup(i64),

    /// Close burst detail view
    CloseBurstDetail,

    /// Set the best photo in a burst group
    SetBestFromBurst(i64, i64),

    /// Keep only the best from a burst
    KeepBestFromBurst(i64),

    /// Trash non-best photos in a burst
    TrashNonBestFromBurst(i64),

    /// Dismiss a burst group without trashing
    DismissBurstGroup(i64),

    // --- Phase 6: Search, Cull, Trash ---
    SearchInputChanged(String),
    SearchSuggestionSelected(String),
    ExecuteSearch,
    SearchComplete(Vec<crate::services::SearchResultGroup>, Vec<i64>),
    SearchSuggestionsLoaded(Vec<String>),
    EnterCullMode(Vec<i64>),
    EnterCullFromSearch,
    ExitCullMode,
    CullNext,
    CullPrev,
    CullToggleTrash,
    CullUndo,
    CullFinish,
    CullConfirmTrash,

    LoadTrash,
    TrashLoaded(Vec<TrashedPhotoRecord>, TrashStats),
    TrashPhotos(Vec<i64>),
    RestorePhoto(i64),
    RestoreSelected,
    ToggleTrashSelection(i64),
    PermanentlyDeletePhoto(i64),
    ConfirmPermanentlyDeletePhoto(i64),
    EmptyTrash,
    ConfirmEmptyTrash,

    // --- Phase 7: Settings, Reindexing, Geocoding ---
    SetTheme(AppTheme),
    SetThumbnailSize(u32),
    SetScanHiddenFolders(bool),
    SetFaceConfidence(f32),
    SetClusteringThreshold(f32),
    SetBurstWindow(i64),
    SetTrashAutoDelete(u32),
    SetDateFormat(DateFormat),

    RescanLibrary,
    RebuildFaceClusters,

    CheckForChanges,
    ChangesDetected(IndexChanges),
    ApplyChanges,
    ChangesApplied(ApplyResult),

    RunGeocoding,
    GeocodingProgress { processed: usize, total: usize },
    GeocodingComplete,

    /// Cancel face processing
    CancelFaceProcessing,

    /// Rotate photo in detail view (spawns async rotate task)
    RotatePhoto,

    /// Rotated image ready
    RotatedImageReady(Option<std::path::PathBuf>),
}

/// Wrapper for scan result to make it Debug + Clone for Message
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub photo_count: i64,
    pub final_progress: ScanProgress,
}

impl PhotoVault {
    fn merge_detected_and_remembered_drives(&self, detected: Vec<DriveInfo>) -> Vec<DriveInfo> {
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

    fn configured_thumbnail_size(&self) -> ThumbnailSize {
        // Fast-path grid rendering: generate small thumbnails first for responsiveness.
        ThumbnailSize::Small
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
            geocoding_progress: None,
            // Phase 8: Production readiness
            face_cancel_flag: None,
            ml_available: crate::bootstrap::has_face_models(),
            current_photo_people: Vec::new(),
            photo_rotation: 0,
            rotated_image_path: None,
        };

        // Detect drives on startup
        let task = Task::perform(
            async { DriveDetector::detect() },
            Message::DrivesDetected,
        );

        (app, task)
    }

    /// Application title
    pub fn title(&self) -> String {
        match &self.selected_drive {
            Some(path) => format!("PhotoVault - {}", path.display()),
            None => "PhotoVault".to_string(),
        }
    }

    /// Current app theme.
    pub fn theme(&self) -> iced::Theme {
        match self.config.theme {
            AppTheme::Dark => iced::Theme::Dark,
            AppTheme::Light => iced::Theme::Light,
            AppTheme::System => iced::Theme::default(),
        }
    }

    /// Subscription for polling scan progress, keyboard events, and window resize
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        // Background progress polling (scan, face processing, or any active background op)
        let has_background_ops = self.scan_state.is_some()
            || self.face_processing_active
            || self.duplicate_detection_running
            || self.burst_detection_running;

        if has_background_ops {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(120))
                    .map(|_| Message::PollScanChannels),
            );
        }

        // Keyboard events for all views (shortcuts)
        if self.selected_drive.is_some() {
            subs.push(event::listen_with(|event, _status, _id| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                    Some(Message::KeyPressed(key))
                }
                _ => None,
            }));
        }

        Subscription::batch(subs)
    }

    /// Load photos from database
    fn load_photos(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = PhotoRepo::new(&db.conn);
                        // Load all photos (up to 50k for now)
                        let mut photos = match repo.get_all_by_date(50000, 0) {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::error!("Failed to load photos: {}", e);
                                Vec::new()
                            }
                        };

                        // Resolve relative thumbnail paths to absolute (DB stores relative for portability)
                        for photo in &mut photos {
                            if let Some(ref rel_path) = photo.thumbnail_path {
                                let abs_path = drive_path.join(rel_path);
                                photo.thumbnail_path =
                                    Some(abs_path.to_string_lossy().to_string());
                            }
                        }

                        photos
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database for loading photos: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::PhotosLoaded,
        )
    }

    /// Maximum number of thumbnails to process per batch.
    const THUMBNAIL_BATCH_SIZE: usize = 8;
    /// Number of photos inspected per scheduling pass.
    const THUMBNAIL_SCAN_CHUNK: usize = 64;
    /// Max number of queued thumbnail jobs to keep in memory.
    const THUMBNAIL_QUEUE_TARGET: usize = 48;
    /// Number of currently-visible rows to prioritize.
    const THUMBNAIL_VISIBLE_ROWS: usize = 10;
    /// Number of prefetch rows just beyond visible rows.
    const THUMBNAIL_PREFETCH_ROWS: usize = 6;

    fn timeline_columns_for_width(width: f32) -> usize {
        let available_width = (width - 200.0 - 32.0).max(168.0);
        (available_width / 168.0).floor().max(2.0) as usize
    }

    fn begin_thumbnail_generation_epoch(&mut self) {
        self.thumbnail_generation_epoch = self.thumbnail_generation_epoch.wrapping_add(1);
        self.thumbnail_generation_active = false;
        self.thumbnail_queue.clear();
        self.thumbnail_scan_cursor = 0;
    }

    fn seed_thumbnail_queue_for_timeline(&mut self) {
        let columns = Self::timeline_columns_for_width(self.window_width);
        let priority_count = columns * (Self::THUMBNAIL_VISIBLE_ROWS + Self::THUMBNAIL_PREFETCH_ROWS);
        let initial_end = priority_count.min(self.photos.len());

        for photo in self.photos.iter().take(initial_end) {
            if photo.thumbnail_path.is_none() {
                self.thumbnail_queue
                    .push((photo.id, photo.file_path.clone(), photo.file_hash.clone()));
            }
        }

        self.thumbnail_scan_cursor = initial_end;
    }

    fn schedule_thumbnail_chunk(&mut self) {
        if self.thumbnail_queue.len() >= Self::THUMBNAIL_QUEUE_TARGET {
            return;
        }

        let end = (self.thumbnail_scan_cursor + Self::THUMBNAIL_SCAN_CHUNK).min(self.photos.len());

        for photo in &self.photos[self.thumbnail_scan_cursor..end] {
            if self.thumbnail_queue.len() >= Self::THUMBNAIL_QUEUE_TARGET {
                break;
            }
            if photo.thumbnail_path.is_none() {
                self.thumbnail_queue
                    .push((photo.id, photo.file_path.clone(), photo.file_hash.clone()));
            }
        }

        self.thumbnail_scan_cursor = end;
    }

    /// Start background thumbnail generation for the next batch from the queue.
    ///
    /// Drains up to THUMBNAIL_BATCH_SIZE items from `self.thumbnail_queue` and
    /// spawns them concurrently via JoinSet. When the batch finishes, the
    /// `ThumbnailBatchReady` handler will call this again if the queue is
    /// not empty, creating a natural batch chain until all thumbnails are done.
    fn start_thumbnail_generation(&mut self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        // Create thumbnail service if needed
        if self.thumbnail_service.is_none() {
            match ThumbnailService::new(drive_path, 2.0) {
                Ok(service) => {
                    // Load existing thumbnails from disk
                    if let Err(e) = service.load_existing_thumbnails() {
                        tracing::warn!("Failed to load existing thumbnails: {}", e);
                    }
                    self.thumbnail_service = Some(service);
                }
                Err(e) => {
                    tracing::error!("Failed to create thumbnail service: {}", e);
                    return Task::none();
                }
            }
        }

        // If the queue is empty, nothing to do
        if self.thumbnail_queue.is_empty() {
            self.thumbnail_generation_active = false;
            return Task::none();
        }

        self.thumbnail_generation_active = true;

        // Drain the next batch from the front of the queue
        let batch_end = self.thumbnail_queue.len().min(Self::THUMBNAIL_BATCH_SIZE);
        let batch: Vec<(i64, String, String)> = self.thumbnail_queue.drain(..batch_end).collect();
        let remaining = self.thumbnail_queue.len();

        tracing::info!(
            "Starting thumbnail batch: {} photos ({} remaining in queue)",
            batch.len(),
            remaining
        );

        let drive_path = drive_path.clone();
        let thumb_size = self.configured_thumbnail_size();
        let epoch = self.thumbnail_generation_epoch;

        // Clone the shared service into an Arc so all spawn_blocking calls reuse it.
        let service = Arc::new(
            self.thumbnail_service
                .take()
                .expect("thumbnail_service was just set above"),
        );
        let service_for_restore = Arc::clone(&service);

        // Spawn background thumbnail generation for this batch only
        Task::perform(
            async move {
                let mut join_set = JoinSet::new();

                for (photo_id, file_path, file_hash) in batch {
                    let full_path = drive_path.join(&file_path);
                    let svc = Arc::clone(&service);

                    join_set.spawn_blocking(move || {
                        if !full_path.exists() {
                            return None;
                        }

                        match svc.generate_thumbnail(
                            &full_path,
                            &file_hash,
                            thumb_size,
                        ) {
                            Ok(path) => Some((photo_id, path)),
                            Err(e) => {
                                tracing::debug!(
                                    "Thumbnail generation failed for {}: {}",
                                    file_path,
                                    e
                                );
                                None
                            }
                        }
                    });
                }

                // Collect results as they complete
                let mut results = Vec::new();
                while let Some(res) = join_set.join_next().await {
                    if let Ok(Some((id, path))) = res {
                        results.push((id, path));
                    }
                }

                // Return the Arc so we can restore the service
                (results, service_for_restore)
            },
            move |(results, _service_arc)| Message::ThumbnailBatchReady(epoch, results),
        )
    }

    /// Load face clusters from the database
    fn load_face_clusters(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();

        Task::perform(
            async move {
                // Regenerate any missing face crop thumbnails in a blocking thread
                // (handles faces detected before crop-saving code was added)
                let drive_for_regen = drive_path.clone();
                let regen_result = tokio::task::spawn_blocking(move || {
                    FaceProcessor::regenerate_missing_crops(&drive_for_regen)
                })
                .await;
                match regen_result {
                    Ok(Ok(count)) => {
                        if count > 0 {
                            tracing::info!("Regenerated {} face crop thumbnails", count);
                        }
                    }
                    Ok(Err(e)) => tracing::warn!("Failed to regenerate face crops: {}", e),
                    Err(e) => tracing::warn!("Face crop regeneration task panicked: {}", e),
                }

                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let face_repo = FaceRepo::new(&db.conn);
                        let mut clusters = face_repo.get_all_clusters().unwrap_or_default();
                        tracing::info!(
                            "load_face_clusters: got {} clusters from DB",
                            clusters.len()
                        );
                        FaceRepo::populate_face_thumbnails(&mut clusters, &drive_path);
                        clusters
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database for face clusters: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::FaceClustersLoaded,
        )
    }

    /// Load trash items and stats from DB.
    fn load_trash(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();
        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = TrashRepo::new(&db.conn);
                        let items = repo.get_all().unwrap_or_default();
                        let stats = TrashService::get_stats(&db.conn).unwrap_or_default();
                        (items, stats)
                    }
                    Err(e) => {
                        tracing::error!("Failed to open DB for trash load: {}", e);
                        (Vec::new(), TrashStats::default())
                    }
                }
            },
            |(items, stats)| Message::TrashLoaded(items, stats),
        )
    }

    /// Handle messages
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NavigateTo(view) => {
                tracing::info!("NavigateTo: {:?}", view);
                if view == self.current_view {
                    return Task::none();
                }
                if view != View::Timeline {
                    self.begin_thumbnail_generation_epoch();
                }
                // If navigating to Timeline, always reload photos from DB
                // (photos may have new thumbnails, or user may have re-scanned)
                let task = if view == View::Timeline {
                    self.load_photos()
                } else if view == View::People {
                    self.load_face_clusters()
                } else if view == View::Duplicates {
                    // Trigger duplicate detection when navigating to Duplicates view
                    self.current_view = view;
                    return self.update(Message::RunDuplicateDetection);
                } else if view == View::Bursts {
                    // Trigger burst detection when navigating to Bursts view
                    self.current_view = view;
                    return self.update(Message::RunBurstDetection);
                } else if view == View::Trash {
                    self.current_view = view;
                    return self.update(Message::LoadTrash);
                } else {
                    Task::none()
                };
                self.current_view = view;
                task
            }

            Message::SelectDrive(path) => {
                tracing::info!("Selected drive: {:?}", path);
                self.begin_thumbnail_generation_epoch();

                match Database::open_for_drive(&path) {
                    Ok(db) => {
                        // Create schema if needed
                        if db.needs_schema().unwrap_or(true) {
                            if let Err(e) = create_schema(&db.conn) {
                                tracing::error!("Failed to create schema: {}", e);
                                return Task::none();
                            }
                        }

                        // Backup database before migrations
                        if let Err(e) = Database::backup(&path, 3) {
                            tracing::debug!("DB backup skipped: {}", e);
                        }

                        if let Err(e) = migrations::run_migrations(&db.conn) {
                            tracing::error!("Failed to run migrations: {}", e);
                            return Task::none();
                        }

                        // Quick integrity check on open
                        match db.check_integrity() {
                            Ok(true) => {}
                            Ok(false) => tracing::warn!("Database integrity check failed for {:?}", path),
                            Err(e) => tracing::debug!("Could not run integrity check: {}", e),
                        }

                        // Get photo count
                        let repo = PhotoRepo::new(&db.conn);
                        self.photo_count = repo.count().unwrap_or(0);

                        self.selected_drive = Some(path);
                        if let Some(ref p) = self.selected_drive {
                            self.config.remember_drive(p.clone());
                            if let Err(e) = self.config.save() {
                                tracing::warn!("Failed to save remembered drive: {}", e);
                            }
                        }
                        self.database = Some(db);

                        // Kick off background geocoding once a drive is selected.
                        if self.geocoding_progress.is_none() {
                            let _ = self.update(Message::RunGeocoding);
                        }

                        // If library is empty, start scanning
                        if self.photo_count == 0 {
                            return self.update(Message::StartScan);
                        } else {
                            self.current_view = View::Timeline;
                            // Load photos for timeline
                            return self.load_photos();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database: {}", e);
                    }
                }

                Task::none()
            }

            Message::BrowseForFolder => {
                tracing::info!("Browse for folder requested");
                Task::perform(
                    async {
                        let result = rfd::AsyncFileDialog::new()
                            .set_title("Select a folder containing your photos")
                            .pick_folder()
                            .await;
                        result.map(|handle| handle.path().to_path_buf())
                    },
                    Message::FolderSelected,
                )
            }

            Message::FolderSelected(path) => {
                if let Some(path) = path {
                    return self.update(Message::SelectDrive(path));
                }
                Task::none()
            }

            Message::DrivesDetected(drives) => {
                let merged = self.merge_detected_and_remembered_drives(drives);
                tracing::info!("Detected {} drives (including remembered)", merged.len());
                self.drives = merged;
                Task::none()
            }

            Message::StartScan => {
                let Some(drive_path) = &self.selected_drive else {
                    return Task::none();
                };

                // Take the database -- scanner will own it
                let Some(database) = self.database.take() else {
                    tracing::error!("No database available for scanning");
                    return Task::none();
                };

                tracing::info!("Starting scan of {:?}", drive_path);
                // Non-blocking: stay on current view (or Scanning if first scan)
                if self.photo_count == 0 {
                    self.current_view = View::Scanning;
                }

                let drive_path = drive_path.clone();
                let drive_path_for_recovery = drive_path.clone();

                // Start the scanner
                let (progress_rx, cancel_flag, join_handle) =
                    crate::services::scanner::start_scan(
                        drive_path,
                        database,
                        self.config.scan_hidden_folders,
                    );

                // Store scan state
                self.scan_state = Some(ScanState {
                    progress: ScanProgress::default(),
                    progress_receiver: progress_rx,
                    cancel_flag,
                });

                // Spawn a task to await the join handle and return the result
                Task::perform(
                    async move {
                        match join_handle.await {
                            Ok(result) => {
                                let count = PhotoRepo::new(&result.database.conn)
                                    .count()
                                    .unwrap_or(0);
                                (
                                    result.database,
                                    ScanResult {
                                        photo_count: count,
                                        final_progress: result.final_progress,
                                    },
                                )
                            }
                            Err(e) => {
                                tracing::error!("Scanner thread panicked: {}", e);
                                // Return a zero-count result instead of panicking
                                // Re-open DB for recovery
                                let db = Database::open_for_drive(&drive_path_for_recovery)
                                    .expect("Failed to re-open database after scanner panic");
                                (
                                    db,
                                    ScanResult {
                                        photo_count: 0,
                                        final_progress: ScanProgress::default(),
                                    },
                                )
                            }
                        }
                    },
                    |(_database, scan_result)| {
                        // We need to return the database AND the result.
                        // Since Message must be Clone+Debug, we return just the result
                        // and handle database restoration separately.
                        Message::ScanFinished(scan_result)
                    },
                )
            }

            Message::PollScanChannels => {
                if let Some(ref mut state) = self.scan_state {
                    // Drain all available progress updates
                    while let Ok(progress) = state.progress_receiver.try_recv() {
                        state.progress = progress;
                    }
                }

                if let Some(ref mut rx) = self.face_progress_receiver {
                    while let Ok(progress) = rx.try_recv() {
                        self.face_processing_progress = Some(progress);
                    }
                }
                Task::none()
            }

            Message::CancelScan => {
                if let Some(ref state) = self.scan_state {
                    state.cancel_flag.store(true, Ordering::Relaxed);
                    tracing::info!("Scan cancellation requested");
                }
                // Don't clear scan_state yet -- wait for ScanFinished
                Task::none()
            }

            Message::ScanFinished(result) => {
                tracing::info!(
                    "Scan finished: {} photos indexed",
                    result.photo_count
                );
                self.photo_count = result.photo_count;

                // Update the final progress in scan state so UI shows completion
                if let Some(ref mut state) = self.scan_state {
                    state.progress = result.final_progress;
                }

                // Re-open the database (scanner consumed it, we need a fresh connection)
                if let Some(ref drive_path) = self.selected_drive {
                    match Database::open_for_drive(drive_path) {
                        Ok(db) => {
                            // Run maintenance after bulk scan
                            let _ = db.run_maintenance();
                            self.database = Some(db);
                        }
                        Err(e) => {
                            tracing::error!("Failed to re-open database: {}", e);
                        }
                    }
                }

                // If still on Scanning view (first scan), auto-advance
                if self.current_view == View::Scanning {
                    Task::none()
                } else {
                    // Scan was running in background — clear state and reload if on Timeline
                    self.scan_state = None;
                    if self.current_view == View::Timeline {
                        self.load_photos()
                    } else {
                        Task::none()
                    }
                }
            }

            Message::ScanComplete => {
                // User clicked "Continue" after scan completed
                self.scan_state = None;
                self.current_view = View::Timeline;
                // Load photos for the timeline
                self.load_photos()
            }

            // --- Phase 3 handlers ---
            Message::PhotosLoaded(photos) => {
                tracing::info!("Loaded {} photos for timeline", photos.len());
                self.begin_thumbnail_generation_epoch();
                self.photos = photos;
                self.photo_count = self.photos.len() as i64;

                // Prioritize visible timeline region first, then continue incrementally.
                self.seed_thumbnail_queue_for_timeline();
                self.schedule_thumbnail_chunk();

                tracing::info!(
                    "Queued {} thumbnails ({} photos total)",
                    self.thumbnail_queue.len(),
                    self.photos.len()
                );

                if !self.thumbnail_queue.is_empty() {
                    // Start processing the first batch
                    self.start_thumbnail_generation()
                } else if self.thumbnail_scan_cursor < self.photos.len() {
                    let epoch = self.thumbnail_generation_epoch;
                    Task::perform(
                        async {
                            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                        },
                        move |_| Message::ContinueThumbnailScheduling(epoch),
                    )
                } else {
                    Task::none()
                }
            }

            Message::SelectPhoto(photo_id) => {
                // Find the photo index
                if let Some(idx) = self.photos.iter().position(|p| p.id == photo_id) {
                    self.previous_view = Some(self.current_view.clone());
                    self.selected_photo_index = Some(idx);
                    self.current_view = View::PhotoDetail;
                    self.photo_rotation = 0;
                    self.rotated_image_path = None;

                    // Look up people in this photo
                    self.current_photo_people.clear();
                    if let Some(ref db) = self.database {
                        let face_repo = FaceRepo::new(&db.conn);
                        if let Ok(names) = face_repo.get_person_names_for_photo(photo_id) {
                            self.current_photo_people = names;
                        }
                    }
                }
                Task::none()
            }

            Message::ClosePhotoDetail => {
                self.selected_photo_index = None;
                // Return to whatever view we came from
                self.current_view = self.previous_view.take().unwrap_or(View::Timeline);
                Task::none()
            }

            Message::PreviousPhoto => {
                self.rotated_image_path = None;
                self.photo_rotation = 0;
                if let Some(ref mut idx) = self.selected_photo_index {
                    if *idx > 0 {
                        *idx -= 1;
                    }
                }
                Task::none()
            }

            Message::NextPhoto => {
                self.rotated_image_path = None;
                self.photo_rotation = 0;
                if let Some(ref mut idx) = self.selected_photo_index {
                    if *idx + 1 < self.photos.len() {
                        *idx += 1;
                    }
                }
                Task::none()
            }

            Message::ThumbnailBatchReady(epoch, results) => {
                if epoch != self.thumbnail_generation_epoch {
                    tracing::debug!("Ignoring stale thumbnail batch for epoch {}", epoch);
                    return Task::none();
                }
                tracing::info!("Thumbnail batch ready: {} thumbnails generated", results.len());

                // Restore the thumbnail service from the Arc (it was taken in start_thumbnail_generation)
                // If the Arc still has other refs, just recreate from drive_path
                if self.thumbnail_service.is_none() {
                    if let Some(ref drive_path) = self.selected_drive {
                        if let Ok(service) = ThumbnailService::new(drive_path, 2.0) {
                            self.thumbnail_service = Some(service);
                        }
                    }
                }

                // Update in-memory photo data and DB
                if let Some(ref drive_path) = self.selected_drive {
                    // Update in-memory list (keep absolute paths for UI display)
                    for (photo_id, path) in &results {
                        if let Some(photo) = self.photos.iter_mut().find(|p| p.id == *photo_id) {
                            photo.thumbnail_path =
                                Some(path.to_string_lossy().to_string());
                        }
                    }

                    // Batch update DB (store relative paths for portability)
                    // When done, send ThumbnailBatchSaved to trigger the next batch
                    let drive_path = drive_path.clone();
                    let results_for_db = results;
                    return Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                if !results_for_db.is_empty() {
                                    let mut pending = Vec::with_capacity(results_for_db.len());
                                    for (photo_id, path) in &results_for_db {
                                        let rel_path = path
                                            .strip_prefix(&drive_path)
                                            .unwrap_or(path)
                                            .to_string_lossy()
                                            .to_string();
                                        pending.push((*photo_id, rel_path));
                                    }

                                    let mut idx = 0;
                                    while idx < pending.len() {
                                        let end = (idx + THUMBNAIL_DB_FLUSH_BATCH).min(pending.len());
                                        if let Ok(tx) = db.conn.unchecked_transaction() {
                                            for (photo_id, rel_path) in &pending[idx..end] {
                                                let _ = tx.execute(
                                                    "UPDATE photos SET thumbnail_path = ?1 WHERE id = ?2",
                                                    rusqlite::params![rel_path, photo_id],
                                                );
                                            }
                                            let _ = tx.commit();
                                        }
                                        idx = end;
                                    }
                                }
                            }
                        },
                        move |_| Message::ThumbnailBatchSaved(epoch),
                    );
                }
                Task::none()
            }

            Message::ThumbnailBatchSaved(epoch) => {
                if epoch != self.thumbnail_generation_epoch {
                    tracing::debug!("Ignoring stale thumbnail saved callback for epoch {}", epoch);
                    return Task::none();
                }
                self.thumbnail_generation_active = false;
                self.schedule_thumbnail_chunk();
                // Previous batch DB write completed; start the next batch
                if !self.thumbnail_queue.is_empty() {
                    tracing::info!(
                        "Thumbnail batch saved, starting next batch ({} remaining)",
                        self.thumbnail_queue.len()
                    );
                    self.start_thumbnail_generation()
                } else if self.thumbnail_scan_cursor < self.photos.len() {
                    Task::perform(
                        async {
                            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                        },
                        move |_| Message::ContinueThumbnailScheduling(epoch),
                    )
                } else {
                    tracing::info!("All thumbnails generated successfully");
                    self.thumbnail_generation_active = false;
                    Task::none()
                }
            }

            Message::ContinueThumbnailScheduling(epoch) => {
                if epoch != self.thumbnail_generation_epoch {
                    tracing::debug!("Ignoring stale thumbnail scheduling tick for epoch {}", epoch);
                    return Task::none();
                }
                if self.current_view != View::Timeline {
                    self.thumbnail_generation_active = false;
                    return Task::none();
                }
                self.schedule_thumbnail_chunk();
                if !self.thumbnail_queue.is_empty() {
                    self.start_thumbnail_generation()
                } else if self.thumbnail_scan_cursor < self.photos.len() {
                    Task::perform(
                        async {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        },
                        move |_| Message::ContinueThumbnailScheduling(epoch),
                    )
                } else {
                    self.thumbnail_generation_active = false;
                    Task::none()
                }
            }

            Message::KeyPressed(key) => {
                // --- View-specific shortcuts ---
                if self.current_view == View::PhotoDetail {
                    match key {
                        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                            return self.update(Message::PreviousPhoto);
                        }
                        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                            return self.update(Message::NextPhoto);
                        }
                        keyboard::Key::Named(keyboard::key::Named::Escape) => {
                            return self.update(Message::ClosePhotoDetail);
                        }
                        keyboard::Key::Named(keyboard::key::Named::Delete)
                        | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                            // Trash current photo
                            if let Some(idx) = self.selected_photo_index {
                                if let Some(photo) = self.photos.get(idx) {
                                    return self.update(Message::TrashPhotos(vec![photo.id]));
                                }
                            }
                        }
                        keyboard::Key::Character(ref ch) => {
                            if ch.to_lowercase() == "r" {
                                return self.update(Message::RotatePhoto);
                            }
                        }
                        _ => {}
                    }
                } else if self.current_view == View::Cull {
                    match key {
                        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                            return self.update(Message::CullPrev);
                        }
                        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                            return self.update(Message::CullNext);
                        }
                        keyboard::Key::Named(keyboard::key::Named::Enter) => {
                            return self.update(Message::CullFinish);
                        }
                        keyboard::Key::Named(keyboard::key::Named::Escape) => {
                            return self.update(Message::ExitCullMode);
                        }
                        keyboard::Key::Character(ref ch) => {
                            let lower = ch.to_lowercase();
                            if lower == "x" {
                                return self.update(Message::CullToggleTrash);
                            }
                            if lower == "u" {
                                return self.update(Message::CullUndo);
                            }
                        }
                        _ => {}
                    }
                } else if self.current_view == View::ClusterDetail {
                    if let keyboard::Key::Named(keyboard::key::Named::Escape) = key {
                        return self.update(Message::BackToPeople);
                    }
                } else if self.current_view == View::DuplicateDetail {
                    if let keyboard::Key::Named(keyboard::key::Named::Escape) = key {
                        return self.update(Message::CloseDuplicateDetail);
                    }
                } else if self.current_view == View::BurstDetail {
                    if let keyboard::Key::Named(keyboard::key::Named::Escape) = key {
                        return self.update(Message::CloseBurstDetail);
                    }
                }

                // --- Global shortcuts (work from any non-detail view) ---
                match key {
                    keyboard::Key::Character(ref ch) => {
                        let lower = ch.to_lowercase();
                        // '/' or 'f' → focus search (unless in a text-entry context)
                        if (lower == "/" || lower == "f")
                            && !matches!(
                                self.current_view,
                                View::PhotoDetail | View::Cull | View::Search
                            )
                            && self.editing_cluster_id.is_none()
                        {
                            return self.update(Message::NavigateTo(View::Search));
                        }
                    }
                    _ => {}
                }

                Task::none()
            }

            // --- Phase 6 handlers ---
            Message::SearchInputChanged(input) => {
                self.search_query = input.clone();

                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    return Task::perform(
                        async move {
                            if input.trim().is_empty() {
                                return Vec::new();
                            }
                            match Database::open_for_drive(&drive_path) {
                                Ok(db) => SearchService::get_suggestions(&db.conn, &input)
                                    .unwrap_or_default(),
                                Err(_) => Vec::new(),
                            }
                        },
                        Message::SearchSuggestionsLoaded,
                    );
                }

                Task::none()
            }

            Message::SearchSuggestionSelected(value) => {
                self.search_query = value;
                self.update(Message::ExecuteSearch)
            }

            Message::SearchSuggestionsLoaded(suggestions) => {
                self.search_suggestions = suggestions;
                Task::none()
            }

            Message::ExecuteSearch => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                let query_text = self.search_query.clone();
                let drive_path = drive_path.clone();
                self.search_loading = true;

                Task::perform(
                    async move {
                        match Database::open_for_drive(&drive_path) {
                            Ok(db) => {
                                let parsed = crate::search::QueryParser::parse(&query_text);
                                let rows = SearchService::search(&db.conn, &parsed)
                                    .unwrap_or_default();
                                let ids = rows.iter().map(|r| r.photo_id).collect::<Vec<_>>();
                                let groups = SearchService::group_by_date(rows);
                                (groups, ids)
                            }
                            Err(_) => (Vec::new(), Vec::new()),
                        }
                    },
                    |(groups, ids)| Message::SearchComplete(groups, ids),
                )
            }

            Message::SearchComplete(groups, ids) => {
                self.search_loading = false;
                self.search_results = Some(groups);
                self.search_result_photo_ids = ids;
                Task::none()
            }

            Message::EnterCullFromSearch => {
                if self.search_result_photo_ids.is_empty() {
                    return Task::none();
                }
                self.update(Message::EnterCullMode(self.search_result_photo_ids.clone()))
            }

            Message::EnterCullMode(photo_ids) => {
                if photo_ids.is_empty() {
                    return Task::none();
                }
                self.cull_return_view = Some(self.current_view.clone());
                self.cull_state = Some(CullState::new(photo_ids));
                self.cull_confirm_pending = false;
                self.current_view = View::Cull;
                Task::none()
            }

            Message::ExitCullMode => {
                self.cull_state = None;
                self.cull_confirm_pending = false;
                self.current_view = self.cull_return_view.take().unwrap_or(View::Timeline);
                Task::none()
            }

            Message::CullNext => {
                if let Some(ref mut cull) = self.cull_state {
                    cull.next();
                    self.cull_confirm_pending = false;
                }
                Task::none()
            }

            Message::CullPrev => {
                if let Some(ref mut cull) = self.cull_state {
                    cull.prev();
                    self.cull_confirm_pending = false;
                }
                Task::none()
            }

            Message::CullToggleTrash => {
                if let Some(ref mut cull) = self.cull_state {
                    cull.toggle_trash();
                    self.cull_confirm_pending = false;
                }
                Task::none()
            }

            Message::CullUndo => {
                if let Some(ref mut cull) = self.cull_state {
                    cull.undo();
                    self.cull_confirm_pending = false;
                }
                Task::none()
            }

            Message::CullFinish => {
                if let Some(ref cull) = self.cull_state {
                    if cull.marked_for_trash.is_empty() {
                        return self.update(Message::ExitCullMode);
                    }
                    self.cull_confirm_pending = true;
                    return Task::none();
                }
                Task::none()
            }

            Message::CullConfirmTrash => {
                if let Some(ref cull) = self.cull_state {
                    let ids = cull.marked_for_trash.iter().copied().collect::<Vec<_>>();
                    self.cull_confirm_pending = false;
                    return self.update(Message::TrashPhotos(ids));
                }
                Task::none()
            }

            Message::LoadTrash => self.load_trash(),

            Message::TrashLoaded(items, stats) => {
                self.trash_items = items;
                self.trash_stats = stats;
                self.selected_trash_ids.clear();
                self.confirm_empty_trash = false;
                self.confirm_delete_photo_id = None;
                Task::none()
            }

            Message::TrashPhotos(photo_ids) => {
                if photo_ids.is_empty() {
                    return self.update(Message::ExitCullMode);
                }
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let drive_path = drive_path.clone();

                let task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let _ = TrashService::trash_photos(&db.conn, &photo_ids);
                        }
                    },
                    |_| Message::LoadTrash,
                );

                // refresh local photo list and exit cull
                self.cull_state = None;
                self.cull_confirm_pending = false;
                self.current_view = View::Trash;
                let reload = self.load_photos();
                Task::batch([task, reload])
            }

            Message::RestorePhoto(photo_id) => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let drive_path = drive_path.clone();
                let task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let _ = TrashService::restore_photos(&db.conn, &[photo_id]);
                        }
                    },
                    |_| Message::LoadTrash,
                );
                let reload = self.load_photos();
                Task::batch([task, reload])
            }

            Message::RestoreSelected => {
                let ids = self.selected_trash_ids.iter().copied().collect::<Vec<_>>();
                if ids.is_empty() {
                    return Task::none();
                }
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let drive_path = drive_path.clone();
                let task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let _ = TrashService::restore_photos(&db.conn, &ids);
                        }
                    },
                    |_| Message::LoadTrash,
                );
                let reload = self.load_photos();
                Task::batch([task, reload])
            }

            Message::ToggleTrashSelection(photo_id) => {
                if !self.selected_trash_ids.insert(photo_id) {
                    self.selected_trash_ids.remove(&photo_id);
                }
                Task::none()
            }

            Message::PermanentlyDeletePhoto(photo_id) => {
                self.confirm_delete_photo_id = Some(photo_id);
                Task::none()
            }

            Message::ConfirmPermanentlyDeletePhoto(photo_id) => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let drive_path = drive_path.clone();
                let task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let _ = TrashService::permanent_delete(&db.conn, &[photo_id], &drive_path);
                        }
                    },
                    |_| Message::LoadTrash,
                );
                let reload = self.load_photos();
                Task::batch([task, reload])
            }

            Message::EmptyTrash => {
                self.confirm_empty_trash = true;
                Task::none()
            }

            Message::ConfirmEmptyTrash => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let drive_path = drive_path.clone();
                let task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let _ = TrashService::empty_trash(&db.conn, &drive_path);
                        }
                    },
                    |_| Message::LoadTrash,
                );
                let reload = self.load_photos();
                Task::batch([task, reload])
            }

            // --- Phase 7 handlers ---
            Message::SetTheme(theme) => {
                self.config.theme = theme;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::SetThumbnailSize(size) => {
                self.config.thumbnail_size = size;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                if self.current_view == View::Timeline {
                    self.begin_thumbnail_generation_epoch();
                    self.seed_thumbnail_queue_for_timeline();
                    self.schedule_thumbnail_chunk();
                    if !self.thumbnail_queue.is_empty() {
                        return self.start_thumbnail_generation();
                    }
                }
                Task::none()
            }

            Message::SetScanHiddenFolders(enabled) => {
                self.config.scan_hidden_folders = enabled;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::SetFaceConfidence(v) => {
                self.config.face_detection_confidence = v.clamp(0.0, 1.0);
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::SetClusteringThreshold(v) => {
                self.config.face_clustering_threshold = v.clamp(0.0, 1.0);
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::SetBurstWindow(seconds) => {
                self.config.burst_time_window_seconds = seconds.max(1);
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::SetTrashAutoDelete(days) => {
                self.config.trash_auto_delete_days = days;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::SetDateFormat(format) => {
                self.config.date_format = format;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                Task::none()
            }

            Message::RescanLibrary => self.update(Message::StartScan),

            Message::RebuildFaceClusters => self.update(Message::RunClustering),

            Message::CheckForChanges => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let drive_path = drive_path.clone();
                let scan_hidden_folders = self.config.scan_hidden_folders;

                Task::perform(
                    async move {
                        match Database::open_for_drive(&drive_path) {
                            Ok(db) => {
                                let reindexer = Reindexer::new_with_options(scan_hidden_folders);
                                reindexer.detect_changes(&db.conn, &drive_path).unwrap_or_default()
                            }
                            Err(e) => {
                                tracing::error!("CheckForChanges DB open failed: {}", e);
                                IndexChanges::default()
                            }
                        }
                    },
                    Message::ChangesDetected,
                )
            }

            Message::ChangesDetected(changes) => {
                self.pending_index_changes = Some(changes.clone());
                if changes.is_empty() {
                    tracing::info!("No index changes detected");
                    return Task::none();
                }
                self.update(Message::ApplyChanges)
            }

            Message::ApplyChanges => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                let Some(changes) = self.pending_index_changes.clone() else {
                    return Task::none();
                };

                let drive_path = drive_path.clone();
                let scan_hidden_folders = self.config.scan_hidden_folders;
                Task::perform(
                    async move {
                        match Database::open_for_drive(&drive_path) {
                            Ok(db) => {
                                let reindexer = Reindexer::new_with_options(scan_hidden_folders);
                                reindexer.apply_changes(&db.conn, &changes).unwrap_or_default()
                            }
                            Err(e) => {
                                tracing::error!("ApplyChanges DB open failed: {}", e);
                                ApplyResult::default()
                            }
                        }
                    },
                    Message::ChangesApplied,
                )
            }

            Message::ChangesApplied(result) => {
                tracing::info!("Applied index changes: {:?}", result);
                self.pending_index_changes = None;

                let mut tasks = vec![self.load_photos()];
                if result.new_files > 0 {
                    tasks.push(self.update(Message::StartScan));
                }
                Task::batch(tasks)
            }

            Message::RunGeocoding => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };
                if self.geocoding_progress.is_some() {
                    return Task::none();
                }
                let drive_path = drive_path.clone();
                self.geocoding_progress = Some((0, 0));

                Task::perform(
                    async move {
                        use crate::db::geonames::{geonames_db_exists, geonames_db_path};

                        if !geonames_db_exists() {
                            tracing::warn!("GeoNames DB missing at {}", geonames_db_path().display());
                            return (0usize, 0usize);
                        }

                        let geocoder = match GeocodingService::new(geonames_db_path()) {
                            Ok(g) => g,
                            Err(e) => {
                                tracing::error!("Failed to open geonames DB: {}", e);
                                return (0, 0);
                            }
                        };

                        let db = match Database::open_for_drive(&drive_path) {
                            Ok(db) => db,
                            Err(e) => {
                                tracing::error!("Failed to open drive DB for geocoding: {}", e);
                                return (0, 0);
                            }
                        };

                        let mut stmt = match db.conn.prepare(
                            "SELECT id, gps_latitude, gps_longitude FROM photos WHERE gps_latitude IS NOT NULL AND gps_longitude IS NOT NULL AND (location_city IS NULL OR location_country IS NULL)",
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!("Failed to query photos for geocoding: {}", e);
                                return (0, 0);
                            }
                        };

                        let rows: Vec<(i64, f64, f64)> = stmt
                            .query_map([], |row| {
                                Ok((
                                    row.get::<_, i64>(0)?,
                                    row.get::<_, f64>(1)?,
                                    row.get::<_, f64>(2)?,
                                ))
                            })
                            .map(|iter| iter.filter_map(|r| r.ok()).collect())
                            .unwrap_or_default();

                        let total = rows.len();
                        let mut processed = 0usize;

                        if total == 0 {
                            return (0, 0);
                        }

                        if let Ok(tx) = db.conn.unchecked_transaction() {
                            for (id, lat, lon) in rows {
                                if let Some(result) = geocoder.reverse_geocode(lat, lon) {
                                    let _ = tx.execute(
                                        "UPDATE photos SET location_city = ?1, location_country = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
                                        rusqlite::params![result.city, result.country, id],
                                    );
                                }
                                processed += 1;
                            }
                            let _ = tx.commit();
                            return (processed, total);
                        }

                        for (id, lat, lon) in rows {
                            if let Some(result) = geocoder.reverse_geocode(lat, lon) {
                                let _ = db.conn.execute(
                                    "UPDATE photos SET location_city = ?1, location_country = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
                                    rusqlite::params![result.city, result.country, id],
                                );
                            }
                            processed += 1;
                        }

                        (processed, total)
                    },
                    |(processed, total)| Message::GeocodingProgress { processed, total },
                )
            }

            Message::GeocodingProgress { processed, total } => {
                self.geocoding_progress = Some((processed, total));
                if total == 0 {
                    self.geocoding_progress = None;
                    return Task::none();
                }
                if processed >= total {
                    return self.update(Message::GeocodingComplete);
                }
                Task::none()
            }

            Message::GeocodingComplete => {
                self.geocoding_progress = None;
                self.load_photos()
            }

            Message::NoOp => Task::none(),

            // --- Phase 4: Face processing handlers ---
            Message::ProcessFaces => {
                if self.face_processing_active {
                    tracing::info!("ProcessFaces: already active, ignoring");
                    return Task::none();
                }

                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                tracing::info!("ProcessFaces: starting face processing pipeline");
                self.face_processing_active = true;
                self.face_processing_progress = Some(FaceProcessingProgress::default());
                self.face_processing_error = None;

                // Reset all faces_processed flags so every photo gets re-analyzed
                if let Some(ref db) = self.database {
                    let _ = db.conn.execute("UPDATE photos SET faces_processed = FALSE", []);
                    tracing::info!("Reset faces_processed flags for all photos");
                }

                let drive_path = drive_path.clone();
                let detector_confidence = self.config.face_detection_confidence;
                let model_dir = crate::bootstrap::model_dir();

                let detector_path = crate::bootstrap::detector_model_path();
                let embedder_path = crate::bootstrap::embedder_model_path();
                if !detector_path.exists() || !embedder_path.exists() {
                    self.face_processing_active = false;
                    self.face_processing_progress = None;
                    self.face_processing_error = Some(format!(
                        "Face models missing. Expected {} and {}",
                        detector_path.display(),
                        embedder_path.display()
                    ));
                    return Task::none();
                }

                let (progress_tx, progress_rx) = async_channel::bounded(32);
                self.face_progress_receiver = Some(progress_rx);

                let cancel_flag = Arc::new(AtomicBool::new(false));
                self.face_cancel_flag = Some(Arc::clone(&cancel_flag));

                // Spawn blocking face processing task
                let process_task = Task::perform(
                    async move {
                        let handle = tokio::task::spawn_blocking(move || {
                            FaceProcessor::process_photos(
                                &drive_path,
                                &model_dir,
                                detector_confidence,
                                Some(progress_tx),
                                Some(cancel_flag),
                            )
                        });

                        match handle.await {
                            Ok(result) => result,
                            Err(e) => Err(format!("Face processing thread panicked: {}", e)),
                        }
                    },
                    Message::FaceProcessingComplete,
                );

                process_task
            }

            Message::CancelFaceProcessing => {
                if let Some(ref flag) = self.face_cancel_flag {
                    flag.store(true, Ordering::Relaxed);
                    tracing::info!("Face processing cancellation requested");
                }
                Task::none()
            }

            Message::RotatePhoto => {
                self.photo_rotation = (self.photo_rotation + 90) % 360;

                // Get the current image path and rotate it in a background thread
                let source_path = if let Some(ref rp) = self.rotated_image_path {
                    Some(rp.clone())
                } else if let Some(idx) = self.selected_photo_index {
                    if let (Some(photo), Some(ref drive)) = (self.photos.get(idx), &self.selected_drive) {
                        let orig = drive.join(&photo.file_path);
                        if orig.exists() { Some(orig) } else { None }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(src) = source_path {
                    Task::perform(
                        async move {
                            let handle = tokio::task::spawn_blocking(move || {
                                let img = image::open(&src).ok()?;
                                let rotated = img.rotate90();
                                let temp_dir = std::env::temp_dir().join("photovault");
                                let _ = std::fs::create_dir_all(&temp_dir);
                                let temp_path = temp_dir.join("rotated_view.jpg");
                                rotated.save(&temp_path).ok()?;
                                Some(temp_path)
                            });
                            handle.await.ok().flatten()
                        },
                        Message::RotatedImageReady,
                    )
                } else {
                    Task::none()
                }
            }

            Message::RotatedImageReady(path) => {
                self.rotated_image_path = path;
                Task::none()
            }

            Message::FaceProcessingComplete(result) => {
                self.face_processing_active = false;
                self.face_progress_receiver = None;
                self.face_processing_progress = None;
                self.face_cancel_flag = None;

                match result {
                    Ok(result) => {
                        self.face_processing_error = None;
                        tracing::info!(
                            "Face processing complete: {} photos, {} faces, {} clusters",
                            result.photos_processed,
                            result.faces_detected,
                            result.clusters_created
                        );
                    }
                    Err(e) => {
                        self.face_processing_error = Some(e.clone());
                        tracing::error!("Face processing failed: {}", e);
                    }
                }

                // Reload clusters
                self.load_face_clusters()
            }

            Message::RunClustering => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                let drive_path = drive_path.clone();
                let clustering_threshold = self.config.face_clustering_threshold;

                Task::perform(
                    async move {
                        let handle = tokio::task::spawn_blocking(move || {
                            let db = Database::open_for_drive(&drive_path)
                                .map_err(|e| format!("Failed to open database: {}", e))?;
                            let face_repo = FaceRepo::new(&db.conn);

                            // Get all faces with embeddings
                            let all_faces = face_repo
                                .get_all_faces_with_embeddings()
                                .map_err(|e| format!("Failed to get faces: {}", e))?;

                            if all_faces.is_empty() {
                                return Ok(0);
                            }

                            // Clear existing clusters before re-clustering to avoid duplicates
                            face_repo
                                .delete_all_clusters()
                                .map_err(|e| format!("Failed to clear existing clusters: {}", e))?;

                            let epsilon = (1.0_f32 - clustering_threshold).clamp(0.2, 0.9);
                            let clusterer = crate::ml::FaceClusterer::new().with_epsilon(epsilon);
                            let assignments = clusterer.cluster(&all_faces);

                            // Group face IDs by cluster
                            let mut cluster_groups: std::collections::HashMap<i32, Vec<i64>> =
                                std::collections::HashMap::new();
                            for (face_id, cluster_id) in &assignments {
                                if *cluster_id >= 0 {
                                    cluster_groups
                                        .entry(*cluster_id)
                                        .or_default()
                                        .push(*face_id);
                                }
                            }

                            let mut clusters_created = 0;
                            for (_label, face_ids) in &cluster_groups {
                                if face_ids.len() >= 2 {
                                    let _ = face_repo.create_cluster(face_ids);
                                    clusters_created += 1;
                                }
                            }

                            Ok(clusters_created)
                        });

                        match handle.await {
                            Ok(result) => result,
                            Err(e) => Err(format!("Clustering thread panicked: {}", e)),
                        }
                    },
                    Message::ClusteringComplete,
                )
            }

            Message::ClusteringComplete(result) => {
                match result {
                    Ok(count) => {
                        tracing::info!("Clustering complete: {} clusters created", count);
                    }
                    Err(e) => {
                        tracing::error!("Clustering failed: {}", e);
                    }
                }
                self.load_face_clusters()
            }

            Message::FaceClustersLoaded(clusters) => {
                tracing::info!(
                    "FaceClustersLoaded: received {} clusters (previously had {})",
                    clusters.len(),
                    self.face_clusters.len()
                );
                self.face_clusters = clusters;
                Task::none()
            }

            Message::SelectCluster(cluster_id) => {
                self.selected_cluster_id = Some(cluster_id);
                self.current_view = View::ClusterDetail;

                // Load photos for this cluster from already-loaded photos
                if let Some(ref db) = self.database {
                    let face_repo = FaceRepo::new(&db.conn);
                    match face_repo.get_photos_for_cluster(cluster_id) {
                        Ok(photo_ids) => {
                            self.cluster_photos = self
                                .photos
                                .iter()
                                .filter(|p| photo_ids.contains(&p.id))
                                .cloned()
                                .collect();
                            tracing::info!(
                                "Loaded {} photos for cluster {}",
                                self.cluster_photos.len(),
                                cluster_id
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to load cluster photos: {}", e);
                            self.cluster_photos = Vec::new();
                        }
                    }
                }
                Task::none()
            }

            Message::BackToPeople => {
                self.current_view = View::People;
                self.selected_cluster_id = None;
                self.cluster_photos.clear();
                Task::none()
            }

            Message::StartEditClusterName(cluster_id) => {
                // Set up editing state with current name
                let current_name = self
                    .face_clusters
                    .iter()
                    .find(|c| c.id == cluster_id)
                    .and_then(|c| c.name.clone())
                    .unwrap_or_default();

                self.editing_cluster_id = Some(cluster_id);
                self.edit_cluster_name = current_name;
                Task::none()
            }

            Message::EditClusterName(_cluster_id, name) => {
                self.edit_cluster_name = name;
                Task::none()
            }

            Message::SaveClusterName(cluster_id) => {
                let name = self.edit_cluster_name.clone();
                self.editing_cluster_id = None;

                // Update in-memory
                if let Some(cluster) = self.face_clusters.iter_mut().find(|c| c.id == cluster_id) {
                    if name.is_empty() {
                        cluster.name = None;
                    } else {
                        cluster.name = Some(name.clone());
                    }
                }

                // Find other clusters with the same name for auto-merge
                let same_name_ids: Vec<i64> = if !name.is_empty() {
                    self.face_clusters
                        .iter()
                        .filter(|c| c.id != cluster_id && c.name.as_deref() == Some(&name))
                        .map(|c| c.id)
                        .collect()
                } else {
                    Vec::new()
                };

                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                let drive_path = drive_path.clone();
                let save_task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let face_repo = FaceRepo::new(&db.conn);
                            let _ = face_repo.name_cluster(cluster_id, &name);

                            // Auto-merge: if other clusters share this name, merge them in
                            for source_id in same_name_ids {
                                tracing::info!(
                                    "Auto-merging cluster {} into {} (same name: {})",
                                    source_id, cluster_id, name
                                );
                                let _ = face_repo.merge_clusters(source_id, cluster_id);
                            }
                        }
                    },
                    |_| Message::NoOp,
                );

                // Reload clusters after save+merge
                let reload_task = self.load_face_clusters();
                Task::batch([save_task, reload_task])
            }

            Message::ToggleMergeMode => {
                self.merge_mode_active = !self.merge_mode_active;
                if !self.merge_mode_active {
                    self.merge_selected_clusters.clear();
                }
                Task::none()
            }

            Message::ToggleMergeSelect(cluster_id) => {
                if let Some(pos) = self.merge_selected_clusters.iter().position(|&id| id == cluster_id) {
                    self.merge_selected_clusters.remove(pos);
                } else {
                    self.merge_selected_clusters.push(cluster_id);
                }
                Task::none()
            }

            Message::MergeSelectedClusters => {
                if self.merge_selected_clusters.len() < 2 {
                    return Task::none();
                }

                // Merge all selected into the first selected (target)
                let target_id = self.merge_selected_clusters[0];
                let source_ids: Vec<i64> = self.merge_selected_clusters[1..].to_vec();

                self.merge_mode_active = false;
                self.merge_selected_clusters.clear();

                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                let drive_path = drive_path.clone();
                let merge_task = Task::perform(
                    async move {
                        if let Ok(db) = Database::open_for_drive(&drive_path) {
                            let face_repo = FaceRepo::new(&db.conn);
                            for source_id in source_ids {
                                let _ = face_repo.merge_clusters(source_id, target_id);
                            }
                        }
                    },
                    |_| Message::NoOp,
                );

                let reload_task = self.load_face_clusters();
                Task::batch([merge_task, reload_task])
            }

            // --- Phase 5: Duplicate & Burst Detection handlers ---
            Message::RunDuplicateDetection => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                self.duplicate_detection_running = true;

                let drive_path = drive_path.clone();

                Task::perform(
                    async move {
                        let handle = tokio::task::spawn_blocking(move || {
                            let db = Database::open_for_drive(&drive_path)
                                .map_err(|e| format!("Failed to open database: {}", e))?;

                            // Run duplicate detection
                            let dup_groups = DuplicateDetector::find_duplicates(&db.conn)
                                .map_err(|e| format!("Duplicate detection failed: {}", e))?;

                            // Sync to database
                            let repo = DuplicateRepo::new(&db.conn);
                            let sync_data: Vec<(String, Vec<i64>, Option<i64>)> = dup_groups
                                .iter()
                                .map(|g| (g.hash.clone(), g.photo_ids.clone(), g.suggested_keep_id))
                                .collect();
                            repo.sync_duplicate_groups(&sync_data)
                                .map_err(|e| format!("Failed to sync duplicate groups: {}", e))?;

                            // Load groups and wasted space
                            let groups = repo.get_all_groups()
                                .map_err(|e| format!("Failed to load groups: {}", e))?;
                            let wasted = DuplicateDetector::calculate_wasted_space(&db.conn)
                                .unwrap_or(0);

                            // Build overview summaries per group
                            let mut overview = Vec::new();
                            for g in &groups {
                                let members = repo.get_group_members(g.id).unwrap_or_default();
                                if members.is_empty() {
                                    overview.push((g.id, 0, None));
                                    continue;
                                }

                                let mut total = 0u64;
                                let mut max_size = 0u64;
                                let mut preview_photo_id = None;

                                for m in &members {
                                    let s = m.file_size.unwrap_or(0).max(0) as u64;
                                    total += s;
                                    if s > max_size {
                                        max_size = s;
                                    }
                                    if m.is_suggested_keep {
                                        preview_photo_id = Some(m.photo_id);
                                    }
                                }

                                if preview_photo_id.is_none() {
                                    preview_photo_id = members.first().map(|m| m.photo_id);
                                }

                                let recoverable = total.saturating_sub(max_size);
                                overview.push((g.id, recoverable, preview_photo_id));
                            }

                            Ok::<(Vec<DuplicateGroupRecord>, u64, Vec<(i64, u64, Option<i64>)>), String>((
                                groups, wasted, overview,
                            ))
                        });

                        match handle.await {
                            Ok(Ok((groups, wasted, overview))) => (groups, wasted, overview),
                            Ok(Err(e)) => {
                                tracing::error!("Duplicate detection failed: {}", e);
                                (Vec::new(), 0, Vec::new())
                            }
                            Err(e) => {
                                tracing::error!("Duplicate detection thread panicked: {}", e);
                                (Vec::new(), 0, Vec::new())
                            }
                        }
                    },
                    |(groups, wasted, overview)| {
                        Message::DuplicateDetectionComplete(groups, wasted, overview)
                    },
                )
            }

            Message::DuplicateDetectionComplete(groups, wasted, overview) => {
                tracing::info!(
                    "Duplicate detection complete: {} groups, {} bytes wasted",
                    groups.len(),
                    wasted
                );
                self.duplicate_groups = groups;
                self.duplicate_wasted_space = wasted;
                self.duplicate_detection_running = false;
                self.duplicate_overview = overview;
                Task::none()
            }

            Message::OpenDuplicateGroup(group_id) => {
                // Find the group record
                let group = self
                    .duplicate_groups
                    .iter()
                    .find(|g| g.id == group_id)
                    .cloned();

                if let Some(group) = group {
                    self.selected_duplicate_group = Some(group);

                    // Load members from DB
                    if let Some(ref drive_path) = self.selected_drive {
                        if let Ok(db) = Database::open_for_drive(drive_path) {
                            let repo = DuplicateRepo::new(&db.conn);
                            self.selected_duplicate_members =
                                repo.get_group_members(group_id).unwrap_or_default();
                        }
                    }

                    self.current_view = View::DuplicateDetail;
                }
                Task::none()
            }

            Message::CloseDuplicateDetail => {
                self.selected_duplicate_group = None;
                self.selected_duplicate_members.clear();
                self.current_view = View::Duplicates;
                Task::none()
            }

            Message::SetKeepDuplicate(group_id, photo_id) => {
                if let Some(ref drive_path) = self.selected_drive {
                    if let Ok(db) = Database::open_for_drive(drive_path) {
                        let repo = DuplicateRepo::new(&db.conn);
                        let _ = repo.set_keep_photo(group_id, photo_id);

                        // Reload members
                        self.selected_duplicate_members =
                            repo.get_group_members(group_id).unwrap_or_default();
                    }
                }
                Task::none()
            }

            Message::KeepSuggestedDuplicate(group_id) => {
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let task = Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let repo = DuplicateRepo::new(&db.conn);
                                // Trash non-suggested photos
                                if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                                    for pid in &photo_ids {
                                        let _ = db.conn.execute(
                                            "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                            rusqlite::params![pid],
                                        );
                                    }
                                }
                                // Remove the group
                                let _ = repo.delete_group(group_id);
                            }
                        },
                        |_| Message::RunDuplicateDetection,
                    );
                    return task;
                }
                Task::none()
            }

            Message::TrashNonSuggestedDuplicates(group_id) => {
                // Same as KeepSuggested — soft-delete non-keep photos and remove group
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let task = Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let repo = DuplicateRepo::new(&db.conn);
                                if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                                    for pid in &photo_ids {
                                        let _ = db.conn.execute(
                                            "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                            rusqlite::params![pid],
                                        );
                                    }
                                }
                                let _ = repo.delete_group(group_id);
                            }
                        },
                        |_| Message::RunDuplicateDetection,
                    );
                    // After trashing, go back to duplicates list
                    self.selected_duplicate_group = None;
                    self.selected_duplicate_members.clear();
                    self.current_view = View::Duplicates;
                    return task;
                }
                Task::none()
            }

            Message::DismissDuplicateGroup(group_id) => {
                // Just remove the group from DB without trashing any photos
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let task = Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let repo = DuplicateRepo::new(&db.conn);
                                let _ = repo.delete_group(group_id);
                            }
                        },
                        |_| Message::RunDuplicateDetection,
                    );
                    return task;
                }
                Task::none()
            }

            Message::RunBurstDetection => {
                let Some(ref drive_path) = self.selected_drive else {
                    return Task::none();
                };

                self.burst_detection_running = true;

                let drive_path = drive_path.clone();
                let burst_window = self.config.burst_time_window_seconds.max(1);

                Task::perform(
                    async move {
                        let handle = tokio::task::spawn_blocking(move || {
                            let db = Database::open_for_drive(&drive_path)
                                .map_err(|e| format!("Failed to open database: {}", e))?;

                            // Run burst detection
                            let detector = BurstDetector::new(BurstConfig {
                                max_gap_seconds: burst_window,
                                min_photos: 3,
                            });
                            let burst_groups = detector.find_bursts(&db.conn)
                                .map_err(|e| format!("Burst detection failed: {}", e))?;

                            // Sync to database
                            let repo = BurstRepo::new(&db.conn);
                            let sync_data: Vec<(String, String, Vec<i64>)> = burst_groups
                                .iter()
                                .map(|g| {
                                    (
                                        g.start_time.to_rfc3339(),
                                        g.end_time.to_rfc3339(),
                                        g.photo_ids.clone(),
                                    )
                                })
                                .collect();
                            repo.sync_burst_groups(&sync_data)
                                .map_err(|e| format!("Failed to sync burst groups: {}", e))?;

                            // Set a default best pick quickly (first/earliest member).
                            let groups_from_db = repo.get_all_groups()
                                .map_err(|e| format!("Failed to load groups: {}", e))?;

                            for group in &groups_from_db {
                                let members = repo.get_group_members(group.id).unwrap_or_default();
                                if let Some(first) = members.first() {
                                    let _ = repo.set_suggested_best(group.id, first.photo_id);
                                }
                            }

                            // Reload groups
                            let final_groups = repo.get_all_groups()
                                .map_err(|e| format!("Failed to reload groups: {}", e))?;

                            // Calculate saveable count
                            let total_photos: usize = final_groups.iter().map(|g| g.photo_count as usize).sum();
                            let saveable = if total_photos > final_groups.len() {
                                total_photos - final_groups.len()
                            } else {
                                0
                            };

                            // Build overview preview strips (up to 5 photo ids per group)
                            let mut previews: Vec<(i64, Vec<i64>)> = Vec::new();
                            for g in &final_groups {
                                let members = repo.get_group_members(g.id).unwrap_or_default();
                                let ids: Vec<i64> = members.into_iter().take(5).map(|m| m.photo_id).collect();
                                previews.push((g.id, ids));
                            }

                            Ok::<(Vec<BurstGroupRecord>, usize, Vec<(i64, Vec<i64>)>), String>((
                                final_groups,
                                saveable,
                                previews,
                            ))
                        });

                        match handle.await {
                            Ok(Ok((groups, saveable, previews))) => (groups, saveable, previews),
                            Ok(Err(e)) => {
                                tracing::error!("Burst detection failed: {}", e);
                                (Vec::new(), 0, Vec::new())
                            }
                            Err(e) => {
                                tracing::error!("Burst detection thread panicked: {}", e);
                                (Vec::new(), 0, Vec::new())
                            }
                        }
                    },
                    |(groups, saveable, previews)| {
                        Message::BurstDetectionComplete(groups, saveable, previews)
                    },
                )
            }

            Message::BurstDetectionComplete(groups, saveable, previews) => {
                tracing::info!(
                    "Burst detection complete: {} groups, {} saveable photos",
                    groups.len(),
                    saveable
                );
                self.burst_groups = groups;
                self.burst_saveable_count = saveable;
                self.burst_detection_running = false;
                self.burst_overview_previews = previews;
                Task::none()
            }

            Message::OpenBurstGroup(group_id) => {
                let group = self
                    .burst_groups
                    .iter()
                    .find(|g| g.id == group_id)
                    .cloned();

                if let Some(group) = group {
                    self.selected_burst_group = Some(group);

                    // Load members from DB
                    if let Some(ref drive_path) = self.selected_drive {
                        if let Ok(db) = Database::open_for_drive(drive_path) {
                            let repo = BurstRepo::new(&db.conn);
                            self.selected_burst_members =
                                repo.get_group_members(group_id).unwrap_or_default();
                        }
                    }

                    self.current_view = View::BurstDetail;
                }
                Task::none()
            }

            Message::CloseBurstDetail => {
                self.selected_burst_group = None;
                self.selected_burst_members.clear();
                self.current_view = View::Bursts;
                Task::none()
            }

            Message::SetBestFromBurst(group_id, photo_id) => {
                if let Some(ref drive_path) = self.selected_drive {
                    if let Ok(db) = Database::open_for_drive(drive_path) {
                        let repo = BurstRepo::new(&db.conn);
                        let _ = repo.set_suggested_best(group_id, photo_id);

                        // Reload members
                        self.selected_burst_members =
                            repo.get_group_members(group_id).unwrap_or_default();
                    }
                }
                Task::none()
            }

            Message::KeepBestFromBurst(group_id) => {
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let task = Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let repo = BurstRepo::new(&db.conn);
                                if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                                    for pid in &photo_ids {
                                        let _ = db.conn.execute(
                                            "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                            rusqlite::params![pid],
                                        );
                                    }
                                }
                                let _ = repo.delete_group(group_id);
                            }
                        },
                        |_| Message::RunBurstDetection,
                    );
                    return task;
                }
                Task::none()
            }

            Message::TrashNonBestFromBurst(group_id) => {
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let task = Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let repo = BurstRepo::new(&db.conn);
                                if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                                    for pid in &photo_ids {
                                        let _ = db.conn.execute(
                                            "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                            rusqlite::params![pid],
                                        );
                                    }
                                }
                                let _ = repo.delete_group(group_id);
                            }
                        },
                        |_| Message::RunBurstDetection,
                    );
                    self.selected_burst_group = None;
                    self.selected_burst_members.clear();
                    self.current_view = View::Bursts;
                    return task;
                }
                Task::none()
            }

            Message::DismissBurstGroup(group_id) => {
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let task = Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let repo = BurstRepo::new(&db.conn);
                                let _ = repo.delete_group(group_id);
                            }
                        },
                        |_| Message::RunBurstDetection,
                    );
                    return task;
                }
                Task::none()
            }
        }
    }

    /// Render the application
    pub fn view(&self) -> Element<'_, Message> {
        // Show scanning progress if scanning
        if self.current_view == View::Scanning {
            if let Some(ref state) = self.scan_state {
                return ScanProgressView::view(&state.progress);
            } else {
                return ScanProgressView::view(&ScanProgress::default());
            }
        }

        // Photo detail view (full-screen overlay, no sidebar)
        if self.current_view == View::PhotoDetail {
            if let Some(idx) = self.selected_photo_index {
                if let Some(photo) = self.photos.get(idx) {
                    let has_prev = idx > 0;
                    let has_next = idx + 1 < self.photos.len();
                    if let Some(ref drive_path) = self.selected_drive {
                        return PhotoDetailView::view(
                            photo,
                            has_prev,
                            has_next,
                            drive_path,
                            &self.current_photo_people,
                            self.rotated_image_path.as_ref(),
                        );
                    }
                }
            }
            // Fallback: shouldn't happen, but return to timeline
            return TimelineView::view();
        }

        // If no drive selected, show welcome screen
        if self.selected_drive.is_none() {
            return WelcomeView::view(&self.drives);
        }

        // Main layout: sidebar + content
        let sidebar = Sidebar::view(&self.current_view, self.config.theme);

        let content = match self.current_view {
            View::Welcome => WelcomeView::view(&self.drives),
            View::Scanning => unreachable!(), // Handled above
            View::Timeline => {
                if self.photos.is_empty() {
                    TimelineView::view()
                } else {
                    // Calculate responsive column count:
                    // Sidebar is ~200px, padding 32px total, each thumb is 160+8px gap
                    let available_width = (self.window_width - 200.0 - 32.0).max(168.0);
                    let columns = (available_width / 168.0).floor().max(2.0) as usize;
                    TimelineView::view_with_photos(&self.photos, columns)
                }
            }
            View::People => PeopleView::view_with_clusters(
                &self.face_clusters,
                self.editing_cluster_id,
                &self.edit_cluster_name,
                self.face_processing_active,
                self.face_processing_progress.as_ref(),
                self.face_processing_error.as_deref(),
                self.merge_mode_active,
                &self.merge_selected_clusters,
                self.ml_available,
            ),
            View::ClusterDetail => {
                // Find the selected cluster record for display
                let cluster = self
                    .selected_cluster_id
                    .and_then(|id| self.face_clusters.iter().find(|c| c.id == id));

                if let Some(cluster) = cluster {
                    let is_editing = self.editing_cluster_id == Some(cluster.id);
                    let available_width = (self.window_width - 200.0 - 32.0).max(168.0);
                    let columns = (available_width / 168.0).floor().max(2.0) as usize;

                    PeopleView::view_cluster_detail(
                        cluster,
                        &self.cluster_photos,
                        is_editing,
                        &self.edit_cluster_name,
                        columns,
                    )
                } else {
                    // Cluster not found — fallback to People view
                    PeopleView::view_with_clusters(
                        &self.face_clusters,
                        self.editing_cluster_id,
                        &self.edit_cluster_name,
                        self.face_processing_active,
                        self.face_processing_progress.as_ref(),
                        self.face_processing_error.as_deref(),
                        self.merge_mode_active,
                        &self.merge_selected_clusters,
                        self.ml_available,
                    )
                }
            }
            View::Search => SearchView::view(
                &self.search_query,
                &self.search_suggestions,
                self.search_results.as_ref(),
                self.search_loading,
                self.selected_drive.as_deref(),
                &self.photos,
            ),
            View::Cull => {
                if let Some(ref state) = self.cull_state {
                    CullView::view(
                        state,
                        "Quick Cull",
                        &self.photos,
                        self.selected_drive.as_deref(),
                        self.cull_confirm_pending,
                    )
                } else {
                    SearchView::view(
                        &self.search_query,
                        &self.search_suggestions,
                        self.search_results.as_ref(),
                        self.search_loading,
                        self.selected_drive.as_deref(),
                        &self.photos,
                    )
                }
            }
            View::Trash => TrashView::view(
                &self.trash_items,
                &self.trash_stats,
                &self.selected_trash_ids,
                self.selected_drive.as_deref(),
                self.confirm_empty_trash,
                self.confirm_delete_photo_id,
            ),
            View::Settings => SettingsView::view(&self.config, self.geocoding_progress),
            View::Duplicates => {
                DuplicatesView::view(
                    &self.duplicate_groups,
                    self.duplicate_wasted_space,
                    self.duplicate_detection_running,
                    self.selected_drive.as_deref(),
                    &self.photos,
                    &self.duplicate_overview,
                )
            }
            View::DuplicateDetail => {
                if let Some(ref group) = self.selected_duplicate_group {
                    if let Some(ref drive_path) = self.selected_drive {
                        DuplicatesView::group_detail_view(
                            group,
                            &self.selected_duplicate_members,
                            drive_path,
                        )
                    } else {
                        DuplicatesView::view(
                            &self.duplicate_groups,
                            self.duplicate_wasted_space,
                            self.duplicate_detection_running,
                            self.selected_drive.as_deref(),
                            &self.photos,
                            &self.duplicate_overview,
                        )
                    }
                } else {
                    DuplicatesView::view(
                        &self.duplicate_groups,
                        self.duplicate_wasted_space,
                        self.duplicate_detection_running,
                        self.selected_drive.as_deref(),
                        &self.photos,
                        &self.duplicate_overview,
                    )
                }
            }
            View::Bursts => {
                BurstsView::view(
                    &self.burst_groups,
                    self.burst_saveable_count,
                    self.burst_detection_running,
                    self.selected_drive.as_deref(),
                    &self.photos,
                    &self.burst_overview_previews,
                )
            }
            View::BurstDetail => {
                if let Some(ref group) = self.selected_burst_group {
                    BurstsView::group_detail_view(group, &self.selected_burst_members)
                } else {
                    BurstsView::view(
                        &self.burst_groups,
                        self.burst_saveable_count,
                        self.burst_detection_running,
                        self.selected_drive.as_deref(),
                        &self.photos,
                        &self.burst_overview_previews,
                    )
                }
            }
            View::PhotoDetail => unreachable!(), // Handled above
        };

        let main_row = row![sidebar, content,];

        // Build status bar if any background operations are active
        let has_status = self.scan_state.is_some()
            || self.face_processing_active
            || self.duplicate_detection_running
            || self.burst_detection_running
            || self.geocoding_progress.is_some();

        if has_status {
            let mut status_parts: Vec<String> = Vec::new();

            if let Some(ref state) = self.scan_state {
                let p = &state.progress;
                if p.is_complete {
                    status_parts.push(format!("Scan complete: {} files", p.files_processed));
                } else {
                    status_parts.push(format!(
                        "Scanning: {}/{} files",
                        p.files_processed, p.files_found
                    ));
                }
            }

            if self.face_processing_active {
                if let Some(ref prog) = self.face_processing_progress {
                    status_parts.push(format!(
                        "Faces: {}/{} photos ({} found)",
                        prog.processed, prog.total, prog.faces_found
                    ));
                } else {
                    status_parts.push("Faces: initializing...".to_string());
                }
            }

            if self.duplicate_detection_running {
                status_parts.push("Detecting duplicates...".to_string());
            }

            if self.burst_detection_running {
                status_parts.push("Detecting bursts...".to_string());
            }

            if let Some((processed, total)) = self.geocoding_progress {
                if total > 0 {
                    status_parts.push(format!("Geocoding: {}/{}", processed, total));
                }
            }

            let status_text = status_parts.join("  |  ");

            let status_bar = container(
                text(status_text)
                    .size(11)
                    .color(Text::SECONDARY),
            )
            .width(Length::Fill)
            .padding([4, 16])
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::SECONDARY.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

            let layout = column![main_row, status_bar];

            container(layout)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(Backgrounds::PRIMARY.into()),
                    ..Default::default()
                })
                .into()
        } else {
            container(main_row)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(Backgrounds::PRIMARY.into()),
                    ..Default::default()
                })
                .into()
        }
    }
}
