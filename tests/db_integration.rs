//! Integration tests for database operations.
//!
//! Tests the full DB lifecycle: schema creation, photo insertion,
//! querying, geocoding updates, trash flow, and data integrity.

use smriti::db::migrations::MAX_KNOWN_SCHEMA_VERSION;
use smriti::db::photo_repo::PhotoInsert;
use smriti::db::{create_schema, BurstRepo, Database, DuplicateRepo, PhotoRepo, TrashRepo};
use smriti::models::MediaType;
use smriti::services::TrashService;
use tempfile::tempdir;

fn setup_db() -> (tempfile::TempDir, Database) {
    let temp = tempdir().unwrap();
    let db = Database::open_for_drive(temp.path()).unwrap();
    create_schema(&db.conn).unwrap();
    (temp, db)
}

fn sample_photo(path: &str, hash: &str) -> PhotoInsert {
    PhotoInsert {
        relative_path: path.to_string(),
        file_name: path.split('/').next_back().unwrap_or(path).to_string(),
        file_hash: hash.to_string(),
        file_size: 1_000_000,
        file_mtime: Some(1700000000),
        date_taken: Some("2024-01-15T14:30:00+00:00".to_string()),
        date_taken_source: Some("exif".to_string()),
        gps_latitude: Some(48.8566),
        gps_longitude: Some(2.3522),
        location_city: Some("Paris".to_string()),
        location_country: Some("France".to_string()),
        camera_make: Some("Canon".to_string()),
        camera_model: Some("EOS R5".to_string()),
        iso: Some(100),
        aperture: Some("f/2.8".to_string()),
        shutter_speed: Some("1/250".to_string()),
        focal_length: Some("50mm".to_string()),
        lens_model: Some("RF 50mm f/1.2L".to_string()),
        flash: Some("Off".to_string()),
        gps_altitude: Some(35.0),
        width: Some(8192),
        height: Some(5464),
        orientation: 1,
        media_type: MediaType::Photo,
        duration_ms: None,
        video_codec: None,
        audio_codec: None,
        frame_rate: None,
        bitrate: None,
        has_audio: false,
    }
}

#[test]
fn test_schema_creation() {
    let (_temp, db) = setup_db();

    // Verify key tables exist
    for table in &[
        "photos",
        "faces",
        "face_clusters",
        "duplicate_groups",
        "burst_groups",
        "trash",
    ] {
        let count: i32 = db
            .conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                    table
                ),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "Table '{}' should exist", table);
    }
}

/// Phase 2 Track A4/B2: composite indexes added in migration v15.
#[test]
fn test_v15_composite_indexes_present_and_used() {
    let (_temp, db) = setup_db();
    smriti::db::migrations::run_migrations(&db.conn).unwrap();

    assert_v15_indexes_present(&db.conn);

    let schema_version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(schema_version, MAX_KNOWN_SCHEMA_VERSION);

    // Verify SQLite's planner picks idx_photos_trashed_date for the
    // timeline's paginated query.
    let plan_rows: Vec<String> = db
        .conn
        .prepare("EXPLAIN QUERY PLAN SELECT id FROM photos WHERE is_trashed = 0 ORDER BY date_taken DESC LIMIT 100 OFFSET 0")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    let joined = plan_rows.join(" | ");
    assert!(
        joined.contains("idx_photos_trashed_date"),
        "timeline pagination should use idx_photos_trashed_date, got: {}",
        joined
    );

    // Idempotent re-run should not error or double-insert the current version.
    smriti::db::migrations::run_migrations(&db.conn).unwrap();
    let version_count: i32 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM schema_version WHERE version = ?1",
            [MAX_KNOWN_SCHEMA_VERSION],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        version_count, 1,
        "migration should record the current version exactly once even on re-run"
    );
}

#[test]
fn test_v14_to_latest_migration_creates_v15_composite_indexes() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE schema_version (
            version INTEGER PRIMARY KEY,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        INSERT INTO schema_version (version) VALUES (14);

        CREATE TABLE photos (
            id INTEGER PRIMARY KEY,
            file_path TEXT NOT NULL UNIQUE,
            file_name TEXT NOT NULL,
            file_hash TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            file_mtime INTEGER,
            date_taken DATETIME,
            date_taken_source TEXT,
            gps_latitude REAL,
            gps_longitude REAL,
            location_city TEXT,
            location_country TEXT,
            camera_make TEXT,
            camera_model TEXT,
            iso INTEGER,
            aperture TEXT,
            shutter_speed TEXT,
            focal_length TEXT,
            lens_model TEXT,
            flash TEXT,
            gps_altitude REAL,
            width INTEGER,
            height INTEGER,
            orientation INTEGER DEFAULT 1,
            thumbnail_path TEXT,
            faces_processed BOOLEAN DEFAULT FALSE,
            content_category TEXT DEFAULT 'photo',
            ocr_text TEXT,
            ocr_processed BOOLEAN DEFAULT FALSE,
            ocr_confidence REAL,
            is_trashed BOOLEAN DEFAULT FALSE,
            trashed_at DATETIME,
            indexed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE face_clusters (
            id INTEGER PRIMARY KEY,
            name TEXT,
            representative_face_id INTEGER,
            face_count INTEGER DEFAULT 0,
            photo_count INTEGER DEFAULT 0,
            is_user_named INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE faces (
            id INTEGER PRIMARY KEY,
            photo_id INTEGER NOT NULL,
            bbox_x REAL NOT NULL,
            bbox_y REAL NOT NULL,
            bbox_width REAL NOT NULL,
            bbox_height REAL NOT NULL,
            embedding BLOB NOT NULL,
            cluster_id INTEGER,
            confidence REAL,
            user_confirmed INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE trash (
            id INTEGER PRIMARY KEY,
            photo_id INTEGER NOT NULL UNIQUE,
            original_path TEXT NOT NULL,
            trashed_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .unwrap();

    smriti::db::migrations::run_migrations(&conn).unwrap();
    assert_v15_indexes_present(&conn);

    let schema_version: i32 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(schema_version, MAX_KNOWN_SCHEMA_VERSION);
}

fn assert_v15_indexes_present(conn: &rusqlite::Connection) {
    for index in &[
        "idx_photos_trashed_date",
        "idx_photos_faces_processed_trashed",
        "idx_faces_cluster_confidence",
        "idx_faces_photo_cluster",
    ] {
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1",
                [*index],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "Index '{}' should exist after v15 migration",
            index
        );
    }
}

#[test]
fn test_insert_and_query_photos() {
    let (_temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);

    let photos = vec![
        sample_photo("photos/IMG_001.jpg", "aaa111"),
        sample_photo("photos/IMG_002.jpg", "bbb222"),
        sample_photo("photos/IMG_003.jpg", "ccc333"),
    ];

    let count = repo.insert_batch(&photos).unwrap();
    assert_eq!(count, 3);

    // Query back
    let loaded = repo.get_all_by_date(100, 0).unwrap();
    assert_eq!(loaded.len(), 3);

    // Verify fields
    let first = &loaded[0];
    assert_eq!(first.camera_model.as_deref(), Some("EOS R5"));
    assert_eq!(first.iso, Some(100));
    assert_eq!(first.location_city.as_deref(), Some("Paris"));
    assert_eq!(first.location_country.as_deref(), Some("France"));
    assert_eq!(first.width, Some(8192));
    assert_eq!(first.height, Some(5464));
}

#[test]
fn test_photo_count() {
    let (_temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);

    assert_eq!(repo.count().unwrap(), 0);

    repo.insert_batch(&[sample_photo("a.jpg", "hash1")])
        .unwrap();
    assert_eq!(repo.count().unwrap(), 1);

    repo.insert_batch(&[
        sample_photo("b.jpg", "hash2"),
        sample_photo("c.jpg", "hash3"),
    ])
    .unwrap();
    assert_eq!(repo.count().unwrap(), 3);
}

#[test]
fn test_upsert_preserves_location() {
    let (_temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);

    // Insert with location
    let mut photo = sample_photo("photo.jpg", "hash1");
    photo.location_city = Some("Tokyo".to_string());
    photo.location_country = Some("Japan".to_string());
    repo.insert_batch(&[photo]).unwrap();

    // Re-insert without location (simulates rescan where geocoder is unavailable)
    let mut photo2 = sample_photo("photo.jpg", "hash1");
    photo2.location_city = None;
    photo2.location_country = None;
    repo.insert_batch(&[photo2]).unwrap();

    // Location should be preserved (COALESCE in ON CONFLICT)
    let loaded = repo.get_all_by_date(10, 0).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].location_city.as_deref(), Some("Tokyo"));
    assert_eq!(loaded[0].location_country.as_deref(), Some("Japan"));
}

#[test]
fn test_trash_and_restore_flow() {
    let (_temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);

    repo.insert_batch(&[
        sample_photo("keep.jpg", "h1"),
        sample_photo("trash_me.jpg", "h2"),
        sample_photo("also_keep.jpg", "h3"),
    ])
    .unwrap();

    // Get the photo to trash
    let photos = repo.get_all_by_date(10, 0).unwrap();
    let trash_id = photos
        .iter()
        .find(|p| p.file_name == "trash_me.jpg")
        .unwrap()
        .id;

    // Trash it
    TrashService::trash_photos(&db.conn, &[trash_id]).unwrap();

    // Should be excluded from normal queries
    let remaining = repo.get_all_by_date(10, 0).unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().all(|p| p.file_name != "trash_me.jpg"));

    // Should appear in trash
    let trash_repo = TrashRepo::new(&db.conn);
    let trash_items = trash_repo.get_all().unwrap();
    assert_eq!(trash_items.len(), 1);

    // Restore it
    TrashService::restore_photos(&db.conn, &[trash_id]).unwrap();

    // Should be back in normal queries
    let all = repo.get_all_by_date(10, 0).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn test_photo_with_no_exif() {
    let (_temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);

    let photo = PhotoInsert {
        relative_path: "screenshots/screen.png".to_string(),
        file_name: "screen.png".to_string(),
        file_hash: "noexif_hash".to_string(),
        file_size: 500_000,
        file_mtime: Some(1700000000),
        date_taken: None,
        date_taken_source: None,
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
        width: None,
        height: None,
        orientation: 1,
        media_type: MediaType::Photo,
        duration_ms: None,
        video_codec: None,
        audio_codec: None,
        frame_rate: None,
        bitrate: None,
        has_audio: false,
    };

    repo.insert_batch(&[photo]).unwrap();
    let loaded = repo.get_all_by_date(10, 0).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].date_taken.is_none());
    assert!(loaded[0].camera_model.is_none());
    assert!(loaded[0].location_city.is_none());
}

#[test]
fn test_database_maintenance() {
    let (_temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);

    repo.insert_batch(&[sample_photo("a.jpg", "h1")]).unwrap();

    // Maintenance should not error
    db.run_maintenance().unwrap();

    // Integrity check should pass
    assert!(db.check_integrity().unwrap());
}

#[test]
fn test_database_backup() {
    let (temp, db) = setup_db();
    let repo = PhotoRepo::new(&db.conn);
    repo.insert_batch(&[sample_photo("a.jpg", "h1")]).unwrap();
    drop(db); // Close connection before backup

    let backup_path = Database::backup(temp.path(), 3).unwrap();
    assert!(backup_path.exists());

    // Backup should be a valid DB
    let backup_db = rusqlite::Connection::open(&backup_path).unwrap();
    let count: i32 = backup_db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(count > 0);
}

/// Phase 2 Track B1: multi-row batch inserts should insert every row
/// exactly once, even when the group size exceeds MAX_ROWS_PER_INSERT
/// (so the chunking path is exercised) and when the group size is an
/// exact multiple of the chunk size (boundary).
#[test]
fn test_burst_and_duplicate_large_group_inserts() {
    let (_temp, db) = setup_db();
    let photo_repo = PhotoRepo::new(&db.conn);

    // Seed 800 photos so the foreign-key constraint on group members
    // has something to point at.
    let photos: Vec<PhotoInsert> = (0..800)
        .map(|i| sample_photo(&format!("photos/IMG_{:04}.jpg", i), &format!("h{:04}", i)))
        .collect();
    photo_repo.insert_batch(&photos).unwrap();

    let all_photo_ids: Vec<i64> = db
        .conn
        .prepare("SELECT id FROM photos ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(all_photo_ids.len(), 800);

    // Burst: one group of 600 members (triggers 3 chunks at
    // MAX_ROWS_PER_INSERT = 200).
    let burst_ids = all_photo_ids[..600].to_vec();
    let burst_repo = BurstRepo::new(&db.conn);
    burst_repo
        .sync_burst_groups(&[(
            "2024-01-01T00:00:00Z".to_string(),
            "2024-01-01T00:00:03Z".to_string(),
            burst_ids.clone(),
        )])
        .unwrap();

    let burst_member_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM burst_group_members", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        burst_member_count, 600,
        "every burst member must land exactly once"
    );

    // Duplicate: one group of 400 members (2 full chunks of 200,
    // exactly at the chunk boundary).
    let dup_ids = all_photo_ids[..400].to_vec();
    let dup_repo = DuplicateRepo::new(&db.conn);
    dup_repo
        .sync_duplicate_groups(&[(
            "dup-hash-abc".to_string(),
            dup_ids.clone(),
            Some(dup_ids[0]),
            "exact",
        )])
        .unwrap();

    let dup_member_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM duplicate_group_members", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        dup_member_count, 400,
        "every duplicate member must land exactly once"
    );

    let keep_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM duplicate_group_members WHERE is_suggested_keep = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        keep_count, 1,
        "only the requested photo should be flagged is_suggested_keep"
    );

    // Idempotent resync: the same groups shouldn't create duplicates
    // (merge-based sync preserves existing groups).
    burst_repo
        .sync_burst_groups(&[(
            "2024-01-01T00:00:00Z".to_string(),
            "2024-01-01T00:00:03Z".to_string(),
            burst_ids,
        )])
        .unwrap();
    dup_repo
        .sync_duplicate_groups(&[("dup-hash-abc".to_string(), dup_ids, None, "exact")])
        .unwrap();

    let burst_groups: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM burst_groups", [], |r| r.get(0))
        .unwrap();
    let dup_groups: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM duplicate_groups", [], |r| r.get(0))
        .unwrap();
    assert_eq!(burst_groups, 1, "resync must not duplicate the burst group");
    assert_eq!(
        dup_groups, 1,
        "resync must not duplicate the duplicate group"
    );
}
