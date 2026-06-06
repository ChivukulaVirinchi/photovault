//! Database module for Smriti
//!
//! Uses SQLite with the database stored on the indexed drive itself,
//! making the index fully portable.

/// Maximum rows per multi-row INSERT statement. SQLite's default
/// SQLITE_MAX_VARIABLE_NUMBER is 999 in older builds and 32766 in
/// newer ones; staying at 200 rows per chunk keeps every existing
/// repo (which never exceeds 5 columns per row) well under either
/// limit, while still giving a ~3× speedup vs. one INSERT per row.
pub const MAX_ROWS_PER_INSERT: usize = 200;

/// Per-drive metadata folder. Kept as `.photovault` for backwards
/// compatibility with libraries indexed before the Smriti rename —
/// renaming would orphan every user's existing thumbnails / db /
/// face crops on disk. Treat the on-disk folder name as part of the
/// wire contract; new code reads this constant rather than the
/// literal so the link to the legacy name stays explicit.
pub const LIBRARY_METADATA_DIR: &str = ".photovault";

pub mod album_repo;
pub mod album_suggestion_repo;
pub mod burst_repo;
pub mod connection;
pub mod document_repo;
pub mod duplicate_repo;
pub mod excluded_folder_repo;
pub mod face_repo;
pub mod geonames;
pub mod inferred_identity_repo;
pub mod migrations;
pub mod photo_repo;
pub mod recent_search_repo;
pub mod schema;
pub mod stack_repo;
pub mod trash_repo;

pub use album_repo::{AlbumRecord, AlbumRepo};
pub use album_suggestion_repo::{AlbumSuggestionRecord, AlbumSuggestionRepo};
pub use burst_repo::{BurstGroupMemberRecord, BurstGroupRecord, BurstRepo};
pub use connection::{db_path_for, open_secondary, tile_cache_dir, Database};
pub use document_repo::DocumentRepo;
pub use duplicate_repo::{DuplicateGroupMemberRecord, DuplicateGroupRecord, DuplicateRepo};
pub use excluded_folder_repo::{ExcludedFolderRecord, ExcludedFolderRepo};
pub use face_repo::{FaceClusterRecord, FaceDetail, FaceRepo, FaceStatus, ReviewItem};
pub use inferred_identity_repo::InferredIdentityRepo;
pub use photo_repo::PhotoRepo;
pub use recent_search_repo::{RecentSearch, RecentSearchRepo};
pub use schema::create_schema;
pub use stack_repo::{PhotoStackMemberRecord, PhotoStackRecord, PhotoStackRepo, StackCandidate};
pub use trash_repo::{TrashRepo, TrashedPhotoRecord};
