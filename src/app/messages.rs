//! Application message enum and result wrappers.

use std::path::PathBuf;

use iced::keyboard;

use crate::config::{AppTheme, DateFormat};
use crate::db::{BurstGroupRecord, DuplicateGroupRecord, FaceClusterRecord, TrashedPhotoRecord};
use crate::models::Photo;
use crate::services::{
    ApplyResult, DriveInfo, FaceProcessingResult, IndexChanges, ScanProgress, TrashStats,
};

use super::state::View;

/// One open popover on the map. Anchored geographically so the card
/// follows the map when panned.
#[derive(Debug, Clone)]
pub struct MapPopover {
    pub anchor: crate::services::map_math::LatLng,
    pub photo_ids: Vec<i64>,
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

    /// File or folder dropped onto the app window. The handler validates
    /// it points at a directory and dispatches `SelectDrive`.
    FolderDropped(PathBuf),

    /// Window close requested by the OS. Triggers WAL flush + DB
    /// flush-and-close so a yanked drive doesn't leave unwritten data.
    AppExiting,

    /// Background prewarm of Small thumbnails after a scan completes.
    /// Best-effort — the on-demand path still works if prewarm aborts.
    PrewarmThumbnails,
    PrewarmThumbnailsComplete(usize),

    /// Drives detected
    DrivesDetected(Vec<DriveInfo>),

    /// Return to the drive/folder picker screen.
    BackToWelcome,

    /// Async resolved place name for currently viewed photo.
    PhotoLocationResolved {
        photo_id: i64,
        location: Option<String>,
    },

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

    /// Toggle selection for a timeline photo
    ToggleTimelinePhotoSelection(i64),

    /// Toggle selection for all photos in a timeline day group
    ToggleTimelineDaySelection(String),

    /// Timeline photo hover changed
    TimelinePhotoHover(Option<i64>),

    /// Timeline day-header hover changed
    TimelineDayHover(Option<String>),

    /// Clear all timeline photo selections
    ClearTimelinePhotoSelection,

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

    /// Keyboard event (key + modifiers)
    KeyPressed(keyboard::Key, keyboard::Modifiers),

    /// No-op message (used as callback when we don't need the result)
    NoOp,

    /// Window resized (logical pixels)
    WindowResized {
        width: f32,
        height: f32,
    },

    // --- Phase 4: Face processing ---
    /// Start face processing pipeline
    ProcessFaces,

    /// Face processing completed
    FaceProcessingComplete(Result<FaceProcessingResult, String>),

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
    ExecuteSearch,
    /// Search completed. `u64` is the generation counter; `Box` avoids a
    /// large inline payload.
    SearchComplete(u64, Box<crate::services::UnifiedSearchResults>),
    /// Debounced search trigger — fires 200ms after the last input change.
    /// Carries generation; ignored if generation doesn't match current.
    SearchDebouncedTick(u64),
    /// Recent searches loaded from DB.
    RecentSearchesLoaded(Vec<crate::db::RecentSearch>),
    /// User clicked a recent search chip.
    SearchRecentSelected(String),
    /// User clicked the X next to a recent search.
    SearchRecentRemove(String),
    /// User clicked "Clear all" on the recent searches list.
    SearchClearRecent,
    /// User clicked a person hit in results.
    SearchOpenPerson(i64),
    /// User clicked an album hit in results.
    SearchOpenAlbum(i64),
    /// User clicked a place hit — re-run search filtered to that city.
    SearchOpenPlace(String),
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
    ConfirmPermanentlyDeletePhoto(i64),
    ConfirmEmptyTrash,

    // --- Documents ---
    LoadDocuments,
    DocumentsLoaded(Vec<Photo>),
    DocumentsSearchChanged(String),
    DocumentsFilterCategory(Option<String>),
    RunDocumentAnalysis,
    DocumentAnalysisComplete(Result<usize, String>),

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
    /// Open the confirm modal before kicking off a destructive face
    /// rebuild. Settings dispatches this; the user confirms; then
    /// `RebuildFaceClusters` runs.
    RequestRebuildFaces,
    FaceDataResetComplete(Result<usize, String>),

    /// Toggle the "Show advanced" disclosure in the Settings actions
    /// section. Transient UI state (not persisted).
    ToggleSettingsAdvanced,

    CheckForChanges,
    ChangesDetected(IndexChanges),
    ApplyChanges,
    ChangesApplied(ApplyResult),

    RunGeocoding,
    GeocodingProgress {
        processed: usize,
        total: usize,
    },
    GeocodingComplete,

    RegenerateRotatedData,
    RotatedDataRegenerated {
        cleared_thumbnails: usize,
        reset_faces: usize,
    },

    RegenerateThumbnails,
    ThumbnailsRegenerated {
        cleared: usize,
    },

    /// Re-read EXIF/filename/mtime metadata and refresh photo dates in DB.
    RefreshPhotoDates,
    /// Background photo-date refresh completed.
    PhotoDatesRefreshed(Result<usize, String>),

    /// Cancel face processing
    CancelFaceProcessing,

    /// Rotate photo in detail view (instant, rotates in-memory thumbnail)
    RotatePhoto,

    /// Toggle metadata panel visibility in photo detail
    ToggleMetadataPanel,

    /// Display image loaded for photo detail (thumbnail decoded for rotation)
    DisplayImageReady(Option<Vec<u8>>, u32, u32),

    /// Toggle sidebar collapsed/expanded
    ToggleSidebar,

    /// Timeline scrolled (absolute offset) — persisted for resume-on-back.
    TimelineScrolled(iced::widget::scrollable::AbsoluteOffset),

    // --- Face Review deck ---
    /// Load pending review items from DB and enter review view
    EnterFaceReview,
    /// Async loader completed with the fetched review items
    FaceReviewLoaded(Vec<crate::db::ReviewItem>),
    /// Confirm the currently-shown face belongs to the candidate cluster
    FaceReviewSame,
    /// Reject the currently-shown face (not the same person as candidate)
    FaceReviewDifferent,
    /// Skip the current item without deciding
    FaceReviewSkip,
    /// Undo the most recent decision
    FaceReviewUndo,
    /// Leave the review view, return to People
    FaceReviewFinish,

    // --- Memories ---
    /// Low-frequency tick for day-rollover detection.
    MemoriesTick,
    /// Async generator result.
    MemoriesRegenerated(Vec<crate::services::MemoryCard>),
    /// Open a specific memory's detail (filmstrip) view.
    OpenMemory(String),
    /// Leave memory detail, return to prior view.
    CloseMemoryDetail,
    /// Hide all memories involving a cluster; persisted.
    BlockMemoriesForPerson(i64),
    /// Global on/off for Memories.
    SetMemoriesEnabled(bool),
    /// Slideshow auto-advance tick (every N seconds while not paused).
    MemorySlideshowTick,
    /// Manual prev/next within the open memory's slideshow.
    MemorySlideshowPrev,
    MemorySlideshowNext,
    /// Toggle slideshow auto-advance.
    MemorySlideshowTogglePause,

    // --- Map view ---
    MapPan {
        dx: f32,
        dy: f32,
    },
    MapPanBy {
        dx: f32,
        dy: f32,
    },
    MapPanStart {
        x: f32,
        y: f32,
    },
    MapPanEnd,
    MapZoomAt {
        x: f32,
        y: f32,
        delta: i8,
    },
    /// Scroll-wheel zoom on the main map. Uses last-known cursor position
    /// so the zoom anchors where the user is pointing.
    MapScrollZoom {
        delta: i8,
    },
    MapResetView,
    MapPinsLoaded(Vec<(i64, crate::services::map_math::LatLng)>),
    MapTileFetched(crate::services::map_math::TileId),
    MapTileFetchFailed(crate::services::map_math::TileId, String),
    MapPinClicked {
        photo_ids: Vec<i64>,
        anchor: crate::services::map_math::LatLng,
    },
    MapClosePopover,
    MapClosePopoverAt(usize),
    MapOpenClusterFilmstrip(Vec<i64>),
    SetMapCacheLimit(u32),
    ClearMapCache,
    MapCacheCleared,

    // --- Photo detail mini-map (separate interaction state) ---
    PhotoMapPanStart {
        x: f32,
        y: f32,
    },
    PhotoMapPan {
        dx: f32,
        dy: f32,
    },
    PhotoMapPanEnd,
    PhotoMapZoomAt {
        x: f32,
        y: f32,
        delta: i8,
    },

    // --- Albums ---
    /// Create a new album with the given name
    CreateAlbum(String),
    /// Delete an album (photos are NOT trashed)
    DeleteAlbum(i64),
    /// Navigate into an album's detail view
    OpenAlbum(i64),
    /// Album list loaded from DB
    AlbumsLoaded(Vec<crate::db::AlbumRecord>),
    /// Album photos loaded for the detail view
    AlbumPhotosLoaded(Vec<Photo>),
    /// Remove photos from an album
    RemovePhotosFromAlbum(i64, Vec<i64>),
    /// Open the album picker overlay for these photo IDs
    OpenAlbumPicker(Vec<i64>),
    /// Close the album picker without acting
    CloseAlbumPicker,
    /// Text changed in the "new album" input inside the picker
    AlbumPickerNameChanged(String),
    /// Toggle the "create new" input in the picker
    AlbumPickerToggleCreate,
    /// Create album from picker and add the queued photos to it
    AlbumPickerCreateAndAdd,
    /// User selected an album from the picker
    AlbumPickerSelect(i64),
    /// Start editing an album's name (inline rename)
    StartEditAlbumName(i64),
    /// Album name text changed during editing
    EditAlbumName(String),
    /// Save the edited album name
    SaveAlbumName(i64),
    /// Create album from current memory's photos ("Save as album")
    SaveMemoryAsAlbum,
    /// Return from album detail to albums grid
    BackToAlbums,

    // --- Album Suggestions ---
    /// Trigger background detection of trip/event suggestions
    RunSuggestionDetection,
    /// Detection pipeline finished with diagnostics.
    SuggestionsDetectedWithDiagnostics {
        suggestions: Vec<crate::db::AlbumSuggestionRecord>,
        diagnostics: crate::services::album_suggestions::SuggestionDiagnostics,
    },
    /// Pending suggestions loaded from DB
    SuggestionsLoaded(Vec<crate::db::AlbumSuggestionRecord>),
    /// Begin the accept flow: show inline name editor
    BeginAcceptSuggestion(i64),
    /// Name field changed during accept flow
    AcceptSuggestionNameChanged(String),
    /// Confirm: create album from suggestion + mark accepted
    ConfirmAcceptSuggestion(i64),
    /// Cancel accept flow without creating album
    CancelAcceptSuggestion,
    /// Dismiss suggestion permanently (fingerprint prevents re-detection)
    DismissSuggestion(i64),
    /// User set or cleared the home city override in settings
    SetHomeCity(String),

    // --- Insights Dashboard ---
    /// User selected a year (or None = All Time) in the insights view
    InsightsSelectYear(Option<i32>),
    /// Invalidate Insights cache/state after metadata-changing actions.
    InvalidateInsights,
    /// Insights data computation completed
    InsightsLoaded(Box<crate::services::insights::InsightsData>),
    /// Jump to a date in search from the heatmap
    InsightsJumpToDate(String),
    /// Open photo viewer scoped to a selected month.
    InsightsOpenMonth {
        year: i32,
        month: u32,
    },
    /// Search for a city from the top-locations list
    InsightsSearchCity(String),

    /// Open current memory photo in timeline-scoped detail view.
    OpenMemoryPhotoInTimeline(i64),

    // --- Phase A: Production polish ---
    /// Show a toast notification.
    ToastShow(crate::components::toast::Toast),
    /// Dismiss a toast by id.
    ToastDismiss(u64),
    /// Periodic toast expiration check.
    ToastTick,
    /// Advance spinner animation phase by one frame.
    SpinnerTick,
    /// Restore photos after a trash undo.
    RestorePhotos(Vec<i64>),

    // --- Phase B: Keyboard-first ---
    /// Toggle the `?` keyboard shortcuts overlay.
    ToggleShortcutsOverlay,
    /// Context-aware undo for current view.
    UndoLastAction,

    // --- Phase C: Destructive action polish ---
    /// Request confirmation for a destructive action.
    RequestConfirmation(crate::app::state::PendingConfirmation),
    /// Confirm the currently-pending action.
    ConfirmPending,
    /// Cancel the currently-pending action.
    CancelPending,

    // --- Asset pack installer ---
    /// Trigger one-click installation of optional assets.
    InstallAssetPack,
    /// Async installer completion callback.
    AssetPackInstalled(Result<String, String>),
    /// Dismiss startup installer prompt for this run.
    DismissAssetInstallPrompt,

    // --- In-app updater (Phase 3) ---
    /// Manual or subscription-driven update check.
    CheckForUpdates,
    /// Async check-for-updates completion.
    UpdateCheckResult(Result<crate::services::update_checker::UpdateStatus, String>),
    /// User clicked "Download" in the update banner.
    DownloadUpdate,
    /// Async install completion (either ReplacedRestartRequired or
    /// InstallerLaunched). Streamed download progress isn't wired
    /// to a Message variant yet — the service accepts a progress
    /// callback but the handler passes `None` in v1.0. The banner
    /// shows a spinner instead of a precise percentage until we
    /// thread progress through (post-v1.0 nice-to-have).
    UpdateReady(Result<crate::services::self_replace::InstallOutcome, String>),
    /// User clicked "Later" in the banner; suppress for this release.
    DismissUpdateBanner,
    /// Settings toggle for the background update check.
    SetAutoUpdateCheck(bool),
}

/// Wrapper for scan result to make it Debug + Clone for Message
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub photo_count: i64,
    pub final_progress: ScanProgress,
}
