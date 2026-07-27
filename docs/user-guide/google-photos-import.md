# Importing Google Photos

Smriti can migrate a Google Photos library directly from Google
Takeout ZIP files. You do not need to unzip, combine or rearrange the
export first.

## Before importing

In Google Takeout, export **Google Photos** as ZIP files. Large exports
are split into numbered ZIPs. Download every part from the same export;
Smriti reads them as one set because a media file and its supplemental
JSON metadata can be placed in different parts.

Keep enough free space in the destination library for the uncompressed
unique originals. Smriti checks this before writing.

## Start the import

From Welcome:

1. Choose **Import Google Photos**, or drop all Takeout ZIPs together.
2. Select every ZIP from the export.
3. Choose the folder or drive that will become the Smriti library.

From an open library:

1. Open **Settings → Library → Google Photos**.
2. Choose **Import Takeout ZIPs**.
3. Select every ZIP from the export.

That is the whole migration. Progress remains visible while you browse
the app.

## What Smriti preserves

- Each byte-identical original is stored once, even when Takeout repeats
  it in year and album folders.
- Distinct edited versions remain distinct files.
- Google Photos capture dates, including corrected dates in supplemental
  metadata.
- GPS coordinates and offline place names when GeoNames is installed.
- Favorites.
- User-created Google Photos albums.
- Embedded EXIF camera, lens and exposure details through Smriti's normal
  metadata reader.

Imported originals are ordinary files under:

```text
<library>/Imported from Google Photos/<year>/
```

Takeout ZIPs are never modified or deleted.

## Resume and repeat

The import is restart-safe. Completed originals are atomically placed
before being recorded in the library-local Takeout ledger. If the app
closes, the drive disconnects, or you cancel the job, select the same
ZIP files again. Smriti verifies and reuses completed files instead of
creating duplicates.

Re-importing the same export is also safe. A later export can restore
additional album or metadata records for an already imported original.

## Warnings and unsupported data

Smriti imports photo and video formats supported by the normal scanner.
It reports unsupported files, very small invalid media, malformed
sidecars and corrupt ZIP entries at completion. A missing sidecar does
not block its media file; embedded EXIF and filename dates are used
instead.

Only ZIP Takeout exports are supported. TGZ archives must be requested
again from Takeout as ZIP. Google comments, sharing permissions and
Google's people labels are not turned into Smriti features. Smriti
performs its own local face recognition.

## Safety

Archive paths are validated before extraction, files are streamed
instead of expanded into a second temporary tree, and a file is moved
into the library only after it has been fully written and hashed.
Existing files are never overwritten.
