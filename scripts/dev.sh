#!/usr/bin/env bash
# Smriti dev launcher (Linux / macOS) — see scripts/dev.ps1 for the
# rationale. Short version: `cargo tauri dev` keeps rebuilding on
# SQLite WAL/SHM file changes and there's no clean config-file fix
# in tauri-cli 2.11. `--no-watch` is the only escape hatch.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "Smriti dev — watcher OFF (Vite HMR still active for frontend)."
echo "To rebuild Rust changes, stop (Ctrl+C) and re-run this script."
cargo tauri dev --no-watch
