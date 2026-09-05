# Settings

Settings control theme, indexing behavior, face thresholds, burst
windows, map cache, updates, and maintenance actions.

Changes are persisted locally in user config storage.

## Appearance

- **Theme** — dark, light, or follow the system theme.
- **Show photo stacks in timeline** — collapse conservative burst and
  duplicate groups into one timeline tile. Turn this off to show every
  photo individually.

## Indexing

- **Scan hidden folders** — include directories starting with `.`.
  Off by default because hidden folders often contain metadata, not
  photos.
- **Date format** — how dates render in day headers and insights.

## Assets

- **Asset inventory** — shows local runtimes, models, and offline data
  Smriti can use, including their status, size, and on-disk location.
- **Asset locations** — each row shows the exact file path Smriti is
  using. In development checkouts and portable installs, assets may be
  resolved from different search roots.

Assets live outside the main app binary so the installer stays small.
Timeline browsing still works when optional assets are missing; local
face recognition, place lookup, and local visual search depend on the
matching assets being installed.

### Visual search

- **Download visual model** — installs the optional local
  `ViT-B-32 SigLIP2 256` semantic search model. The download is large
  and is stored in the user asset directory, not bundled into Smriti.
- **Index visual search** — creates per-library image vectors under
  `<library>/.photovault/semantic`. It also requires the ONNX Runtime
  installed by smart setup. It is resumable; the Welcome setup starts it
  automatically and later scans pick up pending files.
- **Recheck visual search** — refreshes the status counts shown in
  Settings.

Once installed and indexed, Search can match text such as `beach sunset`
against image meaning, and Photo Detail can show visually similar photos.

## Face Recognition

- **Face confidence** — minimum detection score for a face to be
  stored. Lower values catch more faces but also more false
  positives.
- **Clustering threshold** — how tolerant the system is when grouping
  visually similar faces. Lower means tighter clusters (more groups,
  cleaner), higher means looser (fewer groups, more mixing). Default
  works for most libraries.

See the [People and Faces](people.md) guide for the full pipeline.

## Burst detection

- **Burst window** — time window (in seconds) used to group burst
  shots. Default 3 seconds matches most camera apps.

Photo stacks reuse burst and duplicate detection results. After either
detector runs, Smriti refreshes stack suggestions and chooses a best
photo using the detector's suggestion plus quality signals such as
resolution, file size, faces, sharpness, and exposure.

## Trash

Trashing is reversible. Permanent deletion is explicit; there is no
automatic age-based deletion.

## Map

- **Tile cache size** — how much disk space tile caching can use.
  Clearing the cache forces a redownload on next view.
- **Home city override** — used for trip-detection and "photos from
  this trip" grouping.

## Memories

- **Enable memories** — toggle the "on this day" cards in the
  Timeline banner and the dedicated Memories view.

## Updates

- **Automatically check for updates** — when enabled, Smriti
  queries `api.github.com` at most once every 24 hours to see if a
  new release has been published. This is **opt-in** and off by
  default. See [PRIVACY.md](../../PRIVACY.md) for the full
  disclosure.
- **Check for updates now** — run the check immediately, regardless
  of the automatic-check setting.

When an update is available, a banner appears at the top of the app
with a **Download** button. What the button does depends on how you
installed Smriti:

- **AppImage, portable Windows zip** — downloads the new artifact,
  verifies its SHA256 against the published `SHA256SUMS`, and
  atomically replaces the running binary. You'll be prompted to
  relaunch.
- **Windows MSI** — downloads the new MSI, triggers the Windows
  installer with a UAC prompt. Smriti exits; relaunch from the
  Start menu when the install completes.
- **macOS .dmg** — downloads the new .dmg and opens it. Drag the
  new `.app` into `/Applications` as you did for the first install.
- **System package manager (apt, brew, flatpak, winget)** — the
  banner shows the matching upgrade command instead of
  self-replacing. Smriti won't interfere with your package
  manager.
- **Source build** — the banner suggests `git pull && cargo build
  --release`.

## Maintenance actions

These are one-shot operations — Smriti doesn't run them on a
schedule.

- **Import Takeout ZIPs** — migrate all parts of a Google Photos
  Takeout export into the open library without manually extracting it.
- **Rescan Library** — re-walk the drive and pick up newly added,
  moved, or deleted files.
- **Rebuild Faces (Full)** — re-run face detection + clustering
  from scratch. Useful after a face-model update.
- **Check for Changes** — scan for moves and deletions without
  re-indexing unchanged files.
- **Regenerate Thumbnails** — invalidate cached paths and run the thumbnail pass.
- **Refresh Capture Dates** — re-read capture dates for photos and
  videos. Smriti prefers embedded metadata, then strict filename dates,
  and uses file modified time only as a marked fallback.
- **Asset Inventory** — inspect which local model/runtime/data files
  are present and which exact paths Smriti resolved.
- **Download Assets** — downloads the optional Smriti asset pack from
  the GitHub release and installs it into the app's managed asset
  folder.
- **Fix Rotated Photos** — regenerate cached data for photos whose
  EXIF orientation wasn't previously applied.

## Keyboard shortcuts

See the [Keyboard shortcuts](keyboard-shortcuts.md) guide for the
full list. Settings also renders an inline reference at the bottom.
