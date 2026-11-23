//! Face database repository
//!
//! Handles all database operations for faces and face clusters.

mod gallery;
mod read;
mod undo;
mod write;

use rusqlite::Connection;

pub use undo::{FaceSnapshot, FaceState};

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
    pub photo_count: i64,
    /// Path to the representative face thumbnail (computed, not stored in DB)
    pub face_thumbnail_path: Option<String>,
}

/// Face confirmation status filter for queries.
#[derive(Debug, Clone)]
pub enum FaceStatus {
    Confirmed,
    Unconfirmed,
    All,
}

/// A face row with related metadata, used by face-list views.
#[derive(Debug, Clone)]
pub struct FaceDetail {
    pub face_id: i64,
    pub photo_id: i64,
    pub cluster_id: Option<i64>,
    pub confidence: f32,
    pub user_confirmed: i32,
}

/// Face database repository
/// What a merge changed, so it can be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeReceipt {
    pub source_id: i64,
    pub target_id: i64,
    pub source_name: Option<String>,
    pub source_user_named: bool,
    /// Faces that moved from the source to the target.
    pub face_ids: Vec<i64>,
    /// The target was unnamed and took the source's name.
    pub target_took_name: bool,
}

pub struct FaceRepo<'a> {
    pub(crate) conn: &'a Connection,
}

impl<'a> FaceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}
