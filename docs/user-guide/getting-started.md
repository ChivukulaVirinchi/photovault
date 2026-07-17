# Getting Started

This guide walks through the first ten minutes of using Smriti: choosing a
library, what happens during the first scan, and where your data lives.

## What Smriti is

Smriti is a desktop photo library — Linux, macOS, Windows. It points at a
folder or drive containing your photos and indexes them in place. Your
originals stay where they are; Smriti reads them, extracts metadata
(dates, GPS, camera, faces), and writes a small SQLite database
alongside the photos.

There's no cloud upload, no account, no server. Open the app, browse,
close it.

## First run

1. **Launch Smriti.** The Welcome screen invites you to pick or drop a
   folder.
2. **Pick a library.** Drag a folder onto the window, or click
   **Browse** to pick one. Smriti remembers it and offers it again
   next time.
3. **(Optional) Install the asset pack.** A first-run prompt may ask
   to download ~300 MB of ONNX face-recognition models + the GeoNames
   offline geocoding database. Declining is fine — face features and
   place names are the only things gated on this. The rest of the app
   works without it.
4. **Watch the scan.** The timeline populates live as photos are
   indexed. The first scan on a 10K-photo library typically takes
   5–15 minutes. Face detection then runs as a separate pass and can
   take longer — see [People and Faces](people.md) for how to speed
   that up with an optional remote GPU.

## Where your data goes

- **Originals** — untouched, in their folders.
- **Database** — at `<library>/.photovault/photovault.db`, on the same
  drive as the photos. Unplug the drive, plug it into another machine,
  point Smriti there: same library, same faces, same albums.
- **Thumbnails** — at `<library>/.photovault/thumbnails/`, three sizes,
  generated on demand.
- **Application settings** — in your OS's user config directory
  (Linux: `~/.config/smriti/`, macOS:
  `~/Library/Application Support/smriti/`, Windows:
  `%APPDATA%\smriti\`). Theme preference, sidebar state, recently
  opened libraries.

The library is fully portable. Backing it up = backing up the drive.

## What to do next

- Browse the [Timeline](timeline.md) — your main view.
- Once face detection finishes, head to [People](people.md) to name a
  few faces.
- Try [Search](search.md): `paris 2018`, `dad in tokyo`, `this month`.
- Open the [Map](map.md) view if your photos have GPS data.

## Common first-run questions

**Why is the scan so slow?**
The first scan walks every file under the library root, reads EXIF,
hashes each file for dedup, and (with the asset pack installed) runs
face detection. Modern 10K-photo libraries take 5–20 minutes total.
Subsequent reindexes are incremental — only changed files are
revisited. See [Indexing](indexing.md).

**Where do I add more folders?**
Smriti indexes one library at a time. Switch libraries via the
Welcome screen or **Settings → Open another library**. Each library
has its own `.photovault/` database; switching back and forth is
instant.

**Can I move my library to another drive?**
Yes. Copy the folder including the hidden `.photovault/` directory.
Open Smriti on the new machine, point it at the new location.
Everything resumes — thumbnails, face names, albums, search history.

**Face recognition is disabled — what now?**
The asset pack wasn't installed, or your hardware ran out of memory
during model load. See
[Troubleshooting](troubleshooting.md).

## See also

- [Indexing](indexing.md) — what scanning does, how reindex works
- [Settings](settings.md) — every preference and what it changes
- [Privacy](../../PRIVACY.md) — the exact network touchpoints
