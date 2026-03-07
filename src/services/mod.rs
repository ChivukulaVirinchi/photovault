//! Application services

pub mod drive_detector;
pub mod exif_extractor;
pub mod face_processor;
pub mod scanner;
pub mod thumbnail;

pub use drive_detector::{DriveDetector, DriveInfo};
pub use exif_extractor::{ExifExtractor, ImageMetadata};
pub use face_processor::{
    FaceProcessingPhase, FaceProcessingProgress, FaceProcessingResult, FaceProcessor,
};
pub use scanner::ScanProgress;
pub use thumbnail::{ThumbnailResult, ThumbnailService, ThumbnailSize};
