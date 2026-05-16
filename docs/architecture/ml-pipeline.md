# ML Pipeline

Smriti's face-recognition pipeline runs entirely on-device by default,
with an opt-in remote-GPU bridge for the embedding step. All models
ship as ONNX and are loaded via the ONNX Runtime crate.

## Pipeline stages

```
photo  ──►  detection  ──►  alignment  ──►  embedding  ──►  clustering  ──►  identity
           (SCRFD)          (112×112)        (AdaFace /         (gallery +     (per-person
                                              GLinTR-100)        agglom.)       clusters)
```

### 1. Detection

**SCRFD** locates faces in each photo and produces:

- Bounding box,
- 5-point landmark set (eyes, nose tip, mouth corners),
- Detection score.

Only faces above the **face-confidence** threshold (configurable in
Settings) are retained.

### 2. Alignment

Each detected face is rotated and scaled to a canonical 112×112 crop
using the landmarks. The crop is what the embedder sees — the
embedder doesn't see the original photo.

### 3. Embedding

A face-recognition model produces a 512-dimensional unit vector that
captures identity:

- **Default**: **AdaFace IR-101** trained on WebFace12M.
- **Alternative**: **GLinTR-100** — selectable in Settings.

Embeddings from different models are not interchangeable. Smriti
refuses to mix them in the same library.

Embedding is the compute-heavy step. The optional [GPU bridge](../face-gpu-bridge.md)
offloads this to a free Kaggle/Colab notebook for a 10–70× speedup
on the one-time pass.

### 4. Clustering

A two-stage process:

1. **Gallery retrieval**: each new face is compared against the
   gallery of existing per-person clusters via k-nearest-neighbor
   match with confidence bands. High-confidence matches auto-assign;
   ambiguous matches are queued for review; low-confidence faces
   drop to stage 2.
2. **Agglomerative complete-link** clusters the remaining unresolved
   faces pairwise. Bounded at 2,000 faces per pass for performance;
   beyond that, faces route to the ambiguous queue or wait for the
   next rescue pass. The cap will lift when HNSW-based approximate
   clustering replaces the current complete-link implementation.

After clustering:

- A **unify pass** merges clusters that look like the same person
  split by lighting variance.
- A **rescue pass** matches orphan clusters against the existing
  gallery with looser thresholds.

### 5. Identity

Each cluster becomes a "person" record with:

- A hero face (highest-scoring detection),
- An aggregated embedding centroid,
- A confidence score per assigned face.

The user names the cluster from the People view. Naming is purely a
display label — the embedding is what identifies the cluster.

## Tuning

- **Face confidence** (Settings) — minimum detection score to retain
  a face. Lower catches more faces, including more false positives.
- **Clustering threshold** (Settings) — controls how aggressively
  faces are grouped. Lower → tighter clusters (more, smaller groups);
  higher → looser (fewer, larger).

Defaults work for most libraries. Adjust only if you see consistent
over- or under-clustering after running a full pass.

## Privacy

- Models run locally by default. Nothing leaves your machine unless
  you explicitly enable the [GPU bridge](../face-gpu-bridge.md).
- When the bridge is enabled, only the 112×112 aligned face crop is
  sent — never the original photo, EXIF, or filename.
- Embeddings and detection metadata are stored only in your
  library's local SQLite database.

## Code map

- `src/ml/` — ONNX runtime wrapper, model loaders, inference glue.
- `src/services/face_processor.rs` — orchestration (detection →
  embedding → clustering).
- `src/services/face_processor/` — submodules per stage.
- `src/db/face_repo/` — persistence of faces and clusters.

## See also

- [Face GPU Bridge](../face-gpu-bridge.md) — opt-in remote
  acceleration.
- [People and Faces user guide](../user-guide/people.md) — user-
  facing perspective.
- [Face Recognition Improvements](../FACE_RECOGNITION_IMPROVEMENTS.md)
  — historical notes on tuning decisions.
