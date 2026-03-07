//! Face Clustering using DBSCAN
//!
//! Groups similar face embeddings into clusters (people).
//! Uses cosine distance (1 - cosine_similarity) as the distance metric.

use std::collections::HashMap;

use ndarray::{Array1, Array2};

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

    /// Set minimum samples per cluster
    pub fn with_min_samples(mut self, min_samples: usize) -> Self {
        self.min_samples = min_samples;
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

    /// Assign a new face to the closest existing cluster (for incremental clustering)
    ///
    /// Returns `Some(cluster_id)` if the face is close enough, `None` if it's noise.
    pub fn assign_to_cluster(
        &self,
        new_embedding: &FaceEmbedding,
        cluster_centroids: &[(i32, FaceEmbedding)],
    ) -> Option<i32> {
        let mut best_cluster = None;
        let mut best_distance = f32::MAX;

        for (cluster_id, centroid) in cluster_centroids {
            let distance = 1.0 - new_embedding.cosine_similarity(centroid);
            if distance < self.epsilon && distance < best_distance {
                best_distance = distance;
                best_cluster = Some(*cluster_id);
            }
        }

        best_cluster
    }

    /// Calculate the centroid (average) of a set of embeddings
    pub fn calculate_centroid(embeddings: &[FaceEmbedding]) -> Option<FaceEmbedding> {
        if embeddings.is_empty() {
            return None;
        }

        let n = embeddings.len() as f32;
        let dim = embeddings[0].vector.len();
        let mut sum = Array1::<f32>::zeros(dim);

        for emb in embeddings {
            sum = sum + &emb.vector;
        }

        let avg = sum / n;

        // L2 normalize
        let norm: f32 = avg.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized = if norm > 0.0 { avg / norm } else { avg };

        Some(FaceEmbedding::new(normalized))
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
    use ndarray::Array1;

    fn make_embedding(values: &[f32]) -> FaceEmbedding {
        FaceEmbedding::new(Array1::from_vec(values.to_vec()))
    }

    #[test]
    fn test_empty_clustering() {
        let clusterer = FaceClusterer::new();
        let result = clusterer.cluster(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_cluster_identical_faces() {
        let clusterer = FaceClusterer::new().with_min_samples(2);

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
        let clusterer = FaceClusterer::new().with_min_samples(2).with_epsilon(0.3);

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

    #[test]
    fn test_centroid_calculation() {
        let emb1 = make_embedding(&[1.0, 0.0, 0.0]);
        let emb2 = make_embedding(&[0.0, 1.0, 0.0]);

        let centroid = FaceClusterer::calculate_centroid(&[emb1, emb2]).unwrap();

        // Centroid should be normalized
        let norm: f32 = centroid.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }
}
