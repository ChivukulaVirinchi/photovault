//! Phase 1 baseline benches for the face clusterer.
//!
//! Phase 2 will replace complete-link with HNSW; these benches lock in
//! the v1.0 baseline so future optimizations can be measured.
//!
//! Run with: `cargo bench --bench clustering`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ndarray::Array1;
use smriti::ml::{ClusterInput, FaceClusterer, FaceEmbedding};

const EMBEDDING_DIM: usize = 512;

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

fn synthesize(axis: usize, rng: &mut Lcg, noise: f32) -> FaceEmbedding {
    let mut v = vec![0.0f32; EMBEDDING_DIM];
    for e in v.iter_mut() {
        *e = rng.next_f32() * noise;
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

fn build_fixture(total_faces: usize, identities: usize) -> Vec<ClusterInput> {
    let mut rng = Lcg::new(0xCAFEF00D);
    let faces_per = total_faces / identities;
    let mut out = Vec::with_capacity(total_faces);
    let mut id_counter: i64 = 0;
    for ident in 0..identities {
        let axis = (ident * EMBEDDING_DIM / identities) % EMBEDDING_DIM;
        for _ in 0..faces_per {
            id_counter += 1;
            out.push(ClusterInput {
                face_id: id_counter,
                photo_id: id_counter,
                embedding: synthesize(axis, &mut rng, 0.05),
            });
        }
    }
    out
}

fn bench_clusterer(c: &mut Criterion) {
    let mut group = c.benchmark_group("complete_link_clustering");
    // Sample sizes chosen to bracket the Phase 1 Stage-B cap (2000).
    for &size in &[100usize, 500, 1000, 2000] {
        let faces = build_fixture(size, 10);
        group.bench_with_input(BenchmarkId::from_parameter(size), &faces, |b, faces| {
            let clusterer = FaceClusterer::new();
            b.iter(|| {
                let _ = black_box(clusterer.cluster(black_box(faces)));
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_clusterer);
criterion_main!(benches);
