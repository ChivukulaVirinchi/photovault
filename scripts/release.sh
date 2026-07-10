#!/usr/bin/env bash
# scripts/release.sh
# One-command release: bump manifests, refresh Cargo.lock, commit,
# push, tag, push tag. The pushed tag triggers
# .github/workflows/release.yml which builds cross-OS artifacts and
# the asset pack.
#
# Usage:
#   ./scripts/release.sh v0.2.0-rc.3
#   ./scripts/release.sh v0.2.0-rc.3 --dry-run
#   ./scripts/release.sh v0.2.0-rc.3 --no-tag
#   ./scripts/release.sh v0.2.0-rc.3 --allow-dirty
#   ./scripts/release.sh v0.2.0
#
# See docs/RELEASE_CHECKLIST.md for the broader release workflow and
# pre-release flag handling.

set -euo pipefail

BRANCH="master"
DRY_RUN=0
NO_TAG=0
ALLOW_DIRTY=0
VERSION_ARG=""

usage() {
    cat <<EOF
Usage: $0 <vX.Y.Z[-suffix]> [--dry-run] [--no-tag] [--allow-dirty] [--branch=<name>]

Examples:
    $0 v0.2.0-rc.3
    $0 v0.2.0-rc.3 --dry-run
    $0 v0.2.0
EOF
    exit 64
}

for arg in "$@"; do
    case "$arg" in
        --dry-run)      DRY_RUN=1 ;;
        --no-tag)       NO_TAG=1 ;;
        --allow-dirty)  ALLOW_DIRTY=1 ;;
        --branch=*)     BRANCH="${arg#--branch=}" ;;
        -h|--help)      usage ;;
        -*)             echo "Unknown flag: $arg" >&2; usage ;;
        *)              if [ -z "$VERSION_ARG" ]; then VERSION_ARG="$arg"; else usage; fi ;;
    esac
done
[ -n "$VERSION_ARG" ] || usage

# --------------------------- helpers ---------------------------

CYAN='\033[1;36m'
GRAY='\033[0;90m'
YEL='\033[1;33m'
RED='\033[1;31m'
NC='\033[0m'

say()  { printf "${CYAN}==> %s${NC}\n" "$*"; }
note() { printf "${GRAY}    %s${NC}\n" "$*"; }
warn() { printf "${YEL}!!  %s${NC}\n" "$*"; }
die()  { printf "${RED}x   %s${NC}\n" "$*" >&2; exit 1; }

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "Required command not found on PATH: $1"
}

# Normalise to TAG_NAME + MANIFEST_VERSION globals.
normalize_version() {
    local raw="$1"
    [ -n "$raw" ] || die "Version cannot be empty."
    case "$raw" in
        v*) TAG_NAME="$raw" ;;
        *)  TAG_NAME="v$raw" ;;
    esac
    MANIFEST_VERSION="${TAG_NAME#v}"
    if ! [[ "$TAG_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]]; then
        die "Version must be SemVer-shaped: vX.Y.Z or vX.Y.Z-suffix (e.g. v0.2.0, v0.2.0-rc.3). Got: $raw"
    fi
}

# Replace the [package] version line in a Cargo.toml without touching
# dependency `version = "..."` entries. Returns 0 if changed, 1 if not.
update_cargo_toml_version() {
    local path="$1" new="$2"
    [ -f "$path" ] || die "File not found: $path"
    # Use awk: walk lines, track when we're inside [package], replace
    # the first `version = "..."` line we see inside it.
    local tmp
    tmp="$(mktemp)"
    local changed
    changed="$(awk -v new="$new" '
        BEGIN { in_pkg = 0; done = 0; changed = 0 }
        /^\[package\][[:space:]]*$/      { in_pkg = 1; print; next }
        in_pkg && /^\[/                  { in_pkg = 0 }
        in_pkg && !done && /^version[[:space:]]*=[[:space:]]*"[^"]*"[[:space:]]*$/ {
            new_line = "version = \"" new "\""
            if ($0 != new_line) changed = 1
            print new_line
            done = 1
            next
        }
        { print }
        END { print "CHANGED=" changed > "/dev/stderr" }
    ' "$path" 2> >(grep '^CHANGED=' | tail -1) > "$tmp")" || true
    # Capture the CHANGED= line from stderr trick is awkward; do a
    # simpler diff-based check instead.
    if cmp -s "$path" "$tmp"; then
        rm -f "$tmp"
        return 1
    fi
    if [ "$DRY_RUN" -eq 0 ]; then
        mv "$tmp" "$path"
    else
        rm -f "$tmp"
    fi
    return 0
}

# Replace the top-level "version" field in tauri.conf.json. Single
# regex; the file has exactly one such field at the top level.
update_tauri_conf_version() {
    local path="$1" new="$2"
    [ -f "$path" ] || die "File not found: $path"
    local tmp
    tmp="$(mktemp)"
    # Sed: replace only the FIRST occurrence using 0,/regex/{...}.
    sed -E '0,/"version"[[:space:]]*:[[:space:]]*"[^"]*"/{s/("version"[[:space:]]*:[[:space:]]*)"[^"]*"/\1"'"$new"'"/}' "$path" > "$tmp"
    if cmp -s "$path" "$tmp"; then
        rm -f "$tmp"
        return 1
    fi
    if [ "$DRY_RUN" -eq 0 ]; then
        mv "$tmp" "$path"
    else
        rm -f "$tmp"
    fi
    return 0
}

# ------------------------- preconditions -------------------------

normalize_version "$VERSION_ARG"

say "Releasing $TAG_NAME (manifest version: $MANIFEST_VERSION)"
[ "$DRY_RUN" -eq 1 ] && warn "DRY RUN — no files written, no git operations"

require_cmd git
require_cmd cargo
require_cmd awk
require_cmd sed

# Move to repo root.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
note "Repo root: $REPO_ROOT"

# Branch check.
CURRENT="$(git rev-parse --abbrev-ref HEAD)"
[ "$CURRENT" = "$BRANCH" ] || die "Current branch is '$CURRENT'; expected '$BRANCH'. Switch with: git checkout $BRANCH"

if [ "$ALLOW_DIRTY" -eq 0 ]; then
    DIRTY="$(git status --porcelain)"
    if [ -n "$DIRTY" ]; then
        warn "Working tree is dirty:"
        printf '%s\n' "$DIRTY" | while read -r line; do note "$line"; done
        die "Commit or stash before releasing. Use --allow-dirty to override."
    fi
fi

# Sync check.
say "Fetching origin..."
git fetch origin "$BRANCH" --tags
LOCAL_HEAD="$(git rev-parse HEAD)"
REMOTE_HEAD="$(git rev-parse "origin/$BRANCH")"
if [ "$LOCAL_HEAD" != "$REMOTE_HEAD" ]; then
    die "Local $BRANCH (${LOCAL_HEAD:0:7}) is not in sync with origin/$BRANCH (${REMOTE_HEAD:0:7}). Pull or push first."
fi

# Tag must not already exist.
if git rev-parse -q --verify "refs/tags/$TAG_NAME" >/dev/null 2>&1; then
    die "Tag already exists locally: $TAG_NAME"
fi
if git ls-remote --exit-code --tags origin "refs/tags/$TAG_NAME" >/dev/null 2>&1; then
    die "Tag already exists on origin: $TAG_NAME"
fi

# --------------------------- bump ---------------------------

CARGO_CHANGED=0
TAURI_CHANGED=0

say "Bumping Cargo.toml -> $MANIFEST_VERSION"
if update_cargo_toml_version "Cargo.toml" "$MANIFEST_VERSION"; then
    CARGO_CHANGED=1
    note "Updated"
else
    warn "Cargo.toml [package] version was already $MANIFEST_VERSION"
fi

say "Bumping src-tauri/tauri.conf.json -> $MANIFEST_VERSION"
if update_tauri_conf_version "src-tauri/tauri.conf.json" "$MANIFEST_VERSION"; then
    TAURI_CHANGED=1
    note "Updated"
else
    warn "tauri.conf.json version was already $MANIFEST_VERSION"
fi

if [ $CARGO_CHANGED -eq 0 ] && [ $TAURI_CHANGED -eq 0 ]; then
    die "Nothing to bump; both files already at $MANIFEST_VERSION. Use scripts/release_publish.ps1 to tag the existing commit."
fi

say "Refreshing Cargo.lock"
if [ "$DRY_RUN" -eq 0 ]; then
    cargo update -p smriti --quiet
fi

# --------------------------- diff preview ---------------------------

say "Diff to commit:"
if [ "$DRY_RUN" -eq 0 ]; then
    git --no-pager diff -- Cargo.toml Cargo.lock src-tauri/tauri.conf.json
fi

echo
echo "About to:"
echo "  1) git commit -m 'chore(release): $TAG_NAME'"
echo "  2) git push origin $BRANCH"
if [ "$NO_TAG" -eq 0 ]; then
    echo "  3) git tag -a $TAG_NAME -m 'Smriti $TAG_NAME'"
    echo "  4) git push origin $TAG_NAME"
fi
echo

if [ "$DRY_RUN" -eq 1 ]; then
    say "DRY RUN complete; nothing changed."
    exit 0
fi

printf "Proceed? [y/N] "
read -r confirm
case "$confirm" in
    [Yy]*) ;;
    *)
        warn "Cancelled. Reverting uncommitted changes."
        git checkout -- Cargo.toml Cargo.lock src-tauri/tauri.conf.json
        exit 1
        ;;
esac

# --------------------------- commit + push ---------------------------

say "Committing"
git add Cargo.toml Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore(release): $TAG_NAME"

say "Pushing to origin/$BRANCH"
git push origin "$BRANCH"

if [ "$NO_TAG" -eq 1 ]; then
    say "Done. Skipping tag (per --no-tag)."
    exit 0
fi

# --------------------------- tag + push tag ---------------------------

say "Tagging $TAG_NAME"
git tag -a "$TAG_NAME" -m "Smriti $TAG_NAME"

say "Pushing tag"
if ! git push origin "$TAG_NAME"; then
    warn "Tag push failed. Cleaning up local tag."
    git tag -d "$TAG_NAME" >/dev/null 2>&1 || true
    die "git push origin $TAG_NAME failed"
fi

# --------------------------- done ---------------------------

REMOTE_URL="$(git config --get remote.origin.url)"
GH_PATH=""
if [[ "$REMOTE_URL" =~ github\.com[:/](.+)$ ]]; then
    GH_PATH="${BASH_REMATCH[1]%.git}"
fi

echo
say "Released $TAG_NAME"
[ -n "$GH_PATH" ] && note "Workflow:  https://github.com/$GH_PATH/actions/workflows/release.yml"
[ -n "$GH_PATH" ] && note "Releases:  https://github.com/$GH_PATH/releases"
if [[ "$TAG_NAME" =~ -(rc|beta|alpha) ]]; then
    echo
    warn "PRE-RELEASE — after the workflow finishes, manually toggle"
    warn "'Set as a pre-release' on the draft. release.yml hardcodes"
    warn "prerelease: false so /releases/latest/ continues to point at"
    warn "the previous stable until you uncheck this manually."
fi
