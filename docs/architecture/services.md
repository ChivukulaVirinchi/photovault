# Services Layer

Services live in `src/services/` and encapsulate domain workflows.
Each service is a pure Rust module — no UI dependencies, no Tauri
dependencies, no async runtime baked in unless the workflow demands
it. Services are the unit of integration testing in `tests/`.

## Service responsibilities

| Service | Role |
|---|---|
| `scanner` | Walks the library, dispatches per-photo work. Async, cancellable. |
| `metadata_processor` | EXIF + GPS + file-hash extraction per photo. |
| `exif_extractor` | Pulls and normalises EXIF tags. |
| `thumbnail` | Three-tier thumbnail generation, lazy + cached. |
| `image_io` | Decodes JPEG/PNG/WebP/HEIC + RAW preview extraction. |
| `raw_preview` | Extracts embedded JPEG previews from TIFF-based RAWs. |
| `geocoding` | Resolves GPS → city/country via local GeoNames DB. |
| `face_processor` | Detection + embedding + clustering orchestration. |
| `search` | Translates `SearchQuery` → SQL → result rows. |
| `insights` | Aggregates per-day / per-month / per-person counts. |
| `memories` | "On this day" / fallback / seasonal generators + ranker. |
| `album_suggestions` | Trip and event detection from geographic + temporal clusters. |
| `burst_detector` | Time-window-based grouping with similarity gate. |
| `duplicate_detector` | SHA-256 exact + DCT-pHash perceptual dedup. |
| `trash` | Soft-delete, restore, permanent-delete, auto-delete. |
| `tile_cache` | LRU-managed cache for OpenStreetMap tiles. |
| `update_checker` | Opt-in version check against the GitHub releases API. |
| `self_replace` | Atomic binary swap for AppImage / portable Windows. |
| `library_health` | Asset-pack presence, DB integrity, model availability. |
| `path_util` | Cross-platform path normalisation. |
| `reindexer` | Detects moves, deletions, and changed files. |
| `camera_names` | Normalises EXIF make/model into human-readable names. |
| `image_utils` | Misc image-processing helpers (rotation, resize). |
| `map_math` | Cluster math for the map view. |
| `drive_detector` | Identifies the indexed drive's filesystem ID. |
| `install_method` | Detects how Smriti was installed (apt, brew, msi, ...). |
| `document_detector` | Heuristic detection of scanned-document photos. |
| `ocr_processor` | (Reserved) OCR pipeline scaffolding. |

## Boundary discipline

- Services accept primitive types or `&rusqlite::Connection`, not
  Tauri `AppState`. This keeps them testable without launching a
  runtime.
- Tauri command handlers in `src-tauri/src/commands/` are thin
  wrappers: deserialize args, call the service, serialize the
  result. Anything over ~15 lines in a handler is a sign the logic
  should move down to a service.
- Engine services never call back into Tauri. The frontend invokes
  commands; commands invoke services.

## Testing

Each service has a `#[cfg(test)] mod tests` block alongside it for
unit tests, plus integration tests in `tests/workflow_*.rs` that
exercise multi-service workflows (scan → metadata → thumbnail →
faces, trash lifecycle, etc.). See [Testing](../TESTING.md).
