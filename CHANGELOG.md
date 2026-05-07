# Changelog

All notable changes to Smriti will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] — 2026-04-21

First pre-v1.0 milestone. Phases 1 and 2 of the v1.0 roadmap
(`plan/V1_RELEASE_ROADMAP.md`) landed: release-blocker correctness
fixes, CI hardening, scale work, and the first end-to-end Timeline
scale-validation on 50K photos.

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
- `plan/V1_RELEASE_ROADMAP.md` captures the full v1.0 arc across
  Phases 1, 2, 3. Phase 1 and 2 are done; Phase 3 is in progress.
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
