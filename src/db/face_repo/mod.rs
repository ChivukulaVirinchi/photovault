//! Face database repository
//!
//! Handles all database operations for faces and face clusters.

mod gallery;
mod read;
mod write;

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
    pub photo_count: i64,
    /// Path to the representative face thumbnail (computed, not stored in DB)
    pub face_thumbnail_path: Option<String>,
}

/// A row from face_review_queue ready to be presented to the user.
///
/// Sample face ids are a small set of representative faces from the candidate
/// cluster, used by the UI to show "here's the person we think this matches."
#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub queue_id: i64,
    pub face_id: i64,
    pub candidate_cluster_id: i64,
    pub candidate_cluster_name: Option<String>,
    pub candidate_cluster_size: i64,
    pub candidate_sample_face_ids: Vec<i64>,
    pub score: f32,
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
