# Changelog

All notable changes to PhotoVault will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Keyboard tab traversal and card highlighting across Timeline/Documents/People/Albums/Duplicates/Bursts.
- Timeline keyboard scrolling with `PageUp`, `PageDown`, `Home`, and `End`.
- Open-source release docs and repository hygiene baseline (licenses, policy docs, templates).

## [0.1.0] - 2026-04-16

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
- Cross-platform support for Linux/Windows/macOS
