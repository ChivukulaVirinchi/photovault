#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_DIR="$ROOT_DIR/data"
MODELS_DIR="$ROOT_DIR/models"
CACHE_DIR="$ROOT_DIR/.cache/downloads"

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo not found in PATH. Install Rust or export PATH to ~/.cargo/bin"
  exit 1
fi

GEONAMES_CITIES_URL="https://download.geonames.org/export/dump/cities1000.zip"
GEONAMES_COUNTRY_URL="https://download.geonames.org/export/dump/countryInfo.txt"
SCRFD_MODEL_URL_DEFAULT="https://huggingface.co/MonsterMMORPG/tools/resolve/main/scrfd_10g_bnkps.onnx"
GLINTR_MODEL_URL_DEFAULT="https://huggingface.co/MonsterMMORPG/tools/resolve/main/glintr100.onnx"
ORT_URL_DEFAULT="https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/onnxruntime-linux-x64-1.23.0.tgz"
SCRFD_MODEL_URL="${SCRFD_MODEL_URL:-$SCRFD_MODEL_URL_DEFAULT}"
GLINTR_MODEL_URL="${GLINTR_MODEL_URL:-$GLINTR_MODEL_URL_DEFAULT}"
ORT_URL="${ORT_URL:-$ORT_URL_DEFAULT}"

mkdir -p "$DATA_DIR" "$MODELS_DIR" "$CACHE_DIR"

download() {
  local url="$1"
  local out="$2"
  if [[ -s "$out" ]]; then
    echo "Using cached: $out"
    return 0
  fi
  echo "Downloading: $url"
  curl -fL \
    --retry 6 \
    --retry-delay 2 \
    --retry-all-errors \
    --connect-timeout 20 \
    -o "$out" \
    "$url"
}

geonames_db_ready() {
  local db="$DATA_DIR/geonames.db"
  if [[ ! -s "$db" ]]; then
    return 1
  fi

  if command -v sqlite3 >/dev/null 2>&1; then
    local cities
    cities="$(sqlite3 "$db" "select count(*) from cities;" 2>/dev/null || echo 0)"
    [[ "${cities:-0}" -gt 1000 ]]
    return $?
  fi

  return 0
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

  if geonames_db_ready; then
    echo "GeoNames DB already ready: $DATA_DIR/geonames.db"
  else
    echo "Building geonames SQLite DB (this can take a while on first run)..."
    (cd "$ROOT_DIR" && cargo run --bin build_geonames)
  fi
}

setup_models() {
  echo "\n==> Setting up face models"

  download "$SCRFD_MODEL_URL" "$MODELS_DIR/scrfd_10g_bnkps.onnx"
  download "$GLINTR_MODEL_URL" "$MODELS_DIR/glintr100.onnx"

  echo "Installed models:"
  echo "- $MODELS_DIR/scrfd_10g_bnkps.onnx"
  echo "- $MODELS_DIR/glintr100.onnx"
}

setup_onnxruntime() {
  echo "\n==> Setting up ONNX Runtime"

  local libs_dir="$ROOT_DIR/libs/onnxruntime"
  local ort_tgz="$CACHE_DIR/onnxruntime-linux-x64.tgz"
  local extract_dir="$CACHE_DIR/onnxruntime-linux-x64"

  mkdir -p "$libs_dir"

  if ls "$libs_dir"/libonnxruntime.so* >/dev/null 2>&1; then
    echo "Using existing ONNX Runtime in $libs_dir"
    return 0
  fi

  download "$ORT_URL" "$ort_tgz"
  rm -rf "$extract_dir"
  mkdir -p "$extract_dir"
  tar -xzf "$ort_tgz" -C "$extract_dir"

  local found
  found="$(find "$extract_dir" -type f -name 'libonnxruntime.so*' | head -n 1 || true)"
  if [[ -z "$found" ]]; then
    echo "ERROR: libonnxruntime.so not found in extracted archive"
    exit 1
  fi

  cp "$found" "$libs_dir/"

  if command -v patchelf >/dev/null 2>&1; then
    patchelf --set-rpath '$ORIGIN' "$libs_dir/$(basename "$found")" || true
  fi

  if [[ ! -e "$libs_dir/libonnxruntime.so" ]]; then
    ln -s "$(basename "$found")" "$libs_dir/libonnxruntime.so" || true
  fi

  echo "Installed ONNX Runtime: $libs_dir/$(basename "$found")"
}

main() {
  echo "PhotoVault asset setup"
  echo "Root: $ROOT_DIR"

  setup_geonames
  setup_onnxruntime
  setup_models

  echo "\nDone."
}

main "$@"
