# Smriti Manual

The full user guide and contributor reference for Smriti, the offline
photo library.

## User guide

- [Importing Google Photos](user-guide/google-photos-import.md) — migrate
  a Google Takeout export directly from its ZIP files.

- [Getting Started](user-guide/getting-started.md) — first run, picking a
  library, where data lives.
- [Indexing Photos](user-guide/indexing.md) — what scanning does, what
  it skips, how to refresh.
- [Timeline](user-guide/timeline.md) — the main view, navigation,
  keyboard shortcuts.
- [People and Faces](user-guide/people.md) — face detection, naming,
  merging, reviewing.
- [Albums](user-guide/albums.md) — manual albums and suggested trips
  / events.
- [Memories](user-guide/memories.md) — "this day, N years ago" cards.
- [Map View](user-guide/map.md) — geographic exploration and tile
  caching.
- [Insights](user-guide/insights.md) — heatmaps, top people, top
  locations.
- [Search](user-guide/search.md) — query syntax and examples.
- [Cleanup](user-guide/cleanup.md) — duplicates, bursts, and the
  trash workflow.
- [Settings](user-guide/settings.md) — every preference and what it
  changes.
- [Keyboard Shortcuts](user-guide/keyboard-shortcuts.md) — full list.
- [FAQ](user-guide/faq.md) — quick answers to common questions.
- [Troubleshooting](user-guide/troubleshooting.md) — common issues
  and fixes.

## For contributors

- [Build from Source](BUILD.md) — toolchain setup per OS.
- [Architecture Overview](architecture/overview.md) — the three-crate
  split: engine, Tauri shell, frontend.
- [Database](architecture/database.md) — schema and migration model.
- [Services](architecture/services.md) — domain workflows.
- [ML Pipeline](architecture/ml-pipeline.md) — face detection +
  embedding + clustering.
- [Face GPU Bridge](face-gpu-bridge.md) — opt-in remote GPU
  acceleration.
- [Testing](TESTING.md) — the test pyramid and what's enforced in CI.
  tags.

## Privacy and data

- [Privacy policy](../PRIVACY.md) — every outbound HTTP request,
  enumerated.
- [Security](../SECURITY.md) — private vulnerability disclosure.
- [Third-party licenses](../THIRD_PARTY_LICENSES.md) — full
  attribution list.
