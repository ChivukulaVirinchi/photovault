//! Face Clustering using DBSCAN
//!
//! Groups similar face embeddings into clusters (people).
//! Uses cosine distance (1 - cosine_similarity) as the distance metric.

use std::collections::HashMap;

use ndarray::Array2;

use super::FaceEmbedding;

/// DBSCAN-based face clusterer
pub struct FaceClusterer {
    /// Minimum samples to form a cluster
    min_samples: usize,

    /// Maximum distance (1 - cosine_similarity) for same cluster
    epsilon: f32,
}

impl FaceClusterer {
    /// Create a new clusterer with default parameters
    pub fn new() -> Self {
        Self {
            min_samples: 2, // At least 2 faces to form a cluster
            epsilon: 0.4,   // ~0.6 cosine similarity threshold
        }
    }

    /// Set epsilon (distance threshold)
    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    /// Cluster faces and return a map from face_id to cluster_id.
    ///
    /// Cluster IDs are non-negative integers. Noise points get cluster_id = -1.
    pub fn cluster(&self, faces: &[(i64, FaceEmbedding)]) -> HashMap<i64, i32> {
        if faces.is_empty() {
            return HashMap::new();
        }

        let n = faces.len();

        // Build distance matrix (1 - cosine_similarity)
        let mut distances = Array2::<f32>::zeros((n, n));
        for i in 0..n {
            for j in (i + 1)..n {
                let sim = faces[i].1.cosine_similarity(&faces[j].1);
                let dist = 1.0 - sim;
                distances[[i, j]] = dist;
                distances[[j, i]] = dist;
            }
        }

        // Run DBSCAN
        let labels = self.dbscan(&distances);

        // Map back to face IDs
        faces
            .iter()
            .enumerate()
            .map(|(i, (face_id, _))| (*face_id, labels[i]))
            .collect()
    }

    /// DBSCAN implementation
    ///
    /// Returns a vector of cluster labels. -1 means noise (unclustered).
    fn dbscan(&self, distances: &Array2<f32>) -> Vec<i32> {
        let n = distances.nrows();
        // -2 = undefined (not yet visited), -1 = noise
        let mut labels = vec![-2i32; n];
        let mut cluster_id: i32 = 0;

        for i in 0..n {
            if labels[i] != -2 {
                continue; // Already processed
            }

            // Find neighbors within epsilon
            let neighbors = self.region_query(distances, i);

            if neighbors.len() < self.min_samples {
                labels[i] = -1; // Mark as noise
                continue;
            }

            // Start a new cluster
            labels[i] = cluster_id;

            // Expand cluster using a seed set
            let mut seed_set: Vec<usize> = neighbors;
            let mut j = 0;

            while j < seed_set.len() {
                let q = seed_set[j];

                if labels[q] == -1 {
                    // Change noise to border point
                    labels[q] = cluster_id;
                }

                if labels[q] != -2 {
                    j += 1;
                    continue; // Already processed
                }

                labels[q] = cluster_id;

                let q_neighbors = self.region_query(distances, q);
                if q_neighbors.len() >= self.min_samples {
                    // Add new neighbors to seed set (avoid duplicates)
                    for &neighbor in &q_neighbors {
                        if !seed_set.contains(&neighbor) {
                            seed_set.push(neighbor);
                        }
                    }
                }

                j += 1;
            }

            cluster_id += 1;
        }

        // Convert remaining -2 to -1 (shouldn't happen, but safety)
        for label in &mut labels {
            if *label == -2 {
                *label = -1;
            }
        }

        labels
    }

    /// Find all points within epsilon distance of the given point
    fn region_query(&self, distances: &Array2<f32>, point: usize) -> Vec<usize> {
        let n = distances.nrows();
        (0..n)
            .filter(|&i| distances[[point, i]] <= self.epsilon)
            .collect()
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
    fn make_embedding(values: &[f32]) -> FaceEmbedding {
        FaceEmbedding::new(ndarray::Array1::from_vec(values.to_vec()))
    }

    #[test]
    fn test_empty_clustering() {
        let clusterer = FaceClusterer::new();
        let result = clusterer.cluster(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_cluster_identical_faces() {
        let clusterer = FaceClusterer::new();

        let emb = vec![1.0; 512];
        let faces = vec![
            (1, make_embedding(&emb)),
            (2, make_embedding(&emb)),
            (3, make_embedding(&emb)),
        ];

        let result = clusterer.cluster(&faces);

        // All identical embeddings should be in the same cluster
        let c1 = result[&1];
        assert_eq!(c1, result[&2]);
        assert_eq!(c1, result[&3]);
        assert!(c1 >= 0); // Not noise
    }

    #[test]
    fn test_cluster_distinct_faces() {
        let clusterer = FaceClusterer::new().with_epsilon(0.3);

        // Two groups of very different embeddings
        let mut group_a = vec![0.0f32; 512];
        group_a[0] = 1.0;

        let mut group_b = vec![0.0f32; 512];
        group_b[1] = 1.0;

        let faces = vec![
            (1, make_embedding(&group_a)),
            (2, make_embedding(&group_a)),
            (3, make_embedding(&group_b)),
            (4, make_embedding(&group_b)),
        ];

        let result = clusterer.cluster(&faces);

        // Group A faces should be in one cluster
        assert_eq!(result[&1], result[&2]);
        // Group B faces should be in one cluster
        assert_eq!(result[&3], result[&4]);
        // The two groups should be in different clusters
        assert_ne!(result[&1], result[&3]);
    }

}
