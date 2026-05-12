import { call } from "./index";
import type {
  DriveDto,
  JobIdDto,
  LibraryHandleDto,
  LibraryOpenResult,
} from "./types";

export const library = {
  listDrives: () => call<DriveDto[]>("library_list_drives"),
  current: () => call<LibraryHandleDto | null>("library_current"),
  open: (drivePath: string) =>
    call<LibraryOpenResult>("library_open", { drive_path: drivePath }),
  close: () => call<null>("library_close"),
  startScan: (scanHidden = false) =>
    call<JobIdDto>("library_start_scan", { scan_hidden_folders: scanHidden }),
  cancelScan: (jobId: string) =>
    call<null>("library_cancel_scan", { job_id: jobId }),
  resolvePath: (photoId: number) =>
    call<{ absolute_path: string }>("library_resolve_path", {
      photo_id: photoId,
    }),

  // Streaming-scanner pipeline stages. After the initial walk inserts
  // stub rows, two background passes fill the rest in: EXIF/geocoding
  // and thumbnails. Both are pause/resume-able and persist progress
  // via DB stage flags (`metadata_extracted`, `thumbnailed`).
  startMetadataExtraction: () =>
    call<JobIdDto>("library_start_metadata_extraction"),
  startThumbnailPass: () => call<JobIdDto>("library_start_thumbnail_pass"),

  // Counts that drive the resume banners on Timeline. Each returns
  // `{ pending_photos: <i64> }`.
  pendingMetadataCount: () =>
    call<{ pending_photos: number }>("library_pending_metadata_count"),
  pendingThumbnailCount: () =>
    call<{ pending_photos: number }>("library_pending_thumbnail_count"),
};
