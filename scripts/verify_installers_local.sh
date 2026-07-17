#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

status_line() {
  printf "%-28s %s\n" "$1" "$2"
}

fail=0

check_tauri_bundles() {
  npm ci --prefix src-ui >/dev/null
  npm run build --prefix src-ui >/dev/null
  if ! (cd src-tauri && cargo tauri build --ci --features heic >/dev/null); then
    status_line "Tauri bundles" "FAIL"
    fail=1
    return
  fi

  case "$(uname -s)" in
    Linux)
      local deb appimage
      deb=$(find target/release/bundle/deb -type f -name '*.deb' -print -quit 2>/dev/null || true)
      appimage=$(find target/release/bundle/appimage -type f -name '*.AppImage' -print -quit 2>/dev/null || true)
      [[ -n "$deb" ]] && status_line "Linux DEB" "PASS ($deb)" || { status_line "Linux DEB" "FAIL"; fail=1; }
      [[ -n "$appimage" ]] && status_line "Linux AppImage" "PASS ($appimage)" || { status_line "Linux AppImage" "FAIL"; fail=1; }
      ;;
    Darwin)
      local dmg
      dmg=$(find target/release/bundle/dmg -type f -name '*.dmg' -print -quit 2>/dev/null || true)
      [[ -n "$dmg" ]] && status_line "macOS DMG" "PASS ($dmg)" || { status_line "macOS DMG" "FAIL"; fail=1; }
      ;;
    *)
      status_line "Tauri bundles" "FAIL (unsupported host $(uname -s))"
      fail=1
      ;;
  esac
}

check_assets_pack() {
  if ./scripts/release_local.sh assets-pack >/dev/null; then
    status_line "assets pack" "PASS (Smriti-Assets-local.zip)"
  else
    status_line "assets pack" "FAIL"
    fail=1
  fi
}

echo "Smriti local installer verification"
echo "====================================="

check_tauri_bundles
check_assets_pack

if [[ $fail -ne 0 ]]; then
  echo
  echo "Overall: FAIL"
  exit 1
fi

echo
echo "Overall: PASS"
