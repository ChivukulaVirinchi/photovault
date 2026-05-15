# Smriti dev launcher (Windows / PowerShell).
#
# Use this INSTEAD of 'cargo tauri dev'. It passes '--no-watch', which
# is the only thing that actually stops tauri-cli's dev watcher from
# rebuilding the binary every time SQLite flushes a WAL/SHM file,
# every time you save a frontend route, every time you touch a doc.
#
# We tried .taurignore at the workspace root (filtered out as hidden),
# TAURI_CLI_WATCHER_IGNORE_FILENAME (only changes the filename, still
# subject to other walker rules), and dropping '.' from workspace
# members (Cargo includes the root crate implicitly). None of those
# work on tauri-cli 2.11. --no-watch is the documented escape hatch.
#
# What you lose: automatic rebuild on Rust source changes. Vite still
# does frontend HMR -- Svelte / TS / CSS edits hot-reload as before.
# If you DO change Rust and want it reflected, stop this script and
# re-run it, or run 'cargo build -p smriti-tauri' in a second terminal
# (the tauri runner picks up the new binary on next launch).
#
# This file is intentionally pure ASCII. PowerShell 5.1 (the default
# shipping with Windows 10) reads .ps1 files in the OEM code page
# when no BOM is present, which mangles UTF-8 multi-byte characters
# (em-dashes etc.) and breaks string parsing mid-line.

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Write-Host "Smriti dev -- watcher OFF (Vite HMR still active for frontend)." -ForegroundColor Cyan
Write-Host "To rebuild Rust changes, stop (Ctrl+C) and re-run this script." -ForegroundColor DarkGray
cargo tauri dev --no-watch
