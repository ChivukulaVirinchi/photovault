//! Application services

pub mod album_suggestions;
pub mod assistant;
pub mod burst_detector;
pub mod camera_names;
pub mod document_detector;
pub mod drive_detector;
pub mod duplicate_detector;
pub mod exclusions;
pub mod exif_extractor;
pub mod face_processor;
pub mod geocoding;
pub mod image_io;
pub mod image_utils;
pub mod insights;
pub mod install_method;
pub mod library_health;
pub mod map_math;
pub mod memories;
pub mod metadata_processor;
pub mod ocr_processor;
pub mod path_util;
pub mod photo_stacks;
#[cfg(feature = "raw")]
pub mod raw_preview;
pub mod reindexer;
pub mod scanner;
pub mod search;
pub mod self_replace;
pub mod semantic;
pub mod takeout_import;
pub mod thumbnail;
pub mod tile_cache;
pub mod trash;
pub mod update_checker;

pub use burst_detector::{BurstConfig, BurstDetector};
pub use document_detector::DocumentDetector;
pub use drive_detector::{DriveDetector, DriveInfo};
pub use duplicate_detector::DuplicateDetector;
pub use face_processor::{
    EmbedderRoute, FaceProcessingProgress, FaceProcessingResult, FaceProcessingStage, FaceProcessor,
};
pub use geocoding::GeocodingService;
pub use library_health::LibraryHealth;
pub use memories::MemoryCard;
pub use metadata_processor::MetadataProgress;
pub use ocr_processor::{OcrProcessor, OcrProgress};
pub use photo_stacks::{PhotoStackService, StackRefreshResult};
pub use reindexer::{ApplyResult, IndexChanges, Reindexer};
pub use scanner::ScanProgress;
pub use search::{
    AlbumHit, PersonHit, PlaceHit, SearchResult, SearchService, UnifiedSearchResults,
};
pub use semantic::{
    SemanticIndexCache, SemanticIndexStats, SemanticModelRunner, SemanticSearchService,
    SemanticStatus,
};
pub use thumbnail::{ThumbnailService, ThumbnailSize};
pub use tile_cache::TileCache;
pub use trash::{TrashService, TrashStats};
