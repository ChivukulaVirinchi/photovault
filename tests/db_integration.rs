//! Integration tests for database operations.
//!
//! Tests the full DB lifecycle: schema creation, photo insertion,
//! querying, geocoding updates, trash flow, and data integrity.

use photovault::db::{create_schema, Database, PhotoRepo, TrashRepo};
use photovault::db::photo_repo::PhotoInsert;
use photovault::services::TrashService;
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
        file_name: path.split('/').last().unwrap_or(path).to_string(),
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
    }
}

#[test]
fn test_schema_creation() {
    let (_temp, db) = setup_db();

    // Verify key tables exist
    for table in &["photos", "faces", "face_clusters", "duplicate_groups", "burst_groups", "trash"] {
        let count: i32 = db.conn.query_row(
            &format!("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'", table),
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1, "Table '{}' should exist", table);
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

    repo.insert_batch(&[sample_photo("a.jpg", "hash1")]).unwrap();
    assert_eq!(repo.count().unwrap(), 1);

    repo.insert_batch(&[
        sample_photo("b.jpg", "hash2"),
        sample_photo("c.jpg", "hash3"),
    ]).unwrap();
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
    ]).unwrap();

    // Get the photo to trash
    let photos = repo.get_all_by_date(10, 0).unwrap();
    let trash_id = photos.iter().find(|p| p.file_name == "trash_me.jpg").unwrap().id;

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
    let count: i32 = backup_db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(count > 0);
}
