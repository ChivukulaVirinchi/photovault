# Phase 3: Code Structure & UX Polish

## Part A: File Splits

Three files exceed 800 lines and need splitting into modules.

### Files to Split

| File | Lines | Split Into |
|------|-------|-----------|
| `src/app.rs` | 4,159 | `src/app/` directory (12 files) |
| `src/db/face_repo.rs` | 877 | `src/db/face_repo/` directory (5 files) |
| `src/views/people.rs` | 825 | `src/views/people/` directory (3 files) |

### Rust Module Split Pattern

Convert `foo.rs` to `foo/mod.rs` with submodules. Re-export everything from `mod.rs` to keep the public API identical. No downstream code changes needed.

```
Before: src/app.rs
After:  src/app/mod.rs        (re-exports + trait impls)
        src/app/messages.rs   (Message enum)
        src/app/state.rs      (PhotoVault struct + new())
        src/app/handlers/...  (update logic by domain)
        src/app/views.rs      (view rendering)
```

All `PhotoVault` struct fields become `pub(crate)` so handler submodules can access them.

---

### Split 1: `src/db/face_repo.rs` (877 lines) -> `src/db/face_repo/`

Start here — smallest split, good practice run.

| New File | Contents | ~Lines |
|----------|----------|--------|
| `mod.rs` | `FaceRepo` struct, `FaceClusterRecord`, `GalleryEmbedding`, re-exports | ~60 |
| `read.rs` | `get_all_faces_with_embeddings`, `get_all_faces_with_photo_embeddings`, `get_unclustered_*`, `get_all_clusters`, `get_all_faces_with_paths`, `get_person_names_for_photo`, `get_photos_for_cluster`, `get_contextual_cluster_candidates` | ~350 |
| `write.rs` | `insert_face`, `insert_face_tx`, `mark_photo_processed`, `mark_photo_processed_tx`, `reset_if_no_faces`, `name_cluster` | ~120 |
| `clustering.rs` | `create_cluster`, `merge_clusters`, `delete_all_clusters`, `assign_face_to_cluster`, `get_cluster_centroids` | ~200 |
| `gallery.rs` | `get_gallery_embeddings`, `get_cluster_photo_ids`, `refresh_all_galleries`, `refresh_gallery_tx`, `populate_face_thumbnails`, `normalize_cluster_stats`, `refresh_cluster_stats_tx` | ~250 |

`mod.rs` structure:
```rust
mod read;
mod write;
mod clustering;
mod gallery;

pub use read::*;
pub use write::*;
pub use clustering::*;
pub use gallery::*;

pub struct FaceRepo<'a> { pub(crate) conn: &'a Connection }
pub struct FaceClusterRecord { ... }
pub struct GalleryEmbedding { ... }

impl<'a> FaceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
}
```

Each subfile adds methods via `impl<'a> FaceRepo<'a> { ... }` importing `use super::FaceRepo`.

**Downstream impact:** `db/mod.rs` already has `pub mod face_repo;` — no change needed. All types remain at `db::face_repo::FaceClusterRecord` etc.

---

### Split 2: `src/views/people.rs` (825 lines) -> `src/views/people/`

| New File | Contents | ~Lines |
|----------|----------|--------|
| `mod.rs` | `PeopleView` struct, re-exports | ~30 |
| `grid.rs` | `view_with_clusters()` — main grid view, status bar, merge bar, empty state | ~400 |
| `detail.rs` | `view_cluster_detail()` — back nav, face avatar, name editing, photo grid | ~200 |
| `cards.rs` | `person_card()`, `person_card_merge()` — individual cluster card rendering | ~270 |

`mod.rs` structure:
```rust
mod grid;
mod detail;
mod cards;

pub struct PeopleView;

impl PeopleView {
    pub fn view_with_clusters(...) -> Element<'_, Message> {
        grid::view_with_clusters(...)
    }
    pub fn view_cluster_detail(...) -> Element<'_, Message> {
        detail::view_cluster_detail(...)
    }
}
```

Or simply make grid/detail/cards functions `pub` and call them directly.

**Downstream impact:** `views/mod.rs` has `pub mod people;` — no change. `app.rs` calls `PeopleView::view_with_clusters(...)` — no change.

---

### Split 3: `src/app.rs` (4,159 lines) -> `src/app/`

This is the largest split. The key insight: the `update()` method is a ~2500-line match statement. Each arm group becomes its own handler module.

| New File | Contents | ~Lines |
|----------|----------|--------|
| `mod.rs` | iced trait impl delegation, `title()`, `theme()`, `subscription()` | ~120 |
| `messages.rs` | `Message` enum, `View` enum, `ScanResult` | ~250 |
| `state.rs` | `PhotoVault` struct fields, `ScanState`, `new()` constructor, helper methods (`timeline_columns_for_width`, `configured_thumbnail_size`, `merge_detected_and_remembered_drives`) | ~350 |
| `views.rs` | `view()` method body — match on `current_view`, delegates to view modules | ~150 |
| `handlers/mod.rs` | Top-level `handle()` dispatch function | ~50 |
| `handlers/scanning.rs` | Drive selection, scan start/poll/cancel/finish | ~300 |
| `handlers/timeline.rs` | Photo loading, selection, navigation, detail view, rotation | ~350 |
| `handlers/thumbnails.rs` | Thumbnail batch scheduling, generation, saving | ~300 |
| `handlers/faces.rs` | Face processing, clustering, people view interactions, merge | ~400 |
| `handlers/duplicates_bursts.rs` | Duplicate detection, burst detection, group actions | ~400 |
| `handlers/search_cull.rs` | Search input, execution, cull mode enter/exit/navigate | ~400 |
| `handlers/trash.rs` | Trash/restore/permanent delete/empty | ~250 |
| `handlers/documents.rs` | Document loading, analysis, filtering | ~150 |
| `handlers/settings.rs` | Theme/config changes, rescan, geocoding, maintenance | ~400 |

`handlers/mod.rs` dispatch pattern:
```rust
pub fn handle(app: &mut PhotoVault, message: Message) -> Task<Message> {
    use Message::*;
    match message {
        // Scanning
        SelectDrive(_) | BrowseFolder | FolderSelected(_) | DrivesDetected(_)
        | StartScan | PollScanChannels | CancelScan | ScanFinished(_) 
        | ScanComplete(_) => scanning::handle(app, message),

        // Timeline & photo detail
        PhotosLoaded(_) | SelectPhoto(_) | ClosePhotoDetail | NextPhoto
        | PreviousPhoto | ToggleMetadataPanel | RotatePhoto 
        | DisplayImageReady(_) | ... => timeline::handle(app, message),

        // ... etc for each domain
    }
}
```

Each handler module:
```rust
use super::super::state::PhotoVault;
use super::super::messages::Message;

pub fn handle(app: &mut PhotoVault, message: Message) -> Task<Message> {
    match message {
        Message::SelectDrive(drive) => { ... }
        // only handles its own message variants
        _ => Task::none(),
    }
}
```

**Downstream impact:** `main.rs` has `mod app;` and uses `app::PhotoVault`, `app::Message` — `mod.rs` re-exports both, no change needed.

---

## Part B: UX Fixes

### UX 1: Scroll position preservation

**Files:** `app/state.rs`, `app/handlers/timeline.rs`
**Problem:** Timeline -> PhotoDetail -> back loses scroll position.

**Fix:**
1. Add field to PhotoVault: `timeline_scroll_offset: iced::widget::scrollable::RelativeOffset`
2. When navigating TO PhotoDetail from Timeline, save current scroll offset
3. When navigating BACK to Timeline, return `scrollable::snap_to(self.timeline_scroll_id, self.timeline_scroll_offset)`

iced 0.13 provides `scrollable::Id` and `scrollable::snap_to()` for this purpose. The `timeline_scroll_id` is created once in `new()`.

---

### UX 2: ETA for face processing

**Files:** `app/state.rs`, `views/people/grid.rs`
**Problem:** Face processing shows count but no time estimate.

**Fix:**
1. Add field: `face_processing_start_time: Option<std::time::Instant>`
2. Set it when face processing starts (in `Message::ProcessFaces` handler)
3. In People view status bar, compute and display ETA:

```rust
if processed > 0 {
    let elapsed = start_time.elapsed().as_secs_f64();
    let rate = processed as f64 / elapsed;
    let remaining_secs = (total - processed) as f64 / rate;
    // Format as "~2m 30s remaining" or "~45s remaining"
}
```

---

### UX 3: Pause/resume for long operations

**Files:** `services/face_processor.rs`, `app/state.rs`, `app/handlers/faces.rs`
**Problem:** Cancel means start over. No pause.

**Fix:**
1. Add `pause_flag: Arc<AtomicBool>` alongside existing `cancel_flag`
2. In the face processing loop (now rayon parallel), check pause before each photo:

```rust
while pause_flag.load(Ordering::Relaxed) {
    std::thread::sleep(Duration::from_millis(100));
    if cancel_flag.load(Ordering::Relaxed) { break; }
}
```

3. Add `Message::PauseFaceProcessing` / `Message::ResumeFaceProcessing`
4. UI: Toggle button (Pause <-> Resume) next to the existing Cancel button

---

### UX 4: Thumbnail quality from config

**Files:** `services/thumbnail.rs`, `app/state.rs`
**Problem:** `configured_thumbnail_size()` always returns `Medium` (500px). Config has `thumbnail_size` field but it's not wired through.

**Fix:** Wire `config.thumbnail_size` to the thumbnail service. Map the config value to the `ThumbnailSize` enum:
- <= 300: Small
- <= 500: Medium  
- \> 500: Large

Or better: pass the raw pixel value directly to the resize logic instead of using the enum.

---

### UX 5: Photo list pagination

**Files:** `app/state.rs`, `app/handlers/timeline.rs`, `views/timeline.rs`
**Problem:** `self.photos: Vec<Photo>` holds ALL photos. 100k+ = high memory.

**Fix (incremental approach):**
1. Load photos in pages of 2000: `photos_page: usize`, `photos_per_page: 2000`
2. Add `load_more_photos()` triggered when scrolling near the bottom
3. Use `PhotoRepo::get_all_by_date(limit, offset)` which already supports pagination
4. Append new pages to `self.photos` as user scrolls

This is a pragmatic middle ground — not true virtual scrolling (which would require a custom iced widget), but keeps initial memory bounded. 2000 photos at ~200 bytes each = ~400KB, very manageable.

---

## Order of Implementation

| Step | Change | Complexity |
|------|--------|------------|
| 1 | Split `face_repo.rs` | Mechanical, low risk |
| 2 | Split `people.rs` | Mechanical, low risk |
| 3 | Split `app.rs` | Large but mechanical |
| 4 | UX 2 — ETA display | Simple addition |
| 5 | UX 1 — Scroll preservation | Needs iced scrollable API |
| 6 | UX 4 — Thumbnail config wiring | Simple wiring |
| 7 | UX 3 — Pause/resume | Moderate |
| 8 | UX 5 — Pagination | Most complex UX change |

## Verification

| Change | Verification |
|--------|-------------|
| All splits | `cargo build` succeeds, `cargo test` passes, no downstream changes |
| UX 1 | Timeline -> PhotoDetail -> Back: scroll position within 50px of original |
| UX 2 | Start face processing on 100+ photos: ETA appears after ~5 photos |
| UX 3 | Pause mid-processing: stops within 1s. Resume: continues from where it paused |
| UX 4 | Change thumbnail size in settings, regenerate: thumbnails match new size |
| UX 5 | Open library with 10k+ photos: initial load is fast, scroll to load more works |
