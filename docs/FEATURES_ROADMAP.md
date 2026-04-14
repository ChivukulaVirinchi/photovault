# PhotoVault Feature Roadmap — Google-Photos-Parity

Living list of candidate features grouped by impact/effort. As each is scoped
and built, it moves to a dedicated plan file under `docs/plans/` and gets
ticked off here.

Constraints for everything in this list:
- Strictly offline, single-user. No auth, no cloud, no shared libraries.
- OS-level share (Windows Share / xdg-open) is the only sharing we allow.
- Database lives on the indexed drive for full portability.

---

## Tier 1 — The "magic" features (actively planning)

| # | Feature | Plan file | Status |
|---|---------|-----------|--------|
| 1 | Memories / Rediscovery | `docs/plans/memories.md` | designing |
| 2 | Map view | `docs/plans/map_view.md` | pending |
| 3 | Auto-generated albums (trips / events) | `docs/plans/auto_albums.md` | pending |
| 4 | Insights dashboard | `docs/plans/insights.md` | pending |
| 5 | Unified search expansion | `docs/plans/unified_search.md` | pending |

All of these are purely data-surfacing on top of information we already
index. No new ML models required for Tier 1.

## Tier 2 — Content understanding (needs small ML models)

| # | Feature | Notes |
|---|---------|-------|
| 6 | Scene / object classification | Small ONNX classifier (~10-20 MB). Unlocks search by "food", "beach", "dog", etc. |
| 7 | Similar-photos suggestions | Beyond duplicates/bursts — "more like this" in detail view. Perceptual hash or scene embeddings. |
| 8 | Quality-based best-of highlights | "Top 50 of 2024" using face sharpness + aesthetic scoring. Needs a tiny NIMA-style model. |
| 9 | Auto-archive suggestions | Blurry / near-dupe / accidental shot queue for bulk cleanup. |

## Tier 3 — Quality-of-life polish

| # | Feature | Notes |
|---|---------|-------|
| 10 | Timeline zoom levels | Day → Month → Year views. Huge nav win for >20k photo libraries. |
| 11 | Jump-to-date scrubber | Right-edge year scrubber, `G`/`/date` shortcut. |
| 12 | OS share integration | Windows Share dialog / xdg-open. Share the file, not a URL. |
| 13 | Export / batch copy | Select photos → copy to folder, preserving metadata. |
| 14 | Video + RAW support | ffmpeg for video thumbs; libraw/rawler for RAW. Large surface, completeness win. |
| 15 | Panorama / Live Photo / Burst as timeline cards | Richer card types with flip-through, scrub, play-on-hover. |

## Tier 4 — Editing (defer until explicitly requested)

| # | Feature | Notes |
|---|---------|-------|
| 16 | In-viewer editing (crop / rotate / exposure / filters) | Whole rabbit hole. Save as copy, never overwrite. |

## Out of scope

- Cloud backup / sync
- Shared albums, shared libraries, chat sharing
- Web interface
- Real-time collaboration
- Face enhancement / AI "fix"
