# Build from Source

Smriti builds natively on Linux, Windows, and macOS.

## Prerequisites

### Linux

- Rust toolchain (stable) via `rustup`
- Build deps for iced/wgpu stack (Ubuntu/Debian example):

```bash
sudo apt-get update
sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb-shape0-dev libxcb-xfixes0-dev pkg-config
```

### Windows

- Rust toolchain via `rustup`
- Visual Studio Build Tools (Desktop development with C++)
- Build from PowerShell at the UNC path (WSL shared filesystem)

### macOS

- Xcode command line tools
- Rust toolchain via `rustup`

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

Debug build:

```bash
cargo build
RUST_LOG=photovault=debug cargo run
```

Release build:

```bash
cargo build --release
./target/release/photovault
```

Windows release binary:

```powershell
.\target\release\smriti.exe
```

## Test and lint

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Linux packaging (Ubuntu + AppImage)

### Ubuntu / Debian `.deb`

Local build:

```bash
cargo install cargo-deb --locked
cargo deb
```

Output:
- `target/debian/*.deb`

Install test:

```bash
sudo dpkg -i target/debian/*.deb
photovault
```

Uninstall test:

```bash
sudo apt remove photovault
```

### Linux AppImage

AppImage is built in CI from release tags (`v*`) via `.github/workflows/release.yml`.
Current output name:
- `Smriti-x86_64.AppImage`

Run:

```bash
chmod +x Smriti-x86_64.AppImage
./Smriti-x86_64.AppImage
```

## Windows packaging (MSI + ZIP)

MSI packaging should run on native Windows with MSVC + WiX installed.

### Prerequisites

- WiX Toolset v3 (for `candle.exe` and `light.exe`)
- `cargo-wix`:

```powershell
cargo install cargo-wix --locked
```

### Build portable ZIP payload (core app)

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

### Build MSI installer

From repository root:

```powershell
cargo wix --target x86_64-pc-windows-msvc --output target\wix\Smriti-Setup-x64.msi
```

Output:
- `target\wix\Smriti-Setup-x64.msi`

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

1. `PHOTOVAULT_ASSET_PACK_PATH` (local zip path override)
2. `PHOTOVAULT_ASSET_PACK_URL` (custom URL override)
3. latest published release URL (`.../releases/latest/download/Smriti-Assets.zip`)
4. version-pinned fallback (`.../releases/download/v<app-version>/Smriti-Assets.zip`)

If you are testing locally before publishing a release, set `PHOTOVAULT_ASSET_PACK_PATH`.
