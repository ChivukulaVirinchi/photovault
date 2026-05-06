//! Directory scanning service
//!
//! Recursively scans directories to find supported image files,
//! extracts EXIF metadata, and stores everything in the database.
//! Uses rayon for parallel hash computation and EXIF extraction.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

use crate::db::photo_repo::{PhotoInsert, PhotoRepo};
use crate::db::Database;
use crate::services::exif_extractor::ExifExtractor;
use crate::services::GeocodingService;

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

/// A candidate file discovered during directory walk
struct FileCandidate {
    path: PathBuf,
    relative_path: String,
    file_name: String,
    file_size: i64,
    mtime: Option<i64>,
}

/// Start a scan in a background thread.
///
/// Takes ownership of the Database (moved into the scanner thread),
/// and returns it via the `ScanResult` when complete.
pub fn start_scan(
    root_path: PathBuf,
    database: Database,
    scan_hidden_folders: bool,
) -> (
    Receiver<ScanProgress>,
    Arc<AtomicBool>,
    tokio::task::JoinHandle<ScanResult>,
) {
    let (progress_tx, progress_rx) = bounded::<ScanProgress>(100);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = cancel_flag.clone();

    let handle = tokio::task::spawn_blocking(move || {
        run_scan(
            root_path,
            database,
            progress_tx,
            cancel_flag_clone,
            scan_hidden_folders,
        )
    });

    (progress_rx, cancel_flag, handle)
}

/// The actual scan logic that runs in a blocking thread.
fn run_scan(
    root_path: PathBuf,
    database: Database,
    progress_tx: Sender<ScanProgress>,
    cancel_flag: Arc<AtomicBool>,
    scan_hidden_folders: bool,
) -> ScanResult {
    let start_time = Instant::now();
    let mut errors = Vec::new();

    let repo = PhotoRepo::new(&database.conn);

    // ---- Phase 1: Collect candidate files (serial walkdir, fast) ----
    let mut candidates: Vec<FileCandidate> = Vec::new();

    let walker = WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip(e, scan_hidden_folders));

    for entry in walker {
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("Scan cancelled during discovery");
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Access error: {}", e));
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        if !is_supported_file(&entry) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("Metadata error for {:?}: {}", entry.path(), e));
                continue;
            }
        };

        if metadata.len() < MIN_FILE_SIZE {
            continue;
        }

        let relative_path = entry
            .path()
            .strip_prefix(&root_path)
            .map(crate::services::path_util::relative_path_for_storage)
            .unwrap_or_else(|_| {
                crate::services::path_util::relative_path_for_storage(entry.path())
            });

        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        candidates.push(FileCandidate {
            path: entry.path().to_path_buf(),
            relative_path,
            file_name: entry.file_name().to_string_lossy().to_string(),
            file_size: metadata.len() as i64,
            mtime,
        });
    }

    let total_found = candidates.len() as u64;

    // Send discovery progress
    let _ = progress_tx.send_blocking(ScanProgress {
        files_found: total_found,
        files_processed: 0,
        bytes_processed: 0,
        current_directory: String::new(),
        current_file: format!("Processing {} files...", total_found),
        errors: errors.clone(),
        is_complete: false,
        elapsed_seconds: start_time.elapsed().as_secs_f64(),
    });

    // ---- Phase 2: Parallel hash + EXIF extraction ----
    let processed_count = Arc::new(AtomicU64::new(0));
    let bytes_count = Arc::new(AtomicU64::new(0));

    let processed: Vec<Option<PhotoInsert>> = candidates
        .par_iter()
        .map(|candidate| {
            if cancel_flag.load(Ordering::Relaxed) {
                return None;
            }

            let hash = match calculate_hash(&candidate.path) {
                Ok(h) => h,
                Err(_) => return None,
            };

            let exif = ExifExtractor::extract(&candidate.path);

            processed_count.fetch_add(1, Ordering::Relaxed);
            bytes_count.fetch_add(candidate.file_size as u64, Ordering::Relaxed);

            Some(PhotoInsert {
                relative_path: candidate.relative_path.clone(),
                file_name: candidate.file_name.clone(),
                file_hash: hash,
                file_size: candidate.file_size,
                file_mtime: candidate.mtime,
                date_taken: exif.date_taken.map(|d| d.to_rfc3339()),
                date_taken_source: exif.date_taken_source,
                gps_latitude: exif.gps_latitude,
                gps_longitude: exif.gps_longitude,
                location_city: None, // Geocoding done in Phase 3 (serial)
                location_country: None,
                camera_make: exif.camera_make,
                camera_model: exif.camera_model,
                iso: exif.iso,
                aperture: exif.aperture,
                shutter_speed: exif.shutter_speed,
                focal_length: exif.focal_length,
                lens_model: exif.lens_model,
                flash: exif.flash,
                gps_altitude: exif.gps_altitude,
                width: exif.width.map(|v| v as i32),
                height: exif.height.map(|v| v as i32),
                orientation: exif.orientation.unwrap_or(1) as i32,
            })
        })
        .collect();

    // ---- Phase 3: Serial geocoding + batched DB insert ----
    let geonames_path = crate::db::geonames::geonames_db_path();
    let geocoder = if geonames_path.exists() {
        match GeocodingService::new(&geonames_path) {
            Ok(g) => {
                tracing::info!(
                    "Geocoding enabled (GeoNames at {})",
                    geonames_path.display()
                );
                Some(g)
            }
            Err(e) => {
                tracing::error!(
                    "GeoNames DB found at {} but failed to load: {} — geocoding disabled for this scan",
                    geonames_path.display(),
                    e
                );
                None
            }
        }
    } else {
        tracing::warn!(
            "GeoNames DB missing at {} — photos will not be reverse-geocoded",
            geonames_path.display()
        );
        None
    };

    let mut files_processed: u64 = 0;
    let mut bytes_processed: u64 = 0;
    let mut batch: Vec<PhotoInsert> = Vec::with_capacity(DB_BATCH_SIZE);

    for item in processed.into_iter().flatten() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let mut photo = item;

        // Reverse-geocode GPS coordinates (~1ms per lookup)
        if let (Some(lat), Some(lon), Some(geo)) =
            (photo.gps_latitude, photo.gps_longitude, geocoder.as_ref())
        {
            if let Some(result) = geo.reverse_geocode(lat, lon) {
                photo.location_city = Some(result.city);
                photo.location_country = Some(result.country);
            }
        }

        bytes_processed += photo.file_size as u64;
        batch.push(photo);

        if batch.len() >= DB_BATCH_SIZE {
            match repo.insert_batch(&batch) {
                Ok(_) => {}
                Err(e) => {
                    errors.push(format!("Batch insert error: {}", e));
                }
            }
            files_processed += batch.len() as u64;
            batch.clear();

            // Send progress
            if files_processed % 50 == 0 || files_processed <= 5 {
                let _ = progress_tx.send_blocking(ScanProgress {
                    files_found: total_found,
                    files_processed,
                    bytes_processed,
                    current_directory: String::new(),
                    current_file: String::new(),
                    errors: errors.clone(),
                    is_complete: false,
                    elapsed_seconds: start_time.elapsed().as_secs_f64(),
                });
            }
        }
    }

    // Flush remaining batch
    if !batch.is_empty() {
        if let Err(e) = repo.insert_batch(&batch) {
            errors.push(format!("Final batch insert error: {}", e));
        }
        files_processed += batch.len() as u64;
    }

    let final_progress = ScanProgress {
        files_found: total_found,
        files_processed,
        bytes_processed,
        current_directory: String::new(),
        current_file: String::new(),
        errors,
        is_complete: true,
        elapsed_seconds: start_time.elapsed().as_secs_f64(),
    };

    let _ = progress_tx.send_blocking(final_progress.clone());

    tracing::info!(
        "Scan complete: {} files in {:.2}s",
        total_found,
        start_time.elapsed().as_secs_f64()
    );

    ScanResult {
        database,
        final_progress,
    }
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
}
