#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-ci}"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"

usage() {
  cat <<'EOF'
Usage: scripts/ci_local.sh [ci|release|all]

ci            Run the same core checks as .github/workflows/ci.yml.
release       Run the release build path for the current machine.
all           Run ci, then release.

Set TARGET=x86_64-unknown-linux-gnu to override the release target.
EOF
}

step() {
  printf "\n==> %s\n" "$1"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

run_ci() {
  require_cmd cargo
  require_cmd npm

  step "Rust format"
  cargo fmt --all --check

  step "Frontend dependencies"
  npm ci --prefix src-ui

  step "Frontend type check"
  npm run check --prefix src-ui

  step "Frontend tests"
  npm run test --prefix src-ui

  step "Frontend build"
  npm run build --prefix src-ui

  step "Rust clippy"
  cargo clippy --all-targets -p smriti -p smriti-tauri -- -D warnings

  step "Rust tests"
  cargo test -p smriti -p smriti-tauri
}

run_release() {
  require_cmd cargo
  require_cmd npm

  step "Frontend dependencies"
  npm ci --prefix src-ui

  step "Frontend tests"
  npm run test --prefix src-ui

  step "Frontend build"
  npm run build --prefix src-ui

  step "Tauri Linux release build"
  if command -v cargo-tauri >/dev/null 2>&1; then
    cargo tauri build --config src-tauri/tauri.conf.json --target "$TARGET" --features heic
  else
    echo "cargo-tauri is not installed; checking the release Rust target instead." >&2
    echo "Install with: cargo install tauri-cli --locked" >&2
    cargo build -p smriti-tauri --release --target "$TARGET" --features heic
  fi
}

case "$MODE" in
  ci)
    run_ci
    ;;
  release|release-linux)
    run_release
    ;;
  all)
    run_ci
    run_release
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac
