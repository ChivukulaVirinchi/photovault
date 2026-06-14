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

use std::collections::HashMap;

use smriti::config::AppConfig;
use smriti::db::album_repo::AlbumRecord;
use smriti::db::album_suggestion_repo::AlbumSuggestionRecord;
use smriti::db::burst_repo::{BurstGroupMemberRecord, BurstGroupRecord};
use smriti::db::duplicate_repo::{DuplicateGroupMemberRecord, DuplicateGroupRecord};
use smriti::db::face_repo::{FaceClusterRecord, FaceDetail, ReviewItem};
use smriti::db::recent_search_repo::RecentSearch;
use smriti::db::trash_repo::TrashedPhotoRecord;
use smriti::models::{ContentCategory, MediaType, Photo};
use smriti::services::album_suggestions::DetectedSuggestion;
use smriti::services::burst_detector::BurstGroup;
use smriti::services::drive_detector::DriveInfo;
use smriti::services::duplicate_detector::DuplicateGroup;
use smriti::services::geocoding::GeocodingResult;
use smriti::services::insights::{CameraStat, CountryStat, InsightsData, LocationStat, PersonStat};
use smriti::services::library_health::LibraryHealth;
use smriti::services::memories::{MemoryCard, MemoryKind};
use smriti::services::search::{
    AlbumHit, InterpretedFilter, PersonHit, PlaceHit, SearchResult, UnifiedSearchResults,
};
use smriti::services::trash::TrashStats;

use smriti_tauri_lib::dto::{
    AlbumDto, AlbumHitDto, AlbumSuggestionDto, AssetHealthDto, AssetInventoryDto, AssetItemDto,
    BurstGroupSummaryDto, BurstMemberDto, CameraStatDto, ContentCategoryDto, CountryStatDto,
    DetectedBurstGroupDto, DetectedDuplicateGroupDto, DetectedSuggestionDto, DriveDto,
    DuplicateGroupSummaryDto, DuplicateMemberDto, FaceDetailDto, InsightsDto, LibraryHealthDto,
    LocationDto, LocationStatDto, MemoryCardDto, PersonDto, PersonHitDto, PersonStatDto, PhotoDto,
    PhotoSummaryDto, PlaceHitDto, RecentSearchDto, ReviewItemDto, SearchPhotoDto, SearchResultsDto,
    SettingsDto, TrashStatsDto, TrashedPhotoDto,
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
        media_type: MediaType::Photo,
        duration_ms: None,
        video_codec: None,
        audio_codec: None,
        frame_rate: None,
        bitrate: None,
        has_audio: false,
        thumbnail_path: Some(".photovault/thumbnails/medium/v2/de/deadbeef.jpg".into()),
        faces_processed: true,
        content_category: ContentCategory::Photo,
        ocr_text: Some("Hello world".into()),
        ocr_processed: true,
        ocr_confidence: Some(0.92),
        is_favorite: true,
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
        media_type: MediaType::Photo,
        duration_ms: None,
        video_codec: None,
        audio_codec: None,
        frame_rate: None,
        bitrate: None,
        has_audio: false,
        thumbnail_path: None,
        faces_processed: false,
        content_category: ContentCategory::Photo,
        ocr_text: None,
        ocr_processed: false,
        ocr_confidence: None,
        is_favorite: false,
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
fn content_category_dto_all_variants() {
    let dto: Vec<ContentCategoryDto> = [
        ContentCategory::Photo,
        ContentCategory::BusinessCard,
        ContentCategory::Document,
        ContentCategory::Screenshot,
        ContentCategory::Presentation,
        ContentCategory::Whiteboard,
        ContentCategory::Receipt,
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    assert_json_snapshot!(dto);
}

#[test]
fn content_category_from_dto_all_variants() {
    let categories: Vec<ContentCategory> = [
        ContentCategoryDto::Photo,
        ContentCategoryDto::BusinessCard,
        ContentCategoryDto::Document,
        ContentCategoryDto::Screenshot,
        ContentCategoryDto::Presentation,
        ContentCategoryDto::Whiteboard,
        ContentCategoryDto::Receipt,
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    assert_json_snapshot!(categories);
}

// ---------- Insights stats ----------

#[test]
fn person_stat_dto() {
    let stat = PersonStat {
        cluster_id: 21,
        name: "Asha".into(),
        photo_count: 88,
        face_id: Some(701),
        face_crop_path: Some(".photovault/faces/701.jpg".into()),
    };
    let dto: PersonStatDto = stat.into();
    assert_json_snapshot!(dto);
}

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

#[test]
fn insights_dto() {
    let mut heatmap = HashMap::new();
    heatmap.insert("2024-01-01".into(), 3);
    let data = InsightsData {
        total_photos: 120,
        date_range_start: Some("2020-01-01".into()),
        date_range_end: Some("2024-12-31".into()),
        people_count: 4,
        album_count: 7,
        country_count: 2,
        city_count: 5,
        photos_with_gps: 80,
        hero_photo_id: Some(42),
        hero_thumbnail_path: Some(".photovault/thumbnails/hero.jpg".into()),
        heatmap,
        heatmap_year: 2024,
        monthly_counts: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        top_people: vec![PersonStat {
            cluster_id: 21,
            name: "Asha".into(),
            photo_count: 88,
            face_id: Some(701),
            face_crop_path: Some(".photovault/faces/701.jpg".into()),
        }],
        top_locations: vec![LocationStat {
            city: "Visakhapatnam".into(),
            country: "India".into(),
            photo_count: 58,
        }],
        top_countries: vec![CountryStat {
            country: "India".into(),
            photo_count: 90,
        }],
        top_cameras: vec![CameraStat {
            camera: "Nikon Z 7II".into(),
            photo_count: 44,
        }],
        available_years: vec![2024, 2023, 2022],
    };
    let dto: InsightsDto = data.into();
    assert_json_snapshot!(dto);
}

// ---------- People ----------

#[test]
fn person_dto() {
    let cluster = FaceClusterRecord {
        id: 21,
        name: Some("Asha".into()),
        representative_face_id: Some(701),
        photo_count: 88,
        face_thumbnail_path: Some(".photovault/faces/701.jpg".into()),
    };
    let dto: PersonDto = cluster.into();
    assert_json_snapshot!(dto);
}

#[test]
fn review_item_dto() {
    let item = ReviewItem {
        queue_id: 9,
        face_id: 101,
        candidate_cluster_id: 21,
        candidate_cluster_name: Some("Asha".into()),
        candidate_cluster_size: 14,
        candidate_sample_face_ids: vec![701, 702, 703],
        score: 0.87,
    };
    let dto: ReviewItemDto = item.into();
    assert_json_snapshot!(dto);
}

#[test]
fn face_detail_dto() {
    let detail = FaceDetail {
        face_id: 101,
        photo_id: 7,
        cluster_id: Some(21),
        confidence: 0.95,
        user_confirmed: 1,
    };
    let dto: FaceDetailDto = detail.into();
    assert_json_snapshot!(dto);
}

// ---------- Albums ----------

#[test]
fn album_dto() {
    let album = AlbumRecord {
        id: 4,
        name: "Goa 2024".into(),
        cover_photo_id: Some(77),
        cover_auto_picked: true,
        photo_count: 32,
        date_range_start: Some("2024-01-03T10:00:00".into()),
        date_range_end: Some("2024-01-08T18:00:00".into()),
        created_at: "2024-01-10T00:00:00Z".into(),
        updated_at: "2024-01-11T00:00:00Z".into(),
        cover_thumbnail_path: Some(".photovault/thumbnails/cover.jpg".into()),
    };
    let dto: AlbumDto = album.into();
    assert_json_snapshot!(dto);
}

#[test]
fn album_suggestion_dto() {
    let suggestion = AlbumSuggestionRecord {
        id: 12,
        kind: "trip".into(),
        title: "Goa".into(),
        photo_ids_json: "[7,8,9]".into(),
        cover_photo_id: Some(7),
        fingerprint: "trip-goa".into(),
        status: "pending".into(),
        seen_count: 2,
        created_at: "2024-01-12T00:00:00Z".into(),
        cover_thumbnail_path: Some(".photovault/thumbnails/goa.jpg".into()),
    };
    let dto: AlbumSuggestionDto = suggestion.into();
    assert_json_snapshot!(dto);
}

#[test]
fn detected_suggestion_dto() {
    let detected = DetectedSuggestion {
        kind: "event".into(),
        title: "A day in Paris".into(),
        photo_ids: vec![1, 2, 3],
        cover_photo_id: Some(2),
        fingerprint: "event-paris".into(),
    };
    let dto: DetectedSuggestionDto = detected.into();
    assert_json_snapshot!(dto);
}

// ---------- Search ----------

#[test]
fn person_hit_dto() {
    let hit = PersonHit {
        cluster_id: 21,
        name: "Asha".into(),
        photo_count: 88,
        face_thumbnail_path: Some(".photovault/faces/701.jpg".into()),
    };
    let dto: PersonHitDto = hit.into();
    assert_json_snapshot!(dto);
}

#[test]
fn album_hit_dto() {
    let hit = AlbumHit {
        album_id: 4,
        name: "Goa 2024".into(),
        photo_count: 32,
        cover_thumbnail_path: Some(".photovault/thumbnails/cover.jpg".into()),
    };
    let dto: AlbumHitDto = hit.into();
    assert_json_snapshot!(dto);
}

#[test]
fn place_hit_dto() {
    let hit = PlaceHit {
        city: "Paris".into(),
        country: Some("France".into()),
        photo_count: 12,
    };
    let dto: PlaceHitDto = hit.into();
    assert_json_snapshot!(dto);
}

#[test]
fn search_photo_dto() {
    let result = SearchResult {
        photo_id: 7,
        date_taken: Some("2024-01-01T12:00:00".into()),
        location_city: Some("Paris".into()),
        location_country: Some("France".into()),
        thumbnail_path: Some(".photovault/thumbnails/search.jpg".into()),
    };
    let dto: SearchPhotoDto = result.into();
    assert_json_snapshot!(dto);
}

#[test]
fn search_results_dto() {
    let results = UnifiedSearchResults {
        interpreted: vec![InterpretedFilter {
            kind: "place".into(),
            label: "Paris, France".into(),
        }],
        people: vec![PersonHit {
            cluster_id: 21,
            name: "Asha".into(),
            photo_count: 88,
            face_thumbnail_path: Some(".photovault/faces/701.jpg".into()),
        }],
        albums: vec![AlbumHit {
            album_id: 4,
            name: "Goa 2024".into(),
            photo_count: 32,
            cover_thumbnail_path: Some(".photovault/thumbnails/cover.jpg".into()),
        }],
        places: vec![PlaceHit {
            city: "Paris".into(),
            country: Some("France".into()),
            photo_count: 12,
        }],
        photos: vec![SearchResult {
            photo_id: 7,
            date_taken: Some("2024-01-01T12:00:00".into()),
            location_city: Some("Paris".into()),
            location_country: Some("France".into()),
            thumbnail_path: Some(".photovault/thumbnails/search.jpg".into()),
        }],
        photo_ids: vec![7],
        photos_grouped: Vec::new(),
    };
    let dto: SearchResultsDto = results.into();
    assert_json_snapshot!(dto);
}

#[test]
fn recent_search_dto() {
    let recent = RecentSearch {
        query: "paris 2024".into(),
        last_used: "2024-01-13T00:00:00Z".into(),
        use_count: 3,
    };
    let dto: RecentSearchDto = recent.into();
    assert_json_snapshot!(dto);
}

// ---------- Memories ----------

#[test]
fn memory_card_dto() {
    let card = MemoryCard {
        id: "otd-2020-01-01".into(),
        kind: MemoryKind::OnThisDay,
        title: "4 years ago today".into(),
        hero_photo_id: 7,
        hero_thumbnail_path: Some(".photovault/thumbnails/memory.jpg".into()),
        photo_count: 3,
        photo_ids: vec![7, 8, 9],
    };
    let dto: MemoryCardDto = card.into();
    assert_json_snapshot!(dto);
}

// ---------- Duplicates ----------

#[test]
fn duplicate_group_summary_dto() {
    let group = DuplicateGroupRecord {
        id: 31,
        member_count: 3,
        cover_thumbnail_path: Some(".photovault/thumbnails/dup.jpg".into()),
        cover_photo_id: Some(7),
        member_photo_ids: vec![7, 8, 9],
    };
    let dto: DuplicateGroupSummaryDto = group.into();
    assert_json_snapshot!(dto);
}

#[test]
fn duplicate_member_dto() {
    let member = DuplicateGroupMemberRecord {
        photo_id: 7,
        is_suggested_keep: true,
        file_path: Some("photos/a.jpg".into()),
        thumbnail_path: Some(".photovault/thumbnails/a.jpg".into()),
        file_size: Some(1024),
        date_taken: Some("2024-01-01T12:00:00".into()),
    };
    let dto: DuplicateMemberDto = member.into();
    assert_json_snapshot!(dto);
}

#[test]
fn detected_duplicate_group_dto() {
    let group = DuplicateGroup {
        hash: "sha256-deadbeef".into(),
        photo_ids: vec![7, 8],
        suggested_keep_id: Some(7),
        duplicate_type: "exact",
    };
    let dto: DetectedDuplicateGroupDto = group.into();
    assert_json_snapshot!(dto);
}

// ---------- Bursts ----------

#[test]
fn burst_group_summary_dto() {
    let group = BurstGroupRecord {
        id: 41,
        start_time: "2024-01-01T12:00:00Z".into(),
        end_time: "2024-01-01T12:00:05Z".into(),
        photo_count: 3,
        cover_thumbnail_paths: vec![
            ".photovault/thumbnails/burst-a.jpg".into(),
            ".photovault/thumbnails/burst-b.jpg".into(),
        ],
        cover_photo_ids: vec![7, 8],
        member_photo_ids: vec![7, 8, 9],
    };
    let dto: BurstGroupSummaryDto = group.into();
    assert_json_snapshot!(dto);
}

#[test]
fn burst_member_dto() {
    let member = BurstGroupMemberRecord {
        photo_id: 7,
        sharpness_score: Some(0.91),
        blur_score: Some(0.12),
        is_suggested_best: true,
    };
    let dto: BurstMemberDto = member.into();
    assert_json_snapshot!(dto);
}

#[test]
fn detected_burst_group_dto() {
    let group = BurstGroup {
        photo_ids: vec![7, 8, 9],
        start_time: fixed_ts(),
        end_time: fixed_ts() + chrono::Duration::seconds(5),
    };
    let dto: DetectedBurstGroupDto = group.into();
    assert_json_snapshot!(dto);
}

// ---------- Trash ----------

#[test]
fn trashed_photo_dto() {
    let trashed = TrashedPhotoRecord {
        photo_id: 7,
        original_path: "photos/a.jpg".into(),
        trashed_at: "2024-01-14T00:00:00Z".into(),
        file_size: Some(1024),
        thumbnail_path: Some(".photovault/thumbnails/a.jpg".into()),
    };
    let dto: TrashedPhotoDto = trashed.into();
    assert_json_snapshot!(dto);
}

#[test]
fn trash_stats_dto() {
    let stats = TrashStats {
        count: 4,
        total_size: 8192,
    };
    let dto: TrashStatsDto = stats.into();
    assert_json_snapshot!(dto);
}

// ---------- Drive / asset health ----------

#[test]
fn drive_dto() {
    let drive = DriveInfo {
        name: "Photos".into(),
        path: std::path::PathBuf::from("/mnt/photos"),
        stable_id: Some("volume-guid".into()),
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

#[test]
fn asset_inventory_dto() {
    let dto = AssetInventoryDto {
        install_root: "C:/Users/alice/AppData/Roaming/smriti/assets".into(),
        roots: vec![
            "C:/Users/alice/AppData/Roaming/smriti/assets".into(),
            "C:/Program Files/Smriti".into(),
        ],
        total_size_bytes: 42_000,
        assets: vec![
            AssetItemDto {
                id: "runtime.onnx".into(),
                label: "ONNX Runtime".into(),
                kind: "runtime".into(),
                status: "active".into(),
                required: true,
                active: true,
                installable: false,
                removable: false,
                size_bytes: Some(24_000),
                path: Some("C:/Program Files/Smriti/libs/onnxruntime/onnxruntime.dll".into()),
                note: Some("Required for local models.".into()),
            },
            AssetItemDto {
                id: "ocr.model".into(),
                label: "OCR model".into(),
                kind: "model".into(),
                status: "planned".into(),
                required: false,
                active: false,
                installable: false,
                removable: false,
                size_bytes: None,
                path: None,
                note: Some("Not installed in this build.".into()),
            },
        ],
    };
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
