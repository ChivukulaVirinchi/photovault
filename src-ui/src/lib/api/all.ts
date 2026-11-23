// Consolidated API surface for the remaining domains.
// Per-domain files (library.ts, photos.ts, system.ts, events.ts) stay
// for the most-used endpoints; everything else lives here so M3 can ship
// without 13 tiny files.

import { invoke } from "@tauri-apps/api/core";
import { call } from "./index";
import { dataRevision } from "../stores/dataRevision.svelte";
import type {
  AlbumDto,
  AlbumSuggestionDto,
  PersonDto,
  PhotoSummaryDto,
  Page,
  JobIdDto,
} from "./types";

const mutate = async <T>(request: Promise<T>): Promise<T> => {
  const value = await request;
  dataRevision.bump();
  return value;
};

// ---------- people ----------
/** What `people.unmerge` needs to undo a merge. */
export interface MergeUndo {
  source_id: number;
  target_id: number;
  source_name: string | null;
  source_user_named: boolean;
  face_ids: number[];
  target_took_name: boolean;
}

export const people = {
  list: (args: { namedOnly?: boolean; minPhotos?: number } = {}) =>
    call<PersonDto[]>("people_list", {
      named_only: args.namedOnly ?? false,
      min_photos: args.minPhotos ?? null,
    }),
  get: (id: number) => call<PersonDto>("people_get", { id }),
  photoIds: (id: number) => call<number[]>("people_photo_ids", { id }),
  rename: (id: number, name: string | null) =>
    mutate(call<PersonDto>("people_rename", { id, name })),
  merge: (sourceId: number, targetId: number) =>
    mutate(call<{ person: PersonDto; undo: MergeUndo }>("people_merge", {
      source_id: sourceId,
      target_id: targetId,
    })),
  /// Undo a merge; the two are then never merged again.
  unmerge: (undo: MergeUndo) => mutate(call<null>("people_unmerge", undo)),
  /// Delete a cluster — "not a real person." Its faces are never regrouped.
  delete: (id: number) => mutate(call<null>("people_delete", { id })),
  /// Whether this person is kept out of Memories; pass `hidden` to change it.
  memoriesHidden: (id: number, hidden?: boolean) =>
    mutate(call<boolean>("people_memories_hidden", { id, hidden: hidden ?? null })),
  startProcessing: (librarySessionId?: number) => call<JobIdDto>("people_start_processing", { library_session_id: librarySessionId }),
  resetAll: () =>
    mutate(call<{
      photos_reflagged: number;
      clusters_dropped: number;
      faces_dropped: number;
    }>("people_reset_all")),
  resetClusters: () =>
    mutate(call<{
      photos_reflagged: number;
      clusters_dropped: number;
      faces_dropped: number;
    }>("people_reset_clusters")),
  photosByPerson: (personId: number, cursor: string | null = null, limit = 200) =>
    call<Page<PhotoSummaryDto>>("photos_list_by_person", {
      person_id: personId,
      cursor,
      limit,
    }),
  /// Photos that haven't yet had face detection — drives the resume banner.
  pendingFaceCount: () =>
    call<{ pending_photos: number }>("people_pending_face_count"),
  faceList: (
    personId: number,
    status: string,
    cursor?: string | null,
    limit?: number,
  ) =>
    call<Page<FaceDetailDto>>("people_face_list", {
      person_id: personId,
      status,
      cursor: cursor ?? null,
      limit: limit ?? 100,
    }),
  unclusteredFaces: (cursor?: string | null, limit?: number, recent = false) =>
    call<Page<FaceDetailDto>>("people_unclustered_faces", {
      recent,
      cursor: cursor ?? null,
      limit: limit ?? 24,
    }),
  // Face decisions return a token `faceUndo` takes back within the session.
  faceConfirm: (faceId: number) =>
    mutate(call<string>("people_face_confirm", { face_id: faceId })),
  faceConfirmToCluster: (faceId: number, clusterId: number) =>
    mutate(call<string>("people_face_confirm_to_cluster", {
      face_id: faceId,
      cluster_id: clusterId,
    })),
  /// "Not a face": the detection is set aside for good.
  faceIgnore: (faceId: number) =>
    mutate(call<string>("people_face_ignore", { face_id: faceId })),
  /// "Not sure" in review: the face stops coming up.
  faceSkip: (faceId: number) => call<string>("people_face_skip", { face_id: faceId }),
  faceReject: (faceId: number, notClusterId?: number) =>
    mutate(call<string>("people_face_reject", {
      face_id: faceId,
      not_cluster_id: notClusterId ?? null,
    })),
  faceReassign: (faceId: number, targetClusterId: number) =>
    mutate(call<string>("people_face_reassign", {
      face_id: faceId,
      target_cluster_id: targetClusterId,
    })),
  /// Take back a face decision; refused if the face changed since.
  faceUndo: (token: string) => mutate(call<null>("people_face_undo", { token })),
  faceSuggestClusters: (faceId: number, topK?: number) =>
    call<ClusterSuggestionDto[]>("people_face_suggest_clusters", {
      face_id: faceId,
      top_k: topK ?? 5,
    }),
  kSimilarToCluster: (clusterId: number, k?: number) =>
    call<FaceDetailDto[]>("people_k_similar_to_cluster", {
      cluster_id: clusterId,
      k: k ?? 20,
    }),
  reviewFaceCount: () =>
    call<ReviewFaceCountDto>("people_review_face_count"),
  nextUnconfirmedFaces: (limit = 200, excludedFaceIds: number[] = []) =>
    call<Page<FaceDetailDto>>("people_next_unconfirmed_faces", {
      limit,
      excluded_face_ids: excludedFaceIds,
    }),
};

export interface FaceDetailDto {
  face_id: number;
  photo_id: number;
  cluster_id: number | null;
  cluster_name: string | null;
  confidence: number;
  user_confirmed: number;
  thumbnail_path: string | null;
}

export interface ClusterSuggestionDto {
  cluster_id: number;
  name: string;
  score: number;
  face_count: number;
  representative_face_id: number | null;
}

export interface ReviewFaceCountDto {
  unconfirmed_total: number;
  clusters_with_unconfirmed: number;
}

// ---------- albums ----------
export const albums = {
  list: () => call<AlbumDto[]>("albums_list"),
  get: (id: number) => call<AlbumDto>("albums_get", { id }),
  photoIds: (id: number) => call<number[]>("albums_photo_ids", { id }),
  create: (name: string, photoIds: number[] = []) =>
    mutate(call<AlbumDto>("albums_create", { name, photo_ids: photoIds })),
  rename: (id: number, name: string) =>
    mutate(call<AlbumDto>("albums_rename", { id, name })),
  delete: (id: number) => mutate(call<null>("albums_delete", { id })),
  addPhotos: (id: number, photoIds: number[]) =>
    mutate(call<{ count: number }>("albums_add_photos", {
      id,
      photo_ids: photoIds,
    })),
  removePhotos: (id: number, photoIds: number[]) =>
    mutate(call<{ count: number }>("albums_remove_photos", {
      id,
      photo_ids: photoIds,
    })),
  export: (albumId: number, destinationDir: string | null = null, folderName: string | null = null) =>
    call<JobIdDto>("albums_export", {
      album_id: albumId,
      destination_dir: destinationDir,
      folder_name: folderName,
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
      call<JobIdDto>("albums_suggestions_run_detection", {
        home_city_override: homeCity ?? null,
      }),
    preview: (id: number, limit = 60) =>
      call<PhotoSummaryDto[]>("albums_suggestions_preview", { id, limit }),
    accept: (id: number, name?: string) =>
      mutate(call<AlbumDto>("albums_suggestions_accept", {
        id,
        name: name ?? null,
      })),
    dismiss: (id: number) =>
      mutate(call<null>("albums_suggestions_dismiss", { id })),
    resetAll: () =>
      mutate(call<{ dropped: number }>("albums_suggestions_reset_all")),
  },
};

// ---------- assistant ----------
export type AssistantRunStatus =
  | "running"
  | "waiting_for_approval"
  | "waiting_for_clarification"
  | "results_ready"
  | "completed"
  | "stopped"
  | "failed";

export interface AssistantActivity {
  label: string;
}

export interface AssistantPhotoSample {
  id: number;
  thumbnail_path: string | null;
  date_taken: string | null;
}

export interface AssistantAlbumPreview {
  approval_id: string;
  album_name: string;
  photo_count: number;
  sample: AssistantPhotoSample[];
  people: Array<{ id: number; name: string }>;
  places: Array<{ city: string | null; country: string | null; label: string }>;
  date: { start: string; end: string; label: string } | null;
  media_type: string | null;
  people_only: boolean;
  semantic_text: string | null;
  intent: "create_album" | "search";
}

export interface AssistantRun {
  run_id: string;
  library_root: string;
  status: AssistantRunStatus;
  message: string;
  response: string | null;
  clarification_options: string[];
  activity: AssistantActivity[];
  preview: AssistantAlbumPreview | null;
  album_id: number | null;
}

export const assistant = {
  start: (message: string, runId?: string) => call<AssistantRun>("assistant_start", { message, run_id: runId ?? null }),
  continueRun: (runId: string, message: string) =>
    call<AssistantRun>("assistant_continue", { run_id: runId, message }),
  state: (runId: string) => call<AssistantRun>("assistant_state", { run_id: runId }),
  stop: (runId: string) => call<AssistantRun>("assistant_stop", { run_id: runId }),
  approve: (runId: string, approvalId: string) =>
    call<AssistantRun>("assistant_approve", { run_id: runId, approval_id: approvalId }),
  reject: (runId: string, approvalId: string) =>
    call<AssistantRun>("assistant_reject", { run_id: runId, approval_id: approvalId }),
  clear: () => call<null>("assistant_clear"),
};

// ---------- search ----------
export interface SearchResults {
  interpreted: Array<{
    kind: string;
    label: string;
  }>;
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
    cover_photo_id: number | null;
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
  has_more: boolean;
}

export const search = {
  query: (q: string, offset = 0, limit = 200) =>
    call<SearchResults>("search_query", { q, offset, limit }),
  /// Remember a search the user settled on (Enter, or opening a result).
  remember: (q: string) => call<null>("search_remember", { q }),
  /// The latest remembered searches, newest first.
  recent: () => call<string[]>("search_recent"),
  clearRecent: () => call<null>("search_recent_clear"),
};

// ---------- semantic search ----------
export interface SemanticStatus {
  model_key: string;
  display_name: string;
  model_dir: string;
  assets_installed: boolean;
  onnx_runtime_installed: boolean;
  indexed_photos: number;
  pending_photos: number;
  failed_photos: number;
  vector_bytes: number;
}

export const semantic = {
  status: () => call<SemanticStatus>("semantic_status"),
  startIndexing: () => call<JobIdDto>("semantic_start_indexing"),
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
  saveAsAlbum: (memoryId: string, name?: string) =>
    call<AlbumDto>("memories_save_as_album", {
      memory_id: memoryId,
      name: name ?? null,
    }),
  /** Hide a memory for good; returns the key `unhide` takes. */
  hide: (memoryId: string) => call<string>("memories_hide", { memory_id: memoryId }),
  unhide: (hideKey: string) => call<void>("memories_unhide", { hide_key: hideKey }),
};

// ---------- duplicates ----------
export interface DupGroupSummary {
  id: number;
  member_count: number;
  cover_thumbnail_path: string | null;
  cover_photo_id: number | null;
  member_photo_ids: number[];
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
  list: (limit = 200, offset = 0) =>
    call<DupGroupSummary[]>("duplicates_list", { limit, offset }),
  getGroup: (id: number) =>
    call<{ id: number; members: DupMember[] }>("duplicates_get_group", { id }),
  wastedSpace: () => call<{ bytes: number }>("duplicates_wasted_space"),
  setKeep: (groupId: number, photoId: number, librarySessionId: number) =>
    call<{ id: number; members: DupMember[] }>("duplicates_set_keep", {
      group_id: groupId,
      photo_id: photoId,
      library_session_id: librarySessionId,
    }),
  trashOthers: (groupId: number, librarySessionId: number) =>
    call<{ count: number }>("duplicates_trash_others", { group_id: groupId, library_session_id: librarySessionId }),
  dismiss: (groupId: number, librarySessionId: number) =>
    call<null>("duplicates_dismiss", { group_id: groupId, library_session_id: librarySessionId }),
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
  cover_thumbnail_paths: string[];
  cover_photo_ids: number[];
  member_photo_ids: number[];
}
export interface BurstMember {
  photo_id: number;
  sharpness_score: number | null;
  blur_score: number | null;
  is_suggested_best: boolean;
}
export const bursts = {
  list: (limit = 200, offset = 0) =>
    call<BurstGroupSummary[]>("bursts_list", { limit, offset }),
  getGroup: (id: number) =>
    call<{ id: number; members: BurstMember[] }>("bursts_get_group", { id }),
  setBest: (groupId: number, photoId: number, librarySessionId: number) =>
    call<{ id: number; members: BurstMember[] }>("bursts_set_best", {
      group_id: groupId,
      photo_id: photoId,
      library_session_id: librarySessionId,
    }),
  trashNonBest: (groupId: number, librarySessionId: number) =>
    call<{ count: number }>("bursts_trash_non_best", { group_id: groupId, library_session_id: librarySessionId }),
  dismiss: (groupId: number, librarySessionId: number) =>
    call<null>("bursts_dismiss", { group_id: groupId, library_session_id: librarySessionId }),
  run: () => call<JobIdDto>("bursts_run"),
};

// ---------- stacks ----------
export interface PhotoStackMember {
  photo_id: number;
  thumbnail_path: string | null;
  date_taken: string | null;
  quality_score: number;
  score_reasons: string | null;
  is_cover: boolean;
}
export interface PhotoStack {
  id: number;
  kind: string;
  source_group_id: number;
  cover_photo_id: number;
  member_count: number;
  confidence: number;
  members: PhotoStackMember[];
}
export const stacks = {
  get: (id: number) => call<PhotoStack>("stacks_get", { id }),
  getForPhoto: (photoId: number) =>
    call<PhotoStack | null>("stacks_get_for_photo", { photo_id: photoId }),
  setCover: (stackId: number, photoId: number) =>
    call<PhotoStack>("stacks_set_cover", { stack_id: stackId, photo_id: photoId }),
  removeMember: (stackId: number, photoId: number) =>
    call<PhotoStack | null>("stacks_remove_member", { stack_id: stackId, photo_id: photoId }),
  unstack: (id: number) => call<null>("stacks_unstack", { id }),
  trashOthers: (id: number) => call<{ count: number }>("stacks_trash_others", { id }),
  refresh: () => call<{ stacks_found: number }>("stacks_refresh"),
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
  trashPhotos: (photoIds: number[], librarySessionId: number) =>
    call<{ count: number }>("trash_trash_photos", { photo_ids: photoIds, library_session_id: librarySessionId }),
  restore: (photoIds: number[], librarySessionId: number) =>
    call<{ count: number }>("trash_restore", { photo_ids: photoIds, library_session_id: librarySessionId }),
  permanentDelete: (photoIds: number[], librarySessionId?: number) =>
    call<{ files_deleted: number; db_records_deleted: number; errors: string[]; committed_with_recovery_errors: boolean }>(
      "trash_permanent_delete",
      { photo_ids: photoIds, library_session_id: librarySessionId ?? null },
    ),
  empty: (librarySessionId?: number) =>
    call<{ files_deleted: number; db_records_deleted: number; errors: string[]; committed_with_recovery_errors: boolean }>(
      "trash_empty",
      { library_session_id: librarySessionId ?? null },
    ),
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
  /// "YYYY-MM" -> count, across every year in the library.
  months_by_year: Record<string, number>;
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
  /// Cheap threshold probe — see lib/stores/milestones.svelte.ts.
  milestoneProbe: () => call<MilestoneProbe>("insights_milestone_probe"),
};

/// The handful of counters the milestone thresholds read.
export interface MilestoneProbe {
  total_photos: number;
  first_year: number | null;
  last_year: number | null;
  people_count: number;
  city_count: number;
}

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
  scan_on_open: boolean;
  show_timeline_stacks: boolean;
  date_format: string;
  remembered_drives: string[];
  map_cache_limit_mb: number;
  memories_enabled: boolean;
  home_city_override: string | null;
  auto_update_check_enabled: boolean;
  sidebar_collapsed: boolean;
  thumbnail_cache_gb: number;
  face_gpu_bridge_url: string | null;
  face_gpu_bridge_enabled: boolean;
  face_embedder_model: string;
  assistant_enabled: boolean;
  ai_features_enabled: boolean;
  assistant_provider: "local" | "openai_compatible";
  assistant_base_url: string;
  assistant_model: string;
  assistant_api_key_set: boolean;
  assistant_api_key?: string | null;
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
  /// Walk every GPS-tagged photo and resolve a place name. Default
  /// fills only NULL rows; `forceRefresh = true` re-resolves all and
  /// clears entries that no longer match (used to flush stale data
  /// after geocoder rules change).
  backfill: (forceRefresh = false, librarySessionId?: number) =>
    call<JobIdDto>("geocoding_backfill", { force_refresh: forceRefresh, library_session_id: librarySessionId }),
};

// ---------- system ----------
export interface AssetInventory {
  install_root: string;
  roots: string[];
  total_size_bytes: number;
  assets: AssetItem[];
}

export interface AssetItem {
  id: string;
  label: string;
  kind: string;
  status: "active" | "extra" | "missing" | "planned";
  required: boolean;
  active: boolean;
  installable: boolean;
  removable: boolean;
  size_bytes: number | null;
  path: string | null;
  note: string | null;
}

export interface UpdateStatus {
  current: string;
  latest: string | null;
  newer_available: boolean;
  release_url: string | null;
  body: string | null;
  /** What "Update" does on this install. */
  install: "installer" | "appimage" | "dmg" | "package_manager" | "release_page";
  upgrade_cmd: string | null;
  /** `deb` or `rpm` for Linux package installs. */
  package_format: string | null;
}

export const systemEx = {
  assetsInventory: () => call<AssetInventory>("system_assets_inventory"),
    assetSetupStatus: () => call<{ faces: boolean; visual: boolean; places: boolean }>("system_asset_setup_status"),
    assetDownloadSize: (feature: "all" | "faces" | "visual" | "places") => call<number>("system_asset_download_size", { feature }),
    installAssets: (feature: "all" | "faces" | "visual" | "places" = "all") => call<JobIdDto>("system_install_assets", { feature }),
  openInExplorer: (photoId: number) =>
    call<null>("system_open_in_explorer", { photo_id: photoId }),
  openPath: (path: string) => call<null>("system_open_path", { path }),
  updatesCheck: () => call<UpdateStatus>("system_updates_check"),
  updateInstall: () =>
    call<{ outcome: "installer_launched" | "restart_required" | "release_page" | "package_manager" }>(
      "system_update_install",
    ),
  restart: () => call<void>("system_restart"),
  testGpuBridge: (url: string) =>
    call<{ ok: boolean; latency_ms: number; gpu_name: string; model: string | null }>(
      "system_test_gpu_bridge",
      { url },
    ),
};

export interface CollageLocation {
  path: string;
  label: string;
}

export interface CollageFaceBox { x: number; y: number; width: number; height: number }

export interface CollageSuggestionPhoto {
  id: number;
  thumbnail_path: string | null;
  width: number | null;
  height: number | null;
  orientation: number;
  faces: CollageFaceBox[];
}

/** A collage suggested on the Memories page. Nothing about it is stored. */
export interface CollageSuggestion {
  key: string;
  eyebrow: string;
  title: string;
  /** Feature photo first. */
  photos: CollageSuggestionPhoto[];
}

export const collageApi = {
  /** The best few photos of an album (`-1` is Favourites) or of a set of
   *  photos, feature photo first. */
  pick: (source: { albumId?: number; photoIds?: number[] }) =>
    call<number[]>("collage_pick", { album_id: source.albumId ?? null, photo_ids: source.photoIds ?? [] }),
  /** Today's suggested collages. */
  suggestions: () => call<CollageSuggestion[]>("collage_suggestions"),
  /** Stop suggesting collages from this suggestion's source. */
  dismissSuggestion: (key: string) => call<void>("collage_dismiss_suggestion", { key }),
  /** Face boxes per photo id, normalised to the displayed (rotated) photo. */
  faceBoxes: (photoIds: number[]) =>
    call<Record<string, CollageFaceBox[]>>("collage_face_boxes", {
      photo_ids: photoIds,
    }),
  /** Where Save puts collages (Pictures › Smriti Collages). */
  location: () => invoke<CollageLocation>("collage_location"),
  /** Writes the JPEG into the collages folder under a new name. */
  save: (jpeg: Uint8Array) => invoke<{ path: string; location: CollageLocation }>("collage_save", jpeg),
  reveal: (path: string) => invoke<void>("collage_reveal", { args: { path } }),
};
