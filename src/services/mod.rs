//! Application services

pub mod burst_detector;
pub mod drive_detector;
pub mod duplicate_detector;
pub mod exif_extractor;
pub mod face_processor;
pub mod geocoding;
pub mod reindexer;
pub mod scanner;
pub mod search;
pub mod thumbnail;
pub mod trash;

pub use burst_detector::{BurstConfig, BurstDetector, BurstGroup};
pub use drive_detector::{DriveDetector, DriveInfo};
pub use duplicate_detector::{DuplicateDetector, DuplicateGroup};
pub use exif_extractor::{ExifExtractor, ImageMetadata};
pub use face_processor::{
    FaceProcessingPhase, FaceProcessingProgress, FaceProcessingResult, FaceProcessor,
};
pub use geocoding::{GeocodingResult, GeocodingService};
pub use reindexer::{ApplyResult, IndexChanges, Reindexer};
pub use scanner::ScanProgress;
pub use search::{SearchResult, SearchResultGroup, SearchService};
pub use thumbnail::{ThumbnailResult, ThumbnailService, ThumbnailSize};
pub use trash::{DeleteResult, TrashService, TrashStats};
