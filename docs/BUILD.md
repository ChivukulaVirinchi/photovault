# Build from Source

Smriti builds natively on Linux, Windows, and macOS.

## Prerequisites

### Linux

- Rust toolchain (stable, MSRV 1.88+) via `rustup`
- Node 20.19+ and npm (the frontend is Vite 8 + Svelte 5)
- Tauri build deps. On Ubuntu/Debian:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev \
  pkg-config \
  libheif-dev          # only if you want HEIC decoding
```

### Windows

- Rust toolchain via `rustup`
- Node 20.19+ via the official installer or `winget install OpenJS.NodeJS`
- Visual Studio Build Tools (Desktop development with C++) — provides
  the MSVC linker that `cargo build` and Tauri's bundler both need.
- WebView2 Runtime — preinstalled on Windows 11; auto-downloaded by
  Tauri on Windows 10 if missing.

### macOS

- Xcode command line tools (`xcode-select --install`)
- Rust toolchain via `rustup`
- Node 20.19+ via Homebrew (`brew install node`) or the official installer

### One-time tooling

```bash
cargo install tauri-cli --version "^2" --locked
```

## Asset setup (optional but recommended)

Smriti now ships a small core application.
Face recognition and offline geocoding use an optional asset pack
(ONNX runtime + models + geonames DB) that can be installed in-app with one click.

Manual setup scripts are still available:

Linux/macOS:

```bash
./scripts/setup_assets.sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup_assets.ps1
```

Both scripts are idempotent and useful for local development/testing.

## Build and run

Smriti is a Tauri 2 app: the Rust engine + Tauri shell live in
`src/` and `src-tauri/`, the Svelte 5 frontend lives in `src-ui/`.
The `cargo tauri` CLI orchestrates both halves.

Dev mode (hot reload via Vite, opens a native window):

```bash
cd src-ui && npm install && cd ..   # one-time
./scripts/dev.sh                    # Linux / macOS
# .\scripts\dev.ps1                 # Windows PowerShell
```

Production bundle (`.deb` + AppImage on Linux, `.msi` on Windows,
`.dmg` / tar.gz on macOS):

```bash
cargo tauri build
```

Engine-only build (useful when iterating on `src/` without a window):

```bash
cargo build -p smriti -p smriti-tauri
```

## Test and lint

```bash
cargo fmt --check
cargo clippy --all-targets -p smriti -p smriti-tauri -- -D warnings
cargo test -p smriti -p smriti-tauri
(cd src-ui && npm run check && npm run build)
```

## Linux packaging (.deb + AppImage)

The official path is `cargo tauri build` — it produces both
`.deb` and AppImage in `target/release/bundle/`. Tag-driven
CI (`.github/workflows/release.yml`) runs the same command on a
fresh Ubuntu runner for every `v*` tag.

For a local `.deb` build through the same Tauri path as CI:

```bash
./scripts/release_local.sh ubuntu
```

Output:
- `target/release/bundle/deb/*.deb`

Install test:

```bash
sudo dpkg -i target/release/bundle/deb/*.deb
smriti-tauri
```

Uninstall test:

```bash
sudo apt remove smriti
```

### Linux AppImage

AppImage is built in CI from release tags (`v*`) via `.github/workflows/release.yml`.
Output directory:
- `target/release/bundle/appimage/`

Run:

```bash
chmod +x target/release/bundle/appimage/*.AppImage
target/release/bundle/appimage/*.AppImage
```

## Windows packaging (MSI + ZIP)

MSI packaging should run on native Windows with the MSVC build tools.

### Prerequisites

- Tauri CLI:

```powershell
cargo install tauri-cli --version "^2" --locked
```

### Build MSI installer

From repository root:

```powershell
cd src-tauri
cargo tauri build --ci --bundles msi
```

Output:
- `target\release\bundle\msi\*.msi`

MSI installs the core app. Optional assets are installed separately.

## Troubleshooting

- Optional assets not detected: install via startup prompt, or run setup script and restart app.
- GUI linking errors on Linux: install missing Wayland/X11 dev packages listed above.
- Slow Windows builds on UNC path: expected; primary development should happen in WSL.

## Local installer verification scripts

Linux/macOS shell:

```bash
./scripts/verify_installers_local.sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release_local.ps1 -Mode full
```

These scripts summarize installer packaging status locally before you create release tags.

Mandatory push gate before any `git push`:

```bash
cargo fmt --all --check
cargo clippy --all-targets
cargo test --no-run
```

## Automated release orchestration (draft-only publish flow)

From Windows PowerShell, once local checks are green:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release_publish.ps1 -Version v0.1.0-rc3 -RunLocalChecks -Wait
```

Behavior:

- validates repo state and branch sync
- optionally runs local verification first
- creates and pushes the release tag
- waits for `release.yml` workflow
- verifies expected assets in the GitHub draft release
- prints draft release URL for manual publish

By design, this script does **not** auto-publish the release.

## Asset installer URL behavior

In-app optional asset installation resolves in this order:

1. `SMRITI_ASSET_PACK_PATH` (local zip path override; legacy
   `PHOTOVAULT_ASSET_PACK_PATH` is also accepted)
2. `SMRITI_ASSET_PACK_URL` (custom URL override; legacy
   `PHOTOVAULT_ASSET_PACK_URL` is also accepted)
3. version-pinned release URL (`.../releases/download/v<app-version>/Smriti-Assets.zip`)
4. latest published release fallback (`.../releases/latest/download/Smriti-Assets.zip`)

If you are testing locally before publishing a release, set
`SMRITI_ASSET_PACK_PATH`.
