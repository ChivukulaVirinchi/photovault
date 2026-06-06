# Smriti — Tauri Command Surface

> **Status:** active contract. The Tauri shell (`src-tauri/`) and the
> Svelte frontend (`src-ui/`) both implement against this document.
> The original iced UI was removed in 2026-05; this is now the only
> way the frontend talks to the engine.

---

## Context

Smriti's engine (`src/`) is a pure Rust library — services, DB, ML,
search. The Tauri 2 shell (`src-tauri/`) wraps it in
`#[tauri::command]` handlers, and the Svelte 5 frontend (`src-ui/`)
calls those handlers via Tauri IPC.

The command surface is the *only* thing the frontend sees. Refactoring
it means touching both sides simultaneously and breaking any external
library consumers (we want the engine to grow into a small ecosystem).
So we keep it deliberate, as if it were a public API — because
functionally it is.

This document is the contract: what commands exist, what they accept,
what they return, what they emit, and what errors they raise. Both
the Rust handler authors and the Svelte client authors implement
against this.

---

## Where this work lands

```
smriti/
├── docs/
│   └── COMMAND_SURFACE.md        ← this doc
├── src-tauri/                    ← thin Tauri handler crate
│   ├── Cargo.toml                ← depends on `smriti` as a path dep
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs               ← #[tauri::command] handlers (one file per domain)
│       ├── state.rs              ← AppState: db, services, channels
│       ├── error.rs              ← CommandError + From<AppError>
│       ├── dto.rs                ← serde-only DTOs (PhotoDto, PersonDto, ...)
│       ├── pagination.rs         ← Cursor encode/decode
│       ├── events.rs             ← Event channel constants + payloads
│       └── commands/
│           ├── library.rs
│           ├── photos.rs
│           ├── people.rs
│           ├── albums.rs
│           ├── search.rs
│           ├── memories.rs
│           ├── duplicates.rs
│           ├── bursts.rs
│           ├── stacks.rs
│           ├── trash.rs
│           ├── documents.rs
│           ├── map.rs
│           ├── insights.rs
│           ├── health.rs
│           ├── geocoding.rs
│           ├── settings.rs
│           └── system.rs
└── src/                          ← unchanged Rust core (the "library")
```

The migration is complete: `src/` is library-only (no `main.rs`, no UI), and the Tauri shell in `src-tauri/` is the single bin. The earlier iced UI (`src/views/`, `src/components/`, `src/app/`, `src/theme/`) was removed in 2026-05.

---

## Design principles

1. **Domains, not types.** Group commands by what the user does (`photos.list`, `people.merge`), not by struct (`PhotoRepo::*`). The command surface is a UX-level API.
2. **Narrow boundary.** A view does ≤3 commands per render. If a view needs 8 commands, it's the wrong granularity — collapse them server-side.
3. **Stable shapes.** DTOs are serde-only structs in `src-tauri/src/dto.rs`. They are *never* the same as DB rows or service-internal types — those are free to change without breaking the wire.
4. **Cursor pagination for unbounded lists** (timeline, search results, trash). Offset/limit only for inherently small lists (albums, clusters when count < 1k). The cursor is an opaque base64-encoded `(date_taken, id)` tuple.
5. **Long-running ops use Tauri events, not blocking commands.** `library.start_scan` returns a `job_id` immediately; `scan:progress` events stream until completion. The frontend never awaits a multi-minute promise.
6. **Errors are a tagged enum.** Every command returns `Result<T, CommandError>`. The enum is exhaustive — `Internal` exists but is reserved for genuine bugs. UI code matches on the tag.
7. **No raw blobs over IPC.** Embeddings (2048-byte BLOBs) never cross the boundary. Thumbnails are file paths the frontend reads via Tauri's `convertFileSrc` (custom protocol). File hashes go as hex strings.
8. **Versioned in name, not URL.** Today: command names without a version prefix. If we ever break: introduce `photos.list_v2` and migrate. We will not paint ourselves into a corner with `v1.photos.list`.
9. **Snake-case in Rust, dot-case as logical name.** `#[tauri::command]` defines `photos_list`; the TS client wraps it as `photos.list(...)`. Logical grouping is a frontend convenience.
10. **Idempotency where it matters.** `albums.add_photos` accepts duplicates silently. `trash.trash_photos` on already-trashed ids is a no-op. State-changing commands return the post-state, not just `Ok(())`.

---

## Conventions

### Command naming

| Frontend call | Rust handler | Convention |
|---|---|---|
| `photos.list(args)` | `#[tauri::command] photos_list(...)` | dot-case logical, snake-case physical |
| `people.merge(args)` | `people_merge(...)` | verbs are imperative |
| `memories.today()` | `memories_today(...)` | nouns for queries when natural |

### Argument shape

Every command takes **one** struct argument (named `args` on the wire):

```rust
#[tauri::command]
async fn photos_list(args: PhotosListArgs) -> CommandResult<Page<PhotoDto>> { ... }
```

Rationale: positional args break when fields are added; named-struct args evolve cleanly.

### Pagination

```rust
struct Page<T> {
    items: Vec<T>,
    next_cursor: Option<String>,  // opaque, base64
    has_more: bool,
    total: Option<u64>,            // None when expensive to compute
}
```

Cursor format: `base64(date_taken_iso || '|' || id)`. Tiebreak by id DESC. Default `limit = 200`, max `500`. Server clamps; never errors on out-of-range limit.

### IDs

- All IDs are `i64` (matching `rusqlite`).
- Serialized as JSON numbers (Tauri uses serde_json which preserves i64 up to 2^53; we'll never hit that).
- Frontend treats them as opaque — never increments, never assumes consecutive.

### Dates

ISO-8601 strings (`"2024-03-15T14:23:00Z"`). The frontend parses with `new Date()` or Luxon. Internal Rust uses `chrono::DateTime<Utc>`.

### Thumbnails & file paths

- `thumbnail_path: Option<String>` — absolute path on disk, e.g. `/.photovault/thumbs/ab/abc123_small.jpg`. Frontend converts via `convertFileSrc(path)` from `@tauri-apps/api/core` to render in `<img>`.
- `file_path: String` — relative to drive root, forward-slash normalized. Resolves to absolute via `library.resolve_path` if the frontend ever needs it (rare; thumbnails are usually enough).

---

## Error model

```rust
// src-tauri/src/error.rs
#[derive(Serialize, Debug, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandError {
    #[error("not found: {entity} #{id}")]
    NotFound { entity: String, id: String },

    #[error("validation: {field}: {reason}")]
    Validation { field: String, reason: String },

    #[error("library not opened")]
    LibraryClosed,

    #[error("drive not mounted: {path}")]
    DriveNotMounted { path: String },

    #[error("ml unavailable: {reason}")]
    MlUnavailable { reason: String },

    #[error("conflict: {reason}")]
    Conflict { reason: String },               // e.g. cluster_a == cluster_b in merge

    #[error("operation cancelled")]
    Cancelled,

    #[error("database error")]
    Database { message: String },

    #[error("io error")]
    Io { message: String },

    #[error("network error")]
    Network { message: String },

    #[error("internal error: {message}")]
    Internal { message: String },              // last resort; logged with backtrace
}
```

Every internal `AppError` (`src/error.rs`) maps to one of these in `From<AppError> for CommandError`. The mapping is the *only* place where internal error chains get flattened — handlers stay one-liners.

Frontend type:

```ts
type CommandError =
  | { kind: 'not_found'; entity: string; id: string }
  | { kind: 'validation'; field: string; reason: string }
  | { kind: 'library_closed' }
  | { kind: 'drive_not_mounted'; path: string }
  | { kind: 'ml_unavailable'; reason: string }
  | { kind: 'conflict'; reason: string }
  | { kind: 'cancelled' }
  | { kind: 'database'; message: string }
  | { kind: 'io'; message: string }
  | { kind: 'network'; message: string }
  | { kind: 'internal'; message: string };
```

---

## Event model

Long-running ops emit events on a typed channel. Channel name is the event topic; payload is a single typed struct.

| Channel | Direction | Payload | When |
|---|---|---|---|
| `album_export:progress` | server → client | `JobProgress` | while copying originals to the export folder |
| `album_export:complete` | server → client | `AlbumExportComplete` | after export finishes or is cancelled |
| `scan:progress` | server → client | `ScanProgress` | every 200ms during active scan |
| `scan:complete` | server → client | `ScanResult` | on scan finish (ok or error) |
| `faces:progress` | server → client | `FacesProgress` | every chunk (25 photos) |
| `faces:complete` | server → client | `FacesResult` | on completion |
| `duplicates:progress` | server → client | `JobProgress` | during run |
| `duplicates:complete` | server → client | `DuplicatesResult` | on completion |
| `bursts:progress` | server → client | `JobProgress` | during run |
| `bursts:complete` | server → client | `BurstsResult` | on completion |
| `documents:progress` | server → client | `JobProgress` | during analysis |
| `documents:complete` | server → client | `DocumentsResult` | on completion |
| `thumbnails:progress` | server → client | `JobProgress` | during prewarm/regen |
| `update:download-progress` | server → client | `DownloadProgress` | during update download |
| `update:installed` | server → client | `UpdateInstalled` | after install completes |
| `drives:changed` | server → client | `Vec<DriveDto>` | when USB mount/unmount detected |
| `library:scan-recommended` | server → client | `()` | when reindex detects significant drift |

`JobProgress` is the generic shape:

```rust
struct JobProgress {
    job_id: String,
    stage: String,              // "indexing", "embedding", "clustering", ...
    processed: u64,
    total: Option<u64>,         // None when unknown (e.g. directory walk)
    elapsed_ms: u64,
    eta_ms: Option<u64>,
    message: Option<String>,    // optional human-readable status
}
```

Each long-running command returns a `job_id` immediately. The client subscribes to `<topic>:progress` filtered by `job_id` (Tauri's `listen` handler ignores non-matching events).

---

## DTOs (full reference)

Defined in `src-tauri/src/dto.rs`. All `#[derive(Serialize, Deserialize)]` (Deserialize where the frontend sends it back).

```rust
struct PhotoDto {
    id: i64,
    file_path: String,            // relative to drive_root
    file_name: String,
    file_size: u64,
    file_hash: String,            // hex
    date_taken: Option<String>,   // ISO-8601 UTC
    width: Option<u32>,
    height: Option<u32>,
    orientation: Option<u32>,     // 1..=8
    gps: Option<GpsDto>,
    location: Option<LocationDto>,
    camera: Option<CameraDto>,
    thumbnail_path: Option<String>,
    content_category: ContentCategoryDto,
    ocr: Option<OcrDto>,
    faces_processed: bool,
    is_trashed: bool,
    stack: Option<PhotoStackBadgeDto>,
    indexed_at: String,
}

struct GpsDto { lat: f64, lng: f64, altitude: Option<f64> }
struct LocationDto { city: Option<String>, country: Option<String> }
struct CameraDto {
    make: Option<String>, model: Option<String>, lens: Option<String>,
    iso: Option<u32>, aperture: Option<f32>, shutter_speed: Option<String>,
    focal_length: Option<f32>, flash: Option<bool>,
}
struct OcrDto { text: String, confidence: f32 }

#[serde(rename_all = "snake_case")]
enum ContentCategoryDto { Photo, BusinessCard, Document, Screenshot, Presentation, Whiteboard, Receipt }

struct PhotoSummaryDto {              // light variant for grids
    id: i64,
    thumbnail_path: Option<String>,
    date_taken: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    orientation: Option<u32>,
    is_trashed: bool,
    stack: Option<PhotoStackBadgeDto>,
}

struct PhotoStackBadgeDto {
    id: i64,
    kind: String,                     // burst | exact_duplicate | perceptual_duplicate
    member_count: i64,
    cover_photo_id: i64,
}

struct PhotoStackDto {
    id: i64,
    kind: String,
    source_group_id: i64,
    cover_photo_id: i64,
    member_count: i64,
    confidence: f32,
    members: Vec<PhotoStackMemberDto>,
}

struct PhotoStackMemberDto {
    photo_id: i64,
    thumbnail_path: Option<String>,
    date_taken: Option<String>,
    quality_score: f32,
    score_reasons: Option<String>,
    is_cover: bool,
}

struct PersonDto {                    // a face cluster
    id: i64,
    name: Option<String>,
    photo_count: u32,
    face_count: u32,
    representative_thumbnail_path: Option<String>,  // path to a face crop
    cover_face_id: i64,
}

struct ReviewItemDto {
    queue_id: i64,
    face_id: i64,
    face_thumbnail_path: String,      // crop of the candidate face
    candidate_cluster: PersonDto,
    candidate_sample_thumbnails: Vec<String>,  // up to 4 sample face crops
    score: f32,                       // 0..1, similarity confidence
}

struct AlbumDto {
    id: i64,
    name: String,
    photo_count: u32,
    cover_thumbnail_path: Option<String>,
    date_range: Option<(String, String)>,  // earliest, latest
    created_at: String,
    updated_at: String,
}

struct AlbumSuggestionDto {
    id: i64,
    kind: SuggestionKind,             // Trip | Event
    title: String,
    photo_ids: Vec<i64>,
    cover_thumbnail_path: Option<String>,
    status: SuggestionStatus,         // Pending | Accepted | Dismissed
}
#[serde(rename_all = "snake_case")] enum SuggestionKind { Trip, Event }
#[serde(rename_all = "snake_case")] enum SuggestionStatus { Pending, Accepted, Dismissed }

struct DuplicateGroupDto {
    id: i64,
    duplicate_type: DuplicateType,    // Exact | Perceptual
    members: Vec<DuplicateMemberDto>,
    wasted_bytes: u64,
}
#[serde(rename_all = "snake_case")] enum DuplicateType { Exact, Perceptual }
struct DuplicateMemberDto {
    photo_id: i64,
    file_path: String,
    file_size: u64,
    date_taken: Option<String>,
    thumbnail_path: Option<String>,
    is_suggested_keep: bool,
}

struct BurstGroupDto {
    id: i64,
    start: String, end: String,       // ISO-8601
    members: Vec<BurstMemberDto>,
}
struct BurstMemberDto {
    photo_id: i64,
    thumbnail_path: Option<String>,
    sharpness_score: f32,
    blur_score: f32,
    is_suggested_best: bool,
}

struct MemoryCardDto {
    id: String,                       // synthetic — not DB-backed
    kind: MemoryKind,                 // OnThisDay | YearRecap | SeasonalRecap | FallbackWindow
    title: String,
    subtitle: Option<String>,
    hero_thumbnail_path: Option<String>,
    photo_ids: Vec<i64>,
    people_in_card: Vec<PersonDto>,   // for "block this person" affordance
}
#[serde(rename_all = "snake_case")] enum MemoryKind { OnThisDay, YearRecap, SeasonalRecap, FallbackWindow }

struct InsightsDto { /* mirrors src/services/insights.rs InsightsData; large */ }
struct LibraryHealthDto { /* mirrors src/services/library_health.rs */ }
struct TrashedPhotoDto { /* photo_id, original_path, thumbnail_path, trashed_at, file_size */ }
struct TrashStatsDto { count: u64, total_bytes: u64, oldest_trashed_at: Option<String> }
struct DriveDto { path: String, label: Option<String>, available_bytes: Option<u64>, is_removable: bool }
struct AssetHealthDto { missing_face_models: bool, missing_onnx_runtime: bool, missing_geonames_db: bool, summary: String }
struct AssetInventoryDto { install_root: String, roots: Vec<String>, total_size_bytes: u64, assets: Vec<AssetItemDto> }
struct AssetItemDto { id: String, label: String, kind: String, status: String, required: bool, active: bool, installable: bool, removable: bool, size_bytes: Option<u64>, path: Option<String>, note: Option<String> }
struct UpdateStatusDto { current: String, latest: Option<String>, newer_available: bool, release_url: Option<String>, body: Option<String> }
struct MapPinDto { photo_id: i64, lat: f64, lng: f64, thumbnail_path: Option<String> }
struct SearchResultsDto {
    interpreted: Vec<InterpretedFilterDto>,
    people: Vec<PersonHitDto>,
    albums: Vec<AlbumHitDto>,
    places: Vec<PlaceHitDto>,
    photo_ids: Vec<i64>,
    photos: Vec<SearchPhotoDto>,
}
struct InterpretedFilterDto { kind: String, label: String }
struct PlaceHitDto { city: String, country: Option<String>, photo_count: i64 }
struct SearchPhotoDto { photo_id: i64, date_taken: Option<String>, location_city: Option<String>, location_country: Option<String>, thumbnail_path: Option<String> }
struct SettingsDto { /* one field per UI-exposed setting; see settings domain below */ }
```

---

## Commands by domain

> Notation: `domain.command(args) → return | events emitted`

### 1. `library` — drive selection, scan lifecycle, asset bootstrap

The library is *closed* until `library.open` succeeds. Most other commands fail with `CommandError::LibraryClosed` until then.

| Command | Args | Returns | Notes |
|---|---|---|---|
| `library.list_drives` | `{}` | `Vec<DriveDto>` | sync; cheap; subscribe to `drives:changed` for updates |
| `library.open` | `{ drive_path: String }` | `LibraryOpenResult` | initializes DB, opens services; emits `library:scan-recommended` if drift detected |
| `library.close` | `{}` | `()` | for "switch library" flow; flushes pending writes |
| `library.current` | `{}` | `Option<LibraryHandleDto>` | what's currently open |
| `library.start_scan` | `{ scan_hidden_folders: bool }` | `{ job_id: String }` | emits `scan:progress`, `scan:complete` |
| `library.cancel_scan` | `{ job_id: String }` | `()` | flips cancel flag; scan finishes mid-batch |
| `library.detect_changes` | `{}` | `IndexChangesDto` | reindexer; preview before applying |
| `library.apply_changes` | `{ added: bool, removed: bool, moved: bool, modified: bool }` | `ApplyResultDto` | flags choose which categories to apply |
| `library.exclusions.list` | `{}` | `Vec<ExcludedFolderDto>` | per-library folders skipped by scan/reindex |
| `library.exclusions.preview` | `{ path: String }` | `ExcludedFolderPreviewDto` | validates selected folder and counts indexed items under it |
| `library.exclusions.add` | `{ path: String }` | `ExcludedFolderDto` | recursively excludes the folder and removes matching indexed rows; files stay on disk |
| `library.exclusions.remove` | `{ relative_path: String }` | `()` | future scans can index the folder again |
| `library.regenerate_thumbnails` | `{ photo_ids: Option<Vec<i64>> }` | `{ job_id: String }` | None = all; emits `thumbnails:progress` |
| `library.regenerate_rotated_data` | `{}` | `{ job_id: String }` | recomputes blur/sharpness/aspect after orientation fix |
| `library.refresh_photo_dates` | `{}` | `{ updated: u64 }` | re-reads EXIF date_taken for date-less photos |
| `library.resolve_path` | `{ photo_id: i64 }` | `{ absolute_path: String }` | resolves relative path to absolute, errors if drive not mounted |

**`LibraryOpenResult`** = `{ drive_path, photo_count, first_run: bool, last_scan_at: Option<String> }`
**`IndexChangesDto`** = `{ added: u64, removed: u64, moved: u64, modified: u64, sample: ChangeSampleDto }`
**`ApplyResultDto`** = `{ added: u64, removed: u64, moved: u64, modified: u64, marked_for_face_reprocess: u64 }`

### 2. `photos` — listing, fetching, lookups

| Command | Args | Returns | Notes |
|---|---|---|---|
| `photos.list` | `{ cursor: Option<String>, limit: Option<u32>, include_trashed: bool }` | `Page<PhotoSummaryDto>` | timeline scroll; `total` populated only on first page; hides non-cover stack members when timeline stacks are enabled |
| `photos.get` | `{ id: i64 }` | `PhotoDto` | full detail (lightbox open) |
| `photos.get_many` | `{ ids: Vec<i64> }` | `Vec<PhotoDto>` | up to 500; preserves input order; missing ids dropped silently |
| `photos.list_by_album` | `{ album_id: i64, cursor, limit }` | `Page<PhotoSummaryDto>` | |
| `photos.list_by_person` | `{ person_id: i64, cursor, limit }` | `Page<PhotoSummaryDto>` | |
| `photos.list_by_date` | `{ start: String, end: String, cursor, limit }` | `Page<PhotoSummaryDto>` | inclusive range |
| `photos.list_by_place` | `{ city: Option<String>, country: Option<String>, cursor, limit }` | `Page<PhotoSummaryDto>` | |
| `photos.people_in_photo` | `{ photo_id: i64 }` | `Vec<PersonDto>` | |
| `photos.albums_for_photo` | `{ photo_id: i64 }` | `Vec<AlbumDto>` | for album-picker pre-checked state |

### 3. `people` — face clusters & review queue

| Command | Args | Returns | Notes |
|---|---|---|---|
| `people.list` | `{ named_only: bool, min_photos: Option<u32> }` | `Vec<PersonDto>` | sorted by photo_count DESC; small enough to be unpaged for now |
| `people.get` | `{ id: i64 }` | `PersonDto` | |
| `people.rename` | `{ id: i64, name: Option<String> }` | `PersonDto` | None = clear name |
| `people.merge` | `{ source_id: i64, target_id: i64 }` | `PersonDto` | returns merged cluster; `Conflict` if same id |
| `people.set_representative_face` | `{ person_id: i64, face_id: i64 }` | `PersonDto` | |
| `people.start_processing` | `{}` | `{ job_id: String }` | emits `faces:progress`, `faces:complete` |
| `people.cancel_processing` | `{ job_id: String }` | `()` | |
| `people.rebuild_clusters` | `{}` | `{ job_id: String }` | re-clusters from existing embeddings; no re-detect |
| `people.review.queue` | `{ limit: Option<u32> }` | `Vec<ReviewItemDto>` | default 20 |
| `people.review.same` | `{ queue_id: i64 }` | `()` | assigns face to candidate cluster |
| `people.review.different` | `{ queue_id: i64 }` | `()` | records cannot-merge constraint |
| `people.review.skip` | `{ queue_id: i64 }` | `()` | |
| `people.review.undo` | `{ queue_id: i64 }` | `()` | reverses last decision; UI tracks the id |

### 4. `albums` — manual albums + AI suggestions

| Command | Args | Returns |
|---|---|---|
| `albums.list` | `{}` | `Vec<AlbumDto>` |
| `albums.get` | `{ id: i64 }` | `AlbumDto` |
| `albums.create` | `{ name: String, photo_ids: Vec<i64> }` | `AlbumDto` |
| `albums.rename` | `{ id: i64, name: String }` | `AlbumDto` |
| `albums.delete` | `{ id: i64 }` | `()` |
| `albums.add_photos` | `{ id: i64, photo_ids: Vec<i64> }` | `{ added: u32 }` |
| `albums.remove_photos` | `{ id: i64, photo_ids: Vec<i64> }` | `{ removed: u32 }` |
| `albums.auto_pick_cover` | `{ id: i64 }` | `AlbumDto` |
| `albums.export` | `{ album_id: i64, destination_dir: Option<String>, folder_name: Option<String> }` | `{ job_id: String }` |
| `albums.suggestions.list` | `{ status: Option<SuggestionStatus> }` | `Vec<AlbumSuggestionDto>` |
| `albums.suggestions.run_detection` | `{}` | `SuggestionDiagnosticsDto` | sync (current backend is sync; ~1-10s); if it grows, promote to job |
| `albums.suggestions.preview` | `{ id: i64, limit: Option<u32> }` | `Vec<PhotoSummaryDto>` |
| `albums.suggestions.accept` | `{ id: i64, name: Option<String> }` | `AlbumDto` | creates real album; `name` overrides title |
| `albums.suggestions.dismiss` | `{ id: i64 }` | `()` |

`albums.export` copies original photo/video files into a new folder without
modifying the library. If `destination_dir` is omitted, the backend uses the
user-facing `Pictures/Smriti Exports` folder where available. Completion emits:

```rust
struct AlbumExportComplete {
    job_id: String,
    stage: String,
    processed: u64,
    total: Option<u64>,
    album_id: i64,
    folder_path: String,
    exported: u64,
    skipped_missing: u64,
    failed: u64,
    elapsed_ms: u64,
    message: String,
}
```

### 5. `search` — unified query

| Command | Args | Returns |
|---|---|---|
| `search.query` | `{ q: String }` | `SearchResultsDto` |
| `search.recent.list` | `{ limit: Option<u32> }` | `Vec<RecentSearchDto>` |
| `search.recent.remove` | `{ q: String }` | `()` |
| `search.recent.clear` | `{}` | `()` |

`search.query` is the one-shot entry point. It returns small entity lists
(people/albums/places), interpreted filter chips, matching photo ids for
viewer navigation, and the first bounded photo result set.

Smart search filters are deterministic: dates, people, places, albums,
favourites, and media type are ANDed together. Multiple people mean
"contains all". `only <people>` is strict: the photo must be face-processed,
must contain every requested person, and must not contain another assigned
or unassigned detected face.

### 6. `memories` — N-years-ago rediscovery

| Command | Args | Returns |
|---|---|---|
| `memories.today` | `{}` | `Vec<MemoryCardDto>` |
| `memories.detail` | `{ memory_id: String }` | `{ card: MemoryCardDto, photos: Vec<PhotoSummaryDto> }` |
| `memories.block_person` | `{ person_id: i64 }` | `()` | hides cards prominently featuring this cluster |
| `memories.unblock_person` | `{ person_id: i64 }` | `()` |
| `memories.blocked_people` | `{}` | `Vec<PersonDto>` | for settings UI |
| `memories.save_as_album` | `{ memory_id: String, name: Option<String> }` | `AlbumDto` |

`memories.today` is cheap enough (~50ms) that the frontend can poll on day-rollover client-side via a `setInterval`. No event channel needed.

### 7. `duplicates`

| Command | Args | Returns |
|---|---|---|
| `duplicates.run` | `{ include_perceptual: bool }` | `{ job_id: String }` | emits `duplicates:progress`, `duplicates:complete` |
| `duplicates.list` | `{}` | `Vec<DuplicateGroupDto>` | summary only — no member detail |
| `duplicates.get_group` | `{ id: i64 }` | `DuplicateGroupDto` | full member list |
| `duplicates.set_keep` | `{ group_id: i64, photo_id: i64 }` | `DuplicateGroupDto` |
| `duplicates.trash_others` | `{ group_id: i64 }` | `{ trashed: u32 }` | trashes non-keep members |
| `duplicates.dismiss` | `{ group_id: i64 }` | `()` | mark resolved without trashing |
| `duplicates.wasted_space` | `{}` | `{ bytes: u64 }` |

### 8. `bursts`

| Command | Args | Returns |
|---|---|---|
| `bursts.run` | `{}` | `{ job_id: String }` | emits `bursts:progress`, `bursts:complete` |
| `bursts.list` | `{}` | `Vec<BurstGroupDto>` |
| `bursts.get_group` | `{ id: i64 }` | `BurstGroupDto` |
| `bursts.set_best` | `{ group_id: i64, photo_id: i64 }` | `BurstGroupDto` |
| `bursts.trash_non_best` | `{ group_id: i64 }` | `{ trashed: u32 }` |
| `bursts.dismiss` | `{ group_id: i64 }` | `()` |

### 9. `stacks`

Photo stacks are a presentation layer over conservative burst and
duplicate groups. The cover photo is the suggested best photo, but the
user can override it. Normal viewer next/previous navigation remains
anchored to the visible timeline photo; stack members are browsed via
the stack tray.

| Command | Args | Returns |
|---|---|---|
| `stacks.get` | `{ id: i64 }` | `PhotoStackDto` |
| `stacks.get_for_photo` | `{ photo_id: i64 }` | `Option<PhotoStackDto>` |
| `stacks.set_cover` | `{ stack_id: i64, photo_id: i64 }` | `PhotoStackDto` |
| `stacks.remove_member` | `{ stack_id: i64, photo_id: i64 }` | `Option<PhotoStackDto>` |
| `stacks.unstack` | `{ id: i64 }` | `()` |
| `stacks.trash_others` | `{ id: i64 }` | `{ count: u64 }` |
| `stacks.refresh` | `{}` | `{ stacks_found: u64 }` |

### 10. `trash`

| Command | Args | Returns |
|---|---|---|
| `trash.list` | `{ cursor, limit }` | `Page<TrashedPhotoDto>` |
| `trash.stats` | `{}` | `TrashStatsDto` |
| `trash.trash_photos` | `{ photo_ids: Vec<i64> }` | `{ trashed: u32 }` | already-trashed are silent no-op |
| `trash.restore` | `{ photo_ids: Vec<i64> }` | `{ restored: u32 }` |
| `trash.permanent_delete` | `{ photo_ids: Vec<i64> }` | `{ deleted: u32, freed_bytes: u64 }` | hard delete; UI must confirm |
| `trash.empty` | `{}` | `{ deleted: u32, freed_bytes: u64 }` |

<!--
### 11. `documents` — OCR'd text-bearing photos  [DEFERRED]

The Documents tab is not exposed in the UI right now. The engine still
classifies content categories silently for the timeline badge, and the
IPC commands are still wired into the Tauri shell (so an embedder can
call them), but no user-facing surface ships them today. Block kept
for the day this returns.

| Command | Args | Returns |
|---|---|---|
| `documents.list` | `{ categories: Option<Vec<ContentCategoryDto>>, cursor, limit }` | `Page<PhotoSummaryDto>` | `categories` None = "all non-Photo" |
| `documents.search` | `{ q: String, cursor, limit }` | `Page<PhotoSummaryDto>` | FTS5 over ocr_text |
| `documents.run_analysis` | `{}` | `{ job_id: String }` | emits `documents:progress`, `documents:complete` |
| `documents.set_category` | `{ photo_id: i64, category: ContentCategoryDto }` | `()` | manual override |
-->


### 12. `map`

| Command | Args | Returns |
|---|---|---|
| `map.pins` | `{ bounds: { north, south, east, west }, max_pins: Option<u32> }` | `Vec<MapPinDto>` | server-side aggregation if too many |
| `map.cluster_filmstrip` | `{ photo_ids: Vec<i64> }` | `Vec<PhotoSummaryDto>` | for clicked-pin overlay |
| `map.tile_cache.stats` | `{}` | `{ size_bytes: u64, file_count: u32, limit_bytes: u64 }` |
| `map.tile_cache.set_limit` | `{ limit_mb: u32 }` | `()` |
| `map.tile_cache.clear` | `{}` | `{ freed_bytes: u64 }` |

> **Maps note:** MapLibre GL JS handles tile fetching directly via HTTPS to OSM — Tauri doesn't proxy tiles. The `map.tile_cache.*` commands manage *our* offline tile cache (MapLibre's IndexedDB cache or our pre-cached tiles). Pins are the only photo-side data crossing IPC.

### 13. `insights`

| Command | Args | Returns |
|---|---|---|
| `insights.compute` | `{ year: Option<i32> }` | `InsightsDto` | None = all-time |
| `insights.invalidate` | `{}` | `()` | client signals stale (after big mutation); server clears its cache |

### 14. `health`

| Command | Args | Returns |
|---|---|---|
| `health.compute` | `{}` | `LibraryHealthDto` |

### 15. `geocoding`

| Command | Args | Returns |
|---|---|---|
| `geocoding.run` | `{}` | `{ job_id: String }` | resolves all unresolved GPS-bearing photos; emits a generic `JobProgress` on `geocoding:progress` |
| `geocoding.resolve_one` | `{ lat: f64, lng: f64 }` | `Option<LocationDto>` | sync, ~1-3ms — used by photo_detail when a specific photo opens |

### 16. `settings`

```rust
struct SettingsDto {
    theme: Theme,                     // Light | Dark | System
    thumbnail_grid_size: ThumbnailSize,
    scan_hidden_folders: bool,
    face_confidence_threshold: f32,   // 0..1
    clustering_threshold: f32,        // 0..1
    burst_window_seconds: u32,
    trash_auto_delete_days: Option<u32>,
    date_format: DateFormat,
    home_city: Option<String>,
    memories_enabled: bool,
    show_timeline_stacks: bool,
    auto_update_check_enabled: bool,
    map_cache_limit_mb: u32,
}
```

| Command | Args | Returns |
|---|---|---|
| `settings.get` | `{}` | `SettingsDto` |
| `settings.update` | `Partial<SettingsDto>` | `SettingsDto` | partial; only fields present are written |

### 17. `system` — drives, assets, updates, OS integration

| Command | Args | Returns |
|---|---|---|
| `system.asset_health` | `{}` | `AssetHealthDto` |
| `system.assets_inventory` | `{}` | `AssetInventoryDto` |
| `system.updates.check` | `{}` | `UpdateStatusDto` | network call |
| `system.updates.download` | `{}` | `{ job_id: String }` | emits `update:download-progress` |
| `system.updates.install` | `{}` | `()` | applies a downloaded update; replaces binary or launches installer |
| `system.open_in_explorer` | `{ photo_id: i64 }` | `()` | reveals file in OS file manager |
| `system.open_path` | `{ path: String }` | `()` | opens an existing folder in the OS file manager |
| `system.copy_path_to_clipboard` | `{ photo_id: i64 }` | `()` |
| `system.app_version` | `{}` | `{ version: String, build: String, channel: String }` |

---

## Backend wiring

**`src-tauri/src/state.rs`** — single AppState held by Tauri:

```rust
pub struct AppState {
    pub library: tokio::sync::RwLock<Option<OpenLibrary>>,
    pub jobs: JobRegistry,
    pub event_tx: tauri::AppHandle,  // for emitting events
}

pub struct OpenLibrary {
    pub drive_root: PathBuf,
    pub db: Arc<Mutex<Database>>,    // or Arc<Database> if internally Send
    pub thumbnail_service: Arc<ThumbnailService>,
    pub geocoding_service: Arc<GeocodingService>,
    // ...
}

pub struct JobRegistry {
    inner: Mutex<HashMap<String, JobHandle>>,  // for cancellation
}
pub struct JobHandle {
    pub cancel_flag: Arc<AtomicBool>,
    pub kind: JobKind,
}
```

The DB lives behind a tokio mutex so handlers can `.lock().await` and pass `&mut Connection` to existing repos. We do *not* refactor repos to be `Send + Sync` for now; the lock is the boundary.

**Handler thinness rule.** Each handler is ≤15 lines:

```rust
#[tauri::command]
async fn photos_list(
    args: PhotosListArgs,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let lib = state.library.read().await;
    let lib = lib.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let cursor = pagination::decode(args.cursor.as_deref())?;
    let (rows, next) = PhotoRepo::new().list_after(&db, cursor, args.limit.unwrap_or(200))?;
    Ok(Page {
        items: rows.into_iter().map(PhotoSummaryDto::from).collect(),
        next_cursor: next.map(pagination::encode),
        has_more: next.is_some(),
        total: None,
    })
}
```

Anything more complex is a service-layer change, not a handler change. This keeps the boundary inspectable.

**Cursor pagination.** Repos that feed the timeline expose a
`list_after(cursor, limit)` variant that tiebreaks `(date_taken DESC,
id DESC)` with explicit `IS NULL` ordering, so a stable cursor can be
carried across page boundaries. The older `get_all_by_date(limit,
offset)` method is retained for callers that still want offset
semantics (insights, exports).

---

## Verification

1. `cargo build -p smriti-tauri` succeeds (Linux + Windows)
2. `cargo test -p smriti-tauri` covers:
   - `pagination::encode` ↔ `pagination::decode` roundtrip
   - `From<AppError> for CommandError` exhaustive (one test per AppError variant)
   - DTO `From<Photo>` etc. for every conversion (one fixture per DTO)
3. **Smoke harness:** a short Rust binary (`src-tauri/examples/smoke.rs`) that spins up the Tauri runtime in headless mode, opens a fixture library, and invokes every command once. Asserts no command panics; asserts every error path is reachable. Run as part of CI alongside `cargo clippy --all-targets`.
4. **Frontend types:** TypeScript definitions auto-generated via `tauri-specta` (or hand-written, decision deferred — but the doc is the contract either way). The Svelte client imports them and won't compile if a command name or shape drifts.

---

## Out of scope for this doc

- Tauri scaffolding (`tauri.conf.json`, capabilities) — see `src-tauri/`
- Svelte routing / components / virtualization — see `src-ui/`
- Cross-platform installer pipeline — see
  `.github/workflows/release.yml` (Tauri-bundler driven)
