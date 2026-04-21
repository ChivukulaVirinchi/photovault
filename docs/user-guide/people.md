# People and Faces

PhotoVault detects faces during scanning, groups them into clusters
(one cluster per person), and lets you name, merge, and review those
clusters in the **People** view.

## Running face processing

Face processing runs automatically during the initial scan and on any
rescan when new photos are added. You can also trigger it manually
from Settings.

Detection uses SCRFD (face localization) and GLinTR-100 (identity
embedding), both running locally via ONNX Runtime.

## Clustering pipeline

Clustering happens in two stages:

1. **Gallery retrieval.** Each new face is compared against the gallery
   of existing clusters using k-nearest-neighbor matching with
   confidence bands. High-confidence matches auto-assign. Ambiguous
   matches are queued for review. Low-confidence faces drop to stage 2.
2. **Complete-link agglomerative.** The remaining unresolved faces are
   clustered pairwise. If the number of unresolved faces exceeds
   **2,000** in a single pass, stage 2 is skipped for performance and
   those faces are routed to the ambiguous-review queue or matched
   against galleries by the rescue pass instead. The cap will be lifted
   in a future release once HNSW-based approximate clustering replaces
   complete-link.

After clustering, a post-pass unifies clusters that look like the same
person split by lighting variance, and a rescue pass matches any
remaining orphans against existing galleries with looser thresholds.

## What you can do in People

- **Rename** a cluster by clicking its title.
- **Open** a person to see all photos they appear in.
- **Merge** two clusters into one (e.g. when the system split the same
  person across several clusters).
- **Review ambiguous matches** — the review deck walks you through
  faces the system wasn't sure about, one at a time.

## Tuning

- **Face confidence** in Settings controls the minimum detection score
  for a face to be considered valid.
- **Clustering threshold** in Settings controls how tolerant the system
  is about grouping visually similar faces. Lower values mean tighter
  clusters (more but smaller groups); higher values mean looser
  clusters (fewer but larger groups). The default works for most
  libraries.

## Privacy

Face embeddings and detection crops never leave the device. See
`PRIVACY.md` for the full data-flow description.
