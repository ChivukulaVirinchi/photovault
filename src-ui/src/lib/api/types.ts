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
}

export interface LibraryOpenResult {
  drive_root: string;
  photo_count: number;
  first_run: boolean;
}

export interface PhotoSummaryDto {
  id: number;
  thumbnail_path: string | null;
  date_taken: string | null;
  width: number | null;
  height: number | null;
  orientation: number;
  is_trashed: boolean;
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

export type CommandError =
  | { kind: "not_found"; entity: string; id: string }
  | { kind: "validation"; field: string; reason: string }
  | { kind: "library_closed" }
  | { kind: "drive_not_mounted"; path: string }
  | { kind: "ml_unavailable"; reason: string }
  | { kind: "conflict"; reason: string }
  | { kind: "cancelled" }
  | { kind: "database"; message: string }
  | { kind: "io"; message: string }
  | { kind: "network"; message: string }
  | { kind: "internal"; message: string };
