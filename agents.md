# Smriti — Development Guide

## What is this?

An offline-first desktop photo library manager. Rust engine (services, DB, ML) wrapped in a Tauri 2 shell with a Svelte 5 frontend. Uses `rusqlite` (SQLite) and `ort` (ONNX Runtime for face detection/recognition). Indexes photos from external drives, extracts EXIF metadata, detects faces, clusters them, finds duplicates/bursts, and provides geocoding.

## Architecture

- **`src/`** — pure Rust library: services, DB, ML, models, search, scoring, config. No UI. Re-exported via `src/lib.rs`.
- **`src-tauri/`** — Tauri 2 bin crate. Wraps the engine in `#[tauri::command]` handlers per the contract in `docs/COMMAND_SURFACE.md`.
- **`src-ui/`** — Vite + Svelte 5 frontend. Talks to the backend via Tauri IPC.

The iced UI was removed in 2026-05; the engine is the same.

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

The `target/` directory has separate artifacts per platform. Windows = `smriti.exe`, Linux = `smriti`. They do NOT conflict.

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

Platform-specific paths include:

- **`src/services/drive_detector.rs`** — drive enumeration (Linux: /media, /mnt; Windows: drive letters A-Z; macOS: /Volumes)
- **`src/ml/runtime.rs`** — ONNX Runtime library name (`libonnxruntime.so` on Linux, `onnxruntime.dll` on Windows)

Native installers, credential storage and filesystem durability also have
platform-specific behavior; verify them on their target OS.

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
# Engine + Tauri shell (debug)
cargo build -p smriti -p smriti-tauri

# Frontend (one-time + on every change for production builds)
cd src-ui && npm install && npm run build && cd ..

# Run dev (opens native window with HMR via Vite)
cargo install tauri-cli --version "^2" --locked   # one-time

# Use the wrapper, NOT `cargo tauri dev` directly. The wrapper passes
# `--no-watch`, which is the only thing that stops tauri-cli 2.11's
# dev watcher from rebuilding the binary on every SQLite WAL/SHM
# tick, every doc save, every script touch. Vite HMR still handles
# the frontend; Rust changes need a Ctrl+C + restart.
scripts\dev.ps1     # Windows PowerShell
scripts/dev.sh      # Linux / macOS

# Production bundle (.deb/AppImage on Linux, .msi on Windows, .dmg on macOS)
cargo tauri build

# Tests
cargo test -p smriti
cargo test -p smriti-tauri

# Lint gate (required before any push)
cargo clippy --all-targets -p smriti -p smriti-tauri
cd src-ui && npm run check && npm run build
```

## Agent Build Policy

Rust builds are expensive in this repo because the workspace pulls in Tauri,
image codecs, ONNX Runtime, SQLite, reqwest/rustls, and test/bench tooling.
Agents must avoid turning every small edit into a full cold build.

During implementation, run the narrowest useful check:

```bash
cargo check -p smriti
cargo test -p smriti --test db_integration
cargo test -p smriti test_name
npm run check --prefix src-ui
```

Run broader checks only when the work is complete or before a commit/push:

```bash
scripts/ci_local.sh ci
```

Do not set a one-off `CARGO_TARGET_DIR` unless there is a lock/contention
problem. A different target dir creates another full build cache. If an agent
does use one, it owns cleanup before finishing.

## Disk Hygiene

The `target/` tree grows by 1–3 GB per build and easily hits 14 GB on a busy
session. After **every ~3 Rust builds/checks**, run the cleanup script:

```bash
./scripts/clean_builds.sh        # Linux / WSL
scripts\clean_builds.ps1         # Windows PowerShell
```

What to remove:

- `target/debug/incremental`
- `target/release/incremental`
- old files under `target/release/bundle`, keeping only the newest artifact of
  each package format
- build-script outputs only when proven unused (age alone is not sufficient)
- any temporary target dirs created by the agent, for example
  `/tmp/photovault-codex-target`

What not to remove during normal work:

- `target/debug/deps`
- `target/release/deps`
- the whole `target/` directory
- `Cargo.lock`

Do not run `cargo clean` as routine hygiene. It removes the useful warm cache
and forces another long rebuild. Use `cargo clean` only when disk is critically
low or the user explicitly asks for a full clean.

**Rule for agents:** track build/check count mentally per session. After roughly
three Cargo invocations that compile code, run the cleanup script. If the agent
created an isolated `CARGO_TARGET_DIR`, remove that directory before finishing
unless it is needed for an active command.

## Push gate (mandatory)

Before pushing any commit to remote, the local workspace must pass the local CI
preflight:

```bash
scripts/ci_local.sh ci
```

Do not push if these fail. Full suites belong at the end of the task; focused
tests belong during iteration.

## Project Structure

```
src/                   Rust engine (lib-only after iced removal)
  bootstrap.rs         Runtime asset checks
  config/              Settings (theme, thumbnail size, confidence thresholds)
  db/                  SQLite layer (schema, repos, migrations)
  ml/                  ONNX Runtime + face detection + embedding + clustering
  models/              Data structs
  search/              Query parsing
  services/            Business logic (scanner, thumbnails, faces, duplicates, bursts, geocoding)
src-tauri/             Tauri shell — IPC handlers + state + DTOs
  src/commands/        One file per domain (photos, people, albums, ...)
  src/dto.rs           Wire-format types + From<engine type> impls
  src/state.rs         AppState (open library, job registry)
  src/jobs.rs          Long-running job helpers
src-ui/                Vite + Svelte 5 frontend
  src/routes/          One Svelte component per view
  src/lib/api/         Typed Tauri client (mirrors COMMAND_SURFACE.md)
  src/lib/stores/      Runes-based stores (library, settings)
  src/lib/components/  Shared UI (Sidebar, PageHeader)
docs/COMMAND_SURFACE.md  The IPC contract — frontend implements against it
src/bin/build_geonames.rs  CLI tool to build GeoNames SQLite DB

scripts/               Setup scripts (Linux + Windows) for ONNX runtime + models + GeoNames
models/                ONNX model files (gitignored)
libs/                  ONNX Runtime shared libs (gitignored)
data/                  GeoNames database (gitignored)
```

## Key Dependencies

- **rusqlite 0.32** — SQLite (bundled)
- **ort 2.0.0-rc.11** — ONNX Runtime (load-dynamic)
- **tokio 1** — async runtime
- **image 0.25** / **imageproc 0.26** — image processing
- **rayon 1.10** — parallel processing
- **tauri 2** — desktop shell (IPC, asset protocol, native windowing)
- **svelte 5** + **vite 8** — frontend
- **maplibre-gl 4** — map view + photo-detail minimap
- **Virtualization** — custom virtualizer in `src-ui/src/lib/virtualizer.svelte.ts`

## Optional integrations

Face embedding can optionally be offloaded to a user-owned Kaggle/Colab notebook
(see `docs/face-gpu-bridge.md`). The default flow uses local ONNX Runtime.
