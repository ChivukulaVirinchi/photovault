# Future Scope

Contributor-facing roadmap for Tier 2/3/4 features from the old roadmap.

## Tier 2 - Content understanding

### Scene / object classification

**Status**: open · seeking contributors · complexity: M · skills: Rust, ML

**What**: Add a small ONNX classifier (10-20 MB) to tag photos with scene/object labels like `food`, `beach`, `dog`, `indoor`.

**Why**: Enables content search that is currently impossible without manual tags.

**Technical approach**:
- Add new model wrapper in `src/ml/`
- Run classification in indexing pipeline
- Persist top labels and confidence in a new table
- Extend search filters and result ranking

### Similar photo suggestions

**Status**: open · seeking contributors · complexity: M · skills: Rust, retrieval systems

**What**: "More like this" suggestions in photo detail.

**Why**: Users can browse related moments beyond exact duplicates/bursts.

**Technical approach**:
- Reuse perceptual or embedding vectors
- Build nearest-neighbor lookup with exclusion rules
- Add UI strip in photo detail

### Quality-based highlights

**Status**: open · seeking contributors · complexity: L · skills: Rust, ML

**What**: "Top N of year" based on quality and relevance.

**Why**: Faster curation for large libraries.

**Technical approach**:
- Add aesthetic scoring model (or weighted heuristic)
- Combine with blur/sharpness + face signals
- Expose year-based smart collections

### Auto-archive suggestions

**Status**: open · seeking contributors · complexity: M · skills: Rust, product UX

**What**: Suggest likely throwaways (blur, near-duplicate, accidental shots).

**Why**: Reduces manual cleanup effort.

**Technical approach**:
- Score candidates during indexing
- Store reasons/confidence
- Add review queue UI with one-click actions

## Tier 3 - Quality of life

### Timeline zoom levels

**Status**: open · seeking contributors · complexity: M · skills: Rust, UI

**What**: Day/month/year zoom in timeline.

**Why**: Improves navigation for very large libraries.

### Jump-to-date scrubber

**Status**: open · seeking contributors · complexity: S · skills: Rust, UI

**What**: Right-edge date scrubber and keyboard jump shortcuts.

**Why**: Faster random access in long histories.

### OS share integration

**Status**: open · seeking contributors · complexity: M · skills: Rust, platform APIs

**What**: Native file-sharing flows via OS integrations.

**Why**: Better desktop-native UX without cloud features.

### Export / batch copy

**Status**: open · seeking contributors · complexity: M · skills: Rust, filesystem

**What**: Export selected photos to folder with metadata preserved.

**Why**: Practical handoff workflows.

### Video + RAW support

**Status**: open · seeking contributors · complexity: L · skills: Rust, ffmpeg/libraw

**What**: Add thumbnails and indexing for video and RAW formats.

**Why**: Completeness for modern photo workflows.

### Rich timeline cards

**Status**: open · seeking contributors · complexity: L · skills: Rust, UI

**What**: Panorama/live-photo/burst card experiences in timeline.

**Why**: More expressive browsing and richer context.

## Tier 4 - Editing (deferred)

### In-viewer editing

**Status**: open · deferred until explicit request · complexity: L · skills: Rust, image processing, UI

**What**: Crop/rotate/exposure/filters in photo viewer.

**Why**: Full workflow in one app, if product direction chooses editing.

**Technical approach**:
- Non-destructive edits (save-as-copy)
- Sidecar/DB metadata for edit history
- GPU-friendly preview pipeline

## Claiming work

- Open or comment on a GitHub issue for the feature.
- Mark status as `claimed by @username` when in progress.
