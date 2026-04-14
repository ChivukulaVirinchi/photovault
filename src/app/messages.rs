//! Application message enum and result wrappers.

use std::path::PathBuf;

use iced::keyboard;

use crate::config::{AppTheme, DateFormat};
use crate::db::{
    BurstGroupRecord, DuplicateGroupRecord, FaceClusterRecord, TrashedPhotoRecord,
};
use crate::models::Photo;
use crate::services::{
    ApplyResult, DriveInfo, FaceProcessingResult, IndexChanges, ScanProgress, TrashStats,
};

use super::state::View;

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

    /// Keyboard event
    KeyPressed(keyboard::Key),

    /// No-op message (used as callback when we don't need the result)
    NoOp,

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
    FaceDataResetComplete(Result<usize, String>),

    CheckForChanges,
    ChangesDetected(IndexChanges),
    ApplyChanges,
    ChangesApplied(ApplyResult),

    RunGeocoding,
    GeocodingProgress { processed: usize, total: usize },
    GeocodingComplete,

    RegenerateRotatedData,
    RotatedDataRegenerated { cleared_thumbnails: usize, reset_faces: usize },

    RegenerateThumbnails,
    ThumbnailsRegenerated { cleared: usize },

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
}

/// Wrapper for scan result to make it Debug + Clone for Message
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub photo_count: i64,
    pub final_progress: ScanProgress,
}
