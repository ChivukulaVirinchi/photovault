# Indexing Photos

Smriti's scanner walks your library folder, extracts metadata from each
photo, and writes it to a SQLite database alongside the photos. This
page covers what scanning does, what it skips, and how to refresh
the index.

## What a scan does

For every supported file under the library root:

1. **Checks if Smriti has already indexed it.** Files identified by
   path + size + modification time are skipped on subsequent scans.
2. **Reads EXIF.** Capture date, GPS coordinates, camera make/model,
   ISO, aperture, shutter, focal length, orientation.
3. **Hashes the file** (SHA-256) for byte-level duplicate detection.
4. **Resolves a place name** if GPS is present, using the local
   GeoNames database.
5. **Generates thumbnails** (three sizes) lazily, the first time each
   photo is viewed.
6. **Queues face detection** if the asset pack is installed.

Smriti never copies, moves, or modifies your originals.

## What's indexed

**Image formats:**
JPEG, PNG, WebP, HEIC / HEIF (if `libheif` is compiled in), TIFF, BMP,
GIF (first frame), plus the common camera RAW formats with embedded
JPEG previews: NEF, CR2, ARW, DNG, ORF, RW2, PEF, SRW, RAF.

**Skipped:**
- Files smaller than 10 KB (guards against icons and email
  thumbnails).
- Files without a recognised extension.
- Hidden folders (those starting with `.`) — unless **Scan hidden
  folders** is enabled in Settings.
- `.photovault/` itself, always.

## Reindex actions

Smriti doesn't run scans on a schedule. You re-trigger them from
**Settings → Maintenance**:

- **Rescan Library** — walks the entire library, picks up new files,
  files that changed, files that moved, and files that were deleted.
  Existing rows for unchanged files are left alone.
- **Check for Changes** — like rescan but doesn't re-process unchanged
  files. Faster when you only added or removed a few photos.
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
- **Incremental rescan** when a few hundred new photos appeared:
  seconds to minutes.
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
