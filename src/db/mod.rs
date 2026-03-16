//! Database module for PhotoVault
//!
//! Uses SQLite with the database stored on the indexed drive itself,
//! making the index fully portable.

pub mod burst_repo;
pub mod connection;
pub mod duplicate_repo;
pub mod face_repo;
pub mod migrations;
pub mod photo_repo;
pub mod schema;

pub use burst_repo::{BurstGroupMemberRecord, BurstGroupRecord, BurstRepo};
pub use connection::Database;
pub use duplicate_repo::{DuplicateGroupMemberRecord, DuplicateGroupRecord, DuplicateRepo};
pub use face_repo::{FaceClusterRecord, FaceRecord, FaceRepo};
pub use photo_repo::PhotoRepo;
pub use schema::create_schema;
