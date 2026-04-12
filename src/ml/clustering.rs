//! Face clustering using agglomerative complete-linkage.
//!
//! This avoids DBSCAN chaining by only merging clusters where *all*
//! pairwise distances remain under a strict threshold.

use std::collections::HashMap;

use super::FaceEmbedding;

#[derive(Debug, Clone)]
pub struct ClusterInput {
    pub face_id: i64,
    pub photo_id: i64,
    pub embedding: FaceEmbedding,
}

/// Agglomerative clusterer with complete linkage.
pub struct FaceClusterer {
    /// Maximum cosine distance (1 - cosine similarity) allowed within cluster.
    max_distance: f32,
}

impl FaceClusterer {
    /// Strict default to reduce false merges.
    pub fn new() -> Self {
        Self { max_distance: 0.35 }
    }

    pub fn with_max_distance(mut self, max_distance: f32) -> Self {
        self.max_distance = max_distance.clamp(0.05, 1.0);
        self
    }

    /// Cluster faces and return map of face_id -> cluster_label (-1 = singleton/noise).
    pub fn cluster(&self, faces: &[ClusterInput]) -> HashMap<i64, i32> {
        if faces.is_empty() {
            return HashMap::new();
        }

        // Start with each face in its own cluster.
        let mut clusters: Vec<Vec<usize>> = (0..faces.len()).map(|i| vec![i]).collect();

        loop {
            let mut best_pair: Option<(usize, usize, f32)> = None;

            for i in 0..clusters.len() {
                for j in (i + 1)..clusters.len() {
                    let d = self.complete_link_distance(&clusters[i], &clusters[j], faces);
                    if d > self.max_distance {
                        continue;
                    }

                    let mut merged = clusters[i].clone();
                    merged.extend_from_slice(&clusters[j]);
                    if Self::has_same_photo_conflict(&merged, faces) {
                        continue;
                    }

                    match best_pair {
                        Some((_, _, best_d)) if d >= best_d => {}
                        _ => best_pair = Some((i, j, d)),
                    }
                }
            }

            let Some((a, b, _)) = best_pair else {
                break;
            };

            let mut merged = clusters[a].clone();
            merged.extend_from_slice(&clusters[b]);

            // Merge b into a
            clusters[a] = merged;
            clusters.remove(b);
        }

        let mut out = HashMap::new();
        let mut label = 0i32;
        for members in clusters {
            if members.len() < 2 {
                out.insert(faces[members[0]].face_id, -1);
            } else {
                for idx in members {
                    out.insert(faces[idx].face_id, label);
                }
                label += 1;
            }
        }
        out
    }

    fn complete_link_distance(&self, a: &[usize], b: &[usize], faces: &[ClusterInput]) -> f32 {
        let mut max_d = 0.0f32;
        for &i in a {
            for &j in b {
                let sim = faces[i].embedding.cosine_similarity(&faces[j].embedding);
                let d = 1.0 - sim;
                if d > max_d {
                    max_d = d;
                }
            }
        }
        max_d
    }

    fn has_same_photo_conflict(cluster: &[usize], faces: &[ClusterInput]) -> bool {
        let mut seen = std::collections::HashSet::new();
        for &idx in cluster {
            let pid = faces[idx].photo_id;
            if !seen.insert(pid) {
                return true;
            }
        }
        false
    }
}

impl Default for FaceClusterer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emb(v: &[f32]) -> FaceEmbedding {
        FaceEmbedding::new(ndarray::Array1::from_vec(v.to_vec()))
    }

    #[test]
    fn complete_link_rejects_chain() {
        // A-B close, B-C close, A-C far => should not all merge.
        let mk = |id: i64, p: i64, vec: Vec<f32>| ClusterInput {
            face_id: id,
            photo_id: p,
            embedding: emb(&vec),
        };
        let a = mk(1, 11, vec![1.0, 0.0, 0.0]);
        let b = mk(2, 12, vec![0.8, 0.2, 0.0]);
        let c = mk(3, 13, vec![0.0, 1.0, 0.0]);

        let clusterer = FaceClusterer::new().with_max_distance(0.35);
        let labels = clusterer.cluster(&[a, b, c]);
        // Should not place all three in one non-negative cluster.
        let vals: Vec<i32> = labels.values().copied().filter(|v| *v >= 0).collect();
        assert!(vals.len() < 3);
    }

    #[test]
    fn prevents_same_photo_merge() {
        let mk = |id: i64, p: i64| ClusterInput {
            face_id: id,
            photo_id: p,
            embedding: emb(&[1.0, 0.0, 0.0]),
        };

        let clusterer = FaceClusterer::new();
        let labels = clusterer.cluster(&[mk(1, 42), mk(2, 42)]);
        assert_eq!(labels[&1], -1);
        assert_eq!(labels[&2], -1);
    }
}
