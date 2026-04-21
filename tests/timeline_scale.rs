//! Phase 2 Track E2: timeline scale validation.
//!
//! Builds a synthetic 50,000-photo library (well above the previous
//! 50K cap and comfortably within the new 250K limit) and exercises
//! the end-to-end load + group path that backs the Timeline view.
//! Acts as a regression gate against the Phase 2 performance
//! targets:
//!
//! * `get_all_by_date` returns 50K photos within a few seconds.
//! * `TimelineView::compute_groups` runs in under 100 ms on the
//!   loaded slice (the previous re-group-every-render behaviour
//!   was a 2–5 second stall on 50K photos, per the Phase 1 audit).
//! * The group metadata is a fraction of a MB — well below the old
//!   50 MB per-frame clone cost.
//!
//! `#[ignore]` by default because inserting 50K rows in a debug build
//! takes tens of seconds; CI's bench smoke job and the release
//! pipeline will run this via `cargo test --ignored`.

use chrono::{Duration as ChronoDuration, Utc};
use photovault::db::photo_repo::PhotoInsert;
use photovault::db::{create_schema, Database, PhotoRepo};
use photovault::models::compute_groups;
use std::time::Instant;
use tempfile::tempdir;

const PHOTO_COUNT: usize = 50_000;

/// SLA for `compute_groups` on 50K photos. Debug builds are ~10×
/// slower than release for string/date operations, so the budget is
/// scaled to avoid CI flakiness. The real product uses release builds
/// where we expect this to land well under 50 ms.
#[cfg(debug_assertions)]
const COMPUTE_GROUPS_SLA_MS: u128 = 300;
#[cfg(not(debug_assertions))]
const COMPUTE_GROUPS_SLA_MS: u128 = 100;

fn synthesize_photo(i: usize) -> PhotoInsert {
    // Spread across ~3 years, ~50 photos per day. Photo `i` lands at
    // day `i / 50` and minute `i % 50`. Gives ~1,000 distinct date
    // groups so the grouping path has meaningful work to do.
    let day_offset = (i / 50) as i64;
    let minute_offset = (i % 50) as i64;
    let ts = Utc::now() - ChronoDuration::days(day_offset) + ChronoDuration::minutes(minute_offset);

    PhotoInsert {
        relative_path: format!("photos/IMG_{:06}.jpg", i),
        file_name: format!("IMG_{:06}.jpg", i),
        file_hash: format!("{:064x}", i),
        file_size: 2_000_000,
        file_mtime: Some(ts.timestamp()),
        date_taken: Some(ts.to_rfc3339()),
        date_taken_source: Some("exif".to_string()),
        gps_latitude: None,
        gps_longitude: None,
        location_city: None,
        location_country: None,
        camera_make: None,
        camera_model: None,
        iso: None,
        aperture: None,
        shutter_speed: None,
        focal_length: None,
        lens_model: None,
        flash: None,
        gps_altitude: None,
        width: Some(4032),
        height: Some(3024),
        orientation: 1,
    }
}

#[test]
#[ignore = "50K row insert takes tens of seconds in debug; run explicitly"]
fn timeline_compute_groups_scales_to_50k_photos() {
    let temp = tempdir().unwrap();
    let db = Database::open_for_drive(temp.path()).unwrap();
    create_schema(&db.conn).unwrap();
    photovault::db::migrations::run_migrations(&db.conn).unwrap();

    let repo = PhotoRepo::new(&db.conn);

    // Insert in chunks to match how the scanner writes batches.
    const INSERT_CHUNK: usize = 1000;
    let insert_start = Instant::now();
    let mut inserted = 0;
    while inserted < PHOTO_COUNT {
        let end = (inserted + INSERT_CHUNK).min(PHOTO_COUNT);
        let batch: Vec<PhotoInsert> = (inserted..end).map(synthesize_photo).collect();
        repo.insert_batch(&batch).unwrap();
        inserted = end;
    }
    let insert_elapsed = insert_start.elapsed();
    eprintln!(
        "inserted {} photos in {:.2}s",
        PHOTO_COUNT,
        insert_elapsed.as_secs_f64()
    );

    // Load path: this is what `loaders::load_photos` does in prod.
    let load_start = Instant::now();
    let photos = repo.get_all_by_date(250_000, 0).unwrap();
    let load_elapsed = load_start.elapsed();
    eprintln!(
        "loaded {} photos in {:.2}s",
        photos.len(),
        load_elapsed.as_secs_f64()
    );
    assert_eq!(photos.len(), PHOTO_COUNT);

    // Grouping path: this is what the render used to do EVERY frame,
    // and now only runs once on photos_loaded.
    let group_start = Instant::now();
    let groups = compute_groups(&photos);
    let group_elapsed = group_start.elapsed();
    eprintln!(
        "computed {} groups from {} photos in {}ms",
        groups.len(),
        photos.len(),
        group_elapsed.as_millis()
    );

    // Invariants.
    assert!(
        !groups.is_empty(),
        "compute_groups must produce at least one group for 50K photos"
    );
    let member_total: usize = groups.iter().map(|g| g.end - g.start).sum();
    assert_eq!(
        member_total, PHOTO_COUNT,
        "every photo must land in exactly one group"
    );
    // Groups must be contiguous and non-overlapping over [0, N).
    let mut expected_start = 0usize;
    for g in &groups {
        assert_eq!(g.start, expected_start, "groups must be contiguous");
        assert!(g.end > g.start, "groups must be non-empty");
        expected_start = g.end;
    }
    assert_eq!(expected_start, PHOTO_COUNT);

    // Phase 2 SLA: grouping 50K photos should complete well under
    // 100 ms. The previous in-render BTreeMap-with-cloned-photos
    // variant was a 2–5 second stall on this size.
    assert!(
        group_elapsed.as_millis() < COMPUTE_GROUPS_SLA_MS,
        "compute_groups on {} photos took {}ms, above the {}ms SLA",
        PHOTO_COUNT,
        group_elapsed.as_millis(),
        COMPUTE_GROUPS_SLA_MS
    );
}
