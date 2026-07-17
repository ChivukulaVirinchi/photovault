//! Face clustering using agglomerative complete-linkage.
//!
//! This avoids DBSCAN chaining by only merging clusters where *all*
//! pairwise distances remain under a strict threshold.

use std::collections::HashMap;

use super::FaceEmbedding;

// HNSW deliberately randomizes graph levels. Exact complete-link clustering
// is affordable for small batches and makes repeated runs stable; large
// libraries retain the O(n log n) path.
const EXACT_CLUSTER_LIMIT: usize = 256;

#[derive(Debug, Clone)]
pub struct ClusterInput {
    pub face_id: i64,
    pub photo_id: i64,
    /// Current cluster assignment (if any) — used with face_negatives to
    /// avoid clustering a face back into a cluster it was rejected from.
    pub current_cluster_id: Option<i64>,
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
        Self { max_distance: 0.28 }
    }

    pub fn with_max_distance(mut self, max_distance: f32) -> Self {
        self.max_distance = max_distance.clamp(0.05, 1.0);
        self
    }

    /// Cluster faces and return map of face_id -> cluster_label (-1 = singleton/noise).
    ///
    /// `negatives` is an optional set of (face_id, not_cluster_id) pairs from
    /// the `face_negatives` table. When present, two faces whose current clusters
    /// are blocked by a negative constraint will not be merged.
    pub fn cluster(
        &self,
        faces: &[ClusterInput],
        negatives: Option<&std::collections::HashSet<(i64, i64)>>,
    ) -> HashMap<i64, i32> {
        #[cfg(feature = "hnsw_clustering")]
        {
            if faces.len() <= EXACT_CLUSTER_LIMIT {
                self.cluster_complete_link(faces, negatives)
            } else {
                cluster_hnsw(faces, self.max_distance, negatives)
            }
        }
        #[cfg(not(feature = "hnsw_clustering"))]
        {
            self.cluster_complete_link(faces, negatives)
        }
    }

    /// Legacy O(n²) complete-link path. Kept compiled when
    /// `hnsw_clustering` is off so we can A/B against the new
    /// implementation on a real library by toggling features.
    fn cluster_complete_link(
        &self,
        faces: &[ClusterInput],
        negatives: Option<&std::collections::HashSet<(i64, i64)>>,
    ) -> HashMap<i64, i32> {
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
                    if Self::has_negative_conflict(&clusters[i], &clusters[j], faces, negatives) {
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

    /// If any face in cluster A has a (face_id, current_cluster_id-of-some-face-in-B)
    /// pair in `negatives` — or vice versa — the merge is forbidden.
    fn has_negative_conflict(
        a: &[usize],
        b: &[usize],
        faces: &[ClusterInput],
        negatives: Option<&std::collections::HashSet<(i64, i64)>>,
    ) -> bool {
        let neg = match negatives {
            Some(n) if !n.is_empty() => n,
            _ => return false,
        };
        for &ai in a {
            for &bi in b {
                if let Some(bc) = faces[bi].current_cluster_id {
                    if neg.contains(&(faces[ai].face_id, bc)) {
                        return true;
                    }
                }
                if let Some(ac) = faces[ai].current_cluster_id {
                    if neg.contains(&(faces[bi].face_id, ac)) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// HNSW-based clustering — replaces complete-link's O(n²) with
/// O(n log n) index build + O(n · k) k-NN queries + O(n · α) union-find.
/// Wall time on 10k faces: <30 s vs hours for the legacy path.
///
/// 1. Build an HNSW index over all embeddings using cosine distance.
/// 2. For each face, query top-K nearest neighbours (K=15).
/// 3. Edges with similarity ≥ (1 − max_distance) and no same-photo
///    conflict get unioned via union-find.
/// 4. Each connected component with ≥ 2 members becomes a cluster.
#[cfg(feature = "hnsw_clustering")]
fn cluster_hnsw(
    faces: &[ClusterInput],
    max_distance: f32,
    negatives: Option<&std::collections::HashSet<(i64, i64)>>,
) -> HashMap<i64, i32> {
    use hnsw_rs::prelude::*;
    use std::collections::HashSet;
    if faces.is_empty() {
        return HashMap::new();
    }
    if faces.len() == 1 {
        let mut out = HashMap::new();
        out.insert(faces[0].face_id, -1);
        return out;
    }

    let n = faces.len();
    // M=16 / ef_c=200 are the de-facto-good defaults for cosine
    // recall on ~512-dim vectors.
    let max_nb_connection = 16;
    let ef_c = 200;
    let nb_layer = 16.min(((n as f32).ln().trunc() as usize).max(1));
    let hns: Hnsw<f32, DistCosine> = Hnsw::new(max_nb_connection, n, nb_layer, ef_c, DistCosine {});

    // Insert with HNSW's per-row index as the data id; we map back
    // to face_id via the input slice.
    let data: Vec<(&[f32], usize)> = faces
        .iter()
        .enumerate()
        .map(|(i, f)| (f.embedding.vector.as_slice().unwrap_or(&[]), i))
        .collect();
    hns.parallel_insert_slice(&data);

    // Union-find over face indices.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    // Build a per-component photo set so we don't union two
    // components if doing so would put the same photo's faces
    // into one cluster (same-photo conflict).
    let mut component_photos: HashMap<usize, std::collections::HashSet<i64>> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        component_photos.entry(i).or_default().insert(f.photo_id);
    }

    let knbn = 15;
    let ef_search = ef_c;
    for (i, face_i) in faces.iter().enumerate() {
        let query = match face_i.embedding.vector.as_slice() {
            Some(s) => s,
            None => continue,
        };
        let neighbours = hns.search(query, knbn, ef_search);
        for nb in neighbours {
            let j = nb.d_id;
            if j == i {
                continue;
            }
            // DistCosine returns 1 - cos_sim, which is exactly our
            // existing "max_distance" threshold semantics.
            if nb.distance > max_distance {
                continue;
            }
            let ra = find(&mut parent, i);
            let rb = find(&mut parent, j);
            if ra == rb {
                continue;
            }
            // Check same-photo conflict across the two components.
            let photos_a = component_photos.get(&ra).cloned().unwrap_or_default();
            let photos_b = component_photos.get(&rb).cloned().unwrap_or_default();
            if photos_a.intersection(&photos_b).next().is_some() {
                continue;
            }

            // Check face_negatives constraints: if any face in component A
            // has a negative against the current cluster of any face in
            // component B (or vice versa), skip the merge.
            if let Some(neg) = negatives {
                // Collect face_ids in each component.
                let faces_a: HashSet<i64> = faces
                    .iter()
                    .enumerate()
                    .filter(|(fi, _)| find(&mut parent, *fi) == ra)
                    .map(|(_, f)| f.face_id)
                    .collect();
                let faces_b: HashSet<i64> = faces
                    .iter()
                    .enumerate()
                    .filter(|(fi, _)| find(&mut parent, *fi) == rb)
                    .map(|(_, f)| f.face_id)
                    .collect();

                // Check A→B: any face in A has negative against B's cluster?
                let conflict_a_to_b = faces_a.iter().any(|fa| {
                    faces_b.iter().any(|fb| {
                        let fb_cluster = faces
                            .iter()
                            .find(|f| f.face_id == *fb)
                            .and_then(|f| f.current_cluster_id);
                        fb_cluster.is_some_and(|cid| neg.contains(&(*fa, cid)))
                    })
                });
                // Check B→A: any face in B has negative against A's cluster?
                let conflict_b_to_a = faces_b.iter().any(|fb| {
                    faces_a.iter().any(|fa| {
                        let fa_cluster = faces
                            .iter()
                            .find(|f| f.face_id == *fa)
                            .and_then(|f| f.current_cluster_id);
                        fa_cluster.is_some_and(|cid| neg.contains(&(*fb, cid)))
                    })
                });

                if conflict_a_to_b || conflict_b_to_a {
                    continue;
                }
            }

            union(&mut parent, ra, rb);
            // Move the smaller set into the larger; track on the new root.
            let new_root = find(&mut parent, ra);
            let other = if new_root == ra { rb } else { ra };
            let mut merged = photos_a;
            merged.extend(photos_b);
            component_photos.insert(new_root, merged);
            component_photos.remove(&other);
        }
    }

    // Group by root → emit cluster labels.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }

    let mut out = HashMap::new();
    let mut label = 0i32;
    for (_root, members) in groups {
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
            current_cluster_id: None,
            embedding: emb(&vec),
        };
        let a = mk(1, 11, vec![1.0, 0.0, 0.0]);
        let b = mk(2, 12, vec![0.8, 0.2, 0.0]);
        let c = mk(3, 13, vec![0.0, 1.0, 0.0]);

        let clusterer = FaceClusterer::new().with_max_distance(0.35);
        let labels = clusterer.cluster(&[a, b, c], None);
        // Should not place all three in one non-negative cluster.
        let vals: Vec<i32> = labels.values().copied().filter(|v| *v >= 0).collect();
        assert!(vals.len() < 3);
    }

    #[test]
    fn prevents_same_photo_merge() {
        let mk = |id: i64, p: i64| ClusterInput {
            face_id: id,
            photo_id: p,
            current_cluster_id: None,
            embedding: emb(&[1.0, 0.0, 0.0]),
        };

        let clusterer = FaceClusterer::new();
        let labels = clusterer.cluster(&[mk(1, 42), mk(2, 42)], None);
        assert_eq!(labels[&1], -1);
        assert_eq!(labels[&2], -1);
    }
}
