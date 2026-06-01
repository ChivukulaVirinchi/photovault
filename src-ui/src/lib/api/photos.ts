import { call } from "./index";
import type { Page, PhotoDto, PhotoSummaryDto } from "./types";

export interface ExifExtras {
  software: string | null;
  exposure_bias: string | null;
  modified_at: string | null;
  created_at: string | null;
}

export interface ThumbnailResult {
  thumbnail_path: string | null;
}

export interface SaveVideoProbeArgs {
  id: number;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  poster_jpeg_base64: string | null;
}

export const photos = {
  list: (args: {
    cursor?: string | null;
    limit?: number;
    includeTrashed?: boolean;
  } = {}) =>
    call<Page<PhotoSummaryDto>>("photos_list", {
      cursor: args.cursor ?? null,
      limit: args.limit ?? null,
      include_trashed: args.includeTrashed ?? false,
    }),
  get: (id: number) => call<PhotoDto>("photos_get", { id }),
  getMany: (ids: number[]) => call<PhotoDto[]>("photos_get_many", { ids }),
  setFavorite: (id: number, isFavorite: boolean) =>
    call<PhotoDto>("photos_set_favorite", { id, is_favorite: isFavorite }),
  exifExtras: (id: number) => call<ExifExtras>("photos_exif_extras", { id }),
  /// Request on-demand generation for a photo whose thumbnail isn't on
  /// disk yet. Returns `{ thumbnail_path: null }` on failure (e.g. file
  /// missing, decode error). Concurrency is capped server-side at 8.
  requestThumbnail: (id: number) =>
    call<ThumbnailResult>("photos_request_thumbnail", { id }),
  saveVideoProbe: (args: SaveVideoProbeArgs) =>
    call<ThumbnailResult>("photos_save_video_probe", args),
  listByDate: (start: string, end: string, cursor: string | null = null, limit = 500) =>
    call<Page<PhotoSummaryDto>>("photos_list_by_date", {
      start,
      end,
      cursor,
      limit,
    }),
};
