#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

status_line() {
  printf "%-28s %s\n" "$1" "$2"
}

fail=0

check_core_build() {
  if cargo build --release >/dev/null; then
    status_line "core build" "PASS"
  else
    status_line "core build" "FAIL"
    fail=1
  fi
}

check_deb() {
  if command -v cargo-deb >/dev/null 2>&1 || cargo install cargo-deb --locked >/dev/null; then
    if cargo deb >/dev/null 2>&1; then
      local deb
      deb=$(ls target/debian/*.deb 2>/dev/null | head -n 1 || true)
      if [[ -n "$deb" ]]; then
        status_line "linux deb" "PASS ($deb)"
      else
        status_line "linux deb" "FAIL (no .deb output)"
        fail=1
      fi
    else
      status_line "linux deb" "FAIL (cargo deb failed)"
      fail=1
    fi
  else
    status_line "linux deb" "FAIL (cargo-deb unavailable)"
    fail=1
  fi
}

check_appimage() {
  if ./scripts/release_local.sh linux-appimage >/dev/null 2>&1 && [[ -f Smriti-x86_64.AppImage ]]; then
    status_line "linux appimage" "PASS (Smriti-x86_64.AppImage)"
  else
    status_line "linux appimage" "FAIL"
    fail=1
  fi
}

check_assets_pack() {
  rm -rf assets-pack-local Smriti-Assets-local.zip
  mkdir -p assets-pack-local/libs/onnxruntime assets-pack-local/models assets-pack-local/data
  if [[ -f libs/onnxruntime/libonnxruntime.so ]] && [[ -f models/scrfd_10g_bnkps.onnx ]] && [[ -f models/adaface_ir101_webface12m.onnx ]] && [[ -f data/geonames.db ]]; then
    cp libs/onnxruntime/libonnxruntime.so* assets-pack-local/libs/onnxruntime/ 2>/dev/null || true
    cp models/scrfd_10g_bnkps.onnx assets-pack-local/models/
    cp models/adaface_ir101_webface12m.onnx assets-pack-local/models/
    cp data/geonames.db assets-pack-local/data/
    (cd assets-pack-local && zip -r ../Smriti-Assets-local.zip . >/dev/null)
    status_line "assets pack (linux)" "PASS (Smriti-Assets-local.zip)"
  else
    status_line "assets pack (linux)" "FAIL (missing source assets; run setup_assets.sh)"
    fail=1
  fi
}

echo "Smriti local installer verification"
echo "====================================="

check_core_build
check_deb
check_appimage
check_assets_pack

if [[ $fail -ne 0 ]]; then
  echo
  echo "Overall: FAIL"
  exit 1
fi

echo
echo "Overall: PASS"
