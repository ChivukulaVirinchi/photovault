//! Database module for PhotoVault
//!
//! Uses SQLite with the database stored on the indexed drive itself,
//! making the index fully portable.

pub mod album_repo;
pub mod burst_repo;
pub mod connection;
pub mod document_repo;
pub mod duplicate_repo;
pub mod face_repo;
pub mod geonames;
pub mod inferred_identity_repo;
pub mod migrations;
pub mod photo_repo;
pub mod schema;
pub mod trash_repo;

pub use album_repo::{AlbumRecord, AlbumRepo};
pub use burst_repo::{BurstGroupMemberRecord, BurstGroupRecord, BurstRepo};
pub use connection::{tile_cache_dir, Database};
pub use document_repo::DocumentRepo;
pub use duplicate_repo::{DuplicateGroupMemberRecord, DuplicateGroupRecord, DuplicateRepo};
pub use face_repo::{FaceClusterRecord, FaceRepo, ReviewItem};
pub use inferred_identity_repo::InferredIdentityRepo;
pub use photo_repo::PhotoRepo;
pub use schema::create_schema;
pub use trash_repo::{TrashRepo, TrashedPhotoRecord};
