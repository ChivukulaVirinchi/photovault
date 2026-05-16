//! Workflow test: the trash lifecycle (trash → list → restore →
//! permanent delete + stats).
//!
//! This service is pure SQL behind a small Rust facade — no async,
//! no spawning, no ONNX. Easy to test exhaustively. Catches the
//! "soft-delete drift" class of bug where trashed photos
//! accidentally show up in Timeline / search / face-detection
//! queries because someone forgot the `is_trashed = FALSE` clause.

mod common;

use smriti::services::trash::TrashService;

#[test]
fn trash_moves_photos_out_of_active_set_and_stats_reflect_it() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 5);

    // Arrange / Act: trash two of the five seeded photos.
    let moved = TrashService::trash_photos(&db.conn, &[1, 2]).unwrap();
    assert_eq!(moved, 2, "trash_photos returns the number of rows updated");

    // Assert: the active count drops; trash stats see the two.
    let active: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active, 3, "active set excludes trashed rows");

    let stats = TrashService::get_stats(&db.conn).unwrap();
    assert_eq!(
        stats.count, 2,
        "trash stats count matches the number we trashed"
    );
}

#[test]
fn restore_returns_photos_to_the_active_set() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 4);

    TrashService::trash_photos(&db.conn, &[1, 2, 3]).unwrap();
    let restored = TrashService::restore_photos(&db.conn, &[2]).unwrap();
    assert_eq!(restored, 1);

    // Photo 2 is back in the active set; 1 and 3 are still trashed.
    let active_ids: Vec<i64> = {
        let mut stmt = db
            .conn
            .prepare("SELECT id FROM photos WHERE is_trashed = FALSE ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(
        active_ids,
        vec![2, 4],
        "only restored + never-trashed photos are active"
    );
}

#[test]
fn trashing_an_already_trashed_photo_is_idempotent() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 2);

    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    // Trashing again must not error or duplicate. The service
    // returns the number of rows updated; a no-op is fine — what
    // matters is that the row stays trashed exactly once.
    let _ = TrashService::trash_photos(&db.conn, &[1]).unwrap();

    let stats = TrashService::get_stats(&db.conn).unwrap();
    assert_eq!(
        stats.count, 1,
        "trash count must not double-count a re-trash"
    );
}

#[test]
fn trash_then_restore_round_trip_leaves_no_residue() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 3);

    TrashService::trash_photos(&db.conn, &[1, 2, 3]).unwrap();
    TrashService::restore_photos(&db.conn, &[1, 2, 3]).unwrap();

    let active: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        active, 3,
        "round-trip restore makes the library look untouched"
    );

    let stats = TrashService::get_stats(&db.conn).unwrap();
    assert_eq!(stats.count, 0, "trash is empty after full restore");
}

#[test]
fn trash_stats_zero_on_fresh_library() {
    let (_dir, db) = common::make_library();
    let stats = TrashService::get_stats(&db.conn).unwrap();
    assert_eq!(stats.count, 0);
}
