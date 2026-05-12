//! Directory scanning service
//!
//! Recursively scans directories to find supported image files,
//! extracts EXIF metadata, and stores everything in the database.
//! Uses rayon for parallel hash computation and EXIF extraction.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

use crate::db::photo_repo::{PhotoInsert, PhotoRepo};
use crate::db::Database;

/// Supported image extensions
const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "heic", "heif", "webp"];

/// Directories to skip during scanning
const SKIP_DIRECTORIES: &[&str] = &[
    ".photovault",
    ".Trash",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".DS_Store",
    "Thumbs.db",
    ".thumbnails",
    "@eaDir", // Synology thumbnails
];

/// Minimum file size to consider (10KB)
const MIN_FILE_SIZE: u64 = 10 * 1024;

/// Batch size for database inserts
const DB_BATCH_SIZE: usize = 100;

/// How many file paths we buffer between the walker and the stub-writer.
/// 1000 keeps memory under ~200 KB even on libraries that average 200-byte paths.
const WALKER_CHANNEL_DEPTH: usize = 1000;

/// Bytes used for the fast-hash prefix. 64 KB is enough that JPEG/HEIC
/// markers + first scanline diverge for distinct photos, while staying
/// well within typical disk read-ahead (so it's effectively free).
const FAST_HASH_PREFIX_BYTES: usize = 64 * 1024;

/// How often the stub writer emits a progress event (in files written).
const STUB_PROGRESS_EVERY: u64 = 200;

/// Scan progress information
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_found: u64,
    pub files_processed: u64,
    pub bytes_processed: u64,
    pub current_directory: String,
    pub current_file: String,
    pub errors: Vec<String>,
    pub is_complete: bool,
    pub elapsed_seconds: f64,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self {
            files_found: 0,
            files_processed: 0,
            bytes_processed: 0,
            current_directory: String::new(),
            current_file: "Preparing...".to_string(),
            errors: Vec::new(),
            is_complete: false,
            elapsed_seconds: 0.0,
        }
    }
}

/// Result of a completed scan -- returns the database back to the caller
pub struct ScanResult {
    pub database: Database,
    pub final_progress: ScanProgress,
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub files_inserted: u64,
    pub errors: Vec<String>,
    pub elapsed_seconds: f64,
}

/// A candidate file discovered during directory walk
struct FileCandidate {
    path: PathBuf,
    relative_path: String,
    file_name: String,
    file_size: i64,
    mtime: Option<i64>,
}

/// Start a scan in a background task.
///
/// Borrows the database via `Arc<Mutex<Database>>` so the library
/// stays open and queryable while the scan runs. The caller provides
/// the cancel flag (usually `job.cancel`) so pause/resume is unified
/// across all background jobs.
pub fn start_scan(
    root_path: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
    scan_hidden_folders: bool,
) -> (Receiver<ScanProgress>, tokio::task::JoinHandle<ScanReport>) {
    let (progress_tx, progress_rx) = bounded::<ScanProgress>(64);
    let handle = tokio::task::spawn(async move {
        let result =
            run_scan_streaming(root_path, db, cancel, scan_hidden_folders, progress_tx).await;
        result.unwrap_or_else(|e| ScanReport {
            files_inserted: 0,
            errors: vec![format!("scan failed: {e}")],
            elapsed_seconds: 0.0,
        })
    });
    (progress_rx, handle)
}

/// Cheap identity fingerprint for the scan stage.
///
/// Reads the first 64 KB of the file, hashes (size, mtime, prefix bytes)
/// with SHA-256. The hex digest is stored directly in `photos.file_hash`;
/// downstream code (thumbnails, duplicates) uses the first two hex chars
/// as a shard subdirectory, so no prefix is added — a prefix like `fh:`
/// would collide with that path scheme and produce invalid Windows
/// filenames (`:` is reserved).
///
/// A future deferred Stage 5 worker can overwrite the column with a true
/// full-file SHA-256 if duplicate detection ever needs byte-level
/// certainty; both are hex digests of the same length, so no consumer
/// has to care.
///
/// Collisions: in practice essentially zero for organic photo libraries.
/// Two photos would have to be byte-identical in their first 64 KB AND
/// have the same file size AND the same mtime. Even bursts from the same
/// camera diverge in the first few EXIF bytes (timestamp).
pub(crate) fn calculate_fast_hash<P: AsRef<Path>>(
    path: P,
    file_size: u64,
    mtime: Option<i64>,
) -> std::io::Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(&path)?;
    let mut buf = vec![0u8; FAST_HASH_PREFIX_BYTES];
    let n = file.read(&mut buf)?;

    let mut hasher = Sha256::new();
    hasher.update(file_size.to_le_bytes());
    hasher.update(mtime.unwrap_or(0).to_le_bytes());
    hasher.update(&buf[..n]);
    Ok(format!("{:x}", hasher.finalize()))
}

async fn run_scan_streaming(
    root_path: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
    scan_hidden_folders: bool,
    progress_tx: Sender<ScanProgress>,
) -> Result<ScanReport, String> {
    let start = Instant::now();
    let (paths_tx, paths_rx) = bounded::<FileCandidate>(WALKER_CHANNEL_DEPTH);

    // ----- Producer: walker thread -----
    let walker_cancel = cancel.clone();
    let walker_root = root_path.clone();
    let walker = tokio::task::spawn_blocking(move || -> u64 {
        let mut count: u64 = 0;
        let walker = WalkDir::new(&walker_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !should_skip(e, scan_hidden_folders));

        for entry in walker {
            if walker_cancel.load(Ordering::Relaxed) {
                tracing::info!("Scan cancelled during walk");
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }
            if !is_supported_file(&entry) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() < MIN_FILE_SIZE {
                continue;
            }

            let relative_path = entry
                .path()
                .strip_prefix(&walker_root)
                .map(crate::services::path_util::relative_path_for_storage)
                .unwrap_or_else(|_| {
                    crate::services::path_util::relative_path_for_storage(entry.path())
                });

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            let candidate = FileCandidate {
                path: entry.path().to_path_buf(),
                relative_path,
                file_name: entry.file_name().to_string_lossy().to_string(),
                file_size: metadata.len() as i64,
                mtime,
            };

            // send_blocking is OK here — this whole closure is on a blocking thread.
            if paths_tx.send_blocking(candidate).is_err() {
                break; // receiver dropped (consumer ended early)
            }
            count += 1;
        }
        drop(paths_tx);
        count
    });

    // ----- Consumer: stub-row writer + fast-hash batcher -----
    let writer_cancel = cancel.clone();
    let writer_db = db.clone();
    let writer_progress = progress_tx.clone();
    let writer = tokio::task::spawn(async move {
        let mut errors: Vec<String> = Vec::new();
        let mut buf: Vec<PhotoInsert> = Vec::with_capacity(DB_BATCH_SIZE);
        let mut files_inserted: u64 = 0;

        // Channel receives are async; the actual fast-hash + insert is blocking.
        // We do them in chunks of DB_BATCH_SIZE so the SQLite writer is hit
        // in bursts, not one row at a time.
        loop {
            if writer_cancel.load(Ordering::Relaxed) {
                break;
            }

            let Ok(candidate) = paths_rx.recv().await else {
                break;
            };

            // Fast-hash on this async task is fine — it's tiny I/O (64 KB).
            // Doing it on a tokio worker thread would be marginally faster
            // but adds spawn overhead.
            let hash = match calculate_fast_hash(
                &candidate.path,
                candidate.file_size as u64,
                candidate.mtime,
            ) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("fast-hash {}: {e}", candidate.path.display()));
                    continue;
                }
            };

            buf.push(PhotoInsert {
                relative_path: candidate.relative_path,
                file_name: candidate.file_name,
                file_hash: hash,
                file_size: candidate.file_size,
                file_mtime: candidate.mtime,
                // Everything else stays None / default. Phase 2 fills it in.
                date_taken: None,
                date_taken_source: None,
                gps_latitude: None,
                gps_longitude: None,
                location_city: None,
                location_country: None,
                camera_make: None,
                camera_model: None,
                iso: None,
                aperture: None,
                shutter_speed: None,
                focal_length: None,
                lens_model: None,
                flash: None,
                gps_altitude: None,
                width: None,
                height: None,
                orientation: 1,
            });

            if buf.len() >= DB_BATCH_SIZE {
                let inserted = flush_stub_batch(&writer_db, &mut buf, &mut errors).await;
                files_inserted += inserted;

                if files_inserted.is_multiple_of(STUB_PROGRESS_EVERY) {
                    let _ = writer_progress.try_send(ScanProgress {
                        files_found: files_inserted,
                        files_processed: files_inserted,
                        bytes_processed: 0,
                        current_directory: String::new(),
                        current_file: format!("Indexed {files_inserted} files…"),
                        errors: errors.clone(),
                        is_complete: false,
                        elapsed_seconds: start.elapsed().as_secs_f64(),
                    });
                }
            }
        }

        // Drain final batch.
        if !buf.is_empty() {
            files_inserted += flush_stub_batch(&writer_db, &mut buf, &mut errors).await;
        }
        (files_inserted, errors)
    });

    let total_walked = walker.await.map_err(|e| format!("walker join: {e}"))?;
    let (files_inserted, errors) = writer.await.map_err(|e| format!("writer join: {e}"))?;
    tracing::info!("Phase 1 done: walked {total_walked}, inserted {files_inserted}");

    let report = ScanReport {
        files_inserted,
        errors,
        elapsed_seconds: start.elapsed().as_secs_f64(),
    };

    // Emit final progress; the IPC wrapper interprets is_complete=true.
    let _ = progress_tx
        .send(ScanProgress {
            files_found: report.files_inserted,
            files_processed: report.files_inserted,
            bytes_processed: 0,
            current_directory: String::new(),
            current_file: format!("Indexed {} files", report.files_inserted),
            errors: report.errors.clone(),
            is_complete: true,
            elapsed_seconds: report.elapsed_seconds,
        })
        .await;

    Ok(report)
}

async fn flush_stub_batch(
    db: &Arc<tokio::sync::Mutex<Database>>,
    buf: &mut Vec<PhotoInsert>,
    errors: &mut Vec<String>,
) -> u64 {
    let guard = db.lock().await;
    let repo = PhotoRepo::new(&guard.conn);
    let inserted = match repo.insert_batch_stub(buf) {
        Ok(n) => n as u64,
        Err(e) => {
            errors.push(format!("stub batch insert: {e}"));
            0
        }
    };
    buf.clear();
    inserted
}

/// Check if a directory entry should be skipped
fn should_skip(entry: &DirEntry, scan_hidden_folders: bool) -> bool {
    let file_name = entry.file_name().to_string_lossy();

    if !scan_hidden_folders && file_name.starts_with('.') {
        return true;
    }

    for skip in SKIP_DIRECTORIES {
        if file_name == *skip {
            return true;
        }
    }

    false
}

/// Check if a file has a supported extension
fn is_supported_file(entry: &DirEntry) -> bool {
    entry
        .path()
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SUPPORTED_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Calculate SHA256 hash of a file (full streaming hash).
pub(crate) fn calculate_hash<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536]; // 64KB buffer

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_should_skip_hidden() {
        let temp = tempdir().unwrap();
        let hidden = temp.path().join(".hidden");
        std::fs::create_dir(&hidden).unwrap();

        let walker = WalkDir::new(temp.path()).into_iter();
        for entry in walker {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy() == ".hidden" {
                assert!(should_skip(&entry, false));
                assert!(!should_skip(&entry, true));
            }
        }
    }

    #[test]
    fn test_is_supported_file() {
        let temp = tempdir().unwrap();

        let jpg = temp.path().join("test.jpg");
        File::create(&jpg).unwrap();

        let txt = temp.path().join("test.txt");
        File::create(&txt).unwrap();

        let png = temp.path().join("test.PNG");
        File::create(&png).unwrap();

        let walker = WalkDir::new(temp.path()).into_iter();
        for entry in walker {
            let entry = entry.unwrap();
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            match name.as_str() {
                "test.jpg" => assert!(is_supported_file(&entry)),
                "test.txt" => assert!(!is_supported_file(&entry)),
                "test.PNG" => assert!(is_supported_file(&entry)),
                _ => {}
            }
        }
    }

    #[test]
    fn test_calculate_hash() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.bin");
        let mut f = File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = calculate_hash(&file_path).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_min_file_size_filter() {
        const _: () = assert!(MIN_FILE_SIZE == 10 * 1024);
    }

    #[test]
    fn test_fast_hash_changes_with_mtime() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test_mtime.bin");
        std::fs::write(&file_path, b"some content here").unwrap();

        let hash1 = calculate_fast_hash(&file_path, 100, Some(1700000000)).unwrap();
        let hash2 = calculate_fast_hash(&file_path, 100, Some(1700000001)).unwrap();
        assert_ne!(hash1, hash2, "fast hash must change when mtime changes");
    }

    #[test]
    fn test_fast_hash_changes_with_size() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test_size.bin");
        std::fs::write(&file_path, b"same prefix data").unwrap();

        let hash1 = calculate_fast_hash(&file_path, 100, Some(1700000000)).unwrap();
        let hash2 = calculate_fast_hash(&file_path, 200, Some(1700000000)).unwrap();
        assert_ne!(hash1, hash2, "fast hash must change when file size changes");
    }

    #[test]
    fn test_fast_hash_stable_on_repeat() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test_stable.bin");
        std::fs::write(&file_path, b"repeatable content").unwrap();

        let hash1 = calculate_fast_hash(&file_path, 500, Some(1700000000)).unwrap();
        let hash2 = calculate_fast_hash(&file_path, 500, Some(1700000000)).unwrap();
        assert_eq!(hash1, hash2, "fast hash must be stable on repeat calls");
    }

    #[test]
    fn test_fast_hash_is_plain_hex_digest() {
        // No `fh:` prefix — the hash is consumed directly as a path
        // shard by thumbnail + duplicate code (first two chars as
        // subdirectory). A prefix with `:` would also be illegal on
        // Windows filenames.
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test_prefix.bin");
        std::fs::write(&file_path, b"check prefix").unwrap();

        let hash = calculate_fast_hash(&file_path, 12, None).unwrap();
        assert_eq!(hash.len(), 64, "SHA-256 hex digest is 64 chars");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be pure ASCII hex"
        );
    }
}
