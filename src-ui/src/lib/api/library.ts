import { call } from "./index";
import type {
  DriveDto,
  ExcludedFolderDto,
  ExcludedFolderPreviewDto,
  JobIdDto,
  LibraryHandleDto,
  LibraryOpenResult,
  Page,
  PhotoSummaryDto,
} from "./types";

export const library = {
  listDrives: () => call<DriveDto[]>("library_list_drives"),
  current: () => call<LibraryHandleDto | null>("library_current"),
  open: (drivePath: string) =>
    call<LibraryOpenResult>("library_open", { drive_path: drivePath }),
  compatPhotos: (offset: number, limit = 100) =>
    call<Page<PhotoSummaryDto>>("library_compat_photos_list", { offset, limit }),
  close: () => call<null>("library_close"),
  startScan: (scanHidden = false) =>
    call<JobIdDto>("library_start_scan", { scan_hidden_folders: scanHidden }),
  cancelScan: (jobId: string) =>
    call<null>("library_cancel_scan", { job_id: jobId }),
  resolvePath: (photoId: number) =>
    call<{ absolute_path: string }>("library_resolve_path", {
      photo_id: photoId,
    }),
  exclusions: {
    list: () => call<ExcludedFolderDto[]>("library_exclusions_list"),
    preview: (path: string) =>
      call<ExcludedFolderPreviewDto>("library_exclusions_preview", { path }),
    add: (path: string) =>
      call<ExcludedFolderDto>("library_exclusions_add", { path }),
    remove: (relativePath: string) =>
      call<null>("library_exclusions_remove", { relative_path: relativePath }),
  },

  // Streaming-scanner pipeline stages. After the initial walk inserts
  // stub rows, two background passes fill the rest in: EXIF/geocoding
  // and thumbnails. Both are pause/resume-able and persist progress
  // via DB stage flags (`metadata_extracted`, `thumbnailed`).
  startMetadataExtraction: () =>
    call<JobIdDto>("library_start_metadata_extraction"),
  refreshPhotoDates: () => call<JobIdDto>("library_refresh_photo_dates"),
  startThumbnailPass: () => call<JobIdDto>("library_start_thumbnail_pass"),

  // Counts that drive the resume banners on Timeline. Each returns
  // `{ pending_photos: <i64> }`.
  pendingMetadataCount: () =>
    call<{ pending_photos: number }>("library_pending_metadata_count"),
  pendingThumbnailCount: () =>
    call<{ pending_photos: number }>("library_pending_thumbnail_count"),

  /// Wipe every photo's thumbnail_path + thumbnailed flag, then run
  /// the thumbnail pass over the whole library. Used to upgrade
  /// legacy small thumbnails to the current size after the default
  /// changed. Long-running on big libraries — surfaces a thumbnail
  /// job in the global indicator. The `args` payload is required by
  /// Tauri (the command takes a struct param even though every field
  /// in it is optional).
  regenerateThumbnails: () =>
    call<JobIdDto>("library_regenerate_thumbnails", { photo_ids: null }),
  importGoogleTakeout: (archivePaths: string[]) =>
    call<JobIdDto>("takeout_start_import", { archive_paths: archivePaths }),
};
