// Wire-format DTOs — must mirror src-tauri/src/dto.rs.
// Frontend code treats keys as opaque (snake_case from serde).

export interface Page<T> {
  items: T[];
  next_cursor: string | null;
  has_more: boolean;
  total: number | null;
}

export interface DriveDto {
  name: string;
  path: string;
  is_removable: boolean;
  has_photovault_db: boolean;
  total_size_bytes: number | null;
  free_space_bytes: number | null;
}

export interface LibraryHandleDto {
  drive_root: string;
  photo_count: number;
  read_only: boolean;
  schema_too_new: SchemaTooNewInfo | null;
}

export interface LibraryOpenResult {
  drive_root: string;
  photo_count: number;
  first_run: boolean;
  read_only: boolean;
  schema_too_new: SchemaTooNewInfo | null;
}

export interface SchemaTooNewInfo {
  db_version: number;
  max_supported: number;
}

export interface ExcludedFolderDto {
  relative_path: string;
  indexed_count: number;
  created_at: string;
}

export interface ExcludedFolderPreviewDto {
  relative_path: string;
  indexed_count: number;
}

export interface PhotoSummaryDto {
  id: number;
  thumbnail_path: string | null;
  date_taken: string | null;
  width: number | null;
  height: number | null;
  orientation: number;
  media_type: "photo" | "video";
  duration_ms: number | null;
  is_favorite: boolean;
  is_trashed: boolean;
  stack: PhotoStackBadgeDto | null;
}

export interface PhotoStackBadgeDto {
  id: number;
  kind: string;
  member_count: number;
  cover_photo_id: number;
}

export interface PhotoDto extends PhotoSummaryDto {
  file_path: string;
  file_name: string;
  file_size: number;
  file_hash: string;
  gps: { lat: number; lng: number; altitude: number | null } | null;
  location: { city: string | null; country: string | null } | null;
  camera: {
    make: string | null;
    model: string | null;
    lens: string | null;
    iso: number | null;
    aperture: string | null;
    shutter_speed: string | null;
    focal_length: string | null;
    flash: string | null;
  } | null;
  video: {
    video_codec: string | null;
    audio_codec: string | null;
    frame_rate: number | null;
    bitrate: number | null;
    has_audio: boolean;
  } | null;
  content_category: string;
  ocr: { text: string; confidence: number } | null;
  faces_processed: boolean;
  indexed_at: string;
}

export interface AssetHealthDto {
  missing_face_models: boolean;
  missing_onnx_runtime: boolean;
  missing_geonames_db: boolean;
  summary: string;
}

export interface AppVersionDto {
  version: string;
}

export interface JobIdDto {
  job_id: string;
}

export interface PersonDto {
  id: number;
  name: string | null;
  photo_count: number;
  face_count: number | null;
  representative_thumbnail_path: string | null;
}

export interface AlbumDto {
  id: number;
  name: string;
  photo_count: number;
  photos_added?: number | null;
  date_range_start: string | null;
  date_range_end: string | null;
  cover_photo_id: number | null;
  cover_thumbnail_path: string | null;
  is_virtual: boolean;
  created_by: "user" | "agent";
}

export interface AlbumSuggestionDto {
  id: number;
  kind: string;
  title: string;
  photo_ids: number[];
  cover_photo_id: number | null;
  cover_thumbnail_path: string | null;
}

export type CommandError =
  | { kind: "not_found"; entity: string; id: string }
  | { kind: "validation"; field: string; reason: string }
  | { kind: "library_closed" }
  | { kind: "drive_not_mounted"; path: string }
  | { kind: "ml_unavailable"; reason: string }
  | { kind: "conflict"; reason: string }
  | { kind: "cancelled" }
  | { kind: "database"; message: string }
  | { kind: "schema_too_new"; db_version: number; max_supported: number }
  | { kind: "io"; message: string }
  | { kind: "network"; message: string }
  | { kind: "internal"; message: string };
