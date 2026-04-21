//! Phase 1 baseline bench for full-file SHA256 vs the (deleted)
//! 64KB-prefix quick_hash. Locks in the throughput target so a future
//! optimization (incremental rolling hash, mmap, content-defined
//! chunking) can be measured against it.
//!
//! Run with: `cargo bench --bench hashing`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use tempfile::NamedTempFile;

fn make_file(size_bytes: usize) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("temp file");
    let chunk = vec![0xABu8; 65_536];
    let mut written = 0;
    while written < size_bytes {
        let to_write = (size_bytes - written).min(chunk.len());
        f.write_all(&chunk[..to_write]).unwrap();
        written += to_write;
    }
    f.flush().unwrap();
    f
}

fn full_hash(path: &std::path::Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn bench_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_hash");
    group.sample_size(20);
    for &size_mb in &[1usize, 4, 16] {
        let file = make_file(size_mb * 1024 * 1024);
        let path = file.path().to_path_buf();
        group.throughput(criterion::Throughput::Bytes((size_mb * 1024 * 1024) as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size_mb), &path, |b, path| {
            b.iter(|| {
                let _ = black_box(full_hash(black_box(path)).unwrap());
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_hashing);
criterion_main!(benches);
