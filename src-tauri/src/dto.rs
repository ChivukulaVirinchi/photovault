//! Wire-format DTOs.
//!
//! Every type that crosses the IPC boundary lives here. DTOs are
//! serde-only — they are NEVER reused as DB rows or service-internal
//! types. `From` impls translate one-way from the engine's internal
//! types into DTOs at handler boundaries.
//!
//! Field names use snake_case to match the smriti backend; the
//! Svelte client treats them as opaque keys.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use smriti::config::AppConfig;
use smriti::db::album_repo::AlbumRecord;
use smriti::db::album_suggestion_repo::AlbumSuggestionRecord;
use smriti::db::burst_repo::{BurstGroupMemberRecord, BurstGroupRecord};
use smriti::db::duplicate_repo::{DuplicateGroupMemberRecord, DuplicateGroupRecord};
use smriti::db::excluded_folder_repo::ExcludedFolderRecord;
use smriti::db::face_repo::{FaceClusterRecord, FaceDetail, ReviewItem};
use smriti::db::recent_search_repo::RecentSearch;
use smriti::db::stack_repo::{PhotoStackMemberRecord, PhotoStackRecord};
use smriti::db::trash_repo::TrashedPhotoRecord;
use smriti::models::{ContentCategory, MediaType, Photo};
use smriti::services::album_suggestions::DetectedSuggestion;
use smriti::services::burst_detector::BurstGroup;
use smriti::services::drive_detector::DriveInfo;
use smriti::services::duplicate_detector::DuplicateGroup;
use smriti::services::geocoding::GeocodingResult;
use smriti::services::insights::{CameraStat, CountryStat, InsightsData, LocationStat, PersonStat};
use smriti::services::library_health::LibraryHealth;
use smriti::services::memories::MemoryCard;
use smriti::services::search::{
    AlbumHit, InterpretedFilter, PersonHit, PlaceHit, SearchResult, UnifiedSearchResults,
};
use smriti::services::trash::TrashStats;

/// Generic page-of-T return shape used by every paginated command.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total: Option<u64>,
}

/// Returned by every long-running job command (start_scan, run_dups, ...).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JobIdDto {
    pub job_id: String,
}

// ---------- photos ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhotoDto {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_hash: String,
    pub date_taken: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
    pub media_type: MediaTypeDto,
    pub duration_ms: Option<i64>,
    pub video: Option<VideoDto>,
    pub gps: Option<GpsDto>,
    pub location: Option<LocationDto>,
    pub camera: Option<CameraDto>,
    pub thumbnail_path: Option<String>,
    pub content_category: ContentCategoryDto,
    pub ocr: Option<OcrDto>,
    pub faces_processed: bool,
    pub is_favorite: bool,
    pub is_trashed: bool,
    pub stack: Option<PhotoStackBadgeDto>,
    pub indexed_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhotoSummaryDto {
    pub id: i64,
    pub thumbnail_path: Option<String>,
    pub date_taken: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
    pub media_type: MediaTypeDto,
    pub duration_ms: Option<i64>,
    pub is_favorite: bool,
    pub is_trashed: bool,
    pub stack: Option<PhotoStackBadgeDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhotoStackBadgeDto {
    pub id: i64,
    pub kind: String,
    pub member_count: i64,
    pub cover_photo_id: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MediaTypeDto {
    Photo,
    Video,
}

impl From<MediaType> for MediaTypeDto {
    fn from(t: MediaType) -> Self {
        match t {
            MediaType::Photo => Self::Photo,
            MediaType::Video => Self::Video,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VideoDto {
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub frame_rate: Option<f32>,
    pub bitrate: Option<i64>,
    pub has_audio: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpsDto {
    pub lat: f64,
    pub lng: f64,
    pub altitude: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationDto {
    pub city: Option<String>,
    pub country: Option<String>,
}

impl From<GeocodingResult> for LocationDto {
    fn from(r: GeocodingResult) -> Self {
        Self {
            city: Some(r.city),
            country: Some(r.country),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CameraDto {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<i32>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,
    pub flash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OcrDto {
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ContentCategoryDto {
    Photo,
    BusinessCard,
    Document,
    Screenshot,
    Presentation,
    Whiteboard,
    Receipt,
}

impl From<ContentCategory> for ContentCategoryDto {
    fn from(c: ContentCategory) -> Self {
        match c {
            ContentCategory::Photo => Self::Photo,
            ContentCategory::BusinessCard => Self::BusinessCard,
            ContentCategory::Document => Self::Document,
            ContentCategory::Screenshot => Self::Screenshot,
            ContentCategory::Presentation => Self::Presentation,
            ContentCategory::Whiteboard => Self::Whiteboard,
            ContentCategory::Receipt => Self::Receipt,
        }
    }
}

impl From<ContentCategoryDto> for ContentCategory {
    fn from(c: ContentCategoryDto) -> Self {
        match c {
            ContentCategoryDto::Photo => Self::Photo,
            ContentCategoryDto::BusinessCard => Self::BusinessCard,
            ContentCategoryDto::Document => Self::Document,
            ContentCategoryDto::Screenshot => Self::Screenshot,
            ContentCategoryDto::Presentation => Self::Presentation,
            ContentCategoryDto::Whiteboard => Self::Whiteboard,
            ContentCategoryDto::Receipt => Self::Receipt,
        }
    }
}

impl From<Photo> for PhotoDto {
    fn from(p: Photo) -> Self {
        let gps = p
            .gps_latitude
            .zip(p.gps_longitude)
            .map(|(lat, lng)| GpsDto {
                lat,
                lng,
                altitude: p.gps_altitude,
            });
        let location = if p.location_city.is_some() || p.location_country.is_some() {
            Some(LocationDto {
                city: p.location_city.clone(),
                country: p.location_country.clone(),
            })
        } else {
            None
        };
        let camera_present = p.camera_make.is_some()
            || p.camera_model.is_some()
            || p.lens_model.is_some()
            || p.iso.is_some()
            || p.aperture.is_some()
            || p.shutter_speed.is_some()
            || p.focal_length.is_some()
            || p.flash.is_some();
        let camera_name = smriti::services::camera_names::friendly_camera_name(
            p.camera_make.as_deref(),
            p.camera_model.as_deref(),
        );
        let camera = if camera_present {
            Some(CameraDto {
                make: camera_name.or(p.camera_make),
                model: None,
                lens: p.lens_model,
                iso: p.iso,
                aperture: p.aperture,
                shutter_speed: p.shutter_speed,
                focal_length: p.focal_length,
                flash: p.flash,
            })
        } else {
            None
        };
        let ocr = p.ocr_text.map(|text| OcrDto {
            text,
            confidence: p.ocr_confidence.unwrap_or(0.0),
        });
        let video = if p.media_type == MediaType::Video {
            Some(VideoDto {
                video_codec: p.video_codec,
                audio_codec: p.audio_codec,
                frame_rate: p.frame_rate,
                bitrate: p.bitrate,
                has_audio: p.has_audio,
            })
        } else {
            None
        };
        Self {
            id: p.id,
            file_path: p.file_path,
            file_name: p.file_name,
            file_size: p.file_size,
            file_hash: p.file_hash,
            date_taken: p
                .date_taken
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            width: p.width,
            height: p.height,
            orientation: p.orientation,
            media_type: p.media_type.into(),
            duration_ms: p.duration_ms,
            video,
            gps,
            location,
            camera,
            thumbnail_path: p.thumbnail_path,
            content_category: p.content_category.into(),
            ocr,
            faces_processed: p.faces_processed,
            is_favorite: p.is_favorite,
            is_trashed: p.is_trashed,
            stack: None,
            indexed_at: p.indexed_at.to_rfc3339(),
        }
    }
}

impl From<&Photo> for PhotoSummaryDto {
    fn from(p: &Photo) -> Self {
        Self {
            id: p.id,
            thumbnail_path: p.thumbnail_path.clone(),
            date_taken: p
                .date_taken
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            width: p.width,
            height: p.height,
            orientation: p.orientation,
            media_type: p.media_type.into(),
            duration_ms: p.duration_ms,
            is_favorite: p.is_favorite,
            is_trashed: p.is_trashed,
            stack: None,
        }
    }
}

impl From<Photo> for PhotoSummaryDto {
    fn from(p: Photo) -> Self {
        (&p).into()
    }
}

// ---------- people ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersonDto {
    pub id: i64,
    pub name: Option<String>,
    pub photo_count: i64,
    pub representative_face_id: Option<i64>,
    pub representative_thumbnail_path: Option<String>,
}

impl From<FaceClusterRecord> for PersonDto {
    fn from(c: FaceClusterRecord) -> Self {
        Self {
            id: c.id,
            name: c.name,
            photo_count: c.photo_count,
            representative_face_id: c.representative_face_id,
            representative_thumbnail_path: c.face_thumbnail_path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingFaceCountDto {
    pub pending_photos: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClusteringDiagnosticsDto {
    pub faces_detected: usize,
    pub clusters_created: usize,
    pub photos_processed: usize,
    pub rejected_small: usize,
    pub rejected_lowconf: usize,
    pub rejected_blurry: usize,
    pub rejected_yaw: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewItemDto {
    pub queue_id: i64,
    pub face_id: i64,
    pub candidate_cluster_id: i64,
    pub candidate_cluster_name: Option<String>,
    pub candidate_cluster_size: i64,
    pub candidate_sample_face_ids: Vec<i64>,
    pub score: f32,
}

impl From<ReviewItem> for ReviewItemDto {
    fn from(r: ReviewItem) -> Self {
        Self {
            queue_id: r.queue_id,
            face_id: r.face_id,
            candidate_cluster_id: r.candidate_cluster_id,
            candidate_cluster_name: r.candidate_cluster_name,
            candidate_cluster_size: r.candidate_cluster_size,
            candidate_sample_face_ids: r.candidate_sample_face_ids,
            score: r.score,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FaceDetailDto {
    pub face_id: i64,
    pub photo_id: i64,
    pub cluster_id: Option<i64>,
    pub cluster_name: Option<String>,
    pub confidence: f32,
    pub user_confirmed: i32,
    pub thumbnail_path: Option<String>,
}

impl From<FaceDetail> for FaceDetailDto {
    fn from(f: FaceDetail) -> Self {
        Self {
            face_id: f.face_id,
            photo_id: f.photo_id,
            cluster_id: f.cluster_id,
            cluster_name: None,
            confidence: f.confidence,
            user_confirmed: f.user_confirmed,
            thumbnail_path: Some(format!(".photovault/faces/{}.jpg", f.face_id)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClusterSuggestionDto {
    pub cluster_id: i64,
    pub name: String,
    pub score: f32,
    pub face_count: i64,
    pub representative_face_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewFaceCountDto {
    pub unconfirmed_total: i64,
    pub clusters_with_unconfirmed: i64,
}

// ---------- albums ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumDto {
    pub id: i64,
    pub name: String,
    pub photo_count: i64,
    pub cover_photo_id: Option<i64>,
    pub cover_thumbnail_path: Option<String>,
    pub date_range_start: Option<String>,
    pub date_range_end: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_virtual: bool,
}

impl From<AlbumRecord> for AlbumDto {
    fn from(a: AlbumRecord) -> Self {
        Self {
            id: a.id,
            name: a.name,
            photo_count: a.photo_count,
            cover_photo_id: a.cover_photo_id,
            cover_thumbnail_path: a.cover_thumbnail_path,
            date_range_start: a.date_range_start,
            date_range_end: a.date_range_end,
            created_at: a.created_at,
            updated_at: a.updated_at,
            is_virtual: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumSuggestionDto {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub photo_ids: Vec<i64>,
    pub cover_photo_id: Option<i64>,
    pub cover_thumbnail_path: Option<String>,
    pub status: String,
    pub seen_count: i64,
    pub created_at: String,
}

impl From<AlbumSuggestionRecord> for AlbumSuggestionDto {
    fn from(s: AlbumSuggestionRecord) -> Self {
        let photo_ids = s.photo_ids();
        Self {
            id: s.id,
            kind: s.kind,
            title: s.title,
            photo_ids,
            cover_photo_id: s.cover_photo_id,
            cover_thumbnail_path: s.cover_thumbnail_path,
            status: s.status,
            seen_count: s.seen_count,
            created_at: s.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetectedSuggestionDto {
    pub kind: String,
    pub title: String,
    pub photo_ids: Vec<i64>,
    pub cover_photo_id: Option<i64>,
    pub fingerprint: String,
}

impl From<DetectedSuggestion> for DetectedSuggestionDto {
    fn from(d: DetectedSuggestion) -> Self {
        Self {
            kind: d.kind,
            title: d.title,
            photo_ids: d.photo_ids,
            cover_photo_id: d.cover_photo_id,
            fingerprint: d.fingerprint,
        }
    }
}

// ---------- search ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResultsDto {
    pub interpreted: Vec<InterpretedFilterDto>,
    pub people: Vec<PersonHitDto>,
    pub albums: Vec<AlbumHitDto>,
    pub places: Vec<PlaceHitDto>,
    pub photo_ids: Vec<i64>,
    pub photos: Vec<SearchPhotoDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InterpretedFilterDto {
    pub kind: String,
    pub label: String,
}

impl From<InterpretedFilter> for InterpretedFilterDto {
    fn from(f: InterpretedFilter) -> Self {
        Self {
            kind: f.kind,
            label: f.label,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersonHitDto {
    pub cluster_id: i64,
    pub name: String,
    pub photo_count: i64,
    pub face_thumbnail_path: Option<String>,
}

impl From<PersonHit> for PersonHitDto {
    fn from(h: PersonHit) -> Self {
        Self {
            cluster_id: h.cluster_id,
            name: h.name,
            photo_count: h.photo_count,
            face_thumbnail_path: h.face_thumbnail_path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumHitDto {
    pub album_id: i64,
    pub name: String,
    pub photo_count: i64,
    pub cover_thumbnail_path: Option<String>,
}

impl From<AlbumHit> for AlbumHitDto {
    fn from(h: AlbumHit) -> Self {
        Self {
            album_id: h.album_id,
            name: h.name,
            photo_count: h.photo_count,
            cover_thumbnail_path: h.cover_thumbnail_path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlaceHitDto {
    pub city: String,
    pub country: Option<String>,
    pub photo_count: i64,
}

impl From<PlaceHit> for PlaceHitDto {
    fn from(h: PlaceHit) -> Self {
        Self {
            city: h.city,
            country: h.country,
            photo_count: h.photo_count,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchPhotoDto {
    pub photo_id: i64,
    pub date_taken: Option<String>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
    pub thumbnail_path: Option<String>,
}

impl From<SearchResult> for SearchPhotoDto {
    fn from(r: SearchResult) -> Self {
        Self {
            photo_id: r.photo_id,
            date_taken: r.date_taken,
            location_city: r.location_city,
            location_country: r.location_country,
            thumbnail_path: r.thumbnail_path,
        }
    }
}

impl From<UnifiedSearchResults> for SearchResultsDto {
    fn from(u: UnifiedSearchResults) -> Self {
        Self {
            interpreted: u.interpreted.into_iter().map(Into::into).collect(),
            people: u.people.into_iter().map(Into::into).collect(),
            albums: u.albums.into_iter().map(Into::into).collect(),
            places: u.places.into_iter().map(Into::into).collect(),
            photo_ids: u.photo_ids,
            photos: u.photos.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecentSearchDto {
    pub query: String,
    pub last_used: String,
    pub use_count: i64,
}

impl From<RecentSearch> for RecentSearchDto {
    fn from(r: RecentSearch) -> Self {
        Self {
            query: r.query,
            last_used: r.last_used,
            use_count: r.use_count,
        }
    }
}

// ---------- memories ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryCardDto {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub hero_photo_id: i64,
    pub hero_thumbnail_path: Option<String>,
    pub photo_count: usize,
    pub photo_ids: Vec<i64>,
}

impl From<MemoryCard> for MemoryCardDto {
    fn from(m: MemoryCard) -> Self {
        Self {
            id: m.id,
            kind: format!("{:?}", m.kind).to_lowercase(),
            title: m.title,
            hero_photo_id: m.hero_photo_id,
            hero_thumbnail_path: m.hero_thumbnail_path,
            photo_count: m.photo_count,
            photo_ids: m.photo_ids,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryDetailDto {
    pub card: MemoryCardDto,
    pub photos: Vec<PhotoSummaryDto>,
}

// ---------- duplicates ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateGroupSummaryDto {
    pub id: i64,
    pub member_count: i64,
    pub cover_thumbnail_path: Option<String>,
    pub cover_photo_id: Option<i64>,
    pub member_photo_ids: Vec<i64>,
}

impl From<DuplicateGroupRecord> for DuplicateGroupSummaryDto {
    fn from(g: DuplicateGroupRecord) -> Self {
        Self {
            id: g.id,
            member_count: g.member_count,
            cover_thumbnail_path: g.cover_thumbnail_path,
            cover_photo_id: g.cover_photo_id,
            member_photo_ids: g.member_photo_ids,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateGroupDto {
    pub id: i64,
    pub members: Vec<DuplicateMemberDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateMemberDto {
    pub photo_id: i64,
    pub is_suggested_keep: bool,
    pub file_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub file_size: Option<i64>,
    pub date_taken: Option<String>,
}

impl From<DuplicateGroupMemberRecord> for DuplicateMemberDto {
    fn from(m: DuplicateGroupMemberRecord) -> Self {
        Self {
            photo_id: m.photo_id,
            is_suggested_keep: m.is_suggested_keep,
            file_path: m.file_path,
            thumbnail_path: m.thumbnail_path,
            file_size: m.file_size,
            date_taken: m.date_taken,
        }
    }
}

// ---------- photo stacks ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhotoStackDto {
    pub id: i64,
    pub kind: String,
    pub source_group_id: i64,
    pub cover_photo_id: i64,
    pub member_count: i64,
    pub confidence: f32,
    pub members: Vec<PhotoStackMemberDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhotoStackMemberDto {
    pub photo_id: i64,
    pub thumbnail_path: Option<String>,
    pub date_taken: Option<String>,
    pub quality_score: f32,
    pub score_reasons: Option<String>,
    pub is_cover: bool,
}

impl From<PhotoStackMemberRecord> for PhotoStackMemberDto {
    fn from(m: PhotoStackMemberRecord) -> Self {
        Self {
            photo_id: m.photo_id,
            thumbnail_path: m.thumbnail_path,
            date_taken: m.date_taken,
            quality_score: m.quality_score,
            score_reasons: m.score_reasons,
            is_cover: m.is_cover,
        }
    }
}

pub fn stack_detail_dto(
    stack: PhotoStackRecord,
    members: Vec<PhotoStackMemberRecord>,
) -> PhotoStackDto {
    PhotoStackDto {
        id: stack.id,
        kind: stack.kind,
        source_group_id: stack.source_group_id,
        cover_photo_id: stack.cover_photo_id,
        member_count: stack.member_count,
        confidence: stack.confidence,
        members: members.into_iter().map(Into::into).collect(),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetectedDuplicateGroupDto {
    pub hash: String,
    pub photo_ids: Vec<i64>,
    pub suggested_keep_id: Option<i64>,
    pub duplicate_type: String,
}

impl From<DuplicateGroup> for DetectedDuplicateGroupDto {
    fn from(g: DuplicateGroup) -> Self {
        Self {
            hash: g.hash,
            photo_ids: g.photo_ids,
            suggested_keep_id: g.suggested_keep_id,
            duplicate_type: g.duplicate_type.to_string(),
        }
    }
}

// ---------- bursts ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BurstGroupSummaryDto {
    pub id: i64,
    pub start_time: String,
    pub end_time: String,
    pub photo_count: i64,
    pub cover_thumbnail_paths: Vec<String>,
    pub cover_photo_ids: Vec<i64>,
    pub member_photo_ids: Vec<i64>,
}

impl From<BurstGroupRecord> for BurstGroupSummaryDto {
    fn from(g: BurstGroupRecord) -> Self {
        Self {
            id: g.id,
            start_time: g.start_time,
            end_time: g.end_time,
            photo_count: g.photo_count,
            cover_thumbnail_paths: g.cover_thumbnail_paths,
            cover_photo_ids: g.cover_photo_ids,
            member_photo_ids: g.member_photo_ids,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BurstGroupDto {
    pub id: i64,
    pub members: Vec<BurstMemberDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BurstMemberDto {
    pub photo_id: i64,
    pub sharpness_score: Option<f32>,
    pub blur_score: Option<f32>,
    pub is_suggested_best: bool,
}

impl From<BurstGroupMemberRecord> for BurstMemberDto {
    fn from(m: BurstGroupMemberRecord) -> Self {
        Self {
            photo_id: m.photo_id,
            sharpness_score: m.sharpness_score,
            blur_score: m.blur_score,
            is_suggested_best: m.is_suggested_best,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetectedBurstGroupDto {
    pub photo_ids: Vec<i64>,
    pub start_time: String,
    pub end_time: String,
}

impl From<BurstGroup> for DetectedBurstGroupDto {
    fn from(g: BurstGroup) -> Self {
        Self {
            photo_ids: g.photo_ids,
            start_time: g.start_time.to_rfc3339(),
            end_time: g.end_time.to_rfc3339(),
        }
    }
}

// ---------- trash ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrashedPhotoDto {
    pub photo_id: i64,
    pub original_path: String,
    pub trashed_at: String,
    pub file_size: Option<i64>,
    pub thumbnail_path: Option<String>,
}

impl From<TrashedPhotoRecord> for TrashedPhotoDto {
    fn from(t: TrashedPhotoRecord) -> Self {
        Self {
            photo_id: t.photo_id,
            original_path: t.original_path,
            trashed_at: t.trashed_at,
            file_size: t.file_size,
            thumbnail_path: t.thumbnail_path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrashStatsDto {
    pub count: usize,
    pub total_size: u64,
}

impl From<TrashStats> for TrashStatsDto {
    fn from(s: TrashStats) -> Self {
        Self {
            count: s.count,
            total_size: s.total_size,
        }
    }
}

// ---------- map ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MapPinDto {
    /// Representative photo when count = 1; first member of the cluster otherwise.
    pub photo_id: i64,
    pub lat: f64,
    pub lng: f64,
    pub thumbnail_path: Option<String>,
    /// 1 = single pin, >1 = clustered cluster pin.
    pub count: u32,
    /// Member photo ids for clusters (populated when count > 1).
    /// The filmstrip drawer feeds these directly into
    /// `map_cluster_filmstrip` without needing a re-query.
    #[serde(default)]
    pub photo_ids: Vec<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TileCacheStatsDto {
    pub size_bytes: u64,
    pub file_count: u64,
    pub limit_bytes: u64,
}

// ---------- insights ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsDto {
    pub total_photos: i64,
    pub date_range_start: Option<String>,
    pub date_range_end: Option<String>,
    pub people_count: i64,
    pub album_count: i64,
    pub country_count: i64,
    pub city_count: i64,
    pub photos_with_gps: i64,
    pub hero_photo_id: Option<i64>,
    pub hero_thumbnail_path: Option<String>,
    pub heatmap: std::collections::HashMap<String, i64>,
    pub heatmap_year: i32,
    pub monthly_counts: [i64; 12],
    pub top_people: Vec<PersonStatDto>,
    pub top_locations: Vec<LocationStatDto>,
    pub top_countries: Vec<CountryStatDto>,
    pub top_cameras: Vec<CameraStatDto>,
    pub available_years: Vec<i32>,
}

impl From<InsightsData> for InsightsDto {
    fn from(d: InsightsData) -> Self {
        Self {
            total_photos: d.total_photos,
            date_range_start: d.date_range_start,
            date_range_end: d.date_range_end,
            people_count: d.people_count,
            album_count: d.album_count,
            country_count: d.country_count,
            city_count: d.city_count,
            photos_with_gps: d.photos_with_gps,
            hero_photo_id: d.hero_photo_id,
            hero_thumbnail_path: d.hero_thumbnail_path,
            heatmap: d.heatmap,
            heatmap_year: d.heatmap_year,
            monthly_counts: d.monthly_counts,
            top_people: d.top_people.into_iter().map(Into::into).collect(),
            top_locations: d.top_locations.into_iter().map(Into::into).collect(),
            top_countries: d.top_countries.into_iter().map(Into::into).collect(),
            top_cameras: d.top_cameras.into_iter().map(Into::into).collect(),
            available_years: d.available_years,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersonStatDto {
    pub cluster_id: i64,
    pub name: String,
    pub photo_count: i64,
    pub face_id: Option<i64>,
    pub face_crop_path: Option<String>,
}

impl From<PersonStat> for PersonStatDto {
    fn from(s: PersonStat) -> Self {
        Self {
            cluster_id: s.cluster_id,
            name: s.name,
            photo_count: s.photo_count,
            face_id: s.face_id,
            face_crop_path: s.face_crop_path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationStatDto {
    pub city: String,
    pub country: String,
    pub photo_count: i64,
}

impl From<LocationStat> for LocationStatDto {
    fn from(s: LocationStat) -> Self {
        Self {
            city: s.city,
            country: s.country,
            photo_count: s.photo_count,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CountryStatDto {
    pub country: String,
    pub photo_count: i64,
}

impl From<CountryStat> for CountryStatDto {
    fn from(s: CountryStat) -> Self {
        Self {
            country: s.country,
            photo_count: s.photo_count,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CameraStatDto {
    pub camera: String,
    pub photo_count: i64,
}

impl From<CameraStat> for CameraStatDto {
    fn from(s: CameraStat) -> Self {
        Self {
            camera: s.camera,
            photo_count: s.photo_count,
        }
    }
}

// ---------- library health ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryHealthDto {
    pub total_photos: i64,
    pub missing_thumbnails: i64,
    pub inaccurate_dates: i64,
    pub missing_dates: i64,
    pub heic_count: i64,
    pub heic_decoder_available: bool,
    pub face_processed_no_faces: i64,
}

impl From<LibraryHealth> for LibraryHealthDto {
    fn from(h: LibraryHealth) -> Self {
        Self {
            total_photos: h.total_photos,
            missing_thumbnails: h.missing_thumbnails,
            inaccurate_dates: h.inaccurate_dates,
            missing_dates: h.missing_dates,
            heic_count: h.heic_count,
            heic_decoder_available: h.heic_decoder_available,
            face_processed_no_faces: h.face_processed_no_faces,
        }
    }
}

// ---------- drives, library, system ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DriveDto {
    pub name: String,
    pub path: String,
    pub is_removable: bool,
    pub has_photovault_db: bool,
    pub total_size_bytes: Option<u64>,
    pub free_space_bytes: Option<u64>,
}

impl From<DriveInfo> for DriveDto {
    fn from(d: DriveInfo) -> Self {
        Self {
            name: d.name,
            path: d.path.display().to_string(),
            is_removable: d.is_removable,
            has_photovault_db: d.has_photovault_db,
            total_size_bytes: d.total_size_bytes,
            free_space_bytes: d.free_space_bytes,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryHandleDto {
    pub drive_root: String,
    pub photo_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExcludedFolderDto {
    pub relative_path: String,
    pub indexed_count: i64,
    pub created_at: String,
}

impl From<ExcludedFolderRecord> for ExcludedFolderDto {
    fn from(r: ExcludedFolderRecord) -> Self {
        Self {
            relative_path: r.relative_path,
            indexed_count: r.indexed_count,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExcludedFolderPreviewDto {
    pub relative_path: String,
    pub indexed_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetHealthDto {
    pub missing_face_models: bool,
    pub missing_onnx_runtime: bool,
    pub missing_geonames_db: bool,
    pub summary: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetInventoryDto {
    pub install_root: String,
    pub roots: Vec<String>,
    pub total_size_bytes: u64,
    pub assets: Vec<AssetItemDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetItemDto {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub status: String,
    pub required: bool,
    pub active: bool,
    pub installable: bool,
    pub removable: bool,
    pub size_bytes: Option<u64>,
    pub path: Option<String>,
    pub note: Option<String>,
}

impl From<smriti::bootstrap::AssetHealth> for AssetHealthDto {
    fn from(h: smriti::bootstrap::AssetHealth) -> Self {
        let summary = h.summary();
        Self {
            missing_face_models: h.missing_face_models,
            missing_onnx_runtime: h.missing_onnx_runtime,
            missing_geonames_db: h.missing_geonames_db,
            summary,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppVersionDto {
    pub version: String,
}

// ---------- settings ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettingsDto {
    pub theme: String,
    pub thumbnail_size: u32,
    pub face_detection_confidence: f32,
    pub face_clustering_threshold: f32,
    pub burst_time_window_seconds: i64,
    pub trash_auto_delete_days: u32,
    pub scan_hidden_folders: bool,
    pub show_timeline_stacks: bool,
    pub date_format: String,
    pub remembered_drives: Vec<String>,
    pub map_cache_limit_mb: u32,
    pub memories_enabled: bool,
    pub home_city_override: Option<String>,
    pub auto_update_check_enabled: bool,
    pub sidebar_collapsed: bool,
    pub thumbnail_cache_gb: f64,
    pub face_gpu_bridge_url: Option<String>,
    pub face_gpu_bridge_enabled: bool,
    pub face_embedder_model: String,
}

impl From<&AppConfig> for SettingsDto {
    fn from(c: &AppConfig) -> Self {
        let theme = match c.theme {
            smriti::config::AppTheme::Dark => "dark",
            smriti::config::AppTheme::Light => "light",
            smriti::config::AppTheme::System => "system",
        }
        .to_string();
        let date_format = match c.date_format {
            smriti::config::DateFormat::Locale => "locale",
            smriti::config::DateFormat::Iso => "iso",
            smriti::config::DateFormat::Us => "us",
            smriti::config::DateFormat::Eu => "eu",
        }
        .to_string();
        Self {
            theme,
            thumbnail_size: c.thumbnail_size,
            face_detection_confidence: c.face_detection_confidence,
            face_clustering_threshold: c.face_clustering_threshold,
            burst_time_window_seconds: c.burst_time_window_seconds,
            trash_auto_delete_days: c.trash_auto_delete_days,
            scan_hidden_folders: c.scan_hidden_folders,
            show_timeline_stacks: c.show_timeline_stacks,
            date_format,
            remembered_drives: c
                .remembered_drives
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            map_cache_limit_mb: c.map_cache_limit_mb,
            memories_enabled: c.memories_enabled,
            home_city_override: c.home_city_override.clone(),
            auto_update_check_enabled: c.auto_update_check_enabled,
            sidebar_collapsed: c.sidebar_collapsed,
            thumbnail_cache_gb: c.thumbnail_cache_gb,
            face_gpu_bridge_url: c.face_gpu_bridge_url.clone(),
            face_gpu_bridge_enabled: c.face_gpu_bridge_enabled,
            face_embedder_model: c.face_embedder_model.clone(),
        }
    }
}

// ---------- index changes (reindexer preview) ----------

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct IndexChangesDto {
    pub added: u64,
    pub removed: u64,
    pub moved: u64,
    pub modified: u64,
}

// ---------- metadata extraction progress ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MetadataProgressDto {
    pub job_id: String,
    pub total: u64,
    pub done: u64,
    pub elapsed_ms: u64,
    pub is_complete: bool,
}

/// Per-chunk update emitted by the thumbnail worker so the Timeline can
/// refresh only the affected cells in place (no scroll reset, no
/// full-page refetch). `photo_ids` is the set of rows just upgraded
/// from `thumbnailed = FALSE` to `TRUE`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThumbnailReadyDto {
    pub photo_ids: Vec<i64>,
}

/// Count of photos in a particular pre-processed state — generic shape
/// reused by the metadata and thumbnail resume banners on Timeline.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingCountDto {
    pub pending_photos: i64,
}

// ---------- helper: parse date_taken column ----------
#[allow(dead_code)]
pub(crate) fn rfc3339(d: DateTime<Utc>) -> String {
    d.to_rfc3339()
}
