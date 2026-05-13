# Smriti — Face rec: accuracy gates, K-similar review, batched + remote GPU

## Context

After running on a real 91k-photo library, two pain points dominate:

1. **Accuracy** — many false positives at scale. "Categorised photos as having a person when he wasn't in it." The current pipeline ships clean-dataset thresholds (clustering at cosine ≥ 0.65, blur Laplacian ≥ 10, confidence ≥ 0.3) that work on 5k photos but fail on 200k+ faces, where 0.1% of pairs misclassified means thousands of bridge edges in union-find. Worse: user rejections in the existing review UI evaporate — there's no record that face X is **not** person Y, so the same misclassification can recur on the next clustering run.

2. **Speed** — i7-7567U + 91k photos = hours. Local pipeline already has two-stage decode, HNSW clustering, rayon parallelism, and per-chunk streaming. Remaining single-machine wins (~3–5×) are batched ONNX inference. Beyond that, hardware is the ceiling.

This plan covers six ship-independent phases. Phases A–C and E–F harden the **default** CPU / local-GPU-EP workflow — every user gets a more accurate and faster scan. Phase D adds an **opt-in, additive** Kaggle/Colab GPU bridge for power users who want 10–50× speed-ups on free remote silicon; default flows are untouched.

### User constraints applied
- **Local-first stays first-class.** Kaggle bridge is strictly additive — never required, always falls back to local.
- **Privacy-respecting.** GPU bridge sends only 112×112 aligned face crops (not photos, not metadata) to a notebook URL the user controls.
- **No silent automation.** Build cleanup is a documented rule + script, not a hook.
- **AdaFace swap is in scope** as Phase F.

### Research touchstones
- **digiKam People view** — three-state (Unknown / Unconfirmed / Confirmed) + per-face ✓ / ↺ / ✕. *docs.digikam.org/en/left_sidebar/people_view.html*
- **Apple Photos "Confirm Additional Photos"** — yes/no per face, auto-advance. *discussions.apple.com/thread/254474800*
- **Google Photos face grouping** — K-similar prompt on confirmed exemplars. *support.google.com/photos/answer/6128838*
- **Immich** — every embedding stored individually (we already do this in `person_gallery_embeddings`). *docs.immich.app/features/facial-recognition/*
- **immich-face-fix** — keyboard Y/N/S/Z review. *github.com/pabera/immich-face-fix*
- **AdaFace (CVPR 2022, updated 2024 WebFace12M)** — SOTA on hard poses; drop-in 112×112→512-d replacement for glintr100. *github.com/mk-minchul/AdaFace*

---

## Phases (each ships independently)

| # | Phase | Effort | Ships? |
|---|---|---|---|
| A | Accuracy quick wins (gates + threshold) | ~2 hrs | Standalone |
| B | `face_negatives` + per-face review + K-similar | ~3 days | Standalone (depends on A) |
| C | Batched local embedding | ~half day | Standalone |
| D | Kaggle/Colab GPU bridge (opt-in) | ~2 days | Standalone (depends on C) |
| E | Disk hygiene rule + cleanup script | ~30 min | Standalone |
| F | AdaFace embedder swap | ~1 hr | Standalone |

Recommended order: A → B → C → D → F. E can land anywhere.

---

## Phase A — Accuracy quick wins

### Tighten quality gates (`src/services/face_processor.rs:838–846`)

```rust
const MIN_FACE_AREA_PX2: f32  = 900.0;   // was 400 (≈30×30 vs 20×20)
const MIN_FACE_CONFIDENCE: f32 = 0.55;    // was 0.3
const MIN_LAPLACIAN_VAR: f32  = 40.0;    // was 10.0
const MAX_FACE_YAW_DEG: f32   = 35.0;    // NEW
```

### Add yaw gate (NEW helper)

New `fn estimate_yaw_from_landmarks(landmarks: &[Landmark; 5]) -> f32` in `src/ml/face_detector.rs`. Uses the 5-point landmarks SCRFD already emits: yaw ≈ `atan2(nose_x_offset_from_eyes_midpoint, inter_ocular_distance)`. ~30 LoC. Reject |yaw| > 35° in `face_processor.rs` between the existing gates and the embed call (`face_processor.rs:547`).

### Tighten clustering threshold (`src/ml/clustering.rs:26`)

```rust
const DEFAULT_MAX_DISTANCE: f32 = 0.28;  // was 0.35 (cosine sim ≥ 0.72 vs ≥ 0.65)
```

Also bump the default in `src/config/mod.rs:face_clustering_threshold`. A migration sets existing installs to the new default *only if* they're still on the old default (don't clobber user overrides).

### Surface the rejection counters

The face_processor already logs counts ("dropped N blurry / M small") at `face_processor.rs:655–665`. Expose them via the existing `people_clustering_diagnostics` IPC so the user can see why their face count is lower after the upgrade.

### Expected effect
- 25–35% fewer faces enter clustering → cleaner inputs.
- Far fewer cross-person bridge edges (root cause of the user's complaint).
- More singletons land in the Unknown pool → Phase B's K-similar review converts them to supervision.

### Critical files (Phase A)
- `src/services/face_processor.rs` — gate constants + yaw check
- `src/ml/face_detector.rs` — `estimate_yaw_from_landmarks`
- `src/ml/clustering.rs` — threshold constant
- `src/config/mod.rs` — default value bump
- `src/db/migrations.rs` — conditional default update (v19→v20 alongside Phase B's table)
- `src-tauri/src/commands/people.rs` — surface rejection counts in `people_clustering_diagnostics`

---

## Phase B — `face_negatives` + K-similar review

### Schema (migration v19 → v20)

```sql
CREATE TABLE face_negatives (
  face_id        INTEGER NOT NULL,
  not_cluster_id INTEGER NOT NULL,
  created_at     INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  PRIMARY KEY (face_id, not_cluster_id),
  FOREIGN KEY (face_id)        REFERENCES faces(id)         ON DELETE CASCADE,
  FOREIGN KEY (not_cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE
);
CREATE INDEX idx_face_negatives_cluster ON face_negatives(not_cluster_id);
```

This is the missing supervision channel — every user rejection writes a row, and clustering refuses any merge that violates a row. Reviews **compound** instead of evaporating.

### Backend (`src/db/face_repo/`)

**`read.rs`** — new functions:
- `get_faces_by_cluster(cluster_id, status: FaceStatus, cursor, limit) -> Vec<FaceDetail>` where `FaceStatus = Confirmed | Unconfirmed | All`
- `count_unconfirmed_in_cluster(cluster_id) -> i64`
- `count_unconfirmed_global() -> { total, clusters }`
- `get_negatives_for_face(face_id) -> Vec<i64>`
- `k_similar_to_cluster(cluster_id, k) -> Vec<(face_id, score)>` — uses the centroid of confirmed gallery embeddings; queries HNSW for K nearest among `cluster_id IS NULL OR user_confirmed = 0`; filters out faces with a `face_negatives` row against this cluster.

**`write.rs`** — five mutations, each one transaction (reusing `refresh_cluster_stats_tx` + `refresh_gallery_tx`):
- `confirm_face(face_id)` — `UPDATE faces SET user_confirmed=1`. Adds to gallery if not already.
- `reject_face_to_unknown(face_id)` — `UPDATE faces SET cluster_id=NULL, user_confirmed=0`. **Writes** `INSERT INTO face_negatives(face_id, not_cluster_id) VALUES (?, prev_cluster_id)`.
- `hide_face(face_id)` — `UPDATE faces SET cluster_id=NULL, user_confirmed=-1`. Excluded from future clustering (existing read filter `user_confirmed >= 0`).
- `reassign_face(face_id, new_cluster_id)` — moves face, `user_confirmed=1`, writes negative against old cluster.
- `confirm_face_to_cluster(face_id, cluster_id)` — used by K-similar flow when face was unassigned.

### Clustering respects negatives

`src/ml/clustering.rs` already rejects same-photo merges in both HNSW path (lines 220–227) and complete-link path (lines 124–133). Add a parallel check: at the start of each clustering run, `SELECT face_id, not_cluster_id FROM face_negatives` into a `HashSet<(i64, i64)>`. Reject any merge that would place a face into a cluster it has a negative against.

For HNSW: when a face's candidate cluster has any member with `(face_id, cluster_id)` in the negative set, skip that edge. For complete-link: same check before union.

### IPCs (`src-tauri/src/commands/people.rs`)

| Command | Args | Returns |
|---|---|---|
| `people_face_list` | `{ person_id, status, cursor, limit }` | `Page<FaceDetailDto>` |
| `people_face_confirm` | `{ face_id }` | `()` |
| `people_face_reject` | `{ face_id }` | `()` (records negative) |
| `people_face_hide` | `{ face_id }` | `()` (false positive) |
| `people_face_reassign` | `{ face_id, target_cluster_id }` | `()` (records negative against old) |
| `people_face_suggest_clusters` | `{ face_id, top_k }` | `Vec<ClusterSuggestionDto>` |
| `people_k_similar_to_cluster` | `{ cluster_id, k }` | `Vec<FaceDetailDto>` |
| `people_review_face_count` | `{}` | `{ unconfirmed_total, clusters_with_unconfirmed }` |
| `people_pending_face_count` | `{}` | `{ pending_photos }` (also used by Phase A resume banner) |

DTOs in `src-tauri/src/dto.rs`: `FaceDetailDto`, `ClusterSuggestionDto`, `ReviewFaceCountDto`, `PendingFaceCountDto`. Register in `src-tauri/src/lib.rs`. Document in `docs/COMMAND_SURFACE.md`.

### Frontend

**`src-ui/src/lib/components/FaceCell.svelte` (NEW ~120 LOC)** — 80×80 face crop. Hover reveals three buttons (digiKam pattern): ✓ Confirm (only when unconfirmed) / ↻ Move / ✕ Reject. Click crop → PhotoDetail. Multi-select via existing `handleCellClick` in `selection.svelte.ts`. Subtle ochre ring on unconfirmed faces.

**`src-ui/src/lib/components/ReassignFaceDialog.svelte` (NEW ~180 LOC)** — based on `MergePersonDialog.svelte`. Section 1: top 5 suggested clusters (face thumbnail + name + score) from `people_face_suggest_clusters`. Section 2: autocomplete over all people. One-click move.

**`src-ui/src/lib/components/KSimilarDialog.svelte` (NEW ~150 LOC)** — the Google-Photos flow. Triggered from PersonDetail's "Find more like this" button. Loads `people_k_similar_to_cluster(person_id, 20)`. 4×5 grid of `FaceCell` (always-visible buttons). ✓ → `people_face_confirm_to_cluster`. ✕ → `people_face_reject` (writes negative). On finish, kicks off a targeted propagation pass over the cluster's neighborhood.

**`src-ui/src/routes/PersonDetail.svelte`** — restructure:
- Main grid: `<FaceCell>` over `people.faceList(personId, "confirmed", ...)`.
- "Verify these" strip above (max 12 unconfirmed faces, always-visible action buttons, horizontally scrollable).
- Header: "Find more like this" button (opens `KSimilarDialog`).
- Remove the old × badge from photo cells (revert prior bad UX). The `removePhotoFromCluster` API stays as an internal helper for bulk reassignment.

**`src-ui/src/routes/FaceReview.svelte` (NEW route `#/review-faces`)** — full-screen global review:
```
Person: Aanya     12 of 47 faces
[ large face crop ]
[✓ Yes — same person]  [✕ Not same]
Skip   Undo   Esc to close
Confirmed exemplars: [horizontal strip]
```
Keyboard: `Y` confirm, `N` reject, `S` skip, `Z` undo (one-step stack), `Esc`/`Q` close. Auto-advances to next cluster when current cluster's unconfirmed pool empties.

**`src-ui/src/routes/People.svelte`** — two banners (only when relevant):
1. **Pending detection**: "1,234 photos still need face detection — Resume" → calls existing `people.startProcessing`.
2. **Pending verification**: "47 faces need verification across 3 people — Verify now" → opens FaceReview.

Plus a **streaming "new faces" toast**: subscribe to `face:progress` events during a job. When `new_faces_in_chunk > 0`, show "+12 new faces" toast for 3s. Reuses existing toast component.

**`src-ui/src/lib/components/JobsIndicator.svelte`** — for `j.kind === "faces"`, rename "Cancel" → "Pause" with tooltip: "Stops safely. Resume later — even after moving the drive."

**`src-ui/src/lib/components/SelectionBar.svelte`** — bulk actions in cluster context: "Confirm all" / "Move to…" / "Reject all". Parallel `Promise.all` over face IDs.

### Critical files (Phase B)
- `src/db/schema.rs`, `src/db/migrations.rs` — `face_negatives` (v19→v20)
- `src/db/face_repo/{read,write,gallery}.rs` — new functions
- `src/ml/clustering.rs` — negatives-aware merge in both HNSW and complete-link paths
- `src-tauri/src/commands/people.rs` — 9 new commands
- `src-tauri/src/dto.rs` — 4 new DTOs
- `src-tauri/src/lib.rs`, `docs/COMMAND_SURFACE.md`
- `src-ui/src/lib/api/people.ts` — typed clients
- 4 new Svelte files: `FaceCell.svelte`, `ReassignFaceDialog.svelte`, `KSimilarDialog.svelte`, `FaceReview.svelte`
- Edits: `People.svelte`, `PersonDetail.svelte`, `JobsIndicator.svelte`, `SelectionBar.svelte`

### Reuse
- `populate_face_thumbnails` (`src/db/face_repo/gallery.rs:239`) — face crop file format `.photovault/faces/<id>.jpg`
- `crate::ml::retrieve_candidates` (`src/ml/retrieval.rs:82`) — for suggest-clusters
- `refresh_cluster_stats_tx` (`gallery.rs:164`), `refresh_gallery_tx` (`gallery.rs:28`) — call after every mutation
- `selection.svelte.ts`, `SelectionBar.svelte`, `MergePersonDialog.svelte` — reuse patterns

---

## Phase C — Batched local embedding (3–5× speed-up)

### Why

Today, `src/services/face_processor.rs:567` calls `embedder.embed(&aligned_face)` once per face. A photo with 8 faces does 8 separate ONNX sessions. `glintr100.onnx` already has a dynamic batch dimension; we just don't use it.

### Refactor

**`src/ml/face_embedder.rs`** — add `embed_batch`:

```rust
pub fn embed_batch(&mut self, faces: &[RgbImage]) -> Vec<Option<FaceEmbedding>> {
    // Build [N, 3, 112, 112] tensor, single ONNX run, L2-normalize each output row.
    // Returns one Option per input (None if input was malformed; rare).
}
```

Existing `embed(face)` becomes `embed_batch(&[face]).into_iter().next().flatten()` for back-compat.

**`src/services/face_processor.rs:567`** — collect all aligned crops + landmarks for the chunk (5 photos × ~3 faces avg = ~15 crops), single `embed_batch` call, write embeddings back in the same DB transaction. Locking pattern unchanged.

### Expected effect
- 3–5× faster on face-dense photos (group shots).
- ~2× faster on typical libraries.
- No accuracy change — same model, same crops, same L2 normalization.

### Critical files (Phase C)
- `src/ml/face_embedder.rs` — `embed_batch`
- `src/services/face_processor.rs` — call site

---

## Phase D — Kaggle/Colab GPU bridge (OPT-IN, additive)

### Architecture

Default behavior unchanged. Local CPU + GPU-EP path stays primary. If the user enables "Cloud face acceleration" in Settings and provides a bridge URL, embedding goes remote. **Detection stays local** (fast on CPU; sending full photos would cost 10× bandwidth).

```
                              ┌────────────────────────┐
                              │  user-owned Colab T4   │
                              │  FastAPI + ArcFace ONNX│
                              │     (GPU runtime)      │
                              └────────▲───────────────┘
                                       │ HTTPS (ngrok / cloudflared)
┌──────────────────────────────────────┴───────────────┐
│  Smriti (Linux/Windows/macOS desktop)                │
│                                                       │
│  Detector (SCRFD, LOCAL)                              │
│       │                                               │
│       ▼                                               │
│  aligned 112×112 crops (~5 KB each, JPEG quality 85)  │
│       │                                               │
│       ▼                                               │
│  FaceEmbedder::Remote → POST /embed → 512-d back     │
│         │  on 5xx / timeout / >3 consecutive failures │
│         └──fallback──▶ FaceEmbedder::Local            │
└───────────────────────────────────────────────────────┘
```

### Backend

**`src/ml/face_embedder.rs`** — convert struct to enum dispatch:

```rust
pub enum FaceEmbedder {
    Local(LocalEmbedder),      // current code, renamed
    Remote(RemoteEmbedder),    // new (Phase D)
}

pub struct EmbedderConfig {
    pub model_path:      PathBuf,        // required (fallback path)
    pub gpu_bridge_url:  Option<String>, // optional (preferred when set + reachable)
    pub intra_threads:   usize,
}

impl FaceEmbedder {
    pub fn from_config(rt: &OnnxRuntime, cfg: &EmbedderConfig) -> Result<Self> {
        if let Some(url) = &cfg.gpu_bridge_url {
            // GET <url>/health with 2s timeout; if 200 OK and {gpu: "GPU"}, use Remote
            // Else fall through to Local with a warning
        }
        Ok(FaceEmbedder::Local(LocalEmbedder::new(...)?))
    }
    pub fn embed_batch(&mut self, faces: &[RgbImage]) -> Vec<Option<FaceEmbedding>>;
}
```

**`src/ml/remote_embedder.rs` (NEW ~150 LOC)**:
- `reqwest::blocking::Client` with 30s timeout (existing dep — already used for geocoding)
- `embed_batch`: JPEG-encode each crop (quality 85), multipart POST to `<url>/embed`, parse JSON `{ "embeddings": [[f32; 512], ...] }`
- Auto-fallback: on any error in a batch, return `Vec<None>` for that batch and signal the caller; `face_processor` keeps a `LocalEmbedder` warm on each thread and re-runs the failed batch locally
- Heartbeat: every 30s POST to `/health`; if 3 consecutive failures, the embedder marks remote dead for the rest of the job and switches all subsequent batches to local

**`src/config/mod.rs`** — two new fields:
```rust
pub face_gpu_bridge_url:     Option<String>,  // default None
pub face_gpu_bridge_enabled: bool,            // default false (explicit opt-in)
```

**`src-tauri/src/commands/settings.rs`** — `SettingsUpdateArgs` gains both fields; `settings_update` validates URL format if provided.

**`src-tauri/src/commands/system.rs`** — new `system_test_gpu_bridge(url: String) -> { ok: bool, latency_ms: u32, gpu_name: String }` for the "Test connection" button.

### Frontend

**`src-ui/src/routes/Settings.svelte`** — new collapsible section "Cloud face acceleration (advanced)":
- Toggle: "Use a remote GPU for face embedding"
- Text input: "Bridge URL" (e.g., `https://abc.ngrok.io`)
- "Test connection" button → calls `system_test_gpu_bridge`; shows "✓ T4 GPU @ 45ms" or "✕ unreachable"
- Warning box: "Sends 112×112 face crops (not photos) to a notebook URL you provide. Only enable if you control the endpoint. Falls back to local CPU on failure."
- Link to `docs/face-gpu-bridge.md`: "How to set up a free Kaggle / Colab notebook →"

### Notebook (`notebooks/face_bridge.ipynb` NEW)

Single-file, runnable on Kaggle or Colab. Cells:

1. **Install**: `!pip install fastapi uvicorn nest-asyncio python-multipart onnxruntime-gpu==1.18.0 pyngrok`
2. **Download model**: `!wget -q <stable mirror>/glintr100.onnx -O /content/model.onnx` (same mirror as `setup_assets.sh`)
3. **GPU verify**:
   ```python
   import onnxruntime as ort
   sess = ort.InferenceSession("/content/model.onnx", providers=["CUDAExecutionProvider"])
   print("GPU:", sess.get_providers())
   ```
4. **Server (FastAPI)** — defines `/embed` (multipart POST → JSON embeddings) and `/health` (GPU info).
5. **Tunnel — ngrok (default)**:
   ```python
   from pyngrok import ngrok
   ngrok.set_auth_token("YOUR_TOKEN_HERE")  # free signup at ngrok.com
   public_url = ngrok.connect(8000, "http")
   print(f"BRIDGE URL → {public_url}")
   ```
6. **Tunnel — cloudflared (alternative, commented out)**:
   ```python
   # !wget -q https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 -O cloudflared
   # !chmod +x cloudflared
   # import subprocess; subprocess.Popen(["./cloudflared", "tunnel", "--url", "http://localhost:8000"])
   # The URL prints to cloudflared's stdout — tail the log file.
   ```
7. **Run**: `nest_asyncio.apply(); uvicorn.run(app, host="0.0.0.0", port=8000)`

User flow: open notebook → ngrok token (one-time free signup) → run all → copy URL → paste in Smriti → Test → enable. ~2.5 min for 300k faces on a free T4.

### Documentation

**`docs/face-gpu-bridge.md` (NEW)** — covers:
- What it does, what it sends (112×112 face crops only)
- Privacy notes (no photos, no metadata, no telemetry; user controls the endpoint)
- Three setup paths: **Kaggle** (free, 30 hrs/week, T4×2), **Colab** (free with limits / Pro $10/mo for A100), **local LAN GPU server** (no internet — same notebook, no tunnel)
- Step-by-step walkthroughs for Kaggle and Colab
- ngrok auth token setup (60-second flow)
- cloudflared alternative (no account needed)
- Troubleshooting: session reset, token rotation, automatic fallback to CPU
- Cost expectations: free for Kaggle/Colab basic; $10/mo Colab Pro for unmetered

**`README.md`** — add "Optional: Cloud GPU acceleration" subsection under Features, with one paragraph + link to the doc.

**`CLAUDE.md`** — single line under "Key Dependencies" or a new "Optional integrations" mini-section: "Face embedding can optionally be offloaded to a user-owned Kaggle/Colab notebook (see `docs/face-gpu-bridge.md`). The default flow uses local ONNX Runtime."

### Critical files (Phase D)
- `src/ml/face_embedder.rs` — enum + `from_config` + `embed_batch` dispatch
- `src/ml/remote_embedder.rs` — **new**
- `src/config/mod.rs` — two new fields
- `src-tauri/src/commands/settings.rs` — extend `SettingsUpdateArgs`
- `src-tauri/src/commands/system.rs` — `system_test_gpu_bridge`
- `src-tauri/src/lib.rs` — register
- `src-ui/src/routes/Settings.svelte` — new section
- `notebooks/face_bridge.ipynb` — **new**
- `docs/face-gpu-bridge.md` — **new**
- `README.md`, `CLAUDE.md` — small additions

### Reuse
- `OnnxRuntime` (`src/ml/runtime.rs`) — local path unchanged
- `reqwest` — already in workspace deps (used for geocoding)
- Existing `settings_update` IPC pattern

---

## Phase E — Disk hygiene rule + cleanup script

### `CLAUDE.md` addition

Insert a new `## Disk hygiene` section in `CLAUDE.md`, immediately after the "Build & Run" section (line 107) and before "Push gate (mandatory)":

````markdown
## Disk hygiene

The `target/` tree grows by 1–3 GB per build and easily hits 14 GB on a busy
session. After **every ~3 builds**, and **always after bundling a release**,
run the cleanup script:

```bash
./scripts/clean_builds.sh        # Linux / WSL
scripts\clean_builds.ps1         # Windows PowerShell
```

It keeps the most recent release artifacts and removes stale debug deps,
incremental caches, and old bundles. Don't `cargo clean` unless disk is
critically low — full rebuild costs ~5 min vs the script's ~2 seconds.

**Rule for Claude:** track build count mentally per session. After running
`cargo build` / `cargo tauri build` ~3 times — or any time after bundling
a release — run the cleanup script. Confirm with the user only if free disk
is critical (<2 GB); otherwise just run it.
````

### `scripts/clean_builds.sh` (NEW)
```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

before=$(du -sh target/ 2>/dev/null | cut -f1 || echo "0")
echo "Before: $before"

# Stale incremental caches
rm -rf target/debug/incremental target/release/incremental

# Old bundles: keep the most recent of each format
if [ -d target/release/bundle ]; then
  for fmt in deb rpm appimage msi dmg; do
    find target/release/bundle -type f -name "*.${fmt}" \
      -printf '%T@ %p\n' 2>/dev/null \
      | sort -nr | tail -n +2 | cut -d' ' -f2- | xargs -r rm -f
  done
fi

# Old build dirs (rust-incremental output for hashes no longer needed)
find target/debug/build -maxdepth 1 -type d -mtime +3 -exec rm -rf {} + 2>/dev/null || true

after=$(du -sh target/ 2>/dev/null | cut -f1 || echo "0")
echo "After:  $after"
```

### `scripts/clean_builds.ps1` (NEW)
PowerShell equivalent: same operations using `Remove-Item` and `Get-ChildItem` with `-Recurse -Force -ErrorAction SilentlyContinue`.

### Critical files (Phase E)
- `CLAUDE.md` — new "Disk hygiene" section
- `scripts/clean_builds.sh` — **new**
- `scripts/clean_builds.ps1` — **new**

---

## Phase F — AdaFace embedder swap

### Why

AdaFace IR-101 (CVPR 2022, updated 2024 on WebFace12M) is the current SOTA on hard pose / occlusion / age distributions. ~3–5% accuracy gain on harder splits vs glintr100. Same input shape (112×112 RGB), same output shape (512-d L2-normalized) — drop-in replacement.

### Changes

1. **Download model** — extend `scripts/setup_assets.sh` and `scripts/setup_assets.ps1` to also fetch `adaface_ir101_webface12m.onnx` from a stable mirror (same pattern as glintr100). Keep glintr100 as the legacy fallback.

2. **Config switch** — `src/config/mod.rs` gains `pub face_embedder_model: String` (default `"adaface_ir101_webface12m.onnx"`). AdaFace centroids tend to be tighter — bump `face_clustering_threshold` to **0.30** for AdaFace (vs 0.28 for glintr100, which Phase A picks).

3. **Loader** — `src/services/face_processor.rs:179` reads the model name from config instead of hardcoding `"glintr100.onnx"`.

4. **Re-run banner** — first launch after upgrade, `People.svelte` shows: "We've upgraded the face recognition model. Re-run face detection to apply." Button chains `people_reset_clusters` + `people.startProcessing`.

### Critical files (Phase F)
- `scripts/setup_assets.{sh,ps1}` — extra download
- `src/config/mod.rs` — model selector + threshold default
- `src/services/face_processor.rs` — read model from config
- `src-ui/src/routes/People.svelte` — upgrade banner

---

## Build gates (mandatory before every push)

- `cargo fmt --all --check`
- `cargo clippy --all-targets -p smriti -p smriti-tauri -- -D warnings`
- `cargo test --no-run`
- `(cd src-ui && npm run check && npm run build)`

---

## Verification

### Phase A — accuracy gates
1. Open a real library (~20k photos). Run face detection.
2. Compare `people_clustering_diagnostics` before/after — faces entering clustering should drop 25–35%; the rejection breakdown shows blurry / small / high-yaw counts.
3. Visually spot-check: a person known to have profile shots should have its side-profile faces dropped (yaw gate working).
4. Top 20 clusters: false-merge clusters (mixed identity) should be visibly fewer.

### Phase B — review pipeline + negatives
1. Open any cluster in PersonDetail. Verify face crops fill the grid; no × badge on photo cells.
2. Hover a face → ✓ / ↻ / ✕ appear. Click ✕ on a misclassified face. Face vanishes; `SELECT * FROM face_negatives WHERE face_id = X` returns a row.
3. Run face detection again. Open PersonDetail. The previously-rejected face does **not** reappear in this cluster.
4. From PersonDetail, click "Find more like this" → 4×5 grid appears via `people_k_similar_to_cluster`. Confirm 5 (✓), reject 5 (✕). Confirmed faces appear in main grid next refresh; negatives written for the rejected ones.
5. Verify FaceReview (`#/review-faces`): Y/N/S/Z keyboard works; auto-advances clusters.
6. People page banner counts match `people_review_face_count`.
7. Streaming toast: kick off a face job. "+N new faces" toast appears every few seconds.

### Phase C — batched embedding
1. Run face detection on the same library twice — once on commit before this phase, once after. Compare wall-clock time.
2. Expect 2–3× faster on average libraries, 3–5× on dense (group photo) libraries.
3. Cluster output identical (same model, same crops, same L2). Diff `face_clusters` table rows before/after.

### Phase D — GPU bridge
1. Run `notebooks/face_bridge.ipynb` on Colab. ngrok prints URL.
2. Settings → Cloud face acceleration → paste URL → Test. See "✓ T4 GPU @ XXms".
3. Enable. Run face detection on a 10k-photo library. Notebook logs show batch POSTs. Embedding completes in ~30s (vs ~5min CPU).
4. **Fallback test**: stop the notebook mid-run. Smriti logs "remote bridge unhealthy; falling back to local"; job completes without error.
5. Repeat on Kaggle (cloudflared variant — uncomment that cell).
6. **Privacy audit**: instrument notebook to log POST body sizes. Confirm each upload = N × ~5 KB JPEGs (face crops), never any larger image.

### Phase E — cleanup script
1. Run `cargo build` 3×; `du -sh target/`.
2. `scripts/clean_builds.sh`; `du -sh target/` should drop 30–50% with no broken next build.
3. `cargo build` once more — incremental cache for current toolchain is preserved (build is fast, not full rebuild).

### Phase F — AdaFace
1. Run `scripts/setup_assets.sh`. New ONNX file in `models/`.
2. Launch Smriti → "model upgraded" banner appears.
3. Click rerun. Detection completes. Spot-check: visually-similar people (twins, family) distinguish better; cluster count typically increases slightly (tighter embeddings).

---

## Out of scope (explicitly)

- **DBSCAN/HDBSCAN replacement of threshold-merge clustering**: HNSW + tightened threshold + negatives is enough for v1. Re-evaluate after a real-world run.
- **Detection on the GPU bridge**: bandwidth cost too high; detection is the fast stage anyway.
- **Hosted "Smriti Cloud" service**: conflicts with offline-first ethos.
- **Drag-and-drop face reassignment**: dialog covers v1.
- **Reason picker on rejection**: low value offline.
- **Automated build-cleanup hook**: user chose documented rule + script.
- **Slideshow** — deferred from earlier.

## Sources

- [SCRFD / ArcFace (InsightFace)](https://github.com/deepinsight/insightface) — base models
- [AdaFace (CVPR 2022)](https://github.com/mk-minchul/AdaFace) — Phase F embedder
- [digiKam People view](https://docs.digikam.org/en/left_sidebar/people_view.html) — three-state UX
- [Apple Photos Confirm Additional Photos](https://discussions.apple.com/thread/254474800) — verify-additional pattern
- [Google Photos face grouping](https://support.google.com/photos/answer/6128838) — K-similar prompt
- [Immich facial recognition](https://docs.immich.app/features/facial-recognition/) — per-face embedding precedent
- [immich-face-fix Y/N/S/Z](https://github.com/pabera/immich-face-fix) — keyboard review precedent
- [pyngrok](https://pyngrok.readthedocs.io/) — default tunnel
- [cloudflared quick-start](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/get-started/) — alternative tunnel
- [ONNX Runtime dynamic batch dim](https://onnxruntime.ai/docs/performance/) — batched inference
- [Photoprism face management discussion](https://github.com/photoprism/photoprism/issues/2401) — competitor reference
