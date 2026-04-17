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
      <img src="https://img.shields.io/badge/license-Apache--2.0-blue?style=for-the-badge" alt="License">
    </a>
  </p>

  <img src="website/hero-screenshot.png" alt="PhotoVault screenshot" width="800">
</div>

---

## What it does

- **Indexes photos** from any folder or external drive -- no upload required
- **Recognizes faces** using on-device ML, groups photos by person
- **Suggests albums** for trips and events automatically
- **Surfaces memories** -- "N years ago today" rediscovery
- **Visualizes geography** -- interactive map of where photos were taken
- **Shows insights** -- heatmaps, top people, top locations, monthly breakdown
- **Finds anything** -- unified search across people, albums, places, photos

## Why offline?

Your photos are personal. PhotoVault keeps them that way.

- No cloud upload. Photos stay on your drive.
- No account required. No telemetry. No analytics.
- Works without internet (after initial install).
- Database lives on the indexed drive -- fully portable.

## Install

### Linux
- **AppImage** (universal): [Download](https://github.com/ChivukulaVirinchi/photovault/releases/latest) -> `chmod +x` -> run
- **Debian/Ubuntu**: `.deb` package available on the [releases page](https://github.com/ChivukulaVirinchi/photovault/releases/latest)

### Linux notes
- `.rpm` is not produced by the current CI workflow yet.

### Windows
- Download the **portable .zip** from the [releases page](https://github.com/ChivukulaVirinchi/photovault/releases/latest)
- `.msi` installer support is planned in a future release workflow update.
- Windows SmartScreen may warn -- click "More info" -> "Run anyway"
  (No code-signing certificate yet)

### macOS
- Download the macOS archive from the [releases page](https://github.com/ChivukulaVirinchi/photovault/releases/latest)
- macOS Gatekeeper may warn -- right-click -> "Open" -> "Open anyway"
  (No Apple Developer ID yet)

## Build from source

```bash
git clone https://github.com/ChivukulaVirinchi/photovault.git
cd photovault
./scripts/setup_assets.sh
cargo build --release
./target/release/photovault
```

See `docs/BUILD.md` for full setup including Windows-on-UNC workflow.

### Packaging helpers

For local packaging smoke tests:

```bash
./scripts/release_local.sh ubuntu
./scripts/release_local.sh linux-appimage
```

Windows/macOS installers should be built on their native platforms. The official
release flow is CI-driven from Git tags (`v*`) via `.github/workflows/release.yml`.

## Documentation

- [Build Guide](docs/BUILD.md)
- [Future Scope](docs/FUTURE_SCOPE.md)
- [Face Recognition Improvements](docs/FACE_RECOGNITION_IMPROVEMENTS.md)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
dev setup, testing, and pull request guidelines.

## License

[Apache License 2.0](LICENSE)

## Built with

- [Rust](https://www.rust-lang.org/)
- [iced](https://iced.rs/) -- cross-platform GUI
- [SQLite](https://sqlite.org/) -- embedded database
- [ONNX Runtime](https://onnxruntime.ai/) -- face detection/recognition
- [GeoNames](https://www.geonames.org/) -- offline reverse geocoding
- [OpenStreetMap](https://www.openstreetmap.org/) -- map tiles
