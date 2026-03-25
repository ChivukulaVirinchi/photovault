#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_DIR="$ROOT_DIR/data"
MODELS_DIR="$ROOT_DIR/models"
CACHE_DIR="$ROOT_DIR/.cache/downloads"

GEONAMES_CITIES_URL="https://download.geonames.org/export/dump/cities1000.zip"
GEONAMES_COUNTRY_URL="https://download.geonames.org/export/dump/countryInfo.txt"
MODEL_PACK_URL_DEFAULT="https://github.com/deepinsight/insightface/releases/download/v0.7/antelopev2.zip"
MODEL_PACK_URL="${INSIGHTFACE_MODEL_URL:-$MODEL_PACK_URL_DEFAULT}"

mkdir -p "$DATA_DIR" "$MODELS_DIR" "$CACHE_DIR"

download() {
  local url="$1"
  local out="$2"
  echo "Downloading: $url"
  curl -fL \
    --retry 6 \
    --retry-delay 2 \
    --retry-all-errors \
    --connect-timeout 20 \
    -o "$out" \
    "$url"
}

extract_zip() {
  local zip_path="$1"
  local out_dir="$2"

  if command -v unzip >/dev/null 2>&1; then
    unzip -o "$zip_path" -d "$out_dir" >/dev/null
    return 0
  fi

  python3 - <<'PY' "$zip_path" "$out_dir"
import sys
import zipfile

z = zipfile.ZipFile(sys.argv[1])
z.extractall(sys.argv[2])
PY
}

setup_geonames() {
  echo "\n==> Setting up GeoNames data"

  local cities_zip="$CACHE_DIR/cities1000.zip"
  local country_info="$CACHE_DIR/countryInfo.txt"

  download "$GEONAMES_CITIES_URL" "$cities_zip"
  download "$GEONAMES_COUNTRY_URL" "$country_info"

  extract_zip "$cities_zip" "$DATA_DIR"

  if [[ ! -f "$DATA_DIR/cities1000.txt" ]]; then
    echo "ERROR: cities1000.txt missing after extraction"
    exit 1
  fi

  awk -F '\t' 'BEGIN{OFS="\t"} !/^#/ && NF>=5 {print $1, $5}' "$country_info" > "$DATA_DIR/country_codes.txt"

  if [[ ! -s "$DATA_DIR/country_codes.txt" ]]; then
    echo "ERROR: country_codes.txt is empty"
    exit 1
  fi

  echo "Building geonames SQLite DB..."
  (cd "$ROOT_DIR" && cargo run --bin build_geonames)
}

setup_models() {
  echo "\n==> Setting up face models"

  local models_zip="$CACHE_DIR/insightface_models.zip"
  local extract_dir="$CACHE_DIR/insightface_models"

  rm -rf "$extract_dir"
  mkdir -p "$extract_dir"

  download "$MODEL_PACK_URL" "$models_zip"
  extract_zip "$models_zip" "$extract_dir"

  local detector_path
  local embedder_path

  detector_path="$(find "$extract_dir" -type f -name 'scrfd_10g_bnkps.onnx' | head -n 1 || true)"
  embedder_path="$(find "$extract_dir" -type f -name 'glintr100.onnx' | head -n 1 || true)"

  if [[ -z "$detector_path" ]]; then
    detector_path="$(find "$extract_dir" -type f -name '*scrfd*10g*bnkps*.onnx' | head -n 1 || true)"
  fi

  if [[ -z "$embedder_path" ]]; then
    embedder_path="$(find "$extract_dir" -type f -name 'glintr100*.onnx' | head -n 1 || true)"
  fi

  if [[ -z "$detector_path" || -z "$embedder_path" ]]; then
    echo "ERROR: Could not locate required ONNX files in downloaded model pack."
    echo "Found ONNX files:"
    find "$extract_dir" -type f -name '*.onnx' -print || true
    exit 1
  fi

  cp "$detector_path" "$MODELS_DIR/scrfd_10g_bnkps.onnx"
  cp "$embedder_path" "$MODELS_DIR/glintr100.onnx"

  echo "Installed models:"
  echo "- $MODELS_DIR/scrfd_10g_bnkps.onnx"
  echo "- $MODELS_DIR/glintr100.onnx"
}

main() {
  echo "PhotoVault asset setup"
  echo "Root: $ROOT_DIR"

  setup_geonames
  setup_models

  echo "\nDone."
}

main "$@"
