<div align="center">
  <img src="docs/smriti-logo.svg" alt="Smriti" width="280">
  <p><em>स्मृति — that which is remembered</em></p>
  <p><strong>Your photo library, organized — on your machine.</strong></p>
  <p>
    Lightning fast on hundreds of thousands of photos. Offline-first
    by default. No cloud, no telemetry, no account required.
  </p>

  <p>
    <a href="https://github.com/ChivukulaVirinchi/photovault/releases/latest">
      <img src="https://img.shields.io/github/v/release/ChivukulaVirinchi/photovault?display_name=tag&include_prereleases&label=release" alt="Release">
    </a>
    <a href="https://github.com/ChivukulaVirinchi/photovault/actions/workflows/ci.yml">
      <img src="https://img.shields.io/github/actions/workflow/status/ChivukulaVirinchi/photovault/ci.yml?branch=master&label=CI" alt="CI">
    </a>
    <a href="https://github.com/ChivukulaVirinchi/photovault/blob/master/Cargo.toml">
      <img src="https://img.shields.io/badge/MSRV-1.75-blue" alt="MSRV 1.75">
    </a>
    <a href="LICENSE">
      <img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache 2.0">
    </a>
    <a href="https://github.com/ChivukulaVirinchi/photovault/releases/latest">
      <img src="https://img.shields.io/github/downloads/ChivukulaVirinchi/photovault/total?label=downloads" alt="Downloads">
    </a>
  </p>

  <img src="website/hero-screenshot.png" alt="Smriti screenshot" width="800">
</div>

---

## What it does

- **Indexes photos** from any folder or external drive — no upload required
- **Recognizes faces** on-device, groups photos by person
- **Suggests albums** for trips and events automatically
- **Surfaces memories** — "N years ago today" rediscovery
- **Visualizes geography** — interactive map of where photos were taken
- **Shows insights** — heatmaps, top people, top locations, monthly breakdown
- **Finds anything** — unified search across people, albums, places, photos

## Why offline?

Your photos are personal. Smriti keeps them that way.

- No cloud upload. Photos stay on your drive.
- No account required. No telemetry. No analytics.
- Database lives on the indexed drive — fully portable.
- Works without internet after optional assets are installed.

## Install

### Linux
- **Debian / Ubuntu**: [Download .deb](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-ubuntu-amd64.deb)
- **AppImage (other distros)**: [Download AppImage](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-x86_64.AppImage) → `chmod +x` → run
- **Optional assets pack**: [Smriti-Assets.zip](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-Assets.zip)
  — ML models for face recognition plus offline geocoding data.

Linux release artifacts include `SHA256SUMS` for integrity
verification. Tagged releases run smoke tests for both the `.deb`
and the AppImage in CI before publishing.

### Windows
- **Recommended**: [Download MSI installer](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-Setup-x64.msi)
- **Portable fallback**: [Download ZIP](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/smriti-x86_64-pc-windows-msvc.zip)
- **Optional assets pack**: [Smriti-Assets.zip](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-Assets.zip)

> Smriti's installer is not code-signed. Windows SmartScreen
> may warn on first launch — click **More info** → **Run anyway**.

### macOS
- Download the macOS archive from the [releases page](https://github.com/ChivukulaVirinchi/photovault/releases/latest)
- **Optional assets pack**: [Smriti-Assets.zip](https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-Assets.zip)

> Smriti is not notarized for macOS. Gatekeeper may warn on
> first launch — right-click the app → **Open** → **Open anyway**.

## Automatic updates

Smriti can check for new releases and (where possible) download
and install them for you. The check is **opt-in**: on first run a
prompt asks whether you want it, and you can change the answer later
in **Settings → Advanced**. When enabled, Smriti queries
`api.github.com` at most once every 24 hours; no photo data or
telemetry is sent. See [PRIVACY.md](PRIVACY.md) for the full
disclosure.

If you installed via a system package manager (`apt`, `brew`, `winget`,
`flatpak`), the update banner shows the matching upgrade command
for your platform rather than self-replacing the binary.

## Build from source

```bash
git clone https://github.com/ChivukulaVirinchi/photovault.git
cd photovault
cargo build --release
./target/release/smriti
```

### HEIC support (optional)

iPhone photos are HEIC. To decode them from a source build, install
`libheif` and rebuild with the `heic` feature:

```bash
# Linux (Debian/Ubuntu)
sudo apt-get install libheif-dev

# macOS
brew install libheif

# then
cargo build --release --features heic
```

Shipped binaries (.deb, AppImage, macOS .tar.gz) include HEIC support
out of the box. The Windows MSI ships without HEIC for now —
`libheif` Windows binaries will land in v1.1. Without HEIC enabled,
Smriti still indexes `.heic` files but reports a clear "HEIC
support not compiled in" error when asked to decode one.

Optional — install the asset pack locally for face recognition + geocoding:

```bash
./scripts/setup_assets.sh      # Linux / macOS
.\scripts\setup_assets.ps1     # Windows
```

See [`docs/BUILD.md`](docs/BUILD.md) for full setup details including
the WSL + Windows development workflow.

### Packaging helpers

Local packaging smoke tests:

```bash
./scripts/release_local.sh ubuntu
./scripts/release_local.sh linux-appimage
./scripts/release_local.sh assets-pack
./scripts/release_local.sh verify
```

Windows end-to-end local verification:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release_local.ps1 -Mode full
```

The official release flow is CI-driven from git tags (`v*`) via
[`.github/workflows/release.yml`](.github/workflows/release.yml).

## Documentation

Full user guide and contributor docs are published at
**[chivukulavirinchi.github.io/photovault](https://chivukulavirinchi.github.io/photovault/)**.

Headline entry points:

- [Getting Started](https://chivukulavirinchi.github.io/photovault/docs/user-guide/getting-started.html)
- [Indexing Photos](https://chivukulavirinchi.github.io/photovault/docs/user-guide/indexing.html)
- [People and Faces](https://chivukulavirinchi.github.io/photovault/docs/user-guide/people.html)
- [Build Guide](docs/BUILD.md)
- [Architecture Overview](docs/architecture/overview.md)
- [API Reference (rustdoc)](https://chivukulavirinchi.github.io/photovault/api/)

## Community

- **[GitHub Discussions](https://github.com/ChivukulaVirinchi/photovault/discussions)** — questions, feature ideas, show-and-tell.
- **[Issues](https://github.com/ChivukulaVirinchi/photovault/issues)** — bug reports and feature requests, via the templates.
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — dev setup, commit style, CI gates, architectural rules.
- **[Security advisories](https://github.com/ChivukulaVirinchi/photovault/security/advisories/new)** — private reporting path for vulnerabilities (see [SECURITY.md](SECURITY.md)).
- **Sponsor** — if Smriti is useful to you, the Sponsor button at the top of this repo helps keep it maintained.

> **Note on face-recognition models.** The face detection + recognition
> models come from [InsightFace](https://github.com/deepinsight/insightface)
> and are downloaded from the upstream project on first run, not bundled
> in the installer. See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)
> for attributions.

## License

[Apache License 2.0](LICENSE). See
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) for the full
third-party attribution list.

## Built with

- [Rust](https://www.rust-lang.org/) — the whole app
- [iced](https://iced.rs/) — cross-platform GUI
- [SQLite](https://sqlite.org/) — embedded database
- [ONNX Runtime](https://onnxruntime.ai/) — face detection / recognition
- [GeoNames](https://www.geonames.org/) — offline reverse geocoding
- [OpenStreetMap](https://www.openstreetmap.org/) — map tiles
