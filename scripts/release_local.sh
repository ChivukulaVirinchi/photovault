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
    if ! command -v wget >/dev/null 2>&1; then
      echo "wget is required to fetch appimagetool"
      exit 1
    fi
    ./scripts/setup_assets.sh
    cargo build --release --target x86_64-unknown-linux-gnu
    rm -rf staging-local PhotoVault.AppDir appimagetool-x86_64.AppImage
    mkdir -p staging-local/photovault
    cp target/x86_64-unknown-linux-gnu/release/photovault staging-local/photovault/
    cp -r libs staging-local/photovault/
    cp -r models staging-local/photovault/
    cp -r data staging-local/photovault/
    mkdir -p PhotoVault.AppDir/usr/bin
    mkdir -p PhotoVault.AppDir/usr/lib/photovault
    mkdir -p PhotoVault.AppDir/usr/share/applications
    mkdir -p PhotoVault.AppDir/usr/share/icons/hicolor/256x256/apps
    cp staging-local/photovault/photovault PhotoVault.AppDir/usr/bin/
    cp -r staging-local/photovault/libs PhotoVault.AppDir/usr/lib/photovault/
    cp -r staging-local/photovault/models PhotoVault.AppDir/usr/lib/photovault/
    cp -r staging-local/photovault/data PhotoVault.AppDir/usr/lib/photovault/
    cp packaging/photovault.desktop PhotoVault.AppDir/usr/share/applications/
    cp packaging/photovault.png PhotoVault.AppDir/usr/share/icons/hicolor/256x256/apps/
    cp packaging/photovault.desktop PhotoVault.AppDir/
    cp packaging/photovault.png PhotoVault.AppDir/
    cat > PhotoVault.AppDir/AppRun <<'EOF'
#!/bin/bash
HERE="$(dirname "$(readlink -f "${0}")")"
export PHOTOVAULT_ASSET_DIR="${HERE}/usr/lib/photovault"
cd "${HERE}/usr/bin"
export LD_LIBRARY_PATH="${HERE}/usr/lib/photovault/libs/onnxruntime:$LD_LIBRARY_PATH"
exec "${HERE}/usr/bin/photovault" "$@"
EOF
    chmod +x PhotoVault.AppDir/AppRun
    wget -q https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x appimagetool-x86_64.AppImage
    ./appimagetool-x86_64.AppImage PhotoVault.AppDir PhotoVault-x86_64.AppImage
    echo "Built AppImage: PhotoVault-x86_64.AppImage"
    ;;
  *)
    echo "Unknown target: $TARGET"
    exit 1
    ;;
esac
