//! Wire-format snapshot tests for every `From<EngineType> for Dto`
//! impl in `src-tauri/src/dto.rs`.
//!
//! ## Why
//!
//! The DTOs are the wire-format contract between the Rust backend
//! and the Svelte frontend. The frontend has typed interfaces in
//! `src-ui/src/lib/api/types.ts` that assume specific field names
//! and shapes. When a contributor renames a field in `dto.rs` —
//! intentionally or by accident — the frontend breaks silently at
//! runtime. These snapshot tests catch every such rename at PR time:
//! a contract change shows up as a diff in this file.
//!
//! ## How to read failures
//!
//! When CI fails on one of these tests, the diff insta prints
//! looks like:
//!
//! ```text
//! - Snapshot: photo_dto_full
//! - Source:   src-tauri/tests/dto_snapshots.rs
//! Expected:
//!   { "id": 7, "file_name": "IMG.jpg", ... }
//! Actual:
//!   { "id": 7, "filename": "IMG.jpg", ... }
//!                ^ field renamed
//! ```
//!
//! **Reviewer action**: read the diff. If the rename is intentional
//! and the frontend was updated to match, run
//! `cargo insta review` locally, accept the new snapshot, and
//! commit. If the rename is accidental, ask the contributor to
//! revert.
//!
//! ## How to add a new DTO test
//!
//! Pattern: construct a deterministic instance of the engine
//! source type, run `.into()` to get the DTO, then
//! `assert_json_snapshot!` it. Field VALUES are deterministic so
//! the snapshot is stable across runs and platforms.

use chrono::{TimeZone, Utc};
use insta::assert_json_snapshot;

use smriti::config::AppConfig;
use smriti::models::{ContentCategory, Photo};
use smriti::services::drive_detector::DriveInfo;
use smriti::services::geocoding::GeocodingResult;
use smriti::services::insights::{CameraStat, CountryStat, LocationStat};
use smriti::services::library_health::LibraryHealth;

use smriti_tauri_lib::dto::{
    AssetHealthDto, CameraStatDto, ContentCategoryDto, CountryStatDto, DriveDto, LibraryHealthDto,
    LocationDto, LocationStatDto, PhotoDto, PhotoSummaryDto, SettingsDto,
};

/// Fixed timestamp so date_taken / indexed_at / updated_at don't
/// drift between runs. Use this as the anchor for every test that
/// touches a chrono `DateTime<Utc>`.
fn fixed_ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap()
}

/// Deterministic Photo with every optional field populated. This
/// is the "full house" variant: it exercises every branch in
/// `From<Photo> for PhotoDto` (GPS present, camera present,
/// location present, OCR present).
fn make_photo_full() -> Photo {
    Photo {
        id: 7,
        file_path: "subdir/IMG_0007.jpg".into(),
        file_name: "IMG_0007.jpg".into(),
        file_hash: "deadbeef".into(),
        file_size: 1024,
        date_taken: Some(fixed_ts()),
        date_taken_source: Some("exif".into()),
        gps_latitude: Some(17.68),
        gps_longitude: Some(83.20),
        location_city: Some("Visakhapatnam".into()),
        location_country: Some("India".into()),
        camera_make: Some("NIKON CORPORATION".into()),
        camera_model: Some("NIKON Z 7II".into()),
        iso: Some(400),
        aperture: Some("f/2.8".into()),
        shutter_speed: Some("1/125".into()),
        focal_length: Some("50mm".into()),
        lens_model: Some("NIKKOR Z 24-70mm f/2.8 S".into()),
        flash: Some("Off".into()),
        gps_altitude: Some(15.0),
        width: Some(6048),
        height: Some(4024),
        orientation: 1,
        thumbnail_path: Some(".photovault/thumbnails/medium/v2/de/deadbeef.jpg".into()),
        faces_processed: true,
        content_category: ContentCategory::Photo,
        ocr_text: Some("Hello world".into()),
        ocr_processed: true,
        ocr_confidence: Some(0.92),
        is_trashed: false,
        trashed_at: None,
        indexed_at: fixed_ts(),
        updated_at: fixed_ts(),
    }
}

/// Photo with every optional field set to None — the "minimal"
/// variant. Exercises the None-branch of every `Option` mapping in
/// `From<Photo> for PhotoDto`.
fn make_photo_minimal() -> Photo {
    Photo {
        id: 1,
        file_path: "IMG.jpg".into(),
        file_name: "IMG.jpg".into(),
        file_hash: "abc".into(),
        file_size: 0,
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
        thumbnail_path: None,
        faces_processed: false,
        content_category: ContentCategory::Photo,
        ocr_text: None,
        ocr_processed: false,
        ocr_confidence: None,
        is_trashed: false,
        trashed_at: None,
        indexed_at: fixed_ts(),
        updated_at: fixed_ts(),
    }
}

// ---------- Photo ----------

#[test]
fn photo_dto_full() {
    let dto: PhotoDto = make_photo_full().into();
    assert_json_snapshot!(dto);
}

#[test]
fn photo_dto_minimal() {
    let dto: PhotoDto = make_photo_minimal().into();
    assert_json_snapshot!(dto);
}

#[test]
fn photo_summary_dto_full() {
    let dto: PhotoSummaryDto = make_photo_full().into();
    assert_json_snapshot!(dto);
}

#[test]
fn photo_summary_dto_borrowed() {
    // Exercises the `From<&Photo>` impl rather than `From<Photo>`.
    let photo = make_photo_minimal();
    let dto: PhotoSummaryDto = (&photo).into();
    assert_json_snapshot!(dto);
}

// ---------- Location / Geocoding ----------

#[test]
fn location_dto_from_geocoding_result() {
    let result = GeocodingResult {
        city: "Visakhapatnam".into(),
        country: "India".into(),
    };
    let dto: LocationDto = result.into();
    assert_json_snapshot!(dto);
}

// ---------- Content category enum ----------

#[test]
fn content_category_dto_round_trip_photo() {
    let dto: ContentCategoryDto = ContentCategory::Photo.into();
    assert_json_snapshot!(dto);
}

#[test]
fn content_category_dto_round_trip_screenshot() {
    let dto: ContentCategoryDto = ContentCategory::Screenshot.into();
    assert_json_snapshot!(dto);
}

#[test]
fn content_category_dto_round_trip_receipt() {
    let dto: ContentCategoryDto = ContentCategory::Receipt.into();
    assert_json_snapshot!(dto);
}

// ---------- Insights stats ----------

#[test]
fn camera_stat_dto() {
    let stat = CameraStat {
        camera: "Nikon Z 7II".into(),
        photo_count: 1234,
    };
    let dto: CameraStatDto = stat.into();
    assert_json_snapshot!(dto);
}

#[test]
fn location_stat_dto() {
    let stat = LocationStat {
        city: "Visakhapatnam".into(),
        country: "India".into(),
        photo_count: 5821,
    };
    let dto: LocationStatDto = stat.into();
    assert_json_snapshot!(dto);
}

#[test]
fn country_stat_dto() {
    let stat = CountryStat {
        country: "India".into(),
        photo_count: 9000,
    };
    let dto: CountryStatDto = stat.into();
    assert_json_snapshot!(dto);
}

// ---------- Drive / asset health ----------

#[test]
fn drive_dto() {
    let drive = DriveInfo {
        name: "Photos".into(),
        path: std::path::PathBuf::from("/mnt/photos"),
        is_removable: true,
        has_photovault_db: true,
        total_size_bytes: Some(1_000_000_000_000),
        free_space_bytes: Some(500_000_000_000),
    };
    let dto: DriveDto = drive.into();
    assert_json_snapshot!(dto);
}

#[test]
fn asset_health_dto_all_present() {
    let health = smriti::bootstrap::AssetHealth {
        missing_face_models: false,
        missing_onnx_runtime: false,
        missing_geonames_db: false,
    };
    let dto: AssetHealthDto = health.into();
    assert_json_snapshot!(dto);
}

#[test]
fn asset_health_dto_all_missing() {
    let health = smriti::bootstrap::AssetHealth {
        missing_face_models: true,
        missing_onnx_runtime: true,
        missing_geonames_db: true,
    };
    let dto: AssetHealthDto = health.into();
    assert_json_snapshot!(dto);
}

// ---------- Settings ----------

#[test]
fn settings_dto_defaults() {
    let cfg = AppConfig::default();
    let dto: SettingsDto = (&cfg).into();
    assert_json_snapshot!(dto);
}

// ---------- Library health ----------

#[test]
fn library_health_dto() {
    let health = LibraryHealth {
        total_photos: 91000,
        missing_thumbnails: 0,
        inaccurate_dates: 12,
        missing_dates: 3,
        heic_count: 410,
        heic_decoder_available: true,
        face_processed_no_faces: 7400,
    };
    let dto: LibraryHealthDto = health.into();
    assert_json_snapshot!(dto);
}
