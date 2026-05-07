//! Integration test for the face clustering pipeline.
//!
//! Uses synthetic 512-dimensional embeddings so we can exercise the
//! clusterer without needing ONNX Runtime, real images, or the face
//! detector. Three "identities" are simulated: each identity's faces
//! have embeddings near a shared axis direction with small Gaussian
//! noise, and the three axes are mutually orthogonal so they should
//! always land in separate clusters at reasonable thresholds.

use ndarray::Array1;
use smriti::ml::{ClusterInput, FaceClusterer, FaceEmbedding};
use std::collections::HashMap;

const EMBEDDING_DIM: usize = 512;

/// Deterministic linear-congruential generator so tests are stable
/// without pulling in a dev-dep on `rand`.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_f32(&mut self) -> f32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.state >> 33) as f32) / (u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Build a unit-length embedding pointing predominantly along `axis`,
/// with small noise sprinkled across the remaining dimensions.
fn synthesize_embedding(axis: usize, rng: &mut Lcg, noise_scale: f32) -> FaceEmbedding {
    let mut v = vec![0.0f32; EMBEDDING_DIM];
    for e in v.iter_mut() {
        *e = rng.next_f32() * noise_scale;
    }
    v[axis] += 1.0;
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
    FaceEmbedding::new(Array1::from(v))
}

/// Build a faces-in-cluster fixture: `identities.len()` clusters,
/// each with `faces_per_identity` synthetic faces.
fn build_fixture(identities: &[usize], faces_per_identity: usize, noise: f32) -> Vec<ClusterInput> {
    let mut rng = Lcg::new(0xC0FFEE);
    let mut out = Vec::with_capacity(identities.len() * faces_per_identity);
    let mut face_id: i64 = 0;
    let mut photo_id: i64 = 0;
    for &axis in identities {
        for _ in 0..faces_per_identity {
            face_id += 1;
            photo_id += 1;
            out.push(ClusterInput {
                face_id,
                photo_id,
                embedding: synthesize_embedding(axis, &mut rng, noise),
            });
        }
    }
    out
}

fn cluster_sizes(assignments: &HashMap<i64, i32>) -> Vec<usize> {
    let mut by_cluster: HashMap<i32, usize> = HashMap::new();
    for &cid in assignments.values() {
        if cid >= 0 {
            *by_cluster.entry(cid).or_default() += 1;
        }
    }
    let mut sizes: Vec<usize> = by_cluster.into_values().collect();
    sizes.sort_unstable();
    sizes
}

#[test]
fn three_identities_form_three_clusters() {
    let faces = build_fixture(&[0, 100, 200], 5, 0.01);
    let clusterer = FaceClusterer::new();

    let assignments = clusterer.cluster(&faces);
    let sizes = cluster_sizes(&assignments);

    assert_eq!(
        sizes.len(),
        3,
        "expected 3 clusters for 3 orthogonal identities, got sizes {:?}",
        sizes
    );
    for size in &sizes {
        assert_eq!(
            *size, 5,
            "expected each cluster to hold 5 faces, got {:?}",
            sizes
        );
    }
}

#[test]
fn clustering_is_idempotent_for_the_same_input() {
    let faces = build_fixture(&[0, 100, 200], 4, 0.01);
    let clusterer = FaceClusterer::new();

    let run_a = clusterer.cluster(&faces);
    let run_b = clusterer.cluster(&faces);

    let sizes_a = cluster_sizes(&run_a);
    let sizes_b = cluster_sizes(&run_b);
    assert_eq!(
        sizes_a, sizes_b,
        "clusterer must produce the same cluster-size histogram twice in a row"
    );
}

#[test]
fn looser_threshold_produces_fewer_clusters() {
    // Inverse correctness check: loosening the merge threshold should
    // never produce *more* clusters than a strict threshold on the
    // same input. Catches regressions where threshold handling gets
    // inverted or short-circuited.
    let faces = build_fixture(&[0, 100, 200], 4, 0.01);

    let strict = FaceClusterer::new().with_max_distance(0.2);
    let loose = FaceClusterer::new().with_max_distance(0.9);

    let strict_count = cluster_sizes(&strict.cluster(&faces)).len();
    let loose_count = cluster_sizes(&loose.cluster(&faces)).len();

    assert!(
        loose_count <= strict_count,
        "loose threshold ({} clusters) should not produce more clusters than strict ({})",
        loose_count,
        strict_count
    );
}

#[test]
fn strict_threshold_leaves_noisy_faces_unmerged() {
    // Much noisier fixture pushed through the default strict threshold
    // should still keep the 3 axes apart — but we at minimum expect >=3
    // clusters (the clusterer will rather leave faces as singletons
    // than merge them wrongly). This guards against the clusterer
    // regressing to a DBSCAN-chaining behavior where noisy faces
    // bridge clusters.
    let faces = build_fixture(&[0, 100, 200], 4, 0.15);
    let clusterer = FaceClusterer::new();

    let assignments = clusterer.cluster(&faces);
    let sizes = cluster_sizes(&assignments);
    assert!(
        sizes.len() >= 3,
        "strict clustering must not merge orthogonal identities; sizes {:?}",
        sizes
    );
}

#[test]
fn empty_input_returns_empty_assignments() {
    let clusterer = FaceClusterer::new();
    let assignments = clusterer.cluster(&[]);
    assert!(assignments.is_empty());
}
