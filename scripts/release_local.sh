#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

TARGET="${1:-}"

prepare_frontend() {
  npm ci --prefix src-ui
  npm run build --prefix src-ui
}

tauri_build() {
  (cd src-tauri && cargo tauri build --ci "$@")
}

if [[ -z "$TARGET" ]]; then
  echo "Usage: scripts/release_local.sh <windows|macos|ubuntu|linux-appimage|assets-pack|verify>"
  exit 1
fi

case "$TARGET" in
  windows)
    echo "Windows packaging should run on Windows (MSVC toolchain)."
    echo "Build command: cd src-tauri && cargo tauri build --bundles msi"
    echo "Package output: target/release/bundle/msi/"
    ;;
  macos)
    [[ "$(uname -s)" == "Darwin" ]] || { echo "macOS packaging must run on macOS"; exit 1; }
    prepare_frontend
    tauri_build --bundles dmg --features heic
    echo "Built macOS bundle under target/release/bundle/dmg/"
    ;;
  ubuntu)
    [[ "$(uname -s)" == "Linux" ]] || { echo "Debian packaging must run on Linux"; exit 1; }
    prepare_frontend
    tauri_build --bundles deb --features heic
    echo "Built DEB package under target/release/bundle/deb/"
    ;;
  linux-appimage)
    [[ "$(uname -s)" == "Linux" ]] || { echo "AppImage packaging must run on Linux"; exit 1; }
    prepare_frontend
    tauri_build --bundles appimage --features heic
    echo "Built AppImage under target/release/bundle/appimage/"
    ;;
  assets-pack)
    ./scripts/setup_assets.sh
    rm -rf assets-pack-local Smriti-Assets-local.zip
    case "$(uname -s)" in
      Linux) platform="linux"; runtime_glob="libonnxruntime.so*" ;;
      Darwin) platform="macos"; runtime_glob="libonnxruntime*.dylib" ;;
      *) echo "Unsupported asset-pack host: $(uname -s)"; exit 1 ;;
    esac
    mkdir -p "assets-pack-local/libs/onnxruntime/$platform" assets-pack-local/models assets-pack-local/data
    find libs/onnxruntime -maxdepth 1 -type f -name "$runtime_glob" \
      -exec cp {} "assets-pack-local/libs/onnxruntime/$platform/" \;
    if ! find "assets-pack-local/libs/onnxruntime/$platform" -type f -print -quit | grep -q .; then
      echo "ONNX Runtime was not copied into the asset pack"
      exit 1
    fi
    cp models/scrfd_10g_bnkps.onnx assets-pack-local/models/
    cp models/adaface_ir101_webface12m.onnx assets-pack-local/models/
    cp data/geonames.db assets-pack-local/data/
    (cd assets-pack-local && zip -r ../Smriti-Assets-local.zip .)
    echo "Built optional asset pack: Smriti-Assets-local.zip"
    ;;
  verify)
    ./scripts/verify_installers_local.sh
    ;;
  *)
    echo "Unknown target: $TARGET"
    exit 1
    ;;
esac
