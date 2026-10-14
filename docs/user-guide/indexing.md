# Indexing Photos

Smriti's scanner walks your library folder, extracts metadata from each
photo, and writes it to a SQLite database alongside the photos. This
page covers what scanning does, what it skips, and how to refresh
the index.

## What a scan does

For every supported file under the library root:

1. **Reconciles what's already indexed** (see [Keeping the index in
   sync](#keeping-the-index-in-sync)).
2. **Skips files Smriti already knows** by path, so a rescan of an
   unchanged library costs a folder walk, not a re-read of every photo.
3. **Fingerprints new files** (size, modification time and the first
   64 KB) and adds them to the library.
4. **Reads EXIF.** Capture date, GPS coordinates, camera make/model,
   ISO, aperture, shutter, focal length, orientation.
5. **Resolves a place name** if GPS is present, using the local
   GeoNames database.
6. **Generates thumbnails** in the background.
7. **Runs face detection, then visual-search indexing**, for features
   you have enabled.

Smriti never copies, moves, or modifies your originals.

## What's indexed

**Image formats:**
JPEG, PNG, WebP, HEIC / HEIF (if `libheif` is compiled in), TIFF, BMP,
GIF (first frame), plus the common camera RAW formats with embedded
JPEG previews: NEF, CR2, ARW, DNG, ORF, RW2, PEF, SRW, RAF.

**Skipped:**
- Files smaller than 1 KB (guards against icons and junk).
- Files without a recognised extension.
- Hidden folders (those starting with `.`) — unless **Scan hidden
  folders** is enabled in Settings.
- `.photovault/` itself, always.

## Keeping the index in sync

Smriti doesn't run in the background or on a schedule. Instead, every
time you open a library it quietly checks the folder for changes (turn
this off with **Settings → Check for changes when a library opens**).
To pick up photos added while Smriti is open, reopen the library.

A check:

- **Adds new files.**
- **Follows moved and renamed files**, keeping their faces, albums and
  favourites.
- **Re-processes files edited outside Smriti** (new size or modification
  time). Same-size files whose time shifted by a whole number of quarter
  hours — what FAT/exFAT drives do when the time zone changes — are left
  alone.
- **Moves photos whose files were deleted to Smriti's Trash**, where you
  can restore them. If a large share of the library is missing at once
  (a disconnected folder, share or drive is the usual cause), nothing is
  trashed; the Timeline asks you first.
- **Leaves photos in unreadable folders alone** instead of treating them
  as deleted.

## Maintenance actions

From **Settings → Maintenance**:

- **Refresh Capture Dates** — re-reads capture dates for photos and
  videos. Smriti prefers embedded metadata, then strict filename dates,
  and uses file modified time only as a marked fallback.
- **Regenerate Thumbnails** — discards all thumbnails and lazy-rebuilds
  on next view.

## How long it takes

- **First scan** of a 10,000-photo library on a modern laptop:
  5–15 minutes for metadata, then 30 minutes – 3 hours for face
  detection on CPU (the slow step). A free Kaggle/Colab GPU bridge
  can cut the face pass to ~5 minutes — see
  [the face GPU bridge doc](../face-gpu-bridge.md).
- **Checking for changes** on an unchanged library: one folder walk —
  seconds on an SSD, about a minute for very large libraries on a USB
  hard drive.
- **Thumbnails** are lazy — generated when a view scrolls a photo
  into the viewport, not upfront. Scrolling the Timeline for the
  first time is when most thumbnail work happens.

## The `.photovault/` folder

This hidden directory at the library root contains everything Smriti
needs:

```
<library>/
├── photo1.jpg
├── photo2.jpg
├── trip-2018/
│   └── ...
└── .photovault/
    ├── photovault.db        # SQLite — all metadata
    └── thumbnails/          # three-tier thumbnail cache
```

Keep this folder when you copy or back up the library. Delete it and
Smriti will re-index from scratch on next open.

## See also

- [Settings → Indexing](settings.md) — the scan options
- [People and Faces](people.md) — the face-detection pass
- [Cleanup](cleanup.md) — finding and removing duplicates
