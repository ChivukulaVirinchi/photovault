#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

TARGET="${1:-}"

if [[ -z "$TARGET" ]]; then
  echo "Usage: scripts/release_local.sh <windows|macos|ubuntu|linux-appimage>"
  exit 1
fi

case "$TARGET" in
  windows)
    echo "Windows packaging should run on Windows (MSVC toolchain)."
    echo "Build command: cargo build --release --target x86_64-pc-windows-msvc"
    echo "Package outputs: ZIP (portable), MSI (if wix/cargo-wix configured)"
    ;;
  macos)
    echo "macOS packaging should run on macOS."
    echo "Build command: cargo build --release --target aarch64-apple-darwin"
    echo "Package outputs: tar.gz now; DMG when bundling pipeline is added"
    ;;
  ubuntu)
    cargo install cargo-deb --locked
    cargo deb
    echo "Built DEB package under target/debian/"
    ;;
  linux-appimage)
    echo "Use the GitHub release workflow for AppImage (recommended)."
    echo "It already builds AppImage in CI from release artifacts."
    ;;
  *)
    echo "Unknown target: $TARGET"
    exit 1
    ;;
esac
