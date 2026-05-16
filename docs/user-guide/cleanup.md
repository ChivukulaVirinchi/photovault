# Cleanup

Cleanup brings the three tools for thinning out a library into one
place: **Duplicates**, **Bursts**, and **Trash**. Each surfaces a
different kind of clutter; all of them keep your originals safe
until you confirm a deletion.

## Duplicates

Smriti detects two kinds of duplicates during indexing:

- **Exact duplicates** — files with identical SHA-256 hashes. These
  are byte-for-byte copies, often the result of importing the same
  card twice or copying photos between folders.
- **Perceptual near-duplicates** — files that look the same to the
  eye but aren't byte-identical. Detected via a DCT perceptual hash
  (pHash) with a tight threshold (~94% bit agreement). This catches
  re-encoded copies, JPEG-from-RAW exports, watermarked variants,
  and rescaled exports of the same source.

The Duplicates view groups suspected matches together. For each
group:

- Smriti **suggests a keeper** (typically the largest / highest-
  resolution version with the most-original-looking filename).
- The rest are flagged for trash.
- Override the suggestion with a click; **Apply** moves the marked
  copies to trash.

### Why the threshold is tight

Perceptual hashing is fuzzy by design. A loose threshold flags
visually similar but compositionally different photos as
"duplicates" — same wall, same lighting, but different subjects.
Smriti uses a strict threshold so you don't have to second-guess
every group. Photos that *look* similar but were taken seconds apart
belong to the burst detector, not duplicates.

## Bursts

Bursts are short rapid-fire sequences — your phone's burst mode, or a
photographer firing off frames at a moving subject. Smriti detects
them by:

- **Time gap** — consecutive photos within 10 seconds of each other.
- **Burst length** — 2+ photos.
- **Similarity** — ≥ 65% visual similarity between consecutive
  shots.

The Bursts view shows each detected burst as a card; opening it
reveals the full sequence. Smriti picks a "best" frame as the
keeper (sharpest, with eyes-open if faces are present); the rest can
be trashed in bulk.

Bursts are detected on demand the first time you visit the view, then
cached. New photos trigger a re-scan.

### Tuning burst detection

- **Burst window** in Settings (default 10s) widens or tightens the
  time gap used.
- Detection ignores folder boundaries by default — modern phones
  group photos by month, not by burst session, so requiring same-
  folder grouping misses real bursts.

## Trash

Trashing a photo doesn't delete it. It moves the row's
`is_trashed` flag to true; the photo is hidden from Timeline,
Search, Memories, and every other view. The file on disk is
untouched until you permanently delete it.

The Trash view lists trashed photos. From it you can:

- **Restore** — return a photo to the active library.
- **Delete permanently** — remove the file from disk *and* the
  database row. There's no undo for this.
- **Empty trash** — permanent-delete everything in trash.

### Auto-delete

**Settings → Trash → Auto-delete after** (default 30 days)
permanently deletes photos that have been in the trash longer than
the configured window. Set to 0 to disable auto-delete (you'll have
to empty trash manually).

## Workflow

A typical cleanup pass:

1. Open **Duplicates** → review the suggested keeps → trash the
   redundant copies.
2. Open **Bursts** → review each burst → keep the best, trash the
   rest.
3. Browse the Timeline for anything obviously bad you missed.
4. Open **Trash** → verify the list → empty trash, or wait for
   auto-delete.

## See also

- [Settings → Trash](settings.md) — auto-delete window
- [Settings → Burst detection](settings.md) — burst window
- [Indexing](indexing.md) — when these detectors run
