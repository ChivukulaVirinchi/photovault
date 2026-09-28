# Changelog

All notable changes to Smriti will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] — 2026-09-27

### Changed
- Startup now opens directly into a responsive native shell; installed visual
  search warms silently after library open without sitting on the open path.
- Large photo grids, search results and route data are virtualized and cached
  with bounded memory use.
- Scan, metadata, thumbnail, duplicate, burst and face-processing work now
  streams useful progress without timer-based polling.

### Fixed
- Removed the startup/loading shell and automatic setup modal that could cover
  the app, and restored direct, reliable navigation between library screens.
- Restored tested route-level code splitting without eagerly parsing MapLibre
  during startup, while keeping the Search and Map route shells immediate.
- Primary library tabs now remain mounted after their first visit, preserving
  map instances, loaded data, filters and scroll positions between tabs.
- Libraries with an existing semantic index now warm ONNX silently in the
  background; the application-wide text runner starts with the native shell,
  uses a low-priority fast-start CPU session instead of compiling a DirectML
  graph, survives library switches, and refreshes active searches when ready.
- Map tiles remain in the bounded persistent cache until eviction, pin payloads
  are route-cached, and the live map instance survives tab navigation.
- Semantic search now caches raw normalized vectors and performs an exact
  cosine scan, removing the long HNSW graph build that delayed the first query.
- The photo viewer now computes fit-to-screen from the incoming image dimensions
  directly, instead of briefly reusing a stale 1:1 scale from the prior frame.
- Timeline scrolling now takes priority over thumbnail generation, browser image
  decoding, reactive thumbnail patches and synchronous position persistence.
- Removed the redundant late-loading Memories strip from Timeline and stale
  “previous results while you type” search copy.
- Excluded MapLibre from Vite dependency optimization so its worker module is
  resolved correctly during development.
- Surprise Me now uses a five-second slide interval by default.
- AppImage startup no longer aborts while parsing the Windows UNC asset glob;
  packaged asset scope is platform-neutral and selected libraries are granted
  access dynamically.
- Hardened library lifecycle, SQLite concurrency, cache invalidation, image
  loading, search paging, trash recovery and cross-platform ONNX loading.
- Removed the unfinished Documents/OCR surface and its unused database state.
- Added regression coverage for startup, runtime initialization and the main
  library workflows.

## [0.3.2] — 2026-09-11

0.3.2 is the release that makes the one-click smart-features setup actually
work. Anyone who downloaded 0.3.1 and found that "Set up smart features"
failed should upgrade.

### Fixed
- **Asset pack now contains a built `geonames.db`.** The 0.3.1 pack shipped
  GeoNames *source text* instead, so the setup job finished with "known missing
  files" and aborted before downloading the visual-search models. Setups that
  already staged the 0.3.1 pack are repaired in place at install time.
- **ONNX Runtime is found on macOS.** The 0.3.1 binary looked for
  `libonnxruntime.so` on every non-Windows platform while macOS packaging ships
  `libonnxruntime*.dylib`, so macOS could never load the runtime.
- macOS, Linux and Windows runtime libraries are all present in the pack.
- Release artifacts now publish `SHA256SUMS`, which the in-app updater requires.
- Releases now also publish stable, human-readable installer names
  (`Smriti-Windows-Setup.exe`, `Smriti-macOS-Apple-Silicon.dmg`,
  `Smriti-Ubuntu-Debian-x64.deb`, `Smriti-Linux-x64.AppImage`,
  `Smriti-Linux-x64.rpm`).

### Added
- **Google Photos Takeout import** — albums, favourites, corrected dates and
  GPS preserved. Guide: `docs/user-guide/google-photos-import.md`.
- **Surprise memory slideshow.**
- **Onboarding pass**: the first-run smart-features dialog, clearer empty
  states, and a friendlier first library open.

### Changed
- Personal memories and multi-stop trips reworked.
- Browsing and large-library performance hardened.
- Documentation corrected: the network disclosure now lists every host that
  Smriti contacts (including Hugging Face for the optional visual-search
  models), and the install matrix states the real artifact list, the
  Apple-Silicon-only macOS build and the current lack of distro packages.

## [0.3.1] — 2026-07-10

### Added
- **Visual search** — on-device semantic search over your library using
  SigLIP2 models (text→photo and photo→photo), with a local vector index and
  its own UI, filters and documentation.
- **Assistant album planning** experiment (sign-in-free, provider-backed and
  off by default).

### Fixed
- ONNX asset-pack runtime lookup on installed builds.
- Semantic model download progress reporting.
- Large-library thumbnail and detail navigation stalls.
- `quinn-proto` advisory (cargo audit).

### Removed
- Stable library identity tracking.

## [0.3.0] — 2026-06-09

Smriti 0.3.0 focuses on making larger real-world libraries feel live and
trustworthy: videos now belong in the same library as photos, search is more
intent-aware, cleanup jobs stream progress instead of feeling frozen, and the
release/asset path is less brittle for installed users.

### Added
- **Video support** in the timeline and detail viewer, including playable
  videos, posters/thumbnails, duration metadata, codec metadata, and video
  date extraction.
- **Photo stacks** for burst and duplicate group covers, with stack metadata
  surfaced in timeline/detail views.
- **Favourites smart album** that appears only when favourites exist and stays
  non-deletable/non-duplicable.
- **Smart search intent parsing** for date/year, place, people, media type,
  favourites, album filters, and strict "only person X" queries. Filters are
  order-independent and compose across supported dimensions.
- **Asset management in Settings**, including inventory display and in-app
  asset installation for ONNX Runtime, SCRFD, AdaFace, and GeoNames.
- **Excluded folders** so users can keep specific child folders out of an
  indexed library.
- **Album export** to an accessible export folder, with conflict-safe filenames
  and an "open folder" completion dialog.
- **Marquee selection across photo listing views**, with bulk actions such as
  album add and trash.
- **Live face-detection preview strip** while the face pipeline is still
  processing unclustered faces.

### Changed
- **Duplicate detection now streams and resumes better**: exact duplicate
  groups are persisted as soon as they are found, perceptual groups use a
  banded pHash candidate index instead of full pairwise comparison, and stale
  groups are refreshed without surfacing trashed photos.
- **Burst detection now streams and cancels better**, persists partial results
  during long runs, prefers cached thumbnails, and builds visual signatures
  lazily only after cheap time/folder checks pass.
- **Cleanup jobs no longer expose fake cancel buttons** for tasks that cannot
  honor cancellation.
- **Timeline/detail navigation is more predictable**: timeline clicks refresh
  browse context, and detail-view chevrons remain discoverable when previous or
  next photos exist.
- **Date repair pipeline is more robust** for photos and videos, with a
  settings action to refresh dates across the library.
- **Maps, albums, memories, suggestions, duplicates, bursts, and search now
  consistently hide trashed photos**.
- **Album suggestions require valid visible covers**, avoiding empty-looking
  suggestion cards.
- **Release-local verification script updated** for the current Tauri binary
  name, package build target, and managed GeoNames asset location.

### Fixed
- Fixed face-cluster timeline query aliases that generated invalid SQL such as
  `NULL AS stack.p.id`.
- Fixed deleted photos lingering in timeline/browse context after trashing.
- Fixed map return navigation so opening a photo from the map preserves the map
  viewport when returning.
- Fixed album and memory detail scroll restoration after preview navigation.
- Fixed album preview image sizing so photos are not vertically compressed.
- Fixed smart-search intersections such as year + city.
- Fixed ambiguous QuickTime metadata test bytes so clippy passes with
  `-D warnings`.

## [0.2.0] — 2026-04-21

First pre-v1.0 milestone. Phases 1 and 2 of the v1.0 roadmap landed:
release-blocker correctness fixes, CI hardening, scale work, and the
first end-to-end Timeline scale-validation on 50K photos.

### Fixed
- **Data integrity**: reindexer move-detection was effectively broken —
  `quick_hash` only hashed the first 64 KB and could never match the
  scanner's stored full-file SHA256. Two large files sharing a 64 KB
  prefix (common with camera EXIF headers) would collide. Replaced
  with the scanner's streaming hash, plus pre-hashed candidate
  HashMap to kill the N×M re-hash loop that had been inside the
  per-missing-file path.
- **Thumbnail cache LRU**: the in-memory cache was a `HashMap` +
  `Instant` that was set on insert but never bumped on access, so
  "LRU eviction" was effectively FIFO. Switched to `lru::LruCache`
  (O(1) eviction, correct access-recency tracking).
- **Clustering NaN safety**: hot-loop comparisons now use
  `f32::total_cmp` and drop NaN similarities explicitly instead of
  `partial_cmp + unwrap_or(Equal)`.
- **Stale single-instance lock recovery**: lockfile failures no
  longer refuse startup; the app now reports the holding PID and
  falls through on lockfile-inaccessible errors.

### Added
- **First-run asset installer UI** — optional ML models and GeoNames
  install via an in-app modal instead of silent `tracing::warn!`s.
  "Reinstall Assets" button in Settings.
- **CI hardening** — MSRV check pinned to 1.75, `cargo-audit`
  advisories gate, `cargo-deny` license + bans gate, Dependabot for
  cargo + github-actions. Clippy runs with `-D warnings`.
- **Docs site wired end-to-end** — `mdbook build` and
  `cargo doc --no-deps` now run in the Pages workflow and deploy
  to `/docs/` and `/api/`. Previously the site published only a
  landing page with stub HTMLs; the 21 pages in `docs/SUMMARY.md`
  had never shipped.
- **Bench baselines** — `benches/clustering.rs` and
  `benches/hashing.rs` (criterion) establish Phase 1 performance
  baselines. CI has a bench smoke-test job.
- **Scale test** — new `tests/timeline_scale.rs` asserts 50K-photo
  `compute_groups` runs in <300 ms debug / <100 ms release, with
  invariant checks on group contiguity and member count.
- **README badges**: CI, MSRV, license, download count, latest
  release.

### Performance
- **Timeline load cap** lifted from 50K → 250K photos. At ~500 bytes
  per `Photo` record that's ~125 MB of in-memory metadata, well
  within desktop budgets but 5× the previous ceiling.
- **Timeline grouping off the render path** — `compute_groups` now
  runs once per photos-load (stored on app state as a zero-copy
  `Vec<DateGroupRange>`, indices into `photos`) instead of re-running
  every scroll frame. Previously a 50K library re-grouped on every
  60 Hz repaint; now it doesn't.
- **`compute_groups` 15× speedup** — caught by the scale test: the
  grouping loop was calling `format!("%Y-%m-%d")` on every photo
  twice per iteration. Compare `NaiveDate` directly, format once per
  group. 50K-photo grouping: 324 ms → 21 ms in debug.
- **Composite indexes** (schema migration v15) covering the hot
  query paths:
  `idx_photos_trashed_date`, `idx_photos_faces_processed_trashed`,
  `idx_faces_cluster_confidence`, `idx_faces_photo_cluster`.
  Verified against query planner.
- **Batch inserts** in `burst_repo` and `duplicate_repo` — moved
  from per-row `execute` inside a transaction to multi-row
  `VALUES (...), (...)` inside a transaction, chunked at
  `MAX_ROWS_PER_INSERT = 200`. Previously `burst_repo::create_group`
  relied on autocommit per member insert.
- **Face-clustering Stage-B cap** — at >2,000 unresolved faces,
  Stage B (complete-link O(n²)) short-circuits and routes overflow
  through the rescue / ambiguous-review pipeline instead of
  freezing the UI.

### Changed
- **SECURITY.md** now routes vulnerability reports through GitHub
  Security Advisories (private) instead of public issues. Defined
  response-time targets by severity.
- **CONTRIBUTING.md** documents the required CI gate matrix.
- **ML model attribution**: `THIRD_PARTY_LICENSES.md` now points
  explicitly at the upstream InsightFace project for both
  face-detection and face-recognition models, rather than deferring
  to a vague "check upstream" note. The models are downloaded from
  upstream on first run, not bundled with the installer.

### Docs
- Full `people.md` user-guide rewrite explaining the two-stage
  clustering pipeline.

## [0.1.0] — 2026-04-16

Initial public release candidate.

### Added
- Photo library indexing from any folder or external drive
- EXIF metadata extraction (date, GPS, camera, exposure)
- SQLite database stored on the indexed drive (fully portable)
- Thumbnail generation with quality tiers
- Face detection and recognition with interactive review queue
- Person clustering with merge / split / rename flows
- Duplicate detection (exact + perceptual)
- Burst detection with best-photo suggestions
- Soft delete with retention policy
- OCR document detection (screenshots, receipts, business cards)
- Map view with tile caching and pin clustering
- Memories: anniversary and recap cards with slideshow
- Manual albums + album suggestions
- Insights dashboard with heatmap and top entities
- Unified search across people, albums, places, photos
- Cross-platform support for Linux / Windows / macOS
- Keyboard tab traversal and card highlighting across Timeline,
  Documents, People, Albums, Duplicates, and Bursts views
- Timeline keyboard scrolling with `PageUp`, `PageDown`, `Home`,
  and `End`
- Open-source release docs and repository hygiene baseline
  (licenses, policy docs, templates)
