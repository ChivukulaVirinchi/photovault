# Face Recognition — Future Improvement Playbook

Status today: the face pipeline is functional and accurate enough for most
personal libraries. Detection → 5-point affine alignment → quality filter →
GLinTR-100 embedding → gallery k-NN retrieval with confidence bands → Stage B
agglomerative fallback → post-pass cluster merge → orphan rescue → interactive
Review UI with sticky user-confirmed gallery members → cannot-merge constraints
→ co-occurrence + temporal context re-ranking → GPU execution providers with
CPU fallback.

Three increments remain that would push accuracy and speed further. They are
*independent* — each can be implemented on its own and each delivers
measurable improvement without needing the others. They're listed in the order
recommended for impact-per-effort.

Throughout this document, **line numbers assume the current state of `main`
as of the last merge on this branch.** Re-grep for the exact anchors before
editing — refactors between now and future work may have shifted them.

---

## B2 — Batched ONNX Inference

### What problem this solves

Today `face_embedder.rs::embed` runs one ONNX `session.run()` call **per
face**. For a photo with 4 faces, we do 4 separate GPU/CPU dispatches, 4
tensor allocations, 4 result extractions. GPU-based inference is especially
hurt by this: the kernel launch overhead dominates when the workload per call
is tiny.

Batching collects N aligned-face images into a single `[N, 3, 112, 112]`
input tensor and runs `session.run()` once, returning N embeddings in one
call.

**Expected speedup:**

| Device | Per-face today | Per-face batched (N=8) | Overall pipeline speedup |
|--------|---------------|------------------------|--------------------------|
| CPU    | ~30-80 ms     | ~18-45 ms              | 1.5-2×                   |
| GPU (DirectML/CUDA) | ~3-10 ms | ~1-3 ms          | 3-5×                     |

The CPU gain is real but modest (cache-locality effect). The GPU gain is
dramatic because batching amortizes kernel-launch cost.

### Files to touch

1. `src/ml/face_embedder.rs` — add a new batch method.
2. `src/services/face_processor.rs` — change the per-face embed loop to
   accumulate a batch, then call `embed_batch`.

Nothing else needs to change. The output type is `Vec<Option<FaceEmbedding>>`
per batch; downstream code already handles `None` per-face failures.

### Step 1 — Add `embed_batch` on FaceEmbedder

Open `src/ml/face_embedder.rs`. Find the existing `embed` function (starts
around line 92). Add a new sibling method `embed_batch` that accepts a slice
of references to aligned RGB images and returns a Vec of optional embeddings
in the same order.

Signature:

```rust
pub fn embed_batch(&mut self, aligned_faces: &[&RgbImage]) -> Vec<Option<FaceEmbedding>>
```

The implementation follows the existing `embed` closely, differing only in
tensor shape and result extraction:

1. **Early exit if empty:** if `aligned_faces.is_empty()`, return `Vec::new()`.

2. **Validate each face is 112×112.** If any face has wrong dims, log a
   warning and mark its slot as `None` in the output. Track a `valid_indices:
   Vec<usize>` mapping from batch-tensor position back to the original input
   index — not every input slot necessarily ends up in the tensor.

3. **Allocate one flat input buffer** of size `3 * 112 * 112 * valid_count`
   as `Vec<f32>`. Stride layout is the same NCHW as the single-face path —
   for batch index `b`, face plane offset is `b * (3 * 112 * 112)`.

4. **Preprocess each valid face into its batch slot.** Reuse the existing
   normalization formula: `(pixel - 127.5) / 127.5` per channel. Loop over
   `b, c, y, x` filling `input[b*hw3 + c*hw + y*112 + x]`.

5. **Build the input tensor with shape `[valid_count, 3, 112, 112]`:**

   ```rust
   let input_tensor = TensorRef::<f32>::from_array_view((
       vec![valid_count as i64, 3, 112, 112],
       input_data.as_slice(),
   ))?;
   ```

6. **Run inference once:** `self.session.run(ort::inputs![input_tensor])?`.

7. **Extract output:** the output tensor shape will be `[valid_count, 512]`.
   Iterate `0..valid_count`, slice out each 512-d vector, L2-normalize (reuse
   the private `normalize` method — promote it to take `&[f32]` if needed),
   and wrap in `Some(FaceEmbedding::new(...))`.

8. **Reconstruct the result vec** with `None` in the originally-invalid
   slots and `Some(emb)` in the valid ones, using the `valid_indices` map.

### Step 2 — Batch in face_processor

Open `src/services/face_processor.rs`. Find the per-face embed loop. Currently
it looks like (simplified):

```rust
for face in &detected {
    // ...quality checks...
    let embedding = EMBEDDER.with(|e| {
        let mut borrow = e.borrow_mut();
        borrow.as_mut().and_then(|emb| emb.embed(aligned))
    });
    if let Some(embedding) = embedding {
        face_inserts.push(FaceInsert { ... });
    }
}
```

Change it to collect qualifying faces first, then call `embed_batch` once per
photo:

1. **Split the loop in two passes:**
   - Pass A: filter detections through quality checks, collect `Vec<(usize,
     &RgbImage)>` where usize is the original detection index and `&RgbImage`
     is the aligned crop.
   - Pass B: call `embed_batch` on the aligned images, iterate results back
     with the detection index to build FaceInserts.

2. **Call `embed_batch`:**

   ```rust
   let alignments: Vec<&RgbImage> = qualified.iter().map(|(_, img)| *img).collect();
   let embeddings = EMBEDDER.with(|e| {
       let mut borrow = e.borrow_mut();
       match borrow.as_mut() {
           Some(emb) => emb.embed_batch(&alignments),
           None => vec![None; alignments.len()],
       }
   });
   ```

3. **Zip results back into FaceInserts:**

   ```rust
   for ((det_idx, aligned), emb_opt) in qualified.iter().zip(embeddings.into_iter()) {
       if let Some(embedding) = emb_opt {
           let face = &detected[*det_idx];
           face_inserts.push(FaceInsert {
               bbox_normalized: face.bbox_normalized,
               confidence: face.confidence,
               embedding,
               aligned_face: (*aligned).clone(),
           });
       }
   }
   ```

**Caution:** Don't batch across photos. The thread-local EMBEDDER instance is
per-photo anyway (each worker processes photos sequentially). Cross-photo
batching complicates the worker model for minimal additional gain. Keep it
within one photo's N faces.

### Step 3 — Tune the batch cap

Huge batches can hit memory issues. Cap at `const MAX_EMBED_BATCH: usize = 16;`
in `face_processor.rs`. If a photo has >16 faces (rare but possible in crowd
shots), split into chunks of 16.

### Verification

1. `cargo test --lib` — 49 tests must still pass.
2. Add a unit test in `face_embedder.rs::tests`:
   - Create a fake 112×112 RGB image full of one color.
   - Call `embed` (single) and `embed_batch` with the same image × 4.
   - Assert all 4 batch results equal the single result within 1e-5.
3. Manual: scan a library with group photos (≥3 faces per photo).
   Log-count `faces_found` should match pre-B2. Wall-clock time should drop.
4. Bench: time a 100-photo library pre/post. Record numbers in the commit
   message.

### Rollback

Pure-addition change: new method + rewired loop. If B2 causes trouble, revert
the face_processor loop to the single-face path; keep `embed_batch` available
(unused but harmless). No DB schema changes, no model changes.

---

## B3 — Always-On Multi-Scale Detection

### What problem this solves

`FaceDetector::detect` runs SCRFD at a fixed 640×640 input. For a 4000×3000
photo, this means a 6.25× downscale — any face smaller than ~30 px in the
original effectively disappears. `detect_adaptive` has a tile-based fallback
(`src/ml/face_detector.rs:129-194`), but it **only triggers when zero faces
are found at the primary pass.** Group photos, distant subjects, and crowd
shots lose small faces silently.

B3 makes the second scale unconditional for large images: always run the
primary pass at 640×640 **and** tile-detect at 1600×1600 with 1200-px stride,
then NMS-pool the union. Small faces get a proper chance.

**Expected recall gain:**

| Photo type | Typical today | Typical after B3 |
|-----------|---------------|------------------|
| Portrait / selfie | ~100% | ~100% |
| Family group (3-5 people) | ~90% | ~95-98% |
| Wedding / event crowd | ~60-75% | ~85-92% |
| Tiny background faces (<30 px) | ~20-30% | ~70-80% |

**Cost:** detection time roughly doubles for large images (small images are
unchanged). On CPU that hurts. On GPU with B1 + B2 live it barely registers.

### Files to touch

Only `src/ml/face_detector.rs`. No changes to embedder, processor, or DB.

### Step 1 — Refactor `detect_adaptive` to always tile

Find the function (line ~129). Currently:

```rust
pub fn detect_adaptive(&mut self, image: &DynamicImage) -> Vec<DetectedFace> {
    let first_pass = self.detect(image);
    if !first_pass.is_empty() {
        return first_pass;
    }
    // ... tile fallback only runs on zero detections ...
}
```

Rewrite to **always** combine:

```rust
pub fn detect_adaptive(&mut self, image: &DynamicImage) -> Vec<DetectedFace> {
    let primary = self.detect(image);

    let (orig_w, orig_h) = image.dimensions();
    if orig_w.max(orig_h) <= 2048 {
        // Image small enough that downscale-to-640 doesn't lose small faces.
        return primary;
    }

    let tiled = self.tile_detect(image, orig_w, orig_h);

    if tiled.is_empty() {
        return primary;
    }
    if primary.is_empty() {
        return tiled;
    }

    // Merge + NMS across the combined set.
    let mut all = primary;
    all.extend(tiled);
    self.non_max_suppression(all)
}
```

### Step 2 — Extract the tile loop into a private method

Move the existing tile body (the `while y <= max_y { while x <= max_x { ...`
block currently inside `detect_adaptive`) into a new private method:

```rust
fn tile_detect(&mut self, image: &DynamicImage, orig_w: u32, orig_h: u32) -> Vec<DetectedFace> {
    let tile = 1600u32;
    let step = 1200u32;
    let mut all_faces = Vec::new();

    let max_x = orig_w.saturating_sub(1);
    let max_y = orig_h.saturating_sub(1);

    let mut y = 0u32;
    while y <= max_y {
        let mut x = 0u32;
        while x <= max_x {
            let crop_w = tile.min(orig_w.saturating_sub(x)).max(1);
            let crop_h = tile.min(orig_h.saturating_sub(y)).max(1);
            let crop = image.crop_imm(x, y, crop_w, crop_h);

            let mut local = self.detect(&crop);
            // Translate bbox/landmarks back to full-frame coords.
            for face in &mut local {
                let (bx, by, bw, bh) = face.bbox;
                let gx = bx + x as f32;
                let gy = by + y as f32;
                face.bbox = (gx, gy, bw, bh);
                face.bbox_normalized = (
                    gx / orig_w as f32,
                    gy / orig_h as f32,
                    bw / orig_w as f32,
                    bh / orig_h as f32,
                );
                for lm in &mut face.landmarks {
                    lm.0 += x as f32;
                    lm.1 += y as f32;
                }
            }
            all_faces.extend(local);

            if x + step >= orig_w {
                break;
            }
            x += step;
        }
        if y + step >= orig_h {
            break;
        }
        y += step;
    }

    all_faces
}
```

This is the same tile logic that's already there, just extracted.

### Step 3 — NMS handles the cross-scale dedupe

`non_max_suppression` already exists and uses IoU thresholding. It'll
correctly suppress duplicate detections of the same face from primary +
tile passes. No changes needed to NMS.

However: primary-scale detections tend to have fuzzier bounding boxes
(resized-down input means coarser localization), tile-scale ones are more
precise. Prefer tile-scale when both are present. Change NMS tie-break:
when two faces have IoU > 0.5, keep the one with higher `confidence` *and*
smaller `bbox.2 * bbox.3` in the original frame (tile-scale bboxes are
tighter). This is a tiny change in `non_max_suppression`.

### Step 4 — Add a toggle config option

Large-image owners might accept the slower scan in exchange for recall.
Small-library users might prefer speed. Add to `config/mod.rs`:

```rust
#[serde(default = "default_multiscale_detection")]
pub multiscale_detection: bool,
```

Default `true` (recall matters more for most users). Wire the value through
`FaceProcessor::process_photos` and into a new method
`detect_with_mode(&self, image, multiscale: bool)` that either calls
`detect_adaptive` (multiscale) or just `detect` (single-scale).

### Verification

1. `cargo test --lib` passes.
2. Hand-pick 10 photos known to have small/distant faces. Before B3: count
   detections. After B3: count detections. Ratio should be 1.2-1.5× on
   photos with >3 faces.
3. Time a 100-photo library; B3 should be ~1.6-2.0× slower without GPU,
   ~1.1-1.3× slower with GPU + B1.
4. Check no regression on small images (<2048 px): detection count identical
   to pre-B3 and time roughly the same.

### Rollback

Pure refactor with additive changes to detect_adaptive. Revert the single
function body if it misbehaves.

---

## B4 — Rotation-Invariant Detection

### What problem this solves

SCRFD is trained predominantly on upright faces. Anything >~30° rotated
(e.g., head tilted to the shoulder, phone held sideways, candid profile
shots) gets missed entirely. B4 detects at multiple orientations and pools.

**Expected recall gain:**

| Photo type | Typical today | After B4 |
|-----------|---------------|----------|
| Standard portrait | ~100% | ~100% |
| Tilted / profile (~30-60°) | ~30-50% | ~85-95% |
| Upside-down phone accident | ~0% | ~95% |
| Sideways landscape (camera rotated) | ~0% | ~95% |

**Cost:** detection time multiplied by rotation count (4 rotations = 4×
detect time). Worth it only after GPU + B2 are landed, unless the user
explicitly accepts slower scans.

### Files to touch

1. `src/ml/face_detector.rs` — add a new `detect_multi_orientation` method.
2. `src/services/face_processor.rs` — call `detect_multi_orientation` instead
   of `detect_adaptive` when the new config flag is on.
3. `src/config/mod.rs` — add the config flag.

### Step 1 — Add rotation helper functions

In `face_detector.rs`, add two private helpers:

```rust
/// Rotate the image 90/180/270 clockwise for multi-orientation detection.
fn rotate_image(image: &DynamicImage, degrees: u32) -> DynamicImage {
    match degrees {
        0 => image.clone(),
        90 => image.rotate90(),
        180 => image.rotate180(),
        270 => image.rotate270(),
        _ => image.clone(),
    }
}

/// Given a face detected in a rotated image, compute its bbox/landmarks
/// in the ORIGINAL (non-rotated) image coordinates.
fn unrotate_face(face: &mut DetectedFace, orig_w: u32, orig_h: u32, degrees: u32) {
    let (fx, fy, fw, fh) = face.bbox;
    let (ox, oy, ow_box, oh_box) = match degrees {
        0 => (fx, fy, fw, fh),
        90 => (fy, orig_w as f32 - (fx + fw), fh, fw),        // 90 CW: rotate back 90 CCW
        180 => (orig_w as f32 - (fx + fw), orig_h as f32 - (fy + fh), fw, fh),
        270 => (orig_h as f32 - (fy + fh), fx, fh, fw),       // 270 CW: rotate back 270 CCW
        _ => (fx, fy, fw, fh),
    };
    face.bbox = (ox, oy, ow_box, oh_box);
    face.bbox_normalized = (
        ox / orig_w as f32,
        oy / orig_h as f32,
        ow_box / orig_w as f32,
        oh_box / orig_h as f32,
    );

    // Landmarks: each (x, y) rotates with the same mapping.
    for lm in &mut face.landmarks {
        let (lx, ly) = *lm;
        let (nx, ny) = match degrees {
            0 => (lx, ly),
            90 => (ly, orig_w as f32 - lx),
            180 => (orig_w as f32 - lx, orig_h as f32 - ly),
            270 => (orig_h as f32 - ly, lx),
            _ => (lx, ly),
        };
        *lm = (nx, ny);
    }
}
```

**Double-check these transforms with a test:** set up a 100×50 image,
pretend a face is detected at (10, 20, 30, 10), rotate 90 CW, unrotate the
bbox, assert you get back (10, 20, 30, 10). This is fiddly sign-flipping
arithmetic; writing the test first will save hours of "why is the bbox on
the wrong shoulder" debugging.

### Step 2 — New `detect_multi_orientation` method

```rust
pub fn detect_multi_orientation(&mut self, image: &DynamicImage) -> Vec<DetectedFace> {
    let (orig_w, orig_h) = image.dimensions();

    let mut all: Vec<DetectedFace> = Vec::new();

    for degrees in [0u32, 90, 180, 270] {
        let rotated = Self::rotate_image(image, degrees);
        let mut faces = if degrees == 0 {
            // At 0°, we want the full pipeline (multi-scale if B3 is live).
            self.detect_adaptive(&rotated)
        } else {
            // At other angles, single-scale is usually sufficient — the
            // "is it a face?" question doesn't need tile-precision since
            // we'll realign later. Skip the extra cost.
            self.detect(&rotated)
        };

        if degrees != 0 {
            for face in &mut faces {
                Self::unrotate_face(face, orig_w, orig_h, degrees);
            }
        }

        all.extend(faces);
    }

    // NMS across all rotations — same face detected at two rotations will
    // have high IoU in original-frame coords, suppressed to one.
    self.non_max_suppression(all)
}
```

### Step 3 — Alignment for tilted faces

A face detected at 90° rotation has landmarks that describe the face as if
the image were rotated. After un-rotation, the landmarks are in original
coords but the head is still physically tilted in the original frame.
The existing alignment code (`src/ml/alignment.rs`) already handles this
correctly because the similarity transform is invariant to rotation — it
maps the 5 landmarks onto the canonical template regardless of their
orientation in the input. So no changes needed on the alignment side.

Verify this: for a tilted test face, the aligned 112×112 output should
show eyes horizontal. If it doesn't, the similarity transform is not fully
handling the rotation — in that case add an explicit `rotate` step before
alignment based on the eye-angle.

### Step 4 — Config + wiring

In `config/mod.rs`:

```rust
#[serde(default = "default_rotation_invariant")]
pub rotation_invariant_detection: bool,
```

Default to `false` because of the 4× cost. Users with tilted-photo libraries
opt in via Settings.

In `face_processor.rs`, swap the detection call based on config:

```rust
let detected = if rotation_invariant {
    DETECTOR.with(|d| d.borrow_mut().as_mut().map(|det| det.detect_multi_orientation(&image)).unwrap_or_default())
} else {
    DETECTOR.with(|d| d.borrow_mut().as_mut().map(|det| det.detect_adaptive(&image)).unwrap_or_default())
};
```

Thread the `rotation_invariant` boolean through `process_photos`'s signature,
sourcing from config at the handler layer (same pattern as the existing
`resolver_weights` parameter).

### Step 5 — UX consideration

In Settings, expose the toggle with a clear tooltip: "Detect faces at all
orientations (catches tilted / profile photos). ~4× slower face scanning.
Recommended for mixed-orientation libraries."

### Verification

1. Unit test the `unrotate_face` math on synthetic bboxes.
2. `cargo test --lib` passes.
3. Curate 10 photos with intentionally tilted faces (phone-rotated
   selfies, someone lying down, sideways landscape shots). Without B4:
   detection rate ~20-40%. With B4: ~90%+.
4. Time cost: 4× on detection, but detection is only ~15-25% of total
   face-processing time (embedding dominates). Expected overall slowdown:
   1.4-1.7× without GPU.

### Rollback

Default-off config. If the math is wrong, faces will land in bizarre
positions in People — noticed immediately. Toggle off in Settings; no DB
changes.

---

## Ordering & combination

If implementing all three, the recommended order is:

1. **B2 first.** Biggest speedup, smallest risk, no behavioral changes.
2. **B3 second.** Uses the speed budget that B2 bought back for detection
   recall.
3. **B4 last, opt-in.** Highest cost, narrowest user benefit (only
   helps tilted-face libraries).

Each is an independent commit. No shared files beyond `face_detector.rs`,
`face_processor.rs`, and `config/mod.rs` — and even those changes don't
conflict.

## What's intentionally out of scope

- **Model swaps** (buffalo_l R50, AdaFace, TransFace). No clear accuracy
  gain on clean faces. Speed gain (R50) is inferior to B1+B2 GPU batching.
  Revisit only if the review-UI feedback loop plateaus below 95% perceived
  accuracy and a specific user complaint points at model limits.
- **Age-chain recognition.** Needs a persistent temporal_face_links table
  and a graph-walk in the resolver. Multi-week effort. Defer until a user
  reports "can't recognize me across >10 years" with evidence the current
  temporal signal isn't catching it.
- **Cloud-offload inference.** Privacy cost is existential to the product.
  Not on the roadmap.
