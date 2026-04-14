# Track B — Speed

Track B is the performance track for the face pipeline. Separate from Track A
(accuracy) because they're independent; Track B can land later without
disturbing Track A's correctness work.

User has explicitly dropped from this plan:
- Buffalo_l (R50) model swap. Stays on glintr100 (R100) because the model
  swap could regress accuracy with no guaranteed speed win on CPU.
- Reusing scan-time 2048px thumbnails for face processing. Risk of subtle
  bugs (stale thumbs, orientation skew, cache misses) outweighs the ~100-300 ms
  per-photo saving.

Remaining steps, ordered by impact:

## B1 — Execution provider plumbing (GPU acceleration)

Goal: use GPU if available, fall back to CPU silently. Target the broadest
GPU support with the fewest backends.

Backend selection by platform, in priority order:

| Platform | Priority 1 | Priority 2 | Fallback |
|----------|-----------|------------|----------|
| Windows  | DirectML  | —          | CPU      |
| Linux    | CUDA      | ROCm       | CPU      |
| macOS    | CoreML    | —          | CPU      |

DirectML is the pragmatic choice on Windows because it transparently covers
NVIDIA + AMD + Intel + Qualcomm GPUs via D3D12. It's Microsoft-shipped
(not FOSS) but free, pre-installed on Win10+, no driver hassle.

CUDA on Linux covers the dominant ML user segment. ROCm support can be added
later via a Cargo feature flag.

### Implementation outline

- Cargo.toml: add `ort` features `directml` (behind `cfg(windows)`) and
  `cuda` (behind `cfg(linux)`). These pull in the required backend crates
  but don't require the shared libraries to be present at runtime - ort
  tries each provider and falls back.
- `src/ml/runtime.rs`: change `load_model_with_threads` to build the
  session with a provider priority list. Log which provider bound the
  session (`tracing::info!("Face models running on: {}", provider_name)`).
- On init failure of a priority provider, catch the error, log, continue
  down the chain. Never surface init errors to the user; always land on CPU.

### Verification

- Windows native: confirm "Running on DirectML" in logs + ~5-10x speedup
  on a 100-photo batch.
- Linux no-GPU: confirm "Running on CPU" + no perceptible slowdown from
  the fallback chain.
- WSL: likely "Running on CPU" because DirectML and CUDA are not usable
  inside standard WSL. Users who want GPU should run the Windows build.

### Caveats

- WSL2's DirectX passthrough is buggy for many workloads; don't try to make
  it work. Document that WSL users run on CPU.
- ROCm requires specific AMD drivers + a full ROCm install. Gate behind
  an explicit cargo feature so it's opt-in.

---

## B2 — Batched ONNX inference

Goal: coalesce multiple faces per `session.run()` call. On CPU this is a
1.5-2x speedup from cache locality; on GPU it's 3-5x because GPU
utilization stops being starved.

Prerequisites: B1 must land first (batching is GPU-leveraged; marginal
on CPU alone).

### Implementation outline

- `face_embedder.rs`: add `embed_batch(&mut self, faces: &[RgbImage])
  -> Vec<Option<FaceEmbedding>>`.
- Pre-allocate the input tensor with shape `[N, 3, 112, 112]` where
  `N <= max_batch_size` (default 8 or 16).
- Single `session.run()` call per batch instead of one per face.
- In `face_processor` worker: collect faces for a photo into a vec,
  then call `embed_batch`.
- For photos with 1 face, small overhead; still worthwhile for bulk.

### Verification

- Bench on a library with many multi-face group photos; target 1.5-2x
  end-to-end speedup on CPU and 3-5x on GPU (once B1 is live).

---

## B3 — Multi-scale detection

Goal: catch small / partially-occluded faces that single-scale detection
misses. Currently SCRFD runs at 640x640; faces smaller than ~30px get
lost in the resize.

### Implementation outline

- Always detect at 640x640 (base). If the image is larger than 2048px on
  either side, also detect at a high-res crop strategy (tile the image
  into overlapping 1280x1280 tiles). NMS-pool detections from both passes.
- Currently there is a fallback tile detector but it only runs when
  zero faces are found - make it unconditional for large images.
- Optionally add a 320x320 downscale pass for very wide group photos
  (catches large faces more robustly after tile detection).

Cost: ~2x detector time, offset by GPU if B1 is live.

### Verification

- Group photos (>=4 faces): expect ~10-20% more faces detected.
- Distant / small faces: noticeable recall improvement.
- Shouldn't increase false positives meaningfully (NMS suppresses
  duplicates across scales).

---

## B4 — Rotation-invariant detection

Goal: catch tilted / profile faces. SCRFD is trained mostly on upright
faces; anything > 30 degrees rotation gets missed.

### Implementation outline

- Detect at original orientation + horizontal flip + 90 / 180 / 270
  rotations (where reasonable — phone photos with 4-way orientation
  ambiguity benefit, traditional landscape/portrait less so).
- NMS-pool all detections back into the original frame.
- Use detected landmarks to estimate head pose post-hoc; for faces
  with large tilt, rotate the 112x112 aligned crop so eyes are
  horizontal before embedding.

Cost: 3-4x detector time. Only worthwhile once B1 is live or for
libraries with many group / candid / action photos.

### Verification

- Hand-pick 20 photos known to contain profile / tilted faces.
  Confirm detection rate rises from ~60% to ~90%+.

---

## Out of scope (for now)

- Cloud GPU offload (Kaggle / Colab / user's own cloud). Breaks the
  offline/privacy model; re-add only if strong user demand appears.
- WebGPU EP. ORT's implementation is immature; revisit in 6-12 months.
- Native Rust ML frameworks (Burn / Candle / wgpu). Full replacement
  of ORT; multi-week effort with uncertain payoff.

---

## Suggested execution order

1. B1 (GPU plumbing) - biggest single speedup, enables B2-B4
2. B2 (batching) - complements GPU utilization
3. B3 (multi-scale) - accuracy + speed-tax offset by GPU
4. B4 (rotation) - biggest detection recall boost, highest cost

B1 alone is likely enough for "lightning fast" perception on Windows.
B2-B4 stack diminishing returns on top.
