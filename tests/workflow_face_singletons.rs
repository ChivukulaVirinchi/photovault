//! Workflow test: every face that detection finds ends up in the
//! People view — even when no two faces match (small-library /
//! all-unique-people case).
//!
//! Regression for the "32 faces, 0 clusters" bug. Before the fix, the
//! clustering pipeline only created `face_clusters` rows for groups of
//! 2+ photos. Singletons (one face per "person") stayed with
//! `cluster_id IS NULL` forever — invisible in the People view, even
//! though detection had successfully found them.
//!
//! The fix added a final pass `promote_orphans_to_singletons` to
//! `FaceProcessor::run_clustering` that creates a singleton cluster
//! for every unclustered face after Stage A, Stage B, the merge pass,
//! and the rescue pass have all run. This test pins that contract.

mod common;

use rusqlite::params;

use smriti::ml::ResolverWeights;
use smriti::services::face_processor::FaceProcessor;

const EMBEDDING_DIM: usize = 512;

/// Serialise an f32 vector to little-endian bytes — the storage
/// format `FaceEmbedding::from_bytes` expects.
fn embedding_bytes(vec: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vec.len() * 4);
    for f in vec {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Build a unit-length 512-d embedding pointing predominantly along
/// `axis`. Used so every synthetic identity is mutually orthogonal:
/// the clusterer should not merge any of them.
fn unit_axis_embedding(axis: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBEDDING_DIM];
    v[axis % EMBEDDING_DIM] = 1.0;
    v
}

/// Insert a photo + a single face referencing that photo. Returns the
/// inserted face_id.
fn insert_face(db: &smriti::db::Database, photo_id: i64, axis: usize) -> i64 {
    db.conn
        .execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                photo_id,
                format!("photos/p{:04}.jpg", photo_id),
                format!("p{:04}.jpg", photo_id),
                format!("hash-{:04}", photo_id),
                256_000i64,
            ],
        )
        .expect("insert photo");

    let emb = embedding_bytes(&unit_axis_embedding(axis));
    db.conn
        .execute(
            "INSERT INTO faces ( \
                photo_id, bbox_x, bbox_y, bbox_width, bbox_height, \
                embedding, cluster_id, confidence, user_confirmed \
             ) VALUES (?1, 0.10, 0.10, 0.20, 0.20, ?2, NULL, 0.95, 0)",
            params![photo_id, emb],
        )
        .expect("insert face");
    db.conn.last_insert_rowid()
}

fn count_clusters(db: &smriti::db::Database) -> i64 {
    db.conn
        .query_row("SELECT COUNT(*) FROM face_clusters", [], |r| r.get(0))
        .expect("count clusters")
}

fn count_orphan_faces(db: &smriti::db::Database) -> i64 {
    db.conn
        .query_row(
            "SELECT COUNT(*) FROM faces WHERE cluster_id IS NULL AND user_confirmed >= 0",
            [],
            |r| r.get(0),
        )
        .expect("count orphans")
}

#[test]
fn every_unique_face_becomes_at_least_a_singleton_cluster() {
    // Arrange: insert 8 mutually-orthogonal faces. With strict thresholds
    // the clusterer will refuse to merge any of them (they're as far
    // apart as 512-d unit vectors can get).
    let (_dir, db) = common::make_library();
    let repo = smriti::db::face_repo::FaceRepo::new(&db.conn);

    let face_count = 8usize;
    for i in 0..face_count {
        insert_face(&db, (i + 1) as i64, i);
    }
    assert_eq!(
        count_orphan_faces(&db),
        face_count as i64,
        "premise: every face should start unclustered"
    );

    // Act: run the public clustering entry point.
    let weights = ResolverWeights::default();
    let result = FaceProcessor::run_clustering(&repo, 0.35, weights);
    assert!(result.is_ok(), "run_clustering failed: {:?}", result);

    // Assert: each face has been promoted to its own singleton cluster.
    // Specifically — no face should still be orphaned, and the number
    // of clusters should equal the number of faces.
    assert_eq!(
        count_orphan_faces(&db),
        0,
        "every face must end up in some cluster after run_clustering"
    );
    assert_eq!(
        count_clusters(&db),
        face_count as i64,
        "expected one singleton cluster per unique face; got {} clusters",
        count_clusters(&db)
    );
}

#[test]
fn clustering_an_empty_library_does_nothing_and_does_not_error() {
    // Smoke test: run_clustering on a fresh library should be a no-op,
    // not a panic. Catches edge-case crashes from empty SELECTs.
    let (_dir, db) = common::make_library();
    let repo = smriti::db::face_repo::FaceRepo::new(&db.conn);

    let weights = ResolverWeights::default();
    let result = FaceProcessor::run_clustering(&repo, 0.35, weights);
    assert!(result.is_ok(), "empty-library clustering should succeed");
    assert_eq!(count_clusters(&db), 0, "no faces, no clusters");
}

#[test]
fn user_hidden_faces_do_not_get_resurrected_as_clusters() {
    // Faces marked user_confirmed = -1 (the "hidden" state) must stay
    // hidden even after promote_orphans_to_singletons runs. Otherwise
    // hiding a face would re-surface it as a singleton cluster on the
    // next clustering pass — a frustrating UX regression to land.
    let (_dir, db) = common::make_library();
    let repo = smriti::db::face_repo::FaceRepo::new(&db.conn);

    insert_face(&db, 1, 0);
    insert_face(&db, 2, 1);
    insert_face(&db, 3, 2);
    // Hide face #2.
    db.conn
        .execute(
            "UPDATE faces SET user_confirmed = -1 WHERE photo_id = 2",
            [],
        )
        .unwrap();

    let weights = ResolverWeights::default();
    FaceProcessor::run_clustering(&repo, 0.35, weights).expect("run_clustering");

    let visible_clusters = count_clusters(&db);
    assert_eq!(
        visible_clusters, 2,
        "expected 2 clusters (one per visible face); hidden face must \
         not be promoted into a cluster of its own"
    );
}
