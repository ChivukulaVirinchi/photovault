#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

before=$(du -sh target/ 2>/dev/null | cut -f1 || echo "0")
echo "Before: $before"

# Stale incremental caches
rm -rf target/debug/incremental target/release/incremental

# Old bundles: keep the most recent of each format
if [ -d target/release/bundle ]; then
  for fmt in deb rpm appimage msi dmg; do
    find target/release/bundle -type f -name "*.${fmt}" \
      -printf '%T@ %p\n' 2>/dev/null \
      | sort -nr | tail -n +2 | cut -d' ' -f2- | xargs -r rm -f
  done
fi

# Old build dirs (rust-incremental output for hashes no longer needed)
find target/debug/build -maxdepth 1 -type d -mtime +3 -exec rm -rf {} + 2>/dev/null || true

after=$(du -sh target/ 2>/dev/null | cut -f1 || echo "0")
echo "After:  $after"
