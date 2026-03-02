//! Directory scanning service
//!
//! Recursively scans directories to find supported image files,
//! extracts EXIF metadata, and stores everything in the database.
//! Runs in a background thread with progress reporting.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

use crate::db::photo_repo::{PhotoInsert, PhotoRepo};
use crate::db::Database;
use crate::services::exif_extractor::ExifExtractor;

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

/// Start a scan in a background thread.
///
/// Takes ownership of the Database (moved into the scanner thread),
/// and returns it via the `ScanResult` when complete.
///
/// Returns:
/// - A `Receiver<ScanProgress>` for progress updates
/// - An `Arc<AtomicBool>` cancel flag
/// - A `tokio::task::JoinHandle<ScanResult>` to await the result
pub fn start_scan(
    root_path: PathBuf,
    database: Database,
) -> (
    Receiver<ScanProgress>,
    Arc<AtomicBool>,
    tokio::task::JoinHandle<ScanResult>,
) {
    let (progress_tx, progress_rx) = bounded::<ScanProgress>(100);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = cancel_flag.clone();

    let handle = tokio::task::spawn_blocking(move || {
        run_scan(root_path, database, progress_tx, cancel_flag_clone)
    });

    (progress_rx, cancel_flag, handle)
}

/// The actual scan logic that runs in a blocking thread.
fn run_scan(
    root_path: PathBuf,
    database: Database,
    progress_tx: Sender<ScanProgress>,
    cancel_flag: Arc<AtomicBool>,
) -> ScanResult {
    let start_time = Instant::now();
    let mut errors = Vec::new();
    let mut current_dir = String::new();
    let mut files_found: u64 = 0;
    let mut files_processed: u64 = 0;
    let mut bytes_processed: u64 = 0;
    let mut batch: Vec<PhotoInsert> = Vec::with_capacity(DB_BATCH_SIZE);

    let repo = PhotoRepo::new(&database.conn);

    // Walk the directory tree
    let walker = WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip(e));

    for entry in walker {
        // Check for cancellation
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("Scan cancelled by user");
            // Flush remaining batch before exit
            if !batch.is_empty() {
                if let Err(e) = repo.insert_batch(&batch) {
                    errors.push(format!("Batch insert error: {}", e));
                }
            }
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Access error: {}", e));
                continue;
            }
        };

        // Update current directory for progress
        if entry.file_type().is_dir() {
            current_dir = entry
                .path()
                .strip_prefix(&root_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| entry.path().to_string_lossy().to_string());
            continue;
        }

        // Skip non-files
        if !entry.file_type().is_file() {
            continue;
        }

        // Check extension
        if !is_supported_file(&entry) {
            continue;
        }

        // Get file metadata
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("Metadata error for {:?}: {}", entry.path(), e));
                continue;
            }
        };

        // Skip small files (likely thumbnails)
        if metadata.len() < MIN_FILE_SIZE {
            continue;
        }

        files_found += 1;

        // Calculate hash
        let hash = match calculate_hash(entry.path()) {
            Ok(h) => h,
            Err(e) => {
                errors.push(format!("Hash error for {:?}: {}", entry.path(), e));
                continue;
            }
        };

        // Get relative path
        let relative_path = entry
            .path()
            .strip_prefix(&root_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| entry.path().to_string_lossy().to_string());

        // Get modification time
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        // Extract EXIF metadata
        let exif = ExifExtractor::extract(entry.path());

        let photo_insert = PhotoInsert {
            relative_path,
            file_name: entry.file_name().to_string_lossy().to_string(),
            file_hash: hash,
            file_size: metadata.len() as i64,
            file_mtime: mtime,
            date_taken: exif.date_taken.map(|d| d.to_rfc3339()),
            date_taken_source: exif.date_taken_source,
            gps_latitude: exif.gps_latitude,
            gps_longitude: exif.gps_longitude,
            camera_make: exif.camera_make,
            camera_model: exif.camera_model,
            width: exif.width.map(|v| v as i32),
            height: exif.height.map(|v| v as i32),
            orientation: exif.orientation.unwrap_or(1) as i32,
        };

        batch.push(photo_insert);

        // Flush batch when full
        if batch.len() >= DB_BATCH_SIZE {
            match repo.insert_batch(&batch) {
                Ok(_) => {}
                Err(e) => {
                    errors.push(format!("Batch insert error: {}", e));
                }
            }
            batch.clear();
        }

        files_processed += 1;
        bytes_processed += metadata.len();

        // Send progress update periodically (every 50 files or every file if < 50 total)
        if files_processed % 50 == 0 || files_processed <= 5 {
            let progress = ScanProgress {
                files_found,
                files_processed,
                bytes_processed,
                current_directory: current_dir.clone(),
                current_file: entry.file_name().to_string_lossy().to_string(),
                errors: errors.clone(),
                is_complete: false,
                elapsed_seconds: start_time.elapsed().as_secs_f64(),
            };
            let _ = progress_tx.send_blocking(progress);
        }
    }

    // Flush remaining batch
    if !batch.is_empty() {
        if let Err(e) = repo.insert_batch(&batch) {
            errors.push(format!("Final batch insert error: {}", e));
        }
    }

    let final_progress = ScanProgress {
        files_found,
        files_processed,
        bytes_processed,
        current_directory: String::new(),
        current_file: String::new(),
        errors,
        is_complete: true,
        elapsed_seconds: start_time.elapsed().as_secs_f64(),
    };

    // Send final progress
    let _ = progress_tx.send_blocking(final_progress.clone());

    tracing::info!(
        "Scan complete: {} files in {:.2}s",
        files_found,
        start_time.elapsed().as_secs_f64()
    );

    ScanResult {
        database,
        final_progress,
    }
}

/// Check if a directory entry should be skipped
fn should_skip(entry: &DirEntry) -> bool {
    let file_name = entry.file_name().to_string_lossy();

    // Skip hidden files/directories (starting with .)
    if file_name.starts_with('.') {
        return true;
    }

    // Skip known system directories
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

/// Calculate SHA256 hash of a file
fn calculate_hash<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
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
                assert!(should_skip(&entry));
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
                "test.PNG" => assert!(is_supported_file(&entry)), // case insensitive
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
        // SHA256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_min_file_size_filter() {
        // Files under 10KB should be skipped
        assert!(MIN_FILE_SIZE == 10 * 1024);
    }
}
