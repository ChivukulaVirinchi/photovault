# Architecture Overview

Smriti is a desktop app split across three crates / workspaces:

- **`src/`** — pure Rust engine, published as the `smriti` library.
  - `src/services/` — scanner, thumbnailer, face pipeline, duplicates,
    bursts, geocoding, update checker.
  - `src/db/` — SQLite repositories, schema, migrations. The on-drive
    database lives at `<drive>/.photovault/photovault.db` (kept under
    that name for backwards compatibility with libraries indexed
    before the rename).
  - `src/ml/` — ONNX Runtime wrapper, face detector, embedder.
  - `src/config/`, `src/models/`, `src/search/` — config,
    domain types and query parsing.
  - `src/bootstrap.rs` — runtime asset checks (ONNX runtime, models,
    GeoNames DB).
- **`src-tauri/`** — Tauri 2 shell. One bin crate that mounts every
  `#[tauri::command]` from `src-tauri/src/commands/`. The contract is
  in `docs/COMMAND_SURFACE.md`. DTOs live in `src-tauri/src/dto.rs`
  and translate one-way from engine types.
- **`src-ui/`** — Vite + Svelte 5 frontend. Routes call into the
  typed Tauri client at `src-ui/src/lib/api/`; state lives in
  runes-based stores under `src-ui/src/lib/stores/`.

The engine has no UI dependencies and is independently testable. The
Tauri shell is a thin adapter; anything more than ~15 lines of logic
in a handler is a service-layer change, not a handler change.
