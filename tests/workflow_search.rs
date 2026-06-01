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

fn insert_video_with(
    db: &smriti::db::Database,
    file_name: &str,
    city: Option<&str>,
    country: Option<&str>,
    date: Option<chrono::DateTime<Utc>>,
) -> i64 {
    let id = insert_photo_with(db, file_name, city, country, date);
    db.conn
        .execute(
            "UPDATE photos SET media_type = 'video', duration_ms = 120000 WHERE id = ?1",
            [id],
        )
        .unwrap();
    id
}

fn add_person(db: &smriti::db::Database, id: i64, name: &str) {
    db.conn
        .execute(
            "INSERT INTO face_clusters (id, name, face_count, photo_count, is_user_named)
             VALUES (?1, ?2, 0, 0, 1)",
            rusqlite::params![id, name],
        )
        .unwrap();
}

fn add_face(db: &smriti::db::Database, photo_id: i64, cluster_id: Option<i64>) {
    db.conn
        .execute(
            "INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_width, bbox_height, embedding, cluster_id, confidence)
             VALUES (?1, 0.0, 0.0, 0.2, 0.2, zeroblob(16), ?2, 0.9)",
            rusqlite::params![photo_id, cluster_id],
        )
        .unwrap();
    db.conn
        .execute(
            "UPDATE photos SET faces_processed = TRUE WHERE id = ?1",
            [photo_id],
        )
        .unwrap();
}

fn ids(results: &smriti::services::search::UnifiedSearchResults) -> Vec<i64> {
    let mut ids = results.photo_ids.clone();
    ids.sort_unstable();
    ids
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

#[test]
fn smart_search_place_and_year_are_intersected() {
    let (_dir, db) = common::make_library();
    let goa_2023 = insert_photo_with(
        &db,
        "goa-2023.jpg",
        Some("Goa"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2023, 1, 10, 12, 0, 0).unwrap()),
    );
    insert_photo_with(
        &db,
        "goa-2022.jpg",
        Some("Goa"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2022, 1, 10, 12, 0, 0).unwrap()),
    );
    insert_photo_with(
        &db,
        "paris-2023.jpg",
        Some("Paris"),
        Some("France"),
        Some(Utc.with_ymd_and_hms(2023, 1, 10, 12, 0, 0).unwrap()),
    );

    let results = SearchService::search_unified(&db.conn, "Goa 2023").unwrap();
    assert_eq!(ids(&results), vec![goa_2023]);
    assert!(results
        .interpreted
        .iter()
        .any(|f| f.kind == "place" && f.label.contains("Goa")));
    assert!(results.interpreted.iter().any(|f| f.kind == "date"));
}

#[test]
fn smart_search_year_then_place_are_intersected() {
    let (_dir, db) = common::make_library();
    let vizianagaram_2025 = insert_photo_with(
        &db,
        "vizianagaram-2025.jpg",
        Some("Vizianagaram"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2025, 2, 10, 12, 0, 0).unwrap()),
    );
    insert_photo_with(
        &db,
        "vizianagaram-2024.jpg",
        Some("Vizianagaram"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2024, 2, 10, 12, 0, 0).unwrap()),
    );
    insert_photo_with(
        &db,
        "goa-2025.jpg",
        Some("Goa"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2025, 2, 10, 12, 0, 0).unwrap()),
    );

    let results = SearchService::search_unified(&db.conn, "2025 Vizianagaram").unwrap();
    assert_eq!(ids(&results), vec![vizianagaram_2025]);
    assert!(results
        .interpreted
        .iter()
        .any(|f| f.kind == "place" && f.label.contains("Vizianagaram")));
    assert!(results.interpreted.iter().any(|f| f.kind == "date"));
}

#[test]
fn smart_search_date_place_media_favourite_order_is_flexible() {
    let (_dir, db) = common::make_library();
    let target = insert_photo_with(
        &db,
        "vizianagaram-video-2025.mp4",
        Some("Vizianagaram"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2025, 2, 10, 12, 0, 0).unwrap()),
    );
    db.conn
        .execute(
            "UPDATE photos SET media_type = 'video', is_favorite = TRUE WHERE id = ?1",
            rusqlite::params![target],
        )
        .unwrap();
    let wrong_city = insert_photo_with(
        &db,
        "goa-video-2025.mp4",
        Some("Goa"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2025, 2, 10, 12, 0, 0).unwrap()),
    );
    db.conn
        .execute(
            "UPDATE photos SET media_type = 'video', is_favorite = TRUE WHERE id = ?1",
            rusqlite::params![wrong_city],
        )
        .unwrap();
    insert_photo_with(
        &db,
        "vizianagaram-photo-2025.jpg",
        Some("Vizianagaram"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2025, 2, 10, 12, 0, 0).unwrap()),
    );
    let wrong_year = insert_photo_with(
        &db,
        "vizianagaram-video-2024.mp4",
        Some("Vizianagaram"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2024, 2, 10, 12, 0, 0).unwrap()),
    );
    db.conn
        .execute(
            "UPDATE photos SET media_type = 'video', is_favorite = TRUE WHERE id = ?1",
            rusqlite::params![wrong_year],
        )
        .unwrap();

    for query in [
        "favourites videos 2025 Vizianagaram",
        "2025 Vizianagaram favourite video",
        "video 2025 Vizianagaram favourite",
        "Vizianagaram favourite 2025 videos",
    ] {
        let results = SearchService::search_unified(&db.conn, query).unwrap();
        assert_eq!(ids(&results), vec![target], "{query}");
        assert!(
            results.interpreted.iter().any(|f| f.kind == "date"),
            "{query}"
        );
        assert!(
            results.interpreted.iter().any(|f| f.kind == "place"),
            "{query}"
        );
        assert!(
            results.interpreted.iter().any(|f| f.kind == "media"),
            "{query}"
        );
        assert!(
            results.interpreted.iter().any(|f| f.kind == "favorite"),
            "{query}"
        );
    }
}

#[test]
fn smart_search_person_and_date_are_intersected() {
    let (_dir, db) = common::make_library();
    add_person(&db, 10, "Dad");
    let dad_2024 = insert_photo_with(
        &db,
        "dad-2024.jpg",
        None,
        None,
        Some(Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap()),
    );
    let dad_2023 = insert_photo_with(
        &db,
        "dad-2023.jpg",
        None,
        None,
        Some(Utc.with_ymd_and_hms(2023, 5, 1, 12, 0, 0).unwrap()),
    );
    add_face(&db, dad_2024, Some(10));
    add_face(&db, dad_2023, Some(10));

    let results = SearchService::search_unified(&db.conn, "Dad 2024").unwrap();
    assert_eq!(ids(&results), vec![dad_2024]);
}

#[test]
fn smart_search_people_join_requires_all_people() {
    let (_dir, db) = common::make_library();
    add_person(&db, 10, "Dad");
    add_person(&db, 20, "Mom");
    let both = insert_photo_with(&db, "both.jpg", None, None, None);
    let dad_only = insert_photo_with(&db, "dad.jpg", None, None, None);
    add_face(&db, both, Some(10));
    add_face(&db, both, Some(20));
    add_face(&db, dad_only, Some(10));

    let results = SearchService::search_unified(&db.conn, "Dad and Mom").unwrap();
    assert_eq!(ids(&results), vec![both]);
}

#[test]
fn smart_search_only_person_excludes_other_and_unknown_faces() {
    let (_dir, db) = common::make_library();
    add_person(&db, 10, "Dad");
    add_person(&db, 20, "Mom");
    let clean = insert_photo_with(&db, "dad-clean.jpg", None, None, None);
    let with_mom = insert_photo_with(&db, "dad-mom.jpg", None, None, None);
    let with_unknown = insert_photo_with(&db, "dad-unknown.jpg", None, None, None);
    let unprocessed = insert_photo_with(&db, "dad-unprocessed.jpg", None, None, None);
    add_face(&db, clean, Some(10));
    add_face(&db, with_mom, Some(10));
    add_face(&db, with_mom, Some(20));
    add_face(&db, with_unknown, Some(10));
    add_face(&db, with_unknown, None);
    db.conn
        .execute(
            "INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_width, bbox_height, embedding, cluster_id, confidence)
             VALUES (?1, 0.0, 0.0, 0.2, 0.2, zeroblob(16), 10, 0.9)",
            [unprocessed],
        )
        .unwrap();

    let results = SearchService::search_unified(&db.conn, "only Dad").unwrap();
    assert_eq!(ids(&results), vec![clean]);

    let results = SearchService::search_unified(&db.conn, "only person Dad").unwrap();
    assert_eq!(ids(&results), vec![clean]);
}

#[test]
fn smart_search_favourites_videos_and_album_filters() {
    let (_dir, db) = common::make_library();
    let fav_video = insert_video_with(
        &db,
        "goa-fav-video.mp4",
        Some("Goa"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap()),
    );
    let fav_photo = insert_photo_with(
        &db,
        "goa-fav-photo.jpg",
        Some("Goa"),
        Some("India"),
        Some(Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap()),
    );
    db.conn
        .execute(
            "UPDATE photos SET is_favorite = TRUE WHERE id IN (?1, ?2)",
            rusqlite::params![fav_video, fav_photo],
        )
        .unwrap();
    let album_repo = smriti::db::album_repo::AlbumRepo::new(&db.conn);
    let album_id = album_repo.create("Goa Trip").unwrap();
    album_repo.add_photos(album_id, &[fav_video]).unwrap();

    let video_results =
        SearchService::search_unified(&db.conn, "favourites videos Goa 2024").unwrap();
    assert_eq!(ids(&video_results), vec![fav_video]);

    let album_results = SearchService::search_unified(&db.conn, "album Goa Trip").unwrap();
    assert_eq!(ids(&album_results), vec![fav_video]);
}
