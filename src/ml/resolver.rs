//! Context-aware resolver for face retrieval.
//!
//! Sits between `retrieve_candidates` and final assignment: re-scores the
//! ranked clusters using contextual signals and returns a stronger
//! recommendation, or flags that the decision should still go to the user.
//!
//! Signals fused:
//!
//! - Co-occurrence: if the photo already has other assigned clusters,
//!   candidates with higher co-occurrence to those clusters are boosted.
//!   "Same event, same cast" principle.
//!
//! - Temporal neighbors: if nearby-in-time photos have the candidate
//!   cluster already assigned, boost. "Same session → same people".
//!
//! The embedding score is always the dominant signal; context only breaks
//! ties and nudges ambiguous cases.

use super::retrieval::RetrievalHit;

/// Numeric weights controlling the relative influence of each signal.
/// Default values roughly chosen so context nudges but doesn't dominate
/// a strong embedding disagreement.
#[derive(Debug, Clone, Copy)]
pub struct ResolverWeights {
    /// Multiplier on the cosine similarity score. Keep at 1.0.
    pub embedding: f32,
    /// Added per other-cluster-in-photo * cooccurrence_count / saturation.
    pub cooccurrence: f32,
    /// Added when the candidate is also assigned in a temporally nearby photo.
    pub temporal: f32,
}

impl Default for ResolverWeights {
    fn default() -> Self {
        Self {
            embedding: 1.0,
            cooccurrence: 0.30,
            temporal: 0.50,
        }
    }
}

/// Context provided at resolve time. All queryable from `FaceRepo`.
#[derive(Debug, Clone)]
pub struct ResolverContext {
    /// Cluster IDs of other assigned faces in the same photo.
    pub photo_other_clusters: Vec<i64>,
    /// (candidate_cluster_id, cooccurrence_count) for each cluster
    /// currently in the photo vs. each candidate. Caller aggregates.
    pub cooccurrence_scores: std::collections::HashMap<i64, i64>,
    /// Cluster IDs assigned to faces in temporally-nearby photos.
    pub temporal_neighbor_clusters: std::collections::HashSet<i64>,
}

/// Apply contextual signals to a ranked list of retrieval hits and return
/// the re-sorted list.
///
/// Output is sorted by adjusted score descending.
pub fn rerank(
    hits: &[RetrievalHit],
    ctx: &ResolverContext,
    weights: ResolverWeights,
) -> Vec<RetrievalHit> {
    if hits.is_empty() {
        return Vec::new();
    }

    // Saturation normalizer: avoid letting a single prolific pair dominate.
    // Map raw count into a bounded log-ish boost.
    let cooccurrence_bonus = |count: i64| -> f32 {
        if count <= 0 {
            0.0
        } else {
            ((count as f32 + 1.0).ln() / 5.0).min(1.0)
        }
    };

    let mut adjusted: Vec<RetrievalHit> = hits
        .iter()
        .map(|h| {
            let mut score = h.score * weights.embedding;

            if weights.cooccurrence > 0.0 && !ctx.photo_other_clusters.is_empty() {
                let raw = ctx
                    .cooccurrence_scores
                    .get(&h.cluster_id)
                    .copied()
                    .unwrap_or(0);
                score += weights.cooccurrence * cooccurrence_bonus(raw);
            }

            if weights.temporal > 0.0
                && ctx.temporal_neighbor_clusters.contains(&h.cluster_id)
            {
                score += weights.temporal * 0.1;
            }

            RetrievalHit {
                cluster_id: h.cluster_id,
                score,
                best_match_face_id: h.best_match_face_id,
                num_matches: h.num_matches,
            }
        })
        .collect();

    adjusted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn hit(cluster_id: i64, score: f32) -> RetrievalHit {
        RetrievalHit {
            cluster_id,
            score,
            best_match_face_id: 0,
            num_matches: 1,
        }
    }

    #[test]
    fn no_context_preserves_order() {
        let hits = vec![hit(1, 0.60), hit(2, 0.55)];
        let ctx = ResolverContext {
            photo_other_clusters: Vec::new(),
            cooccurrence_scores: HashMap::new(),
            temporal_neighbor_clusters: HashSet::new(),
        };
        let out = rerank(&hits, &ctx, ResolverWeights::default());
        assert_eq!(out[0].cluster_id, 1);
        assert_eq!(out[1].cluster_id, 2);
    }

    #[test]
    fn cooccurrence_can_flip_close_pair() {
        // Cluster 2 has ~slightly worse embedding but co-occurs strongly with
        // an already-present cluster. Should win after re-rank.
        let hits = vec![hit(1, 0.52), hit(2, 0.50)];
        let mut co = HashMap::new();
        co.insert(2i64, 50i64); // strong co-occurrence
        let ctx = ResolverContext {
            photo_other_clusters: vec![99],
            cooccurrence_scores: co,
            temporal_neighbor_clusters: HashSet::new(),
        };
        let out = rerank(&hits, &ctx, ResolverWeights::default());
        assert_eq!(out[0].cluster_id, 2);
    }

    #[test]
    fn temporal_boost_applies() {
        let hits = vec![hit(1, 0.50), hit(2, 0.49)];
        let mut neighbors = HashSet::new();
        neighbors.insert(2i64);
        let ctx = ResolverContext {
            photo_other_clusters: Vec::new(),
            cooccurrence_scores: HashMap::new(),
            temporal_neighbor_clusters: neighbors,
        };
        let out = rerank(&hits, &ctx, ResolverWeights::default());
        assert_eq!(out[0].cluster_id, 2);
    }
}
