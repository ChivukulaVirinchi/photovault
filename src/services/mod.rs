//! Application services

pub mod burst_detector;
pub mod document_detector;
pub mod drive_detector;
pub mod duplicate_detector;
pub mod exif_extractor;
pub mod face_processor;
pub mod geocoding;
pub mod image_utils;
pub mod memories;
pub mod ocr_processor;
pub mod reindexer;
pub mod scanner;
pub mod search;
pub mod thumbnail;
pub mod trash;

pub use burst_detector::{BurstConfig, BurstDetector};
pub use document_detector::DocumentDetector;
pub use drive_detector::{DriveDetector, DriveInfo};
pub use duplicate_detector::DuplicateDetector;
pub use face_processor::{FaceProcessingProgress, FaceProcessingResult, FaceProcessor};
pub use geocoding::GeocodingService;
pub use memories::MemoryCard;
pub use ocr_processor::{OcrProcessor, OcrProgress};
pub use reindexer::{ApplyResult, IndexChanges, Reindexer};
pub use scanner::ScanProgress;
pub use search::{SearchResultGroup, SearchService};
pub use thumbnail::{ThumbnailService, ThumbnailSize};
pub use trash::{TrashService, TrashStats};
