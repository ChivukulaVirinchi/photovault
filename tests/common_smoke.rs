//! Sanity tests for the shared test helpers in `tests/common/mod.rs`.
//!
//! If this file fails to compile or its tests fail, every workflow
//! test that imports `mod common;` will be broken — so it's worth
//! its own integration target.

mod common;

#[test]
fn make_jpeg_produces_decodable_bytes() {
    let bytes = common::make_jpeg(32, 24, [128, 64, 32]);
    let img = image::load_from_memory(&bytes).expect("decode generated jpeg");
    assert_eq!(img.width(), 32);
    assert_eq!(img.height(), 24);
}

#[test]
fn make_minimal_nef_wraps_a_decodable_preview() {
    let preview = common::make_jpeg(48, 36, [200, 50, 50]);
    let nef = common::make_minimal_nef(&preview);
    // The bytes between offset 38 and end should be the raw preview JPEG.
    let extracted = &nef[38..];
    assert_eq!(
        extracted.len(),
        preview.len(),
        "preview survives round-trip"
    );
    let img = image::load_from_memory(extracted).expect("decode extracted preview");
    assert_eq!(img.width(), 48);
    assert_eq!(img.height(), 36);
}

#[test]
fn make_library_is_writable_and_schema_is_applied() {
    let (_tempdir, db) = common::make_library();
    // The schema includes a `photos` table — querying it should
    // succeed with zero rows.
    let n: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
        .expect("photos table query");
    assert_eq!(n, 0, "fresh library starts empty");
}

#[test]
fn seed_photos_inserts_correct_count_with_half_gps() {
    let (_tempdir, db) = common::make_library();
    let n = common::seed_photos(&db, 10);
    assert_eq!(n, 10);
    let with_gps: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos WHERE gps_latitude IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(with_gps, 5, "seed_photos GPS-tags the first half");
}
