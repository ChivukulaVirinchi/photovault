# Open Source Release — Implementation Plan

Status: **ready to implement**

Three phases turning PhotoVault from a private dev project into a
proper open-source desktop application with installers, CI/CD,
documentation, and a public landing page.

- **Phase 1** — Repository hygiene (legal, README, contribution infra)
- **Phase 2** — CI/CD + cross-platform installers
- **Phase 3** — User docs + stunning landing site

Prerequisites: production polish (separate plan) should land first so
we don't ship known UX bugs.

Decisions locked in:
- License: **MIT OR Apache-2.0** (dual-license, Rust standard)
- Name: **PhotoVault** (rename later if needed)
- ML models: **bundled in installers** (~280 MB extra download, but no
  first-run network needed — better UX, especially for offline-first ethos)
- Code signing: **skipped for v1.0** (warnings happen, document workaround)
- Website: **hand-written stunning HTML** on user-owned domain (no
  framework — Tailwind CDN + vanilla JS)
- Logo: **placeholder text wordmark** for v1.0 (replace later)

---

# Phase 1 — Repository hygiene

Goal: any developer landing on the repo immediately understands what
the project is, how to contribute, what license applies, and where to
find documentation.

## 1.1 License files

**New files:**
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `LICENSE` (short pointer file)

### `LICENSE` (root)
```
PhotoVault is dual-licensed under either:

- MIT License (see LICENSE-MIT)
- Apache License, Version 2.0 (see LICENSE-APACHE)

at your option.

This means you can choose either license to use the software under.
```

### `LICENSE-MIT`
Standard MIT license text with copyright line:
```
MIT License

Copyright (c) 2026 Virinchi Chivukula and PhotoVault contributors

Permission is hereby granted, free of charge, ...
```

### `LICENSE-APACHE`
Standard Apache 2.0 license text. Use the boilerplate from
https://www.apache.org/licenses/LICENSE-2.0.txt

## 1.2 Cargo.toml metadata

**Modify:** `Cargo.toml`

Replace the `[package]` block with:

```toml
[package]
name = "photovault"
version = "0.1.0"
edition = "2021"
authors = ["Virinchi Chivukula <your-email-here>"]
license = "MIT OR Apache-2.0"
description = "Offline-first desktop photo library manager with face recognition, smart albums, and full-text search."
repository = "https://github.com/ChivukulaVirinchi/photovault"
homepage = "https://your-domain-here.com"
documentation = "https://chivukulavirinchi.github.io/photovault"
readme = "README.md"
keywords = ["photos", "gallery", "face-recognition", "offline", "desktop"]
categories = ["multimedia::images", "gui"]
exclude = [
    "/docs",
    "/scripts",
    "/.github",
    "/website",
    "/assets/screenshots",
    "/.claude",
]
```

Also add release profile tuning at the bottom:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"
```

These cut binary size ~30% and improve runtime performance.

## 1.3 README.md

**New file:** `README.md` at repo root.

Structure (copy this template, fill in real content):

```markdown
<div align="center">
  <h1>PhotoVault</h1>
  <p><strong>An offline-first desktop photo library manager.</strong></p>
  <p>
    Organize, search, and rediscover your photos.
    Works with photos on your local drives. No cloud, no telemetry, no account required.
  </p>

  <p>
    <a href="https://github.com/ChivukulaVirinchi/photovault/releases/latest">
      <img src="https://img.shields.io/github/v/release/ChivukulaVirinchi/photovault?label=Download&style=for-the-badge" alt="Download">
    </a>
    <a href="LICENSE">
      <img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=for-the-badge" alt="License">
    </a>
  </p>

  <img src="assets/screenshots/hero.png" alt="PhotoVault screenshot" width="800">
</div>

---

## What it does

- **Indexes photos** from any folder or external drive — no upload required
- **Recognizes faces** using on-device ML, groups photos by person
- **Suggests albums** for trips and events automatically
- **Surfaces memories** — "N years ago today" rediscovery
- **Visualizes geography** — interactive map of where photos were taken
- **Shows insights** — heatmaps, top people, top locations, monthly breakdown
- **Finds anything** — unified search across people, albums, places, photos

## Why offline?

Your photos are personal. PhotoVault keeps them that way.

- No cloud upload. Photos stay on your drive.
- No account required. No telemetry. No analytics.
- Works without internet (after initial install).
- Database lives on the indexed drive — fully portable.

## Install

### Linux
- **AppImage** (universal): [Download](https://github.com/.../releases/latest) → `chmod +x` → run
- **Debian/Ubuntu**: `.deb` package available on the [releases page](https://github.com/.../releases/latest)
- **Fedora/RHEL**: `.rpm` package available on the [releases page](https://github.com/.../releases/latest)

### Windows
- Download the **portable .zip** or **.msi installer** from the [releases page](https://github.com/.../releases/latest)
- ⚠️ Windows SmartScreen will warn — click "More info" → "Run anyway"
  (We don't have a code-signing certificate yet)

### macOS
- Download the **.dmg** from the [releases page](https://github.com/.../releases/latest)
- ⚠️ macOS Gatekeeper will warn — right-click → "Open" → "Open anyway"
  (We don't have an Apple Developer ID yet)
- Or via Homebrew: `brew install ChivukulaVirinchi/photovault/photovault` *(coming soon)*

## Build from source

```bash
git clone https://github.com/ChivukulaVirinchi/photovault.git
cd photovault
./scripts/setup_assets.sh    # downloads ONNX models + GeoNames data
cargo build --release
./target/release/photovault
```

See [BUILD.md](docs/BUILD.md) for full developer setup including Windows.

## Documentation

- [User Guide](https://your-domain.com/docs/) — feature walkthroughs
- [Keyboard Shortcuts](https://your-domain.com/docs/shortcuts) — full reference
- [FAQ](https://your-domain.com/docs/faq)
- [Troubleshooting](https://your-domain.com/docs/troubleshooting)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
how to set up a dev environment, run tests, and submit pull requests.

Look at [Future Scope](docs/FUTURE_SCOPE.md) for planned features
that need contributors.

## License

Dual-licensed under either:
- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.

## Built with

- [Rust](https://www.rust-lang.org/)
- [iced](https://iced.rs/) — cross-platform GUI
- [SQLite](https://sqlite.org/) — embedded database
- [ONNX Runtime](https://onnxruntime.ai/) — face detection/recognition
- [GeoNames](https://www.geonames.org/) — offline reverse geocoding
- [OpenStreetMap](https://www.openstreetmap.org/) — map tiles
```

## 1.4 PRIVACY.md

**New file:** `PRIVACY.md`

The differentiator. Make it explicit and human-readable.

```markdown
# Privacy

PhotoVault is offline-first. Here's exactly what that means.

## What stays local

**All of your photos.** PhotoVault never uploads, syncs, or transmits
your photos anywhere. They stay on your drive.

**All of your metadata.** Face data, locations, dates, EXIF — all stored
in a SQLite database on the same drive as your photos. Nothing leaves.

**All of your activity.** No analytics, no usage tracking, no telemetry.
Period.

## What touches the network

Three things, by design:

1. **Map tiles** (OpenStreetMap)
   When you open the Map view, PhotoVault downloads map tiles for the
   regions you view. These are cached locally. To opt out: avoid the
   Map view, or limit cache size in Settings.

2. **First-run asset download** (one-time, optional)
   ONNX face recognition models (~280 MB) and GeoNames geocoding data
   (~30 MB) need to be downloaded once on first launch — unless you
   installed via the bundled installer (which includes everything).

3. **Update check** (optional, opt-in)
   If enabled in Settings, PhotoVault checks GitHub Releases for new
   versions. No personal data is sent. Default: off.

## Where your data lives

- **Photo database**: in a `.photovault/` folder on the indexed drive itself
- **Application config**: in your OS user config directory:
  - Linux: `~/.config/photovault/`
  - macOS: `~/Library/Application Support/photovault/`
  - Windows: `%APPDATA%\photovault\`
- **Map tile cache**: in your OS user cache directory
- **Crash logs** (if any): in your OS user data directory

## What we don't do

- No accounts, no sign-in
- No telemetry, no analytics
- No "anonymous usage statistics"
- No cloud backup
- No "shared features" requiring servers
- No third-party trackers
- No ads

## Reporting a privacy concern

Open an issue at https://github.com/ChivukulaVirinchi/photovault/issues
or email [your-email].
```

## 1.5 THIRD_PARTY_LICENSES.md

**New file:** `THIRD_PARTY_LICENSES.md`

```markdown
# Third-Party Licenses and Attributions

PhotoVault uses the following third-party assets and dependencies.

## Data

### GeoNames
Geographical data used for offline reverse geocoding.
- License: [Creative Commons Attribution 4.0 (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/)
- Attribution: © GeoNames, https://www.geonames.org/
- Used in: location lookups in photo detail view, trip detection

### OpenStreetMap
Map tiles used in the Map view.
- License: [Open Database License (ODbL)](https://www.openstreetmap.org/copyright)
- Attribution: © OpenStreetMap contributors
- The map view includes the required attribution overlay.

## ML Models

### MTCNN / RetinaFace (face detection)
- Source: [list source]
- License: [list license]

### GLinTR-100 (face recognition)
- Source: [list source]
- License: [list license]

## Software dependencies

PhotoVault uses many Rust crates. Run the following to generate a
full list of licenses:

```bash
cargo install cargo-license
cargo license --json > THIRD_PARTY_LICENSES.json
```

A summary of major dependencies and their licenses:

| Crate | License |
|-------|---------|
| iced | MIT |
| rusqlite | MIT |
| tokio | MIT |
| ort (ONNX Runtime) | MIT/Apache-2.0 |
| image | MIT/Apache-2.0 |
| chrono | MIT/Apache-2.0 |

For the full list with versions, see `THIRD_PARTY_LICENSES.json`.
```

## 1.6 CONTRIBUTING.md

**New file:** `CONTRIBUTING.md`

```markdown
# Contributing to PhotoVault

Thanks for your interest. Contributions of all sizes are welcome.

## Quick start

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR-USERNAME/photovault.git`
3. Install Rust 1.75+: https://rustup.rs/
4. Set up assets: `./scripts/setup_assets.sh` (Linux) or `scripts\setup_assets.ps1` (Windows)
5. Build and run: `cargo run`
6. Create a feature branch: `git checkout -b my-feature`
7. Make your changes, run `cargo fmt && cargo clippy && cargo test`
8. Push and open a pull request

## Where to find work

- **Good first issues**: see the [`good first issue` label](https://github.com/.../labels/good%20first%20issue)
- **Future scope features**: see [docs/FUTURE_SCOPE.md](docs/FUTURE_SCOPE.md)
- **Documentation**: improvements to user docs are always welcome
- **Bug reports**: search issues, file new ones with the bug template

## Development

### Code style

- Run `cargo fmt` before committing (CI enforces this)
- Run `cargo clippy -- -D warnings` (CI enforces this)
- Write tests for non-trivial logic
- Keep commits focused — one logical change per commit

### Running tests

```bash
cargo test                        # unit tests
cargo test -- --include-ignored   # also runs slow integration tests
```

### Architecture overview

See [docs/architecture/overview.md](docs/architecture/overview.md) for
a high-level explanation of the codebase.

The short version:
- `src/db/` — SQLite layer, one repo per entity
- `src/services/` — business logic (scanning, face processing, etc.)
- `src/ml/` — ONNX model wrappers
- `src/app/` — iced state machine + message handlers
- `src/views/` — UI rendering per view
- `src/components/` — reusable UI widgets

### Critical iced rules

iced 0.13 has a few sharp edges. Read these before writing UI code:
- NEVER use `height(Length::Fill)` inside a `scrollable` — it panics
- A `button` without `on_press` is disabled and BLOCKS event propagation
  to children. Don't wrap interactive widgets in disabled buttons.
- The PhotoDetail view uses an early `return` in `app/views.rs` — any
  overlay (modal, picker) must also be handled there, not just in
  the main content path.

## Pull request guidelines

- One feature per PR. Don't bundle unrelated changes.
- Update relevant documentation.
- Add tests for new behavior.
- Update `CHANGELOG.md` under the `[Unreleased]` section.
- Reference any related issues: "Closes #123"

## Reporting bugs

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md).
Include:
- OS + version
- PhotoVault version
- Steps to reproduce
- Expected vs actual behavior
- Logs from `~/.config/photovault/photovault.log` (Linux) or equivalent

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
By participating, you agree to its terms.

## License

By contributing, you agree that your contributions will be licensed
under the same MIT OR Apache-2.0 dual license as the project.
```

## 1.7 CODE_OF_CONDUCT.md

**New file:** `CODE_OF_CONDUCT.md`

Use the standard Contributor Covenant 2.1 boilerplate. Get from:
https://www.contributor-covenant.org/version/2/1/code_of_conduct/

## 1.8 SECURITY.md

**New file:** `SECURITY.md`

```markdown
# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in PhotoVault, please report
it privately by emailing [your-security-email].

Please do **not** open a public issue for security vulnerabilities.

We aim to respond within 7 days and provide a fix within 30 days for
critical issues.

## Scope

Issues in scope:
- Code execution from malicious image files
- Path traversal in file operations
- SQL injection in database queries
- Sensitive data leaking from local logs/config

Out of scope:
- Bugs in dependencies (report upstream)
- Issues requiring physical access to the device
- Cloud-related concerns (PhotoVault has no cloud component)

## Disclosure

We follow coordinated disclosure: once a fix is released, the
vulnerability will be documented in CHANGELOG.md and credited to the
reporter (if desired).
```

## 1.9 CHANGELOG.md

**New file:** `CHANGELOG.md`

Follow [Keep a Changelog](https://keepachangelog.com/) format.

```markdown
# Changelog

All notable changes to PhotoVault will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-XX-XX

Initial public release.

### Added
- Photo library indexing from any folder or external drive
- EXIF metadata extraction (date, GPS, camera, exposure)
- SQLite database stored on the indexed drive (fully portable)
- Thumbnail generation with three quality tiers
- Face detection and recognition with interactive review queue
- Person clustering with merge / split / rename
- Inferred identities for photos without visible faces
- Duplicate detection (exact + perceptual)
- Burst detection with best-photo suggestions
- Soft delete with auto-purge after configurable retention
- OCR document detection (screenshots, receipts, business cards)
- Map view with tile caching, pin clustering, click-to-explore
- Memories: "N years ago today", seasonal recaps, year recaps
- Memory slideshow with auto-advance
- Manual albums: create, rename, delete, set cover
- Smart album suggestions for trips and events
- Insights dashboard: heatmap, stats, top people, top locations
- Unified search across people, albums, places, photos
- Recent search history
- Cross-platform: Linux, Windows, macOS
- Light + Dark themes
```

## 1.10 GitHub templates

**New files:**
- `.github/ISSUE_TEMPLATE/bug_report.md`
- `.github/ISSUE_TEMPLATE/feature_request.md`
- `.github/ISSUE_TEMPLATE/config.yml`
- `.github/PULL_REQUEST_TEMPLATE.md`

### `.github/ISSUE_TEMPLATE/bug_report.md`

```markdown
---
name: Bug report
about: Something isn't working as expected
labels: bug
---

## Description

A clear description of the bug.

## Steps to reproduce

1. ...
2. ...
3. ...

## Expected behavior

What you expected to happen.

## Actual behavior

What actually happened.

## Environment

- OS: (e.g. Ubuntu 22.04, Windows 11, macOS 14.2)
- PhotoVault version: (Settings → About, or `photovault --version`)
- Library size: (approximately how many photos)

## Logs

Relevant log entries from `~/.config/photovault/photovault.log` (Linux),
`%APPDATA%\photovault\photovault.log` (Windows), or
`~/Library/Application Support/photovault/photovault.log` (macOS).

## Screenshots

If applicable, add screenshots.
```

### `.github/ISSUE_TEMPLATE/feature_request.md`

```markdown
---
name: Feature request
about: Suggest a new feature
labels: enhancement
---

## What problem does this solve?

Describe the user need.

## Proposed solution

How you'd like it to work.

## Alternatives considered

Other approaches you thought about.

## Additional context

Mockups, links, related issues.
```

### `.github/ISSUE_TEMPLATE/config.yml`

```yaml
blank_issues_enabled: false
contact_links:
  - name: Documentation
    url: https://your-domain.com/docs/
    about: User guide and FAQ
  - name: GitHub Discussions
    url: https://github.com/ChivukulaVirinchi/photovault/discussions
    about: Ask questions, share ideas, get help
```

### `.github/PULL_REQUEST_TEMPLATE.md`

```markdown
## Summary

What does this PR do? Reference any related issues with "Closes #123".

## Testing

- [ ] `cargo test` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] Tested manually on: (Linux / Windows / macOS — list platforms)

## Screenshots

For UI changes, include before/after screenshots.

## Checklist

- [ ] Updated CHANGELOG.md under [Unreleased]
- [ ] Updated relevant documentation
- [ ] Added tests for new logic (or explained why not applicable)
```

## 1.11 docs/FUTURE_SCOPE.md (replaces FEATURES_ROADMAP.md)

**New file:** `docs/FUTURE_SCOPE.md`

Move all Tier 2/3/4 features from `FEATURES_ROADMAP.md` into a
contributor-friendly format. Each feature gets:
- Description
- Why it matters
- Technical approach
- Estimated complexity (S/M/L)
- Skills needed
- Status (open / in progress / claimed by @username)

Template entry:

```markdown
### Scene / object classification

**Status**: open · seeking contributors · complexity: M · skills: Rust, ML

**What**: A small ONNX classifier (~10-20 MB) tags each photo with
detected scenes/objects: "food", "beach", "dog", "indoor", etc.

**Why**: Unlocks search by content type. "show me beach photos" or
"all photos with dogs" — currently impossible without this.

**Technical approach**:
- Use a pre-trained MobileNet or EfficientNet variant exported to ONNX
- Run during photo indexing pipeline (after EXIF extraction)
- Store top-3 labels + confidence in a new `photo_tags` table
- Add tag chips to search filters
- See `src/ml/face_processor.rs` for the existing ML pipeline pattern

**Files likely touched**:
- New: `src/ml/scene_classifier.rs`, `src/services/scene_tagger.rs`
- Modified: `src/db/schema.rs` (new table), `src/services/scanner.rs` (pipeline integration), `src/services/search.rs` (tag filter)

**To claim**: comment on issue #N (or open a new issue if none exists)
```

Do this for each Tier 2/3/4 feature from FEATURES_ROADMAP.md. After
this doc exists, **delete FEATURES_ROADMAP.md**.

## 1.12 Reorganize docs/

**Delete:**
- `docs/FEATURES_ROADMAP.md` (content moved to FUTURE_SCOPE.md)

**Move/rename:**
- `docs/MEMORIES.md` → `docs/user-guide/memories.md`
- `docs/SETUP_ASSETS.md` → `docs/BUILD.md` (expand into proper build guide)
- `docs/FACE_RECOGNITION_IMPROVEMENTS.md` → keep as-is (future improvement playbook)

**Final docs/ structure:**
```
docs/
  BUILD.md                    # how to build from source
  FUTURE_SCOPE.md             # contributor-facing feature list
  FACE_RECOGNITION_IMPROVEMENTS.md  # internal improvement playbook
  architecture/               # contributor-facing architecture (Phase 3)
    overview.md
    database.md
    state.md
    services.md
    ml-pipeline.md
  user-guide/                 # user-facing docs (Phase 3)
    getting-started.md
    memories.md
    albums.md
    insights.md
    search.md
    keyboard-shortcuts.md
    faq.md
    troubleshooting.md
  plans/
    production_polish.md      # in-progress
    open_source_release.md    # this file
```

## 1.13 Phase 1 commit

```bash
git commit -m "Open-source release prep Phase 1: legal + repo hygiene

- Dual-license under MIT OR Apache-2.0
- Cargo.toml metadata for crates.io / discoverability
- README.md with install instructions, features, why-offline
- PRIVACY.md documenting offline-first guarantees
- THIRD_PARTY_LICENSES.md for GeoNames / OSM / models
- CONTRIBUTING.md with dev setup + iced gotchas
- CODE_OF_CONDUCT.md (Contributor Covenant 2.1)
- SECURITY.md vulnerability disclosure
- CHANGELOG.md (Keep a Changelog format)
- GitHub issue + PR templates
- docs/FUTURE_SCOPE.md replacing FEATURES_ROADMAP.md with
  contributor-friendly format for Tier 2-4 features
- Reorganized docs/ into BUILD / user-guide / architecture sections"
```

---

# Phase 2 — CI/CD + cross-platform installers

Goal: tagging `v0.1.0` produces verified, downloadable installers for
all 3 platforms automatically. Asset bundling included.

## 2.1 GitHub Actions CI workflow

**New file:** `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [master, main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    name: Clippy
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux deps
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb-shape0-dev libxcb-xfixes0-dev
      - run: cargo clippy --all-targets -- -D warnings

  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux deps
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb-shape0-dev libxcb-xfixes0-dev
      - run: cargo test --no-run
      - run: cargo test
```

## 2.2 Release workflow

**New file:** `.github/workflows/release.yml`

```yaml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            archive: tar.gz
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            archive: zip
          - target: x86_64-apple-darwin
            os: macos-13         # Intel
            archive: tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest     # Apple Silicon
            archive: tar.gz
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux deps
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb-shape0-dev libxcb-xfixes0-dev libssl-dev pkg-config

      - name: Download ML models + GeoNames data
        run: ./scripts/setup_assets.sh
        if: matrix.os != 'windows-latest'
        shell: bash

      - name: Download ML models + GeoNames data (Windows)
        run: powershell -ExecutionPolicy Bypass -File scripts/setup_assets.ps1
        if: matrix.os == 'windows-latest'

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }}

      - name: Stage artifact directory
        shell: bash
        run: |
          mkdir -p staging/photovault
          if [ "${{ matrix.os }}" = "windows-latest" ]; then
            cp target/${{ matrix.target }}/release/photovault.exe staging/photovault/
          else
            cp target/${{ matrix.target }}/release/photovault staging/photovault/
          fi
          # Bundle assets
          cp -r libs staging/photovault/
          cp -r models staging/photovault/
          cp -r data staging/photovault/
          cp README.md LICENSE LICENSE-MIT LICENSE-APACHE staging/photovault/
          cp scripts/setup_assets.sh scripts/setup_assets.ps1 staging/photovault/scripts/ 2>/dev/null || true

      - name: Create archive (tar.gz)
        if: matrix.archive == 'tar.gz'
        run: |
          cd staging
          tar -czf ../photovault-${{ matrix.target }}.tar.gz photovault

      - name: Create archive (zip)
        if: matrix.archive == 'zip'
        shell: pwsh
        run: |
          Compress-Archive -Path staging\photovault -DestinationPath photovault-${{ matrix.target }}.zip

      - uses: actions/upload-artifact@v4
        with:
          name: photovault-${{ matrix.target }}
          path: photovault-${{ matrix.target }}.*

  appimage:
    name: Linux AppImage
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: photovault-x86_64-unknown-linux-gnu
      - name: Extract
        run: tar -xzf photovault-x86_64-unknown-linux-gnu.tar.gz
      - name: Install AppImage tools
        run: |
          wget -q https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
          chmod +x appimagetool-x86_64.AppImage
      - name: Build AppDir
        run: |
          mkdir -p PhotoVault.AppDir/usr/bin
          mkdir -p PhotoVault.AppDir/usr/share/applications
          mkdir -p PhotoVault.AppDir/usr/share/icons/hicolor/256x256/apps
          cp -r photovault/* PhotoVault.AppDir/usr/bin/
          cp packaging/photovault.desktop PhotoVault.AppDir/usr/share/applications/
          cp packaging/photovault.png PhotoVault.AppDir/usr/share/icons/hicolor/256x256/apps/
          cp packaging/photovault.desktop PhotoVault.AppDir/
          cp packaging/photovault.png PhotoVault.AppDir/
          cat > PhotoVault.AppDir/AppRun <<'EOF'
          #!/bin/bash
          HERE="$(dirname "$(readlink -f "${0}")")"
          cd "${HERE}/usr/bin"
          export LD_LIBRARY_PATH="${HERE}/usr/bin/libs/onnxruntime:$LD_LIBRARY_PATH"
          exec "${HERE}/usr/bin/photovault" "$@"
          EOF
          chmod +x PhotoVault.AppDir/AppRun
      - name: Build AppImage
        run: ./appimagetool-x86_64.AppImage PhotoVault.AppDir PhotoVault-x86_64.AppImage
      - uses: actions/upload-artifact@v4
        with:
          name: photovault-appimage
          path: PhotoVault-x86_64.AppImage

  deb:
    name: Linux .deb
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-deb
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux deps
        run: |
          sudo apt-get update
          sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb-shape0-dev libxcb-xfixes0-dev
      - name: Build .deb
        run: cargo deb
      - uses: actions/upload-artifact@v4
        with:
          name: photovault-deb
          path: target/debian/*.deb

  release:
    name: Create GitHub Release
    needs: [build, appimage, deb]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          path: artifacts
      - name: Create release
        uses: softprops/action-gh-release@v1
        with:
          files: artifacts/**/*
          generate_release_notes: true
          draft: true       # initially as draft for manual review
```

## 2.3 cargo-deb config

**Modify:** `Cargo.toml`

Add at the bottom:

```toml
[package.metadata.deb]
maintainer = "Virinchi Chivukula <your-email>"
copyright = "2026, Virinchi Chivukula"
license-file = ["LICENSE", "0"]
extended-description = """\
PhotoVault is an offline-first desktop photo library manager.
Indexes photos from local drives, detects faces, finds duplicates,
suggests albums, and provides full-text search — all without uploading
anything to the cloud.
"""
depends = "$auto"
section = "graphics"
priority = "optional"
assets = [
    ["target/release/photovault", "usr/bin/", "755"],
    ["libs/onnxruntime/libonnxruntime.so*", "usr/lib/photovault/libs/onnxruntime/", "644"],
    ["models/*.onnx", "usr/lib/photovault/models/", "644"],
    ["data/geonames.db", "usr/lib/photovault/data/", "644"],
    ["packaging/photovault.desktop", "usr/share/applications/", "644"],
    ["packaging/photovault.png", "usr/share/icons/hicolor/256x256/apps/", "644"],
    ["README.md", "usr/share/doc/photovault/README", "644"],
]
```

## 2.4 Packaging files

**New files:**
- `packaging/photovault.desktop`
- `packaging/photovault.png` (256x256 PNG, placeholder for now)

### `packaging/photovault.desktop`

```ini
[Desktop Entry]
Name=PhotoVault
Comment=Offline-first photo library manager
Exec=photovault
Icon=photovault
Type=Application
Categories=Graphics;Photography;
Keywords=photos;gallery;offline;
StartupWMClass=photovault
```

## 2.5 Windows installer config

**New file:** `wix/main.wxs` (template for `cargo-wix`)

Generate initial template:
```bash
cargo install cargo-wix
cargo wix init
```

This creates `wix/main.wxs`. Customize to:
- Set product name, manufacturer, upgrade code
- Bundle the libs/, models/, data/ directories alongside the binary
- Add Start Menu shortcut
- Add Add/Remove Programs registration

## 2.6 macOS .app bundle config

**Modify:** `Cargo.toml`

```toml
[package.metadata.bundle]
name = "PhotoVault"
identifier = "com.chivukulavirinchi.photovault"
icon = ["packaging/photovault.icns"]
version = "0.1.0"
copyright = "2026, Virinchi Chivukula"
category = "public.app-category.photography"
short_description = "Offline-first photo library manager"
long_description = """
PhotoVault organizes your photos locally, with face recognition,
smart albums, and full-text search — without uploading anything.
"""
resources = ["libs", "models", "data"]
osx_minimum_system_version = "10.13"
```

Use `cargo-bundle` to build:
```bash
cargo install cargo-bundle
cargo bundle --release
```

Then wrap the .app in a .dmg:
```bash
brew install create-dmg  # on macOS CI runner
create-dmg PhotoVault.app PhotoVault.dmg
```

Add this to the release workflow's macOS jobs.

## 2.7 Logo / icon placeholder

For v1.0 we ship a placeholder. Generate a simple text-based icon:

**New files:**
- `packaging/photovault.png` (256x256, simple "PV" wordmark on accent background)
- `packaging/photovault.ico` (Windows icon, generated from PNG)
- `packaging/photovault.icns` (Mac icon, generated from PNG)

Quick generation:
```bash
# Use ImageMagick to create a placeholder
convert -size 256x256 xc:'#0F0F11' \
  -fill '#D49E3C' \
  -font Helvetica-Bold -pointsize 120 \
  -gravity center -annotate +0+0 'PV' \
  packaging/photovault.png

# Generate Windows .ico
convert packaging/photovault.png \
  -define icon:auto-resize=256,128,64,48,32,16 \
  packaging/photovault.ico

# Generate Mac .icns (on macOS)
mkdir photovault.iconset
sips -z 16 16   packaging/photovault.png --out photovault.iconset/icon_16x16.png
sips -z 32 32   packaging/photovault.png --out photovault.iconset/icon_16x16@2x.png
# ... etc
iconutil -c icns photovault.iconset
```

User can replace these with real artwork later — the file paths stay
the same.

## 2.8 Documentation: install and build

**New file:** `docs/BUILD.md` (replaces `docs/SETUP_ASSETS.md`)

Comprehensive build guide:
- Prerequisites per platform (Rust, libraries, Visual Studio Build Tools, etc.)
- Asset setup (existing scripts)
- Build commands (debug, release)
- Run with logging
- Cross-compile notes
- Troubleshooting common build errors

## 2.9 Verify install on each platform

Manual checklist after first release:
- [ ] Linux AppImage: download, chmod, run — opens to welcome screen
- [ ] Linux .deb: `sudo dpkg -i`, launches from app menu
- [ ] Windows .zip: extract, double-click `.exe`, app opens (warn-through SmartScreen)
- [ ] Windows .msi: install, finds in Start Menu, opens
- [ ] macOS .dmg: open, drag to Applications, right-click → Open (warn-through Gatekeeper)

## 2.10 Phase 2 commit

```bash
git commit -m "Open-source release Phase 2: CI/CD + cross-platform installers

- GitHub Actions CI: fmt, clippy, test on Linux/Windows/macOS
- Release workflow: tagged builds → installers for all 3 platforms
- AppImage build for universal Linux
- cargo-deb config for Debian/Ubuntu .deb
- cargo-wix config for Windows .msi
- cargo-bundle config for macOS .app + .dmg
- Asset bundling: ONNX models + GeoNames included in installers
- packaging/ directory with .desktop, icons, installer configs
- docs/BUILD.md replacing SETUP_ASSETS with full build guide"
```

---

# Phase 3 — User docs + stunning landing site

Goal: a beautiful landing page that makes people want to download, plus
proper docs for users and contributors.

## 3.1 mdBook documentation site

**New file:** `book.toml` at repo root

```toml
[book]
authors = ["Virinchi Chivukula"]
language = "en"
multilingual = false
src = "docs"
title = "PhotoVault Documentation"
description = "Offline-first photo library manager — user guide and API docs"

[output.html]
theme = "docs/theme"
default-theme = "navy"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/ChivukulaVirinchi/photovault"
edit-url-template = "https://github.com/ChivukulaVirinchi/photovault/edit/master/docs/{path}"
site-url = "/"
fold = { enable = true, level = 1 }

[output.html.search]
enable = true
limit-results = 30
```

**New file:** `docs/SUMMARY.md` (mdBook table of contents)

```markdown
# Summary

[Introduction](README.md)

# User Guide

- [Getting Started](user-guide/getting-started.md)
- [Indexing Photos](user-guide/indexing.md)
- [Timeline](user-guide/timeline.md)
- [People & Faces](user-guide/people.md)
- [Albums](user-guide/albums.md)
- [Memories](user-guide/memories.md)
- [Map View](user-guide/map.md)
- [Insights](user-guide/insights.md)
- [Search](user-guide/search.md)
- [Cleanup (Duplicates, Bursts, Trash)](user-guide/cleanup.md)
- [Settings](user-guide/settings.md)
- [Keyboard Shortcuts](user-guide/keyboard-shortcuts.md)
- [FAQ](user-guide/faq.md)
- [Troubleshooting](user-guide/troubleshooting.md)

# For Contributors

- [Building from Source](BUILD.md)
- [Contributing](../CONTRIBUTING.md)
- [Architecture Overview](architecture/overview.md)
- [Database Schema](architecture/database.md)
- [State Machine](architecture/state.md)
- [Services Layer](architecture/services.md)
- [ML Pipeline](architecture/ml-pipeline.md)
- [Future Scope](FUTURE_SCOPE.md)
- [Face Recognition Improvements](FACE_RECOGNITION_IMPROVEMENTS.md)
```

## 3.2 Write user guide pages

For each user-guide page, write 1-3 paragraphs + screenshots showing
how to use the feature. Use existing knowledge from the codebase.

Files to create (in `docs/user-guide/`):
- `getting-started.md` — install, first launch, drive selection, scanning
- `indexing.md` — what gets indexed, where the database lives
- `timeline.md` — grid view, day groups, multi-select, hover, keyboard nav
- `people.md` — face detection, naming clusters, merging, review queue
- `albums.md` — manual albums + suggestions
- `memories.md` — already exists, move/expand
- `map.md` — pin clustering, popovers, mini-map in photo detail
- `insights.md` — year selector, heatmap, click-through navigation
- `search.md` — query syntax, entity results, recent searches
- `cleanup.md` — duplicates, bursts, trash retention
- `settings.md` — every setting explained
- `keyboard-shortcuts.md` — pulled from same source as in-app `?` overlay
- `faq.md` — common questions
- `troubleshooting.md` — common issues + fixes

## 3.3 Write contributor architecture docs

In `docs/architecture/`:

- `overview.md` — high-level diagram, module organization, data flow
- `database.md` — full schema, relationships, migrations
- `state.md` — iced state machine, message routing, View enum
- `services.md` — scanner, face_processor, etc.
- `ml-pipeline.md` — face detection → embedding → clustering pipeline

## 3.4 Build + deploy mdBook

**New file:** `.github/workflows/docs.yml`

```yaml
name: Build docs

on:
  push:
    branches: [master, main]
    paths:
      - 'docs/**'
      - 'book.toml'

permissions:
  contents: read
  pages: write
  id-token: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install mdBook
        run: |
          mkdir -p ~/bin
          curl -sSL https://github.com/rust-lang/mdBook/releases/download/v0.4.40/mdbook-v0.4.40-x86_64-unknown-linux-gnu.tar.gz | tar -xz -C ~/bin
          echo "$HOME/bin" >> $GITHUB_PATH
      - name: Build book
        run: mdbook build
      - uses: actions/upload-pages-artifact@v3
        with:
          path: book

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

Output URL: `https://chivukulavirinchi.github.io/photovault/`

Reference this URL in README, in-app links, etc.

## 3.5 Stunning landing page

**New directory:** `website/`

The landing page is a separate concern from docs. Single-page HTML
with Tailwind CSS via CDN. Goal: people land on it, get the value
prop in 3 seconds, click download.

### `website/index.html`

Structure (write the actual HTML):

```html
<!DOCTYPE html>
<html lang="en" class="scroll-smooth">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>PhotoVault — Your photos, organized. On your machine.</title>
  <meta name="description" content="An offline-first desktop photo library manager. Face recognition, smart albums, full-text search — without uploading anything." />

  <!-- Open Graph / Twitter -->
  <meta property="og:title" content="PhotoVault — Your photos, organized" />
  <meta property="og:description" content="Offline-first photo library manager for desktop." />
  <meta property="og:image" content="https://your-domain.com/og.png" />
  <meta property="og:url" content="https://your-domain.com" />
  <meta name="twitter:card" content="summary_large_image" />

  <!-- Tailwind via CDN (production: pin to a version) -->
  <script src="https://cdn.tailwindcss.com"></script>
  <script>
    tailwind.config = {
      theme: {
        extend: {
          colors: {
            'pv-bg': '#0F0F11',
            'pv-bg-elev': '#1E1E22',
            'pv-text': '#ECECEA',
            'pv-text-soft': '#8A8A8E',
            'pv-accent': '#D49E3C',
            'pv-accent-bright': '#E0AE50',
          },
          fontFamily: {
            sans: ['Inter', 'system-ui', 'sans-serif'],
          },
        },
      },
    };
  </script>

  <!-- Inter font -->
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap" rel="stylesheet">

  <!-- Favicon -->
  <link rel="icon" type="image/png" href="/photovault.png" />
</head>
<body class="bg-pv-bg text-pv-text font-sans antialiased">

  <!-- ============================================ -->
  <!-- NAV -->
  <!-- ============================================ -->
  <nav class="fixed top-0 inset-x-0 z-50 bg-pv-bg/80 backdrop-blur-md border-b border-white/5">
    <div class="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <span class="text-pv-accent font-bold text-xl">●</span>
        <span class="font-semibold tracking-tight">PhotoVault</span>
      </div>
      <div class="hidden md:flex gap-8 text-sm text-pv-text-soft">
        <a href="#features" class="hover:text-pv-text transition">Features</a>
        <a href="#why" class="hover:text-pv-text transition">Why offline</a>
        <a href="#download" class="hover:text-pv-text transition">Download</a>
        <a href="https://chivukulavirinchi.github.io/photovault/" class="hover:text-pv-text transition">Docs</a>
        <a href="https://github.com/ChivukulaVirinchi/photovault" class="hover:text-pv-text transition">GitHub</a>
      </div>
      <a href="#download" class="bg-pv-accent text-pv-bg px-4 py-2 rounded-md text-sm font-medium hover:bg-pv-accent-bright transition">Download</a>
    </div>
  </nav>

  <!-- ============================================ -->
  <!-- HERO -->
  <!-- ============================================ -->
  <section class="pt-32 pb-24 px-6">
    <div class="max-w-5xl mx-auto text-center">
      <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-pv-accent/10 text-pv-accent text-xs font-medium mb-8 border border-pv-accent/20">
        <span class="w-1.5 h-1.5 rounded-full bg-pv-accent"></span>
        Open source · Offline-first · Free forever
      </div>
      <h1 class="text-5xl md:text-7xl font-bold tracking-tight leading-[1.05] mb-6">
        Your photos, organized.<br>
        <span class="text-pv-accent">On your machine.</span>
      </h1>
      <p class="text-lg md:text-xl text-pv-text-soft max-w-2xl mx-auto mb-10 leading-relaxed">
        A desktop photo library manager that works with photos on your local drives.
        Face recognition, smart albums, full-text search — without uploading anything.
      </p>
      <div class="flex flex-wrap items-center justify-center gap-4">
        <a href="#download" class="bg-pv-accent text-pv-bg px-6 py-3 rounded-lg font-medium hover:bg-pv-accent-bright transition shadow-lg shadow-pv-accent/20">
          Download for free
        </a>
        <a href="https://github.com/ChivukulaVirinchi/photovault" class="border border-white/10 text-pv-text px-6 py-3 rounded-lg font-medium hover:bg-white/5 transition">
          View on GitHub
        </a>
      </div>
    </div>

    <!-- Hero screenshot -->
    <div class="max-w-6xl mx-auto mt-20">
      <div class="relative rounded-xl overflow-hidden border border-white/10 shadow-2xl shadow-pv-accent/5">
        <img src="hero-screenshot.png" alt="PhotoVault Timeline view" class="w-full" />
      </div>
    </div>
  </section>

  <!-- ============================================ -->
  <!-- FEATURES GRID -->
  <!-- ============================================ -->
  <section id="features" class="py-24 px-6 bg-pv-bg-elev/30">
    <div class="max-w-6xl mx-auto">
      <h2 class="text-4xl font-bold text-center mb-4">Everything you'd expect.<br>Nothing you didn't ask for.</h2>
      <p class="text-pv-text-soft text-center mb-16 max-w-2xl mx-auto">
        All the features of a modern photo library, with one rule: nothing leaves your device.
      </p>

      <div class="grid md:grid-cols-3 gap-6">
        <!-- Feature card template, repeat for: -->
        <!-- Face recognition, Smart albums, Memories, Map view, Insights, Unified search, Duplicate detection, Burst handling, OCR documents -->
        <div class="bg-pv-bg-elev p-8 rounded-xl border border-white/5 hover:border-pv-accent/30 transition group">
          <div class="w-10 h-10 rounded-lg bg-pv-accent/10 flex items-center justify-center mb-4 group-hover:bg-pv-accent/20 transition">
            <span class="text-pv-accent text-xl">⊙</span>
          </div>
          <h3 class="text-lg font-semibold mb-2">Face recognition</h3>
          <p class="text-sm text-pv-text-soft leading-relaxed">
            Detects and groups faces using on-device ML. Tag people once, find them everywhere.
          </p>
        </div>
        <!-- ... 8 more cards ... -->
      </div>
    </div>
  </section>

  <!-- ============================================ -->
  <!-- WHY OFFLINE -->
  <!-- ============================================ -->
  <section id="why" class="py-24 px-6">
    <div class="max-w-4xl mx-auto">
      <h2 class="text-4xl font-bold mb-12 text-center">Why offline-first?</h2>
      <div class="space-y-12">
        <div class="flex gap-6 items-start">
          <div class="text-pv-accent text-3xl flex-shrink-0">01</div>
          <div>
            <h3 class="text-xl font-semibold mb-2">Your photos are personal.</h3>
            <p class="text-pv-text-soft leading-relaxed">
              Family, kids, intimate moments. They don't belong on someone else's server,
              and they certainly don't belong in someone else's training data.
            </p>
          </div>
        </div>
        <!-- ... more reasons ... -->
      </div>
    </div>
  </section>

  <!-- ============================================ -->
  <!-- SCREENSHOTS GALLERY -->
  <!-- ============================================ -->
  <section class="py-24 px-6 bg-pv-bg-elev/30">
    <div class="max-w-6xl mx-auto">
      <h2 class="text-4xl font-bold text-center mb-16">A closer look</h2>
      <!-- Tabbed or grid screenshots: Map, Insights, Memories, etc. -->
    </div>
  </section>

  <!-- ============================================ -->
  <!-- DOWNLOAD -->
  <!-- ============================================ -->
  <section id="download" class="py-24 px-6">
    <div class="max-w-4xl mx-auto text-center">
      <h2 class="text-4xl font-bold mb-4">Get PhotoVault</h2>
      <p class="text-pv-text-soft mb-12">Free, open source. No account, no signup.</p>

      <div class="grid md:grid-cols-3 gap-6">
        <!-- macOS card -->
        <a href="https://github.com/.../releases/latest/download/PhotoVault.dmg" class="bg-pv-bg-elev p-8 rounded-xl border border-white/10 hover:border-pv-accent/50 transition group text-left">
          <div class="text-3xl mb-4">🍎</div>
          <h3 class="text-xl font-semibold mb-2 group-hover:text-pv-accent transition">macOS</h3>
          <p class="text-sm text-pv-text-soft mb-4">Universal binary (Intel + Apple Silicon)</p>
          <div class="text-pv-accent text-sm font-medium">Download .dmg →</div>
        </a>

        <!-- Windows card -->
        <a href="https://github.com/.../releases/latest/download/PhotoVault-Setup.msi" class="bg-pv-bg-elev p-8 rounded-xl border border-white/10 hover:border-pv-accent/50 transition group text-left">
          <div class="text-3xl mb-4">🪟</div>
          <h3 class="text-xl font-semibold mb-2 group-hover:text-pv-accent transition">Windows</h3>
          <p class="text-sm text-pv-text-soft mb-4">Windows 10 or later</p>
          <div class="text-pv-accent text-sm font-medium">Download .msi →</div>
        </a>

        <!-- Linux card -->
        <a href="https://github.com/.../releases/latest/download/PhotoVault.AppImage" class="bg-pv-bg-elev p-8 rounded-xl border border-white/10 hover:border-pv-accent/50 transition group text-left">
          <div class="text-3xl mb-4">🐧</div>
          <h3 class="text-xl font-semibold mb-2 group-hover:text-pv-accent transition">Linux</h3>
          <p class="text-sm text-pv-text-soft mb-4">AppImage, .deb, .rpm</p>
          <div class="text-pv-accent text-sm font-medium">View all →</div>
        </a>
      </div>

      <p class="text-xs text-pv-text-soft mt-8">
        Download size: ~350 MB (includes ML models). System requirements: 4 GB RAM, 1 GB disk.
      </p>
    </div>
  </section>

  <!-- ============================================ -->
  <!-- FOOTER -->
  <!-- ============================================ -->
  <footer class="py-12 px-6 border-t border-white/5">
    <div class="max-w-6xl mx-auto flex flex-wrap items-center justify-between gap-6">
      <div class="text-sm text-pv-text-soft">
        © 2026 PhotoVault. Open source under MIT or Apache 2.0.
      </div>
      <div class="flex gap-6 text-sm text-pv-text-soft">
        <a href="https://github.com/ChivukulaVirinchi/photovault" class="hover:text-pv-text transition">GitHub</a>
        <a href="https://chivukulavirinchi.github.io/photovault/" class="hover:text-pv-text transition">Docs</a>
        <a href="https://github.com/.../blob/master/PRIVACY.md" class="hover:text-pv-text transition">Privacy</a>
        <a href="https://github.com/.../issues" class="hover:text-pv-text transition">Issues</a>
      </div>
    </div>
  </footer>

</body>
</html>
```

This is a single self-contained file. Just open in a browser to preview.

### Required assets

In `website/`:
- `index.html` (above)
- `hero-screenshot.png` (1600x1000, the killer first impression)
- `feature-*.png` (smaller screenshots for the gallery section)
- `og.png` (1200x630, for social media previews)
- `photovault.png` (favicon, 256x256)

Take screenshots from the running app — Timeline view with photos for
hero, Map view for one feature card, Insights for another, Memories
banner for another.

## 3.6 Deploy website

User has their own domain. Two simplest options:

**Option A: GitHub Pages from `/website` folder + custom domain**
1. Push `website/` to a `gh-pages` branch (or use `/website` on master)
2. In repo settings → Pages → Source: select branch + folder
3. Custom domain: enter your domain
4. Add DNS records (CNAME or A records as instructed by GitHub)
5. Enable HTTPS

**Option B: Netlify / Vercel / Cloudflare Pages**
1. Connect GitHub repo
2. Set build dir to `website/`
3. No build command (static HTML)
4. Add custom domain in their dashboard

I recommend **Option A** for simplicity — single source of truth on
GitHub, no third-party account needed.

## 3.7 Phase 3 commit

```bash
git commit -m "Open-source release Phase 3: docs site + landing page

- mdBook documentation site at /docs/ with user guide + architecture
- 14 user guide pages covering every feature
- 5 architecture docs for contributors
- GitHub Actions workflow auto-builds + deploys to GitHub Pages
- /website/ stunning landing page (single-file HTML + Tailwind CDN)
- Hero, features grid, why-offline, screenshots, download CTAs
- Custom domain ready, deploys via GitHub Pages"
```

---

# Cross-cutting concerns

## What this plan does NOT cover

- **Auto-update mechanism** — defer to v1.1
- **Code signing** — requires developer accounts ($99 Apple, $300 Windows). Defer.
- **Localization (i18n)** — defer to v2.0, marked as future contribution
- **Package manager submissions** (Homebrew tap, AUR, Flathub, Snap, Chocolatey, Scoop, winget) — defer past v1.0
- **Telemetry / crash reporting auto-upload** — out of scope (offline ethos)
- **Crash log generation** — defer (could be in production polish phase)
- **Real logo design** — placeholder text wordmark for v1.0; replace later
- **Localized installers** — English only for v1.0
- **Update notification system** — could be added later as opt-in

## Decisions baked in

| Question | Answer |
|----------|--------|
| License | MIT OR Apache-2.0 dual |
| Project name | PhotoVault |
| Bundle ML models in installers | Yes (~280 MB extra) |
| Code signing | No (ship unsigned, document workaround) |
| Website framework | Hand-written HTML + Tailwind CDN |
| Logo for v1.0 | Text wordmark placeholder |
| Production polish phases (A/B/C) | Land first, before this plan |

## Total estimated work

- Phase 1: ~1.5 days (mostly writing markdown)
- Phase 2: ~3 days (CI debugging, installer config tuning, manual platform tests)
- Phase 3: ~3 days (docs writing + landing page polish + screenshots)

**Total: ~7-8 days of focused work** for a polished v0.1.0 public
release.

## After v0.1.0 release

- Submit to:
  - r/rust (weekly thread)
  - r/selfhosted
  - Hacker News (Show HN)
  - GitHub Trending push
- Open `good first issue` labels for community work
- Monitor issue tracker, respond within 48h
- Plan v0.2.0 milestone with community input
