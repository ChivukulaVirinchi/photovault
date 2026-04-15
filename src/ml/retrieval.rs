//! Gallery-based face retrieval.
//!
//! Given a query embedding and a set of per-cluster galleries (N diverse
//! exemplars per cluster), find the best-matching clusters via k-NN.
//! Each cluster's score is the mean of the top-k cosine similarities among its
//! gallery members. This is strictly stronger than centroid matching: a
//! cluster is "close" if *multiple* of its real members are close, not just
//! its mean.
//!
//! The result drives confidence-banded assignment: HIGH -> auto-assign,
//! LOW -> leave unassigned, AMBIGUOUS -> queue for user review.

use super::FaceEmbedding;

/// A cluster candidate ranked against the query face.
#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub cluster_id: i64,
    /// Mean of the top-k cosine similarities against this cluster's gallery.
    /// Higher = better. Range [-1, 1], but in practice [0, 1].
    pub score: f32,
    /// Face id within the gallery that had the single highest similarity.
    pub best_match_face_id: i64,
    /// How many gallery members exceeded `min_similarity`.
    pub num_matches: usize,
}

/// Assignment recommendation for a query face.
#[derive(Debug, Clone)]
pub enum ConfidenceBand {
    /// Top candidate exceeds `high_threshold` and leads the runner-up by
    /// at least `margin`. Safe to auto-assign.
    High { hit: RetrievalHit },
    /// Between thresholds, or top-2 candidates too close. Queue for review.
    Ambiguous {
        top: RetrievalHit,
        runner_up: Option<RetrievalHit>,
    },
    /// Top score below `low_threshold`. Face does not match any existing
    /// cluster; leave for agglomerative pass or mark as a new singleton.
    Low,
}

/// Thresholds for banding. Tuned for ArcFace / GLinTR L2-normalized embeddings.
#[derive(Debug, Clone, Copy)]
pub struct BandingConfig {
    /// Minimum score to consider any match at all.
    pub low_threshold: f32,
    /// Minimum top score to auto-assign without review.
    pub high_threshold: f32,
    /// Minimum score gap between top and runner-up to auto-assign.
    pub margin: f32,
}

impl Default for BandingConfig {
    fn default() -> Self {
        Self {
            low_threshold: 0.40,
            high_threshold: 0.55,
            margin: 0.08,
        }
    }
}

/// Retrieve ranked cluster candidates for a query embedding.
///
/// `galleries` is a slice of (cluster_id, Vec<(face_id, embedding)>). All
/// embeddings are assumed L2-normalized (as produced by the embedder and
/// stored in `person_gallery_embeddings`).
///
/// `top_k` is the number of best matches per cluster to average into that
/// cluster's score. 5 is a good default.
///
/// `min_similarity` filters individual gallery members below this cosine
/// similarity before they contribute to the score.
///
/// Clusters in `exclude` are skipped entirely -- used to honor
/// cannot-merge constraints and same-photo conflict prevention.
///
/// Returns clusters sorted by descending score. Empty if no cluster scored
/// above `min_similarity`.
pub fn retrieve_candidates(
    query: &FaceEmbedding,
    galleries: &[(i64, Vec<(i64, FaceEmbedding)>)],
    top_k: usize,
    min_similarity: f32,
    exclude: &std::collections::HashSet<i64>,
) -> Vec<RetrievalHit> {
    let mut hits: Vec<RetrievalHit> = Vec::with_capacity(galleries.len());

    for (cluster_id, members) in galleries {
        if exclude.contains(cluster_id) || members.is_empty() {
            continue;
        }

        // Score each member, keep those above min_similarity.
        let mut scored: Vec<(i64, f32)> = members
            .iter()
            .map(|(face_id, emb)| (*face_id, query.cosine_similarity(emb)))
            .filter(|(_, s)| *s >= min_similarity)
            .collect();

        if scored.is_empty() {
            continue;
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let k = top_k.min(scored.len());
        let mean_top_k: f32 = scored.iter().take(k).map(|(_, s)| s).sum::<f32>() / k as f32;

        hits.push(RetrievalHit {
            cluster_id: *cluster_id,
            score: mean_top_k,
            best_match_face_id: scored[0].0,
            num_matches: scored.len(),
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits
}

/// Classify the retrieval result into a confidence band.
pub fn classify(hits: &[RetrievalHit], cfg: &BandingConfig) -> ConfidenceBand {
    match hits.len() {
        0 => ConfidenceBand::Low,
        1 => {
            let top = hits[0].clone();
            if top.score >= cfg.high_threshold {
                ConfidenceBand::High { hit: top }
            } else if top.score >= cfg.low_threshold {
                ConfidenceBand::Ambiguous {
                    top,
                    runner_up: None,
                }
            } else {
                ConfidenceBand::Low
            }
        }
        _ => {
            let top = hits[0].clone();
            let runner_up = hits[1].clone();
            if top.score < cfg.low_threshold {
                ConfidenceBand::Low
            } else if top.score >= cfg.high_threshold && (top.score - runner_up.score) >= cfg.margin
            {
                ConfidenceBand::High { hit: top }
            } else {
                ConfidenceBand::Ambiguous {
                    top,
                    runner_up: Some(runner_up),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array1;
    use std::collections::HashSet;

    fn emb(v: Vec<f32>) -> FaceEmbedding {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normed: Vec<f32> = v.iter().map(|x| x / norm).collect();
        FaceEmbedding::new(Array1::from_vec(normed))
    }

    #[test]
    fn empty_galleries_yield_no_hits() {
        let q = emb(vec![1.0, 0.0, 0.0]);
        let hits = retrieve_candidates(&q, &[], 3, 0.3, &HashSet::new());
        assert!(hits.is_empty());
    }

    #[test]
    fn near_identical_gallery_scores_highest() {
        let q = emb(vec![1.0, 0.0, 0.0]);
        let galleries: Vec<(i64, Vec<(i64, FaceEmbedding)>)> = vec![
            (
                1,
                vec![
                    (10, emb(vec![0.99, 0.01, 0.0])),
                    (11, emb(vec![0.98, 0.0, 0.1])),
                ],
            ),
            (2, vec![(20, emb(vec![0.0, 1.0, 0.0]))]),
        ];

        let hits = retrieve_candidates(&q, &galleries, 3, 0.3, &HashSet::new());
        assert_eq!(hits.len(), 1); // cluster 2 is below threshold
        assert_eq!(hits[0].cluster_id, 1);
        assert!(hits[0].score > 0.98);
    }

    #[test]
    fn exclusion_is_respected() {
        let q = emb(vec![1.0, 0.0, 0.0]);
        let galleries: Vec<(i64, Vec<(i64, FaceEmbedding)>)> = vec![
            (1, vec![(10, emb(vec![0.99, 0.01, 0.0]))]),
            (2, vec![(20, emb(vec![0.95, 0.05, 0.0]))]),
        ];

        let mut exclude = HashSet::new();
        exclude.insert(1);
        let hits = retrieve_candidates(&q, &galleries, 3, 0.3, &exclude);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].cluster_id, 2);
    }

    #[test]
    fn classify_bands() {
        let cfg = BandingConfig::default();

        let low = vec![RetrievalHit {
            cluster_id: 1,
            score: 0.2,
            best_match_face_id: 1,
            num_matches: 1,
        }];
        assert!(matches!(classify(&low, &cfg), ConfidenceBand::Low));

        let high = vec![RetrievalHit {
            cluster_id: 1,
            score: 0.7,
            best_match_face_id: 1,
            num_matches: 3,
        }];
        assert!(matches!(classify(&high, &cfg), ConfidenceBand::High { .. }));

        let ambiguous_pair = vec![
            RetrievalHit {
                cluster_id: 1,
                score: 0.60,
                best_match_face_id: 1,
                num_matches: 2,
            },
            RetrievalHit {
                cluster_id: 2,
                score: 0.57,
                best_match_face_id: 2,
                num_matches: 2,
            },
        ];
        assert!(matches!(
            classify(&ambiguous_pair, &cfg),
            ConfidenceBand::Ambiguous { .. }
        ));
    }
}
