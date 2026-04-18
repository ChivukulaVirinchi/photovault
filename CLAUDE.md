# PhotoVault — Development Guide

## What is this?

An offline-first desktop photo library manager built in Rust with `iced` (GUI), `rusqlite` (SQLite), and `ort` (ONNX Runtime for face detection/recognition). It indexes photos from external drives, extracts EXIF metadata, detects faces, clusters them, finds duplicates/bursts, and provides geocoding.

## Cross-Platform Development (WSL + Windows)

This project targets **both Linux and Windows** from a single codebase. Development uses a single-instance model from WSL.

### How it works

```
Code lives in WSL (single source of truth):
  /home/virinchi/code/rust/photovault

Windows accesses it via UNC path:
  \\wsl.localhost\Ubuntu-24.04\home\virinchi\code\rust\photovault
```

- **One Claude instance (WSL)** makes all code changes
- **One git repo** — all commits happen from WSL
- Each platform has its own native Rust toolchain — no cross-compilation
- File changes are instantly visible to both sides (same filesystem)

### Who does what

| Action | Where | Who |
|--------|-------|-----|
| Code edits | WSL | Claude (this instance) |
| Git operations | WSL | Claude (this instance) |
| Linux build + test | WSL terminal | Claude / user |
| Windows build + test | PowerShell | User runs `cargo build && cargo run` |
| Later: standalone Linux | Clone the repo | Same code, works as-is |

### PowerShell — How to build & test Windows

```powershell
cd \\wsl.localhost\Ubuntu-24.04\home\virinchi\code\rust\photovault

cargo build              # debug build
cargo build --release    # release build
cargo run                # run
cargo test               # tests
```

Note: Windows `cargo build` on the UNC path is slower (~2-3x) due to the WSL filesystem bridge. This is fine for periodic Windows smoke tests — primary development happens in WSL.

The `target/` directory has separate artifacts per platform. Windows = `photovault.exe`, Linux = `photovault`. They do NOT conflict.

### Windows Setup (one-time)

1. **Rust toolchain**: `winget install Rustlang.Rustup` (restart terminal after)
2. **Visual Studio Build Tools**: install "Desktop development with C++" workload (provides MSVC linker)
3. **Assets**: from PowerShell in the project dir, run:
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\setup_assets.ps1
   ```
   This downloads `onnxruntime.dll`, ONNX face models, and GeoNames data.

### Linux Setup (one-time)

```bash
./scripts/setup_assets.sh
```

## Platform-Specific Code

Only two files have `#[cfg]` gates:

- **`src/services/drive_detector.rs`** — drive enumeration (Linux: /media, /mnt; Windows: drive letters A-Z; macOS: /Volumes)
- **`src/ml/runtime.rs`** — ONNX Runtime library name (`libonnxruntime.so` on Linux, `onnxruntime.dll` on Windows)

Everything else is platform-agnostic.

### ONNX Runtime

The `ort` crate uses `load-dynamic` — the shared library is loaded at runtime via dlopen/LoadLibrary.

| Platform | Library file | Location |
|----------|-------------|----------|
| Linux | `libonnxruntime.so` | `libs/onnxruntime/` |
| Windows | `onnxruntime.dll` | `libs/onnxruntime/` |

Resolution order (both platforms):
1. `ORT_DYLIB_PATH` env var
2. `libs/onnxruntime/` relative to executable
3. `libs/onnxruntime/` relative to CWD

### Asset Scripts

| Platform | Script | Downloads |
|----------|--------|-----------|
| Linux | `scripts/setup_assets.sh` | `libonnxruntime.so` (Linux x64), ONNX models, GeoNames |
| Windows | `scripts/setup_assets.ps1` | `onnxruntime.dll` (Win x64), ONNX models, GeoNames |

Both are idempotent — skip files that already exist.

## Build & Run

```bash
# Debug
cargo build
RUST_LOG=photovault=debug cargo run

# Release
cargo build --release
./target/release/photovault       # Linux
.\target\release\photovault.exe   # Windows

# Tests
cargo test

# Lint gate (required before any push)
cargo clippy --all-targets
```

## Push gate (mandatory)

Before pushing any commit to remote, the local workspace must pass:

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets`
3. `cargo test --no-run`

Do not push if any of these fail.

## Project Structure

```
src/
  main.rs              Entry point, single-instance lock
  app.rs               Main state machine (all views + message handling)
  bootstrap.rs         Runtime asset checks
  config/              Settings (theme, thumbnail size, confidence thresholds)
  db/                  SQLite layer (schema, repos, migrations)
  ml/                  ONNX Runtime + face detection + embedding + clustering
  models/              Data structs
  scoring/             Image quality (blur, sharpness)
  search/              Query parsing
  services/            Business logic (scanner, thumbnails, faces, duplicates, bursts, geocoding)
  theme/               UI colors
  views/               UI screens (timeline, people, duplicates, bursts, cull, trash, settings)
  components/          Reusable UI widgets
  bin/build_geonames.rs  CLI tool to build GeoNames SQLite DB

scripts/               Setup scripts (Linux + Windows)
models/                ONNX model files (gitignored)
libs/                  ONNX Runtime shared libs (gitignored)
data/                  GeoNames database (gitignored)
docs/                  Phase documentation
```

## Key Dependencies

- **iced 0.13** — GUI (Elm-style, cross-platform via wgpu)
- **rusqlite 0.32** — SQLite (bundled)
- **ort 2.0.0-rc.11** — ONNX Runtime (load-dynamic)
- **tokio 1** — async runtime
- **image 0.25** / **imageproc 0.24** — image processing
- **rfd 0.15** — native file dialogs
- **rayon 1.10** — parallel processing
