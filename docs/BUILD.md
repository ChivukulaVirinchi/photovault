# Build from Source

PhotoVault builds natively on Linux, Windows, and macOS.

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

## Asset setup (required)

PhotoVault needs ONNX runtime, ONNX models, and GeoNames data.

Linux/macOS:

```bash
./scripts/setup_assets.sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup_assets.ps1
```

Both scripts are idempotent.

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
.\target\release\photovault.exe
```

## Test and lint

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Troubleshooting

- Missing ONNX runtime/model files: rerun setup script and confirm `libs/`, `models/`, `data/` exist.
- GUI linking errors on Linux: install missing Wayland/X11 dev packages listed above.
- Slow Windows builds on UNC path: expected; primary development should happen in WSL.
