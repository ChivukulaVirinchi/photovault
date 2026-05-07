// Consolidated API surface for the remaining domains.
// Per-domain files (library.ts, photos.ts, system.ts, events.ts) stay
// for the most-used endpoints; everything else lives here so M3 can ship
// without 13 tiny files.

import { call } from "./index";
import type {
  AlbumDto,
  AlbumSuggestionDto,
  PersonDto,
  PhotoSummaryDto,
  Page,
  JobIdDto,
} from "./types";

// ---------- people ----------
export const people = {
  list: (args: { namedOnly?: boolean; minPhotos?: number } = {}) =>
    call<PersonDto[]>("people_list", {
      named_only: args.namedOnly ?? false,
      min_photos: args.minPhotos ?? null,
    }),
  get: (id: number) => call<PersonDto>("people_get", { id }),
  rename: (id: number, name: string | null) =>
    call<PersonDto>("people_rename", { id, name }),
  merge: (sourceId: number, targetId: number) =>
    call<PersonDto>("people_merge", {
      source_id: sourceId,
      target_id: targetId,
    }),
  /// Delete a cluster — "not a real person." Faces become unclustered.
  delete: (id: number) => call<null>("people_delete", { id }),
  reviewQueue: (limit = 20) =>
    call<ReviewItem[]>("people_review_queue", { limit }),
  reviewSame: (queueId: number) =>
    call<null>("people_review_same", { queue_id: queueId }),
  reviewDifferent: (queueId: number) =>
    call<null>("people_review_different", { queue_id: queueId }),
  reviewSkip: (queueId: number) =>
    call<null>("people_review_skip", { queue_id: queueId }),
  startProcessing: () => call<JobIdDto>("people_start_processing"),
  cancelProcessing: (jobId: string) =>
    call<null>("people_cancel_processing", { job_id: jobId }),
  photosByPerson: (personId: number, cursor: string | null = null, limit = 200) =>
    call<Page<PhotoSummaryDto>>("photos_list_by_person", {
      person_id: personId,
      cursor,
      limit,
    }),
};

export interface ReviewItem {
  queue_id: number;
  face_id: number;
  candidate_cluster_id: number;
  candidate_cluster_name: string | null;
  candidate_cluster_size: number;
  candidate_sample_face_ids: number[];
  score: number;
}

// ---------- albums ----------
export const albums = {
  list: () => call<AlbumDto[]>("albums_list"),
  get: (id: number) => call<AlbumDto>("albums_get", { id }),
  create: (name: string, photoIds: number[] = []) =>
    call<AlbumDto>("albums_create", { name, photo_ids: photoIds }),
  rename: (id: number, name: string) =>
    call<AlbumDto>("albums_rename", { id, name }),
  delete: (id: number) => call<null>("albums_delete", { id }),
  addPhotos: (id: number, photoIds: number[]) =>
    call<{ count: number }>("albums_add_photos", {
      id,
      photo_ids: photoIds,
    }),
  removePhotos: (id: number, photoIds: number[]) =>
    call<{ count: number }>("albums_remove_photos", {
      id,
      photo_ids: photoIds,
    }),
  photos: (albumId: number, cursor: string | null = null, limit = 200) =>
    call<Page<PhotoSummaryDto>>("photos_list_by_album", {
      album_id: albumId,
      cursor,
      limit,
    }),
  suggestions: {
    list: () =>
      call<AlbumSuggestionDto[]>("albums_suggestions_list"),
    runDetection: (homeCity?: string) =>
      call<{
        total_photos_with_date: number;
        photos_with_city: number;
        home_city: string | null;
        trip_candidates_passed: number;
        event_windows: number;
        created: number;
      }>("albums_suggestions_run_detection", {
        home_city_override: homeCity ?? null,
      }),
    preview: (id: number, limit = 60) =>
      call<PhotoSummaryDto[]>("albums_suggestions_preview", { id, limit }),
    accept: (id: number, name?: string) =>
      call<AlbumDto>("albums_suggestions_accept", {
        id,
        name: name ?? null,
      }),
    dismiss: (id: number) =>
      call<null>("albums_suggestions_dismiss", { id }),
  },
};

// ---------- search ----------
export interface SearchResults {
  people: Array<{
    cluster_id: number;
    name: string;
    photo_count: number;
    face_thumbnail_path: string | null;
  }>;
  albums: Array<{
    album_id: number;
    name: string;
    photo_count: number;
    cover_thumbnail_path: string | null;
  }>;
  places: Array<{
    city: string;
    country: string | null;
    photo_count: number;
  }>;
  photo_ids: number[];
  photos: Array<{
    photo_id: number;
    date_taken: string | null;
    location_city: string | null;
    location_country: string | null;
    thumbnail_path: string | null;
  }>;
}

export const search = {
  query: (q: string) => call<SearchResults>("search_query", { q }),
  recentList: (limit = 10) =>
    call<Array<{ query: string; last_used: string; use_count: number }>>(
      "search_recent_list",
      { limit },
    ),
  recentRemove: (q: string) => call<null>("search_recent_remove", { q }),
  recentClear: () => call<null>("search_recent_clear"),
};

// ---------- memories ----------
export interface MemoryCard {
  id: string;
  kind: string;
  title: string;
  hero_photo_id: number;
  hero_thumbnail_path: string | null;
  photo_count: number;
  photo_ids: number[];
}

export const memories = {
  today: () => call<MemoryCard[]>("memories_today"),
  detail: (memoryId: string) =>
    call<{ card: MemoryCard; photos: PhotoSummaryDto[] }>("memories_detail", {
      memory_id: memoryId,
    }),
  blockedPeople: () => call<PersonDto[]>("memories_blocked_people"),
  blockPerson: (personId: number) =>
    call<null>("memories_block_person", { person_id: personId }),
  unblockPerson: (personId: number) =>
    call<null>("memories_unblock_person", { person_id: personId }),
  saveAsAlbum: (memoryId: string, name?: string) =>
    call<AlbumDto>("memories_save_as_album", {
      memory_id: memoryId,
      name: name ?? null,
    }),
};

// ---------- duplicates ----------
export interface DupGroupSummary {
  id: number;
  member_count: number;
  cover_thumbnail_path: string | null;
}
export interface DupMember {
  photo_id: number;
  is_suggested_keep: boolean;
  file_path: string | null;
  thumbnail_path: string | null;
  file_size: number | null;
  date_taken: string | null;
}
export const duplicates = {
  list: () => call<DupGroupSummary[]>("duplicates_list"),
  getGroup: (id: number) =>
    call<{ id: number; members: DupMember[] }>("duplicates_get_group", { id }),
  wastedSpace: () => call<{ bytes: number }>("duplicates_wasted_space"),
  setKeep: (groupId: number, photoId: number) =>
    call<{ id: number; members: DupMember[] }>("duplicates_set_keep", {
      group_id: groupId,
      photo_id: photoId,
    }),
  trashOthers: (groupId: number) =>
    call<{ count: number }>("duplicates_trash_others", { group_id: groupId }),
  dismiss: (groupId: number) =>
    call<null>("duplicates_dismiss", { group_id: groupId }),
  run: (includePerceptual = true) =>
    call<JobIdDto>("duplicates_run", {
      include_perceptual: includePerceptual,
    }),
};

// ---------- bursts ----------
export interface BurstGroupSummary {
  id: number;
  start_time: string;
  end_time: string;
  photo_count: number;
}
export interface BurstMember {
  photo_id: number;
  sharpness_score: number | null;
  blur_score: number | null;
  is_suggested_best: boolean;
}
export const bursts = {
  list: () => call<BurstGroupSummary[]>("bursts_list"),
  getGroup: (id: number) =>
    call<{ id: number; members: BurstMember[] }>("bursts_get_group", { id }),
  setBest: (groupId: number, photoId: number) =>
    call<{ id: number; members: BurstMember[] }>("bursts_set_best", {
      group_id: groupId,
      photo_id: photoId,
    }),
  trashNonBest: (groupId: number) =>
    call<{ count: number }>("bursts_trash_non_best", { group_id: groupId }),
  dismiss: (groupId: number) =>
    call<null>("bursts_dismiss", { group_id: groupId }),
  run: () => call<JobIdDto>("bursts_run"),
};

// ---------- trash ----------
export const trash = {
  list: (cursor: string | null = null, limit = 200) =>
    call<
      Page<{
        photo_id: number;
        original_path: string;
        trashed_at: string;
        file_size: number | null;
        thumbnail_path: string | null;
      }>
    >("trash_list", { cursor, limit }),
  stats: () =>
    call<{ count: number; total_size: number }>("trash_stats"),
  trashPhotos: (photoIds: number[]) =>
    call<{ count: number }>("trash_trash_photos", { photo_ids: photoIds }),
  restore: (photoIds: number[]) =>
    call<{ count: number }>("trash_restore", { photo_ids: photoIds }),
  permanentDelete: (photoIds: number[]) =>
    call<{ files_deleted: number; db_records_deleted: number; errors: string[] }>(
      "trash_permanent_delete",
      { photo_ids: photoIds },
    ),
  empty: () =>
    call<{ files_deleted: number; db_records_deleted: number; errors: string[] }>(
      "trash_empty",
    ),
};

// ---------- documents ----------
export const documents = {
  list: (categories: string[] | null = null, cursor: string | null = null, limit = 200) =>
    call<Page<PhotoSummaryDto>>("documents_list", {
      categories,
      cursor,
      limit,
    }),
  search: (q: string, cursor: string | null = null, limit = 200) =>
    call<Page<PhotoSummaryDto>>("documents_search", { q, cursor, limit }),
  setCategory: (photoId: number, category: string) =>
    call<null>("documents_set_category", {
      photo_id: photoId,
      category,
    }),
};

// ---------- insights ----------
export interface InsightsData {
  total_photos: number;
  date_range_start: string | null;
  date_range_end: string | null;
  people_count: number;
  album_count: number;
  country_count: number;
  city_count: number;
  photos_with_gps: number;
  hero_photo_id: number | null;
  hero_thumbnail_path: string | null;
  heatmap: Record<string, number>;
  heatmap_year: number;
  monthly_counts: number[];
  top_people: Array<{
    cluster_id: number;
    name: string;
    photo_count: number;
    face_id: number | null;
    face_crop_path: string | null;
  }>;
  top_locations: Array<{ city: string; country: string; photo_count: number }>;
  top_countries: Array<{ country: string; photo_count: number }>;
  top_cameras: Array<{ camera: string; photo_count: number }>;
  available_years: number[];
}
export const insights = {
  compute: (year: number | null = null) =>
    call<InsightsData>("insights_compute", { year }),
};

// ---------- library health ----------
export interface LibraryHealthData {
  total_photos: number;
  missing_thumbnails: number;
  inaccurate_dates: number;
  missing_dates: number;
  heic_count: number;
  heic_decoder_available: boolean;
  face_processed_no_faces: number;
}
export const health = {
  compute: () => call<LibraryHealthData>("health_compute"),
};

// ---------- settings ----------
export interface Settings {
  theme: string;
  thumbnail_size: number;
  face_detection_confidence: number;
  face_clustering_threshold: number;
  burst_time_window_seconds: number;
  trash_auto_delete_days: number;
  scan_hidden_folders: boolean;
  date_format: string;
  remembered_drives: string[];
  map_cache_limit_mb: number;
  memories_enabled: boolean;
  home_city_override: string | null;
  auto_update_check_enabled: boolean;
  sidebar_collapsed: boolean;
  thumbnail_cache_gb: number;
}
export const settings = {
  get: () => call<Settings>("settings_get"),
  update: (patch: Partial<Settings>) =>
    call<Settings>("settings_update", patch),
};

// ---------- map ----------
export interface MapPin {
  photo_id: number;
  lat: number;
  lng: number;
  thumbnail_path: string | null;
  count: number;
  /// Member photo ids when count > 1 (empty for single pins).
  photo_ids: number[];
}
export const map = {
  pins: (
    bounds: { north: number; south: number; east: number; west: number },
    zoom: number,
    maxPins?: number,
  ) =>
    call<MapPin[]>("map_pins", {
      bounds,
      zoom,
      max_pins: maxPins ?? null,
    }),
  /// Every geotagged photo. Used by the client-side clustering map.
  pinsAll: () => call<MapPin[]>("map_pins_all"),
  clusterFilmstrip: (photoIds: number[]) =>
    call<PhotoSummaryDto[]>("map_cluster_filmstrip", { photo_ids: photoIds }),
};

// ---------- geocoding ----------
export const geocoding = {
  resolveOne: (lat: number, lng: number) =>
    call<{ city: string | null; country: string | null } | null>(
      "geocoding_resolve_one",
      { lat, lng },
    ),
  /// Walk every GPS-tagged photo and resolve a place name. Default
  /// fills only NULL rows; `forceRefresh = true` re-resolves all and
  /// clears entries that no longer match (used to flush stale data
  /// after geocoder rules change).
  backfill: (forceRefresh = false) =>
    call<{
      considered: number;
      updated: number;
      cleared: number;
      geonames_db_present: boolean;
    }>("geocoding_backfill", { force_refresh: forceRefresh }),
};

// ---------- system ----------
export const systemEx = {
  openInExplorer: (photoId: number) =>
    call<null>("system_open_in_explorer", { photo_id: photoId }),
  copyPathToClipboard: (photoId: number) =>
    call<{ path: string }>("system_copy_path_to_clipboard", {
      photo_id: photoId,
    }),
  updatesCheck: () =>
    call<{
      current: string;
      latest: string | null;
      newer_available: boolean;
      release_url: string | null;
      body: string | null;
    }>("system_updates_check"),
};
