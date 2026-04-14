//! Face database repository
//!
//! Handles all database operations for faces and face clusters.

mod gallery;
mod read;
mod write;

pub use self::gallery::*;
pub use self::read::*;
pub use self::write::*;

use rusqlite::Connection;

use crate::ml::FaceEmbedding;

#[derive(Debug, Clone)]
pub struct GalleryEmbedding {
    pub cluster_id: i64,
    pub face_id: i64,
    pub embedding: FaceEmbedding,
}

/// Face cluster record from database
#[derive(Debug, Clone)]
pub struct FaceClusterRecord {
    pub id: i64,
    pub name: Option<String>,
    pub representative_face_id: Option<i64>,
    pub face_count: i64,
    pub photo_count: i64,
    /// Path to the representative face thumbnail (computed, not stored in DB)
    pub face_thumbnail_path: Option<String>,
}

/// Face database repository
pub struct FaceRepo<'a> {
    pub(crate) conn: &'a Connection,
}

impl<'a> FaceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}
