//! Workflow test: what the user tells Smriti about people sticks.
//!
//! "Not a face", removing a person and undoing a merge must survive the
//! next clustering run instead of being quietly redone by it, and faces
//! skipped in review stop coming back.

mod common;

use rusqlite::params;

use smriti::db::face_repo::FaceRepo;

fn insert_face(db: &smriti::db::Database, photo_id: i64, cluster_id: Option<i64>) -> i64 {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO photos (id, file_path, file_name, file_hash, file_size)
             VALUES (?1, ?2, ?2, ?3, 1)",
            params![photo_id, format!("p{photo_id}.jpg"), format!("h{photo_id}")],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_width, bbox_height,
                                embedding, cluster_id, confidence)
             VALUES (?1, 0.1, 0.1, 0.3, 0.3, zeroblob(2048), ?2, 0.9)",
            params![photo_id, cluster_id],
        )
        .unwrap();
    db.conn.last_insert_rowid()
}

fn cluster(db: &smriti::db::Database, id: i64, name: Option<&str>) {
    db.conn
        .execute(
            "INSERT INTO face_clusters (id, name, is_user_named) VALUES (?1, ?2, ?3)",
            params![id, name, name.is_some()],
        )
        .unwrap();
}

fn cluster_of(db: &smriti::db::Database, face_id: i64) -> Option<i64> {
    db.conn
        .query_row(
            "SELECT cluster_id FROM faces WHERE id = ?1",
            [face_id],
            |r| r.get(0),
        )
        .unwrap()
}

fn clusterable(db: &smriti::db::Database) -> Vec<i64> {
    FaceRepo::new(&db.conn)
        .get_unclustered_embeddings_after(0)
        .unwrap()
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

#[test]
fn not_a_face_and_removed_people_are_never_regrouped() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, Some("Asha"));
    cluster(&db, 2, None);
    let asha = insert_face(&db, 1, Some(1));
    let poster = insert_face(&db, 2, Some(1));
    let stranger = insert_face(&db, 3, Some(2));
    let repo = FaceRepo::new(&db.conn);

    repo.ignore_face(poster).unwrap();
    assert_eq!(cluster_of(&db, poster), None);
    repo.delete_cluster(2).unwrap();
    assert_eq!(cluster_of(&db, stranger), None);

    // Neither comes back as clustering input nor on the photo.
    assert!(clusterable(&db).is_empty());
    assert!(repo.get_face_boxes_for_photos(&[2, 3]).unwrap().is_empty());
    assert_eq!(cluster_of(&db, asha), Some(1));
}

#[test]
fn an_undone_merge_restores_both_people_and_blocks_the_merge() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, None);
    cluster(&db, 2, Some("Ravi"));
    let target_face = insert_face(&db, 1, Some(1));
    let moved = insert_face(&db, 2, Some(2));
    let repo = FaceRepo::new(&db.conn);

    let receipt = repo.merge_clusters(2, 1).unwrap().expect("merged");
    assert_eq!(cluster_of(&db, moved), Some(1));
    assert!(receipt.target_took_name);

    repo.unmerge_clusters(&receipt).unwrap();
    assert_eq!(cluster_of(&db, moved), Some(2));
    assert_eq!(cluster_of(&db, target_face), Some(1));
    let names: Vec<Option<String>> = db
        .conn
        .prepare("SELECT name FROM face_clusters ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(names, vec![None, Some("Ravi".to_string())]);

    // They were said to be different: merging again is refused.
    assert!(repo.merge_clusters(2, 1).unwrap().is_none());
}

#[test]
fn skipped_review_faces_stop_coming_back() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, Some("Asha"));
    let a = insert_face(&db, 1, Some(1));
    let b = insert_face(&db, 2, Some(1));
    let repo = FaceRepo::new(&db.conn);
    assert_eq!(repo.count_unconfirmed_global().unwrap(), (2, 1));

    repo.skip_review_face(a).unwrap();
    assert_eq!(repo.count_unconfirmed_global().unwrap(), (1, 1));
    let batch = repo.next_unconfirmed_face_batch_excluding(10, &[]).unwrap();
    assert_eq!(batch.iter().map(|f| f.face_id).collect::<Vec<_>>(), vec![b]);

    repo.skip_review_face(b).unwrap();
    assert_eq!(repo.count_unconfirmed_global().unwrap(), (0, 0));
    assert!(repo
        .next_unconfirmed_face_batch_excluding(10, &[])
        .unwrap()
        .is_empty());
}

fn gallery(db: &smriti::db::Database, face_id: i64) -> Vec<(i64, String)> {
    db.conn
        .prepare(
            "SELECT cluster_id, COALESCE(source, '') FROM person_gallery_embeddings
             WHERE face_id = ?1 ORDER BY cluster_id",
        )
        .unwrap()
        .query_map([face_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn negatives(db: &smriti::db::Database, face_id: i64) -> Vec<i64> {
    db.conn
        .prepare("SELECT not_cluster_id FROM face_negatives WHERE face_id = ?1 ORDER BY 1")
        .unwrap()
        .query_map([face_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn undoing_a_yes_takes_the_face_back_out_of_the_gallery() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, Some("Asha"));
    let face = insert_face(&db, 1, Some(1));
    let repo = FaceRepo::new(&db.conn);
    let before = repo.snapshot_face(face).unwrap();

    repo.confirm_face(face).unwrap();
    assert!(gallery(&db, face).contains(&(1, "user_confirmed".into())));

    repo.restore_face(&before).unwrap();
    assert_eq!(repo.face_state(face).unwrap(), Some((Some(1), 0)));
    assert!(!gallery(&db, face).contains(&(1, "user_confirmed".into())));
}

#[test]
fn undoing_a_no_keeps_older_decisions() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, Some("Asha"));
    cluster(&db, 2, Some("Ravi"));
    let face = insert_face(&db, 1, Some(1));
    let repo = FaceRepo::new(&db.conn);
    // An earlier, unrelated "not Ravi".
    db.conn
        .execute(
            "INSERT INTO face_negatives (face_id, not_cluster_id) VALUES (?1, 2)",
            [face],
        )
        .unwrap();
    let before = repo.snapshot_face(face).unwrap();

    repo.reject_face_to_unknown(face, 1).unwrap();
    assert_eq!(negatives(&db, face), vec![1, 2]);
    assert_eq!(cluster_of(&db, face), None);

    repo.restore_face(&before).unwrap();
    assert_eq!(cluster_of(&db, face), Some(1));
    assert_eq!(negatives(&db, face), vec![2]);
}

#[test]
fn moving_a_face_to_someone_else_updates_both_and_undoes_cleanly() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, Some("Asha"));
    cluster(&db, 2, Some("Ravi"));
    let moved = insert_face(&db, 1, Some(1));
    insert_face(&db, 2, Some(1));
    let repo = FaceRepo::new(&db.conn);
    repo.confirm_face(moved).unwrap();
    let face_count = |id: i64| -> i64 {
        db.conn
            .query_row(
                "SELECT face_count FROM face_clusters WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    };
    let before = repo.snapshot_face(moved).unwrap();

    repo.confirm_face_to_cluster(moved, 2).unwrap();
    assert_eq!((face_count(1), face_count(2)), (1, 1));
    assert_eq!(gallery(&db, moved), vec![(2, "user_confirmed".into())]);

    repo.restore_face(&before).unwrap();
    assert_eq!(cluster_of(&db, moved), Some(1));
    assert_eq!((face_count(1), face_count(2)), (2, 0));
    assert_eq!(gallery(&db, moved), vec![(1, "user_confirmed".into())]);
}

#[test]
fn not_a_face_and_not_sure_can_be_undone() {
    let (_dir, db) = common::make_library();
    cluster(&db, 1, Some("Asha"));
    let face = insert_face(&db, 1, Some(1));
    let other = insert_face(&db, 2, Some(1));
    let repo = FaceRepo::new(&db.conn);

    let before = repo.snapshot_face(face).unwrap();
    repo.ignore_face(face).unwrap();
    repo.restore_face(&before).unwrap();
    assert_eq!(repo.face_state(face).unwrap(), Some((Some(1), 0)));

    // Undoing one skip leaves the other skips alone.
    let before = repo.snapshot_face(face).unwrap();
    repo.skip_review_face(other).unwrap();
    repo.skip_review_face(face).unwrap();
    assert_eq!(repo.count_unconfirmed_global().unwrap().0, 0);
    repo.restore_face(&before).unwrap();
    assert_eq!(repo.count_unconfirmed_global().unwrap().0, 1);
}
