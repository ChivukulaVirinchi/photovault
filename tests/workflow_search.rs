//! Workflow test: free-text search against a seeded library.
//!
//! Exercises the parse → SearchService::search → result-rows path
//! end-to-end. The query parser's free-text heuristic is opinionated
//! about which words become locations vs people vs dates, so we
//! pick queries here whose parse path is unambiguous:
//!   - "paris" → Location filter (known-locations list)
//!   - "2024"  → DateRange filter (year detector)
//!
//! For Person filters we'd need a seeded person cluster, which is
//! out of scope at the service-layer test (the People DB is empty
//! in a fresh test library). Person-resolved search has its own
//! integration tests under face_clustering_integration.rs.

mod common;

use chrono::{TimeZone, Utc};

use smriti::db::photo_repo::{PhotoInsert, PhotoRepo};
use smriti::models::MediaType;
use smriti::search::QueryParser;
use smriti::services::search::SearchService;
use smriti::services::trash::TrashService;

/// Insert a photo with explicit overrides for the fields search
/// cares about. Returns the new row id.
fn insert_photo_with(
    db: &smriti::db::Database,
    file_name: &str,
    city: Option<&str>,
    country: Option<&str>,
    date: Option<chrono::DateTime<Utc>>,
) -> i64 {
    let repo = PhotoRepo::new(&db.conn);
    let insert = PhotoInsert {
        relative_path: format!("subdir/{}", file_name),
        file_name: file_name.to_string(),
        file_hash: format!("hash-{}", file_name),
        file_size: 32_000,
        file_mtime: date.map(|d| d.timestamp()),
        date_taken: date.map(|d| d.to_rfc3339()),
        date_taken_source: Some("exif".into()),
        gps_latitude: None,
        gps_longitude: None,
        location_city: city.map(String::from),
        location_country: country.map(String::from),
        camera_make: Some("ACME".into()),
        camera_model: None,
        iso: None,
        aperture: None,
        shutter_speed: None,
        focal_length: None,
        lens_model: None,
        flash: None,
        gps_altitude: None,
        width: Some(1024),
        height: Some(768),
        orientation: 1,
        media_type: MediaType::Photo,
        duration_ms: None,
        video_codec: None,
        audio_codec: None,
        frame_rate: None,
        bitrate: None,
        has_audio: false,
    };
    repo.insert_batch(&[insert]).unwrap();
    db.conn
        .query_row(
            "SELECT id FROM photos WHERE file_name = ?1",
            [file_name],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn search_by_known_city_returns_matching_photos() {
    let (_dir, db) = common::make_library();
    insert_photo_with(&db, "p1.jpg", Some("Paris"), Some("France"), None);
    insert_photo_with(&db, "p2.jpg", Some("Paris"), Some("France"), None);
    insert_photo_with(&db, "berlin.jpg", Some("Berlin"), Some("Germany"), None);

    // "paris" is in the parser's known-locations list, so the bare
    // word parses as a Location filter.
    let query = QueryParser::parse("paris");
    let results = SearchService::search(&db.conn, &query).unwrap();
    assert_eq!(results.len(), 2, "two Paris photos match");
}

#[test]
fn search_by_known_country_returns_matching_photos() {
    let (_dir, db) = common::make_library();
    insert_photo_with(&db, "paris.jpg", Some("Paris"), Some("France"), None);
    insert_photo_with(&db, "tokyo.jpg", Some("Tokyo"), Some("Japan"), None);
    insert_photo_with(&db, "delhi.jpg", Some("Delhi"), Some("India"), None);

    let query = QueryParser::parse("france");
    let results = SearchService::search(&db.conn, &query).unwrap();
    assert_eq!(results.len(), 1, "one photo in France matches");
}

#[test]
fn search_excludes_trashed_photos() {
    let (_dir, db) = common::make_library();
    let id_a = insert_photo_with(&db, "trashy.jpg", Some("Paris"), Some("France"), None);
    let _id_b = insert_photo_with(&db, "kept.jpg", Some("Paris"), Some("France"), None);

    TrashService::trash_photos(&db.conn, &[id_a]).unwrap();

    let query = QueryParser::parse("paris");
    let results = SearchService::search(&db.conn, &query).unwrap();
    assert_eq!(
        results.len(),
        1,
        "trashed photos must not appear in search results"
    );
    assert!(
        results.iter().all(|r| r.photo_id != id_a),
        "specifically not the trashed id"
    );
}

#[test]
fn empty_query_returns_no_results() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 5);
    let query = QueryParser::parse("");
    let results = SearchService::search(&db.conn, &query).unwrap();
    assert!(
        results.is_empty(),
        "empty query intentionally returns nothing (we don't dump the whole library)"
    );
}

#[test]
fn search_by_year_returns_only_matching_photos() {
    let (_dir, db) = common::make_library();
    insert_photo_with(
        &db,
        "old.jpg",
        None,
        None,
        Some(Utc.with_ymd_and_hms(2019, 5, 15, 12, 0, 0).unwrap()),
    );
    insert_photo_with(
        &db,
        "newer.jpg",
        None,
        None,
        Some(Utc.with_ymd_and_hms(2024, 5, 15, 12, 0, 0).unwrap()),
    );
    insert_photo_with(
        &db,
        "newest.jpg",
        None,
        None,
        Some(Utc.with_ymd_and_hms(2024, 5, 16, 12, 0, 0).unwrap()),
    );

    let query = QueryParser::parse("2024");
    let results = SearchService::search(&db.conn, &query).unwrap();
    assert_eq!(results.len(), 2, "only the two 2024 photos match");
}
