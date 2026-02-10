# Phase 2: Directory Scanning & EXIF Extraction

## Overview

This phase implements the core scanning pipeline: discovering photos on a drive, extracting EXIF metadata, and storing everything in the database. By the end, users can scan a folder and see all their photos indexed with metadata.

**Estimated Time:** 3-4 days  
**Difficulty:** Intermediate  
**Prerequisites:** Phase 1 complete

---

## UI Design Guidelines

> **IMPORTANT:** When implementing any UI components in this phase, you MUST read and follow the design principles in `SKILL.md`. This file contains critical guidelines for:
> - Typography and spacing standards
> - Color usage and contrast requirements
> - Animation and interaction patterns
> - Component design principles
> - Accessibility requirements
>
> **Before writing ANY UI code, read SKILL.md thoroughly.** The goal is a delightful, polished user experience - not just functional code.

---

## Goals

- [ ] Implement recursive directory scanning
- [ ] Support JPEG, PNG, HEIC, WebP formats
- [ ] Extract EXIF metadata (date, GPS, camera info)
- [ ] Parse dates from filenames as fallback
- [ ] Calculate SHA256 hashes for duplicate detection
- [ ] Show real-time scan progress in UI
- [ ] Store all data in SQLite database
- [ ] Handle errors gracefully (corrupted files, permissions)

---

## New Files

```
src/
├── services/
│   ├── scanner.rs          # Directory scanning service
│   └── exif_extractor.rs   # EXIF metadata extraction
├── components/
│   └── scan_progress.rs    # Progress indicator component
└── views/
    └── timeline.rs         # Updated with photo display
```

---

## Step 1: Add Dependencies

Update `Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...

# EXIF extraction
kamadak-exif = "0.5"

# Parallel processing
rayon = "1.10"

# Additional async utilities
futures = "0.3"
async-channel = "2.0"

# Regex for filename parsing
regex = "1"
lazy_static = "1.4"
```

---

## Step 2: Scanner Service

### File: `src/services/scanner.rs`

```rust
//! Directory scanning service
//!
//! Recursively scans directories to find supported image files.
//! Runs in a background thread with progress reporting.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

/// Supported image extensions
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp",
];

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

/// A discovered file ready for processing
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub relative_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub file_mtime: Option<i64>,
    pub file_hash: String,
}

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

/// Scanner state
pub struct Scanner {
    root_path: PathBuf,
    files_found: Arc<AtomicU64>,
    files_processed: Arc<AtomicU64>,
    bytes_processed: Arc<AtomicU64>,
    cancel_flag: Arc<AtomicBool>,
    start_time: Option<Instant>,
}

impl Scanner {
    /// Create a new scanner for a directory
    pub fn new<P: AsRef<Path>>(root_path: P) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
            files_found: Arc::new(AtomicU64::new(0)),
            files_processed: Arc::new(AtomicU64::new(0)),
            bytes_processed: Arc::new(AtomicU64::new(0)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            start_time: None,
        }
    }

    /// Start scanning and return a channel of discovered files
    ///
    /// Returns a receiver that yields DiscoveredFile as they're found,
    /// and a receiver for progress updates.
    pub async fn scan(
        &mut self,
    ) -> (
        Receiver<DiscoveredFile>,
        Receiver<ScanProgress>,
    ) {
        let (file_tx, file_rx) = bounded::<DiscoveredFile>(1000);
        let (progress_tx, progress_rx) = bounded::<ScanProgress>(100);

        let root_path = self.root_path.clone();
        let files_found = self.files_found.clone();
        let files_processed = self.files_processed.clone();
        let bytes_processed = self.bytes_processed.clone();
        let cancel_flag = self.cancel_flag.clone();

        // Spawn the scanning task
        tokio::task::spawn_blocking(move || {
            let start_time = Instant::now();
            let mut errors = Vec::new();
            let mut current_dir = String::new();

            // First pass: discover all files
            let walker = WalkDir::new(&root_path)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| !Self::should_skip(e));

            for entry in walker {
                // Check for cancellation
                if cancel_flag.load(Ordering::Relaxed) {
                    tracing::info!("Scan cancelled by user");
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
                    current_dir = entry.path().display().to_string();
                    continue;
                }

                // Skip non-files
                if !entry.file_type().is_file() {
                    continue;
                }

                // Check extension
                if !Self::is_supported_file(&entry) {
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

                files_found.fetch_add(1, Ordering::Relaxed);

                // Calculate hash
                let hash = match Self::calculate_hash(entry.path()) {
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

                let discovered = DiscoveredFile {
                    path: entry.path().to_path_buf(),
                    relative_path,
                    file_name: entry.file_name().to_string_lossy().to_string(),
                    file_size: metadata.len(),
                    file_mtime: mtime,
                    file_hash: hash,
                };

                // Send to channel
                if file_tx.send_blocking(discovered).is_err() {
                    break;
                }

                files_processed.fetch_add(1, Ordering::Relaxed);
                bytes_processed.fetch_add(metadata.len(), Ordering::Relaxed);

                // Send progress update periodically
                let processed = files_processed.load(Ordering::Relaxed);
                if processed % 100 == 0 {
                    let progress = ScanProgress {
                        files_found: files_found.load(Ordering::Relaxed),
                        files_processed: processed,
                        bytes_processed: bytes_processed.load(Ordering::Relaxed),
                        current_directory: current_dir.clone(),
                        current_file: entry.file_name().to_string_lossy().to_string(),
                        errors: errors.clone(),
                        is_complete: false,
                        elapsed_seconds: start_time.elapsed().as_secs_f64(),
                    };
                    let _ = progress_tx.send_blocking(progress);
                }
            }

            // Send final progress
            let final_progress = ScanProgress {
                files_found: files_found.load(Ordering::Relaxed),
                files_processed: files_processed.load(Ordering::Relaxed),
                bytes_processed: bytes_processed.load(Ordering::Relaxed),
                current_directory: String::new(),
                current_file: String::new(),
                errors,
                is_complete: true,
                elapsed_seconds: start_time.elapsed().as_secs_f64(),
            };
            let _ = progress_tx.send_blocking(final_progress);

            tracing::info!(
                "Scan complete: {} files in {:.2}s",
                files_found.load(Ordering::Relaxed),
                start_time.elapsed().as_secs_f64()
            );
        });

        (file_rx, progress_rx)
    }

    /// Cancel an ongoing scan
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    /// Check if a directory entry should be skipped
    fn should_skip(entry: &DirEntry) -> bool {
        // Skip hidden files/directories (starting with .)
        let file_name = entry.file_name().to_string_lossy();
        
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[tokio::test]
    async fn test_scanner_finds_images() {
        let temp = tempdir().unwrap();
        
        // Create test files
        let jpg_path = temp.path().join("test.jpg");
        let mut jpg = File::create(&jpg_path).unwrap();
        // Write enough bytes to pass min size check
        jpg.write_all(&vec![0u8; 15000]).unwrap();
        
        let txt_path = temp.path().join("test.txt");
        File::create(&txt_path).unwrap();
        
        let mut scanner = Scanner::new(temp.path());
        let (file_rx, _progress_rx) = scanner.scan().await;
        
        let mut found_files = Vec::new();
        while let Ok(file) = file_rx.recv().await {
            found_files.push(file);
        }
        
        assert_eq!(found_files.len(), 1);
        assert!(found_files[0].file_name.ends_with(".jpg"));
    }
}
```

---

## Step 3: EXIF Extractor

### File: `src/services/exif_extractor.rs`

```rust
//! EXIF metadata extraction service
//!
//! Extracts date, GPS, camera info from image files.
//! Falls back to filename parsing and file mtime when EXIF unavailable.

use std::path::Path;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use kamadak_exif::{In, Reader as ExifReader, Tag, Value};
use lazy_static::lazy_static;
use regex::Regex;

/// Extracted metadata from an image
#[derive(Debug, Clone, Default)]
pub struct ImageMetadata {
    // Date information
    pub date_taken: Option<DateTime<Utc>>,
    pub date_taken_source: Option<String>, // "exif", "filename", "mtime"

    // GPS information
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,

    // Camera information
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,

    // Image dimensions
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub orientation: Option<u16>,
}

/// EXIF extractor service
pub struct ExifExtractor;

impl ExifExtractor {
    /// Extract metadata from an image file
    pub fn extract<P: AsRef<Path>>(path: P) -> ImageMetadata {
        let path = path.as_ref();
        let mut metadata = ImageMetadata::default();

        // Try EXIF extraction
        if let Some(exif_data) = Self::extract_exif(path) {
            metadata = exif_data;
        }

        // If no date from EXIF, try filename
        if metadata.date_taken.is_none() {
            if let Some(date) = Self::parse_date_from_filename(path) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("filename".to_string());
            }
        }

        // If still no date, use file mtime
        if metadata.date_taken.is_none() {
            if let Some(date) = Self::get_file_mtime(path) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("mtime".to_string());
            }
        }

        // Get image dimensions if not from EXIF
        if metadata.width.is_none() || metadata.height.is_none() {
            if let Some((w, h)) = Self::get_image_dimensions(path) {
                metadata.width = Some(w);
                metadata.height = Some(h);
            }
        }

        metadata
    }

    /// Extract EXIF data from a file
    fn extract_exif<P: AsRef<Path>>(path: P) -> Option<ImageMetadata> {
        let file = std::fs::File::open(path.as_ref()).ok()?;
        let mut bufreader = std::io::BufReader::new(&file);
        let exif = ExifReader::new().read_from_container(&mut bufreader).ok()?;

        let mut metadata = ImageMetadata::default();

        // Date taken
        if let Some(field) = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY) {
            if let Some(date) = Self::parse_exif_date(&field.display_value().to_string()) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("exif".to_string());
            }
        } else if let Some(field) = exif.get_field(Tag::DateTime, In::PRIMARY) {
            if let Some(date) = Self::parse_exif_date(&field.display_value().to_string()) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("exif".to_string());
            }
        }

        // GPS coordinates
        metadata.gps_latitude = Self::extract_gps_coord(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef);
        metadata.gps_longitude = Self::extract_gps_coord(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef);

        // Camera make
        if let Some(field) = exif.get_field(Tag::Make, In::PRIMARY) {
            metadata.camera_make = Some(field.display_value().to_string().trim_matches('"').to_string());
        }

        // Camera model
        if let Some(field) = exif.get_field(Tag::Model, In::PRIMARY) {
            metadata.camera_model = Some(field.display_value().to_string().trim_matches('"').to_string());
        }

        // Image dimensions
        if let Some(field) = exif.get_field(Tag::PixelXDimension, In::PRIMARY) {
            if let Value::Long(ref vec) = field.value {
                if let Some(&w) = vec.first() {
                    metadata.width = Some(w);
                }
            }
        }
        if let Some(field) = exif.get_field(Tag::PixelYDimension, In::PRIMARY) {
            if let Value::Long(ref vec) = field.value {
                if let Some(&h) = vec.first() {
                    metadata.height = Some(h);
                }
            }
        }

        // Orientation
        if let Some(field) = exif.get_field(Tag::Orientation, In::PRIMARY) {
            if let Value::Short(ref vec) = field.value {
                if let Some(&o) = vec.first() {
                    metadata.orientation = Some(o);
                }
            }
        }

        Some(metadata)
    }

    /// Extract GPS coordinate from EXIF
    fn extract_gps_coord(
        exif: &kamadak_exif::Exif,
        coord_tag: Tag,
        ref_tag: Tag,
    ) -> Option<f64> {
        let field = exif.get_field(coord_tag, In::PRIMARY)?;
        let ref_field = exif.get_field(ref_tag, In::PRIMARY)?;

        // Parse coordinate value (degrees, minutes, seconds)
        let coord = match &field.value {
            Value::Rational(rationals) if rationals.len() >= 3 => {
                let degrees = rationals[0].to_f64();
                let minutes = rationals[1].to_f64();
                let seconds = rationals[2].to_f64();
                degrees + minutes / 60.0 + seconds / 3600.0
            }
            _ => return None,
        };

        // Apply reference (N/S or E/W)
        let ref_str = ref_field.display_value().to_string();
        let multiplier = if ref_str.contains('S') || ref_str.contains('W') {
            -1.0
        } else {
            1.0
        };

        Some(coord * multiplier)
    }

    /// Parse EXIF date format: "2019:03:15 14:30:22"
    fn parse_exif_date(date_str: &str) -> Option<DateTime<Utc>> {
        // Remove quotes if present
        let clean = date_str.trim_matches('"');
        
        // Parse "YYYY:MM:DD HH:MM:SS" format
        let parsed = NaiveDateTime::parse_from_str(clean, "%Y:%m:%d %H:%M:%S").ok()?;
        Some(Utc.from_utc_datetime(&parsed))
    }

    /// Parse date from filename patterns
    fn parse_date_from_filename<P: AsRef<Path>>(path: P) -> Option<DateTime<Utc>> {
        lazy_static! {
            // Common filename date patterns
            static ref PATTERNS: Vec<(Regex, &'static str)> = vec![
                // IMG_20190315_143022.jpg
                (Regex::new(r"IMG_(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // 20190315_143022.jpg
                (Regex::new(r"(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // 2019-03-15 14.30.22.jpg
                (Regex::new(r"(\d{4}-\d{2}-\d{2}) (\d{2}\.\d{2}\.\d{2})").unwrap(), "%Y-%m-%d%H.%M.%S"),
                // Screenshot_20190315-143022.png
                (Regex::new(r"Screenshot_(\d{8})-(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // VID_20190315_143022.mp4
                (Regex::new(r"VID_(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // PXL_20190315_143022.jpg (Pixel phones)
                (Regex::new(r"PXL_(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // Just date: 20190315.jpg
                (Regex::new(r"(\d{8})\.").unwrap(), "%Y%m%d"),
            ];
        }

        let filename = path.as_ref().file_name()?.to_str()?;

        for (regex, format) in PATTERNS.iter() {
            if let Some(caps) = regex.captures(filename) {
                // Combine captured groups
                let date_str: String = caps
                    .iter()
                    .skip(1)
                    .filter_map(|m| m.map(|m| m.as_str()))
                    .collect();

                // Try to parse
                if let Ok(parsed) = NaiveDateTime::parse_from_str(&date_str, format) {
                    return Some(Utc.from_utc_datetime(&parsed));
                }
                
                // Try date only format
                if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&date_str, "%Y%m%d") {
                    let datetime = parsed.and_hms_opt(0, 0, 0)?;
                    return Some(Utc.from_utc_datetime(&datetime));
                }
            }
        }

        None
    }

    /// Get file modification time
    fn get_file_mtime<P: AsRef<Path>>(path: P) -> Option<DateTime<Utc>> {
        let metadata = std::fs::metadata(path.as_ref()).ok()?;
        let mtime = metadata.modified().ok()?;
        let duration = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
        
        DateTime::from_timestamp(duration.as_secs() as i64, 0)
    }

    /// Get image dimensions by decoding
    fn get_image_dimensions<P: AsRef<Path>>(path: P) -> Option<(u32, u32)> {
        // Use image crate for dimensions - but only read header, not full image
        let reader = image::io::Reader::open(path.as_ref()).ok()?;
        let (w, h) = reader.into_dimensions().ok()?;
        Some((w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename_date_parsing() {
        let path = std::path::Path::new("IMG_20190315_143022.jpg");
        let date = ExifExtractor::parse_date_from_filename(path);
        assert!(date.is_some());
        
        let dt = date.unwrap();
        assert_eq!(dt.year(), 2019);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn test_exif_date_parsing() {
        let date = ExifExtractor::parse_exif_date("2019:03:15 14:30:22");
        assert!(date.is_some());
        
        let dt = date.unwrap();
        assert_eq!(dt.year(), 2019);
    }
}
```

---

## Step 4: Update Services Module

### File: `src/services/mod.rs`

```rust
//! Application services

pub mod drive_detector;
pub mod scanner;
pub mod exif_extractor;

pub use drive_detector::{DriveDetector, DriveInfo};
pub use scanner::{Scanner, DiscoveredFile, ScanProgress};
pub use exif_extractor::{ExifExtractor, ImageMetadata};
```

---

## Step 5: Photo Repository

### File: `src/db/photo_repo.rs`

```rust
//! Photo database repository
//!
//! Handles all database operations for photos.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};

use crate::models::Photo;
use crate::services::{DiscoveredFile, ImageMetadata};

/// Photo repository for database operations
pub struct PhotoRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PhotoRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert a new photo with its metadata
    pub fn insert(
        &self,
        file: &DiscoveredFile,
        metadata: &ImageMetadata,
    ) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO photos (
                file_path, file_name, file_hash, file_size, file_mtime,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                camera_make, camera_model,
                width, height, orientation
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7,
                ?8, ?9,
                ?10, ?11,
                ?12, ?13, ?14
            )
            ON CONFLICT(file_path) DO UPDATE SET
                file_hash = excluded.file_hash,
                file_size = excluded.file_size,
                file_mtime = excluded.file_mtime,
                date_taken = excluded.date_taken,
                date_taken_source = excluded.date_taken_source,
                gps_latitude = excluded.gps_latitude,
                gps_longitude = excluded.gps_longitude,
                camera_make = excluded.camera_make,
                camera_model = excluded.camera_model,
                width = excluded.width,
                height = excluded.height,
                orientation = excluded.orientation,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![
                file.relative_path,
                file.file_name,
                file.file_hash,
                file.file_size as i64,
                file.file_mtime,
                metadata.date_taken.map(|d| d.to_rfc3339()),
                metadata.date_taken_source,
                metadata.gps_latitude,
                metadata.gps_longitude,
                metadata.camera_make,
                metadata.camera_model,
                metadata.width.map(|v| v as i32),
                metadata.height.map(|v| v as i32),
                metadata.orientation.unwrap_or(1) as i32,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get total photo count
    pub fn count(&self) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
            [],
            |row| row.get(0),
        )
    }

    /// Get photos for a specific date range
    pub fn get_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> SqliteResult<Vec<Photo>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                id, file_path, file_name, file_hash, file_size,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                location_city, location_country,
                camera_make, camera_model,
                width, height, orientation,
                thumbnail_path, faces_processed,
                is_trashed, trashed_at,
                indexed_at, updated_at
            FROM photos
            WHERE date_taken BETWEEN ?1 AND ?2
              AND is_trashed = FALSE
            ORDER BY date_taken DESC
            "#,
        )?;

        let rows = stmt.query_map(
            params![start.to_rfc3339(), end.to_rfc3339()],
            Self::row_to_photo,
        )?;

        let mut photos = Vec::new();
        for row in rows {
            photos.push(row?);
        }

        Ok(photos)
    }

    /// Get all photos ordered by date
    pub fn get_all_by_date(&self, limit: i64, offset: i64) -> SqliteResult<Vec<Photo>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                id, file_path, file_name, file_hash, file_size,
                date_taken, date_taken_source,
                gps_latitude, gps_longitude,
                location_city, location_country,
                camera_make, camera_model,
                width, height, orientation,
                thumbnail_path, faces_processed,
                is_trashed, trashed_at,
                indexed_at, updated_at
            FROM photos
            WHERE is_trashed = FALSE
            ORDER BY date_taken DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit, offset], Self::row_to_photo)?;

        let mut photos = Vec::new();
        for row in rows {
            photos.push(row?);
        }

        Ok(photos)
    }

    /// Get photos grouped by date (for timeline)
    pub fn get_dates_with_counts(&self) -> SqliteResult<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                DATE(date_taken) as photo_date,
                COUNT(*) as photo_count
            FROM photos
            WHERE date_taken IS NOT NULL
              AND is_trashed = FALSE
            GROUP BY DATE(date_taken)
            ORDER BY photo_date DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut dates = Vec::new();
        for row in rows {
            dates.push(row?);
        }

        Ok(dates)
    }

    /// Check if a file hash already exists (for duplicate detection)
    pub fn hash_exists(&self, hash: &str) -> SqliteResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE file_hash = ?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Convert a database row to a Photo struct
    fn row_to_photo(row: &rusqlite::Row) -> SqliteResult<Photo> {
        Ok(Photo {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_hash: row.get(3)?,
            file_size: row.get(4)?,
            date_taken: row.get::<_, Option<String>>(5)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            date_taken_source: row.get(6)?,
            gps_latitude: row.get(7)?,
            gps_longitude: row.get(8)?,
            location_city: row.get(9)?,
            location_country: row.get(10)?,
            camera_make: row.get(11)?,
            camera_model: row.get(12)?,
            width: row.get(13)?,
            height: row.get(14)?,
            orientation: row.get::<_, Option<i32>>(15)?.unwrap_or(1),
            thumbnail_path: row.get(16)?,
            faces_processed: row.get(17)?,
            is_trashed: row.get(18)?,
            trashed_at: row.get::<_, Option<String>>(19)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            indexed_at: row.get::<_, String>(20)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            updated_at: row.get::<_, String>(21)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}
```

Update `src/db/mod.rs`:

```rust
//! Database module for PhotoVault

pub mod connection;
pub mod schema;
pub mod migrations;
pub mod photo_repo;

pub use connection::Database;
pub use schema::create_schema;
pub use photo_repo::PhotoRepo;
```

---

## Step 6: Scan Progress UI Component

### File: `src/components/scan_progress.rs`

```rust
//! Scan progress indicator component
//!
//! Shows a beautiful progress display during scanning.

use iced::widget::{button, column, container, progress_bar, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::colors::{Accent, Backgrounds, Border, Semantic, Text};
use crate::services::ScanProgress;
use crate::app::Message;

/// Format bytes as human-readable size
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Scan progress component
pub struct ScanProgressView;

impl ScanProgressView {
    /// Render the scan progress view
    pub fn view(progress: &ScanProgress) -> Element<'static, Message> {
        let title = text(if progress.is_complete {
            "Scan Complete"
        } else {
            "Scanning..."
        })
        .size(24)
        .color(Text::PRIMARY);

        // Progress stats
        let stats = row![
            Self::stat_item("Files Found", &progress.files_found.to_string()),
            Space::with_width(32),
            Self::stat_item("Processed", &progress.files_processed.to_string()),
            Space::with_width(32),
            Self::stat_item("Size", &format_bytes(progress.bytes_processed)),
        ]
        .align_y(Alignment::Center);

        // Progress bar (estimate based on files found vs processed)
        let progress_value = if progress.files_found > 0 {
            progress.files_processed as f32 / progress.files_found as f32
        } else {
            0.0
        };

        let bar = progress_bar(0.0..=1.0, progress_value)
            .width(Length::Fixed(400.0))
            .height(Length::Fixed(8.0));

        // Current file/directory
        let current = if !progress.is_complete {
            text(&progress.current_file)
                .size(12)
                .color(Text::TERTIARY)
        } else {
            text(format!(
                "Completed in {:.1} seconds",
                progress.elapsed_seconds
            ))
            .size(12)
            .color(Semantic::SUCCESS)
        };

        // Errors summary
        let errors = if !progress.errors.is_empty() {
            let error_count = progress.errors.len();
            Some(
                text(format!("{} errors encountered", error_count))
                    .size(12)
                    .color(Semantic::WARNING),
            )
        } else {
            None
        };

        // Cancel/Done button
        let action_button = if progress.is_complete {
            button(text("Continue").size(14).color(Text::PRIMARY))
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::HOVER.into()),
                        _ => Some(Accent::PRIMARY.into()),
                    };
                    button::Style {
                        background,
                        text_color: Backgrounds::PRIMARY,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::ScanComplete)
        } else {
            button(text("Cancel").size(14).color(Text::SECONDARY))
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    };
                    button::Style {
                        background,
                        text_color: Text::SECONDARY,
                        border: iced::Border {
                            color: Border::VISIBLE,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::CancelScan)
        };

        // Assemble the layout
        let mut content = column![
            title,
            Space::with_height(24),
            stats,
            Space::with_height(16),
            bar,
            Space::with_height(8),
            current,
        ]
        .spacing(0)
        .align_x(Alignment::Center);

        if let Some(err_text) = errors {
            content = content.push(Space::with_height(8));
            content = content.push(err_text);
        }

        content = content.push(Space::with_height(24));
        content = content.push(action_button);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single stat item
    fn stat_item<'a>(label: &str, value: &str) -> Element<'a, Message> {
        column![
            text(value)
                .size(28)
                .color(Text::PRIMARY),
            text(label)
                .size(12)
                .color(Text::SECONDARY),
        ]
        .align_x(Alignment::Center)
        .into()
    }
}
```

Update `src/components/mod.rs`:

```rust
//! Reusable UI components

pub mod sidebar;
pub mod drive_picker;
pub mod scan_progress;

pub use sidebar::Sidebar;
pub use drive_picker::DrivePicker;
pub use scan_progress::ScanProgressView;
```

---

## Step 7: Update Application State

### File: `src/app.rs` (Updated)

```rust
//! Main application state and logic

use std::path::PathBuf;
use std::sync::Arc;

use async_channel::Receiver;
use iced::widget::{column, container, row, Space};
use iced::{Element, Length, Subscription, Task};
use tokio::sync::Mutex;

use crate::components::{Sidebar, ScanProgressView};
use crate::db::{create_schema, Database, PhotoRepo};
use crate::services::{
    DriveDetector, DriveInfo, DiscoveredFile, ExifExtractor, ScanProgress, Scanner,
};
use crate::theme::colors::Backgrounds;
use crate::views::{PeopleView, SearchView, SettingsView, TimelineView, WelcomeView};

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Scanning,
    Timeline,
    People,
    Search,
    Settings,
}

/// Scanning state
pub struct ScanState {
    pub scanner: Scanner,
    pub progress: ScanProgress,
    pub file_receiver: Option<Receiver<DiscoveredFile>>,
    pub progress_receiver: Option<Receiver<ScanProgress>>,
}

/// Application state
pub struct PhotoVault {
    /// Current active view
    current_view: View,

    /// Detected drives
    drives: Vec<DriveInfo>,

    /// Currently selected drive path
    selected_drive: Option<PathBuf>,

    /// Database connection (if a drive is selected)
    database: Option<Arc<Mutex<Database>>>,

    /// Scanning state
    scan_state: Option<ScanState>,

    /// Photo count after indexing
    photo_count: i64,
}

/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Navigate to a different view
    NavigateTo(View),

    /// Select a drive to index
    SelectDrive(PathBuf),

    /// Open folder browser dialog
    BrowseForFolder,

    /// Folder selected from browser
    FolderSelected(Option<PathBuf>),

    /// Refresh drive list
    RefreshDrives,

    /// Drives detected
    DrivesDetected(Vec<DriveInfo>),

    /// Start scanning
    StartScan,

    /// Received a discovered file during scan
    FileDiscovered(DiscoveredFile),

    /// Scan progress update
    ScanProgressUpdate(ScanProgress),

    /// Cancel ongoing scan
    CancelScan,

    /// Scan complete, continue to timeline
    ScanComplete,

    /// Tick for subscription polling
    Tick,
}

impl PhotoVault {
    /// Create new application instance
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            current_view: View::Welcome,
            drives: Vec::new(),
            selected_drive: None,
            database: None,
            scan_state: None,
            photo_count: 0,
        };

        // Detect drives on startup
        let task = Task::perform(
            async { DriveDetector::detect() },
            Message::DrivesDetected,
        );

        (app, task)
    }

    /// Application title
    pub fn title(&self) -> String {
        match &self.selected_drive {
            Some(path) => format!("PhotoVault - {}", path.display()),
            None => "PhotoVault".to_string(),
        }
    }

    /// Handle messages
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NavigateTo(view) => {
                self.current_view = view;
                Task::none()
            }

            Message::SelectDrive(path) => {
                tracing::info!("Selected drive: {:?}", path);

                match Database::open_for_drive(&path) {
                    Ok(db) => {
                        // Create schema if needed
                        if db.needs_schema().unwrap_or(true) {
                            if let Err(e) = create_schema(&db.conn) {
                                tracing::error!("Failed to create schema: {}", e);
                                return Task::none();
                            }
                        }

                        // Get photo count
                        let repo = PhotoRepo::new(&db.conn);
                        self.photo_count = repo.count().unwrap_or(0);

                        self.selected_drive = Some(path);
                        self.database = Some(Arc::new(Mutex::new(db)));

                        // If library is empty, start scanning
                        if self.photo_count == 0 {
                            return self.update(Message::StartScan);
                        } else {
                            self.current_view = View::Timeline;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database: {}", e);
                    }
                }

                Task::none()
            }

            Message::BrowseForFolder => {
                // TODO: Implement native folder picker using rfd crate
                tracing::info!("Browse for folder requested");
                Task::none()
            }

            Message::FolderSelected(path) => {
                if let Some(path) = path {
                    return self.update(Message::SelectDrive(path));
                }
                Task::none()
            }

            Message::RefreshDrives => Task::perform(
                async { DriveDetector::detect() },
                Message::DrivesDetected,
            ),

            Message::DrivesDetected(drives) => {
                tracing::info!("Detected {} drives", drives.len());
                self.drives = drives;
                Task::none()
            }

            Message::StartScan => {
                let Some(drive_path) = &self.selected_drive else {
                    return Task::none();
                };

                tracing::info!("Starting scan of {:?}", drive_path);
                self.current_view = View::Scanning;

                let drive_path = drive_path.clone();

                Task::perform(
                    async move {
                        let mut scanner = Scanner::new(&drive_path);
                        let (file_rx, progress_rx) = scanner.scan().await;
                        (file_rx, progress_rx)
                    },
                    |_| Message::Tick, // Trigger first tick to start polling
                )
            }

            Message::FileDiscovered(file) => {
                // Process the file and store in database
                if let Some(db) = &self.database {
                    let db = db.clone();
                    let file_path = file.path.clone();

                    Task::perform(
                        async move {
                            // Extract EXIF
                            let metadata = ExifExtractor::extract(&file_path);

                            // Store in database
                            let db_guard = db.lock().await;
                            let repo = PhotoRepo::new(&db_guard.conn);
                            if let Err(e) = repo.insert(&file, &metadata) {
                                tracing::error!("Failed to insert photo: {}", e);
                            }
                        },
                        |_| Message::Tick,
                    )
                } else {
                    Task::none()
                }
            }

            Message::ScanProgressUpdate(progress) => {
                if let Some(ref mut state) = self.scan_state {
                    state.progress = progress;
                }
                Task::none()
            }

            Message::CancelScan => {
                if let Some(ref state) = self.scan_state {
                    state.scanner.cancel();
                }
                self.scan_state = None;
                self.current_view = View::Timeline;
                Task::none()
            }

            Message::ScanComplete => {
                self.scan_state = None;

                // Update photo count
                if let Some(db) = &self.database {
                    // Note: In real implementation, access db properly
                    // For now, just switch view
                }

                self.current_view = View::Timeline;
                Task::none()
            }

            Message::Tick => {
                // Poll channels if scanning
                // This is a simplified polling mechanism
                Task::none()
            }
        }
    }

    /// Render the application
    pub fn view(&self) -> Element<Message> {
        // Show scanning progress if scanning
        if self.current_view == View::Scanning {
            if let Some(ref state) = self.scan_state {
                return ScanProgressView::view(&state.progress);
            } else {
                // Show initial scanning state
                let progress = ScanProgress {
                    files_found: 0,
                    files_processed: 0,
                    bytes_processed: 0,
                    current_directory: String::new(),
                    current_file: "Preparing...".to_string(),
                    errors: Vec::new(),
                    is_complete: false,
                    elapsed_seconds: 0.0,
                };
                return ScanProgressView::view(&progress);
            }
        }

        // If no drive selected, show welcome screen
        if self.selected_drive.is_none() {
            return WelcomeView::view(&self.drives);
        }

        // Main layout: sidebar + content
        let sidebar = Sidebar::view(&self.current_view);

        let content = match self.current_view {
            View::Welcome => WelcomeView::view(&self.drives),
            View::Scanning => unreachable!(), // Handled above
            View::Timeline => TimelineView::view(),
            View::People => PeopleView::view(),
            View::Search => SearchView::view(),
            View::Settings => SettingsView::view(),
        };

        let layout = row![sidebar, content,];

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }
}
```

---

## Step 8: Testing the Scanner

Create a test script to verify scanning works:

```bash
# Create a test directory with sample images
mkdir -p ~/test-photos/2019
mkdir -p ~/test-photos/2020

# If you have sample images, copy them there
# Or create dummy files for testing structure (won't have EXIF)
```

Run the app and select the test directory:

```bash
cargo run
```

---

## Verification Checklist

- [ ] Scanner discovers all image files recursively
- [ ] EXIF data extracted correctly (date, GPS, camera)
- [ ] Filename dates parsed when no EXIF
- [ ] File mtime used as last resort
- [ ] SHA256 hashes calculated
- [ ] Progress UI shows file count and size
- [ ] Cancel button stops scan
- [ ] Data persisted to SQLite database
- [ ] Skips hidden files/folders
- [ ] Skips files under 10KB
- [ ] Handles permission errors gracefully

---

## Performance Notes

For a drive with 100,000 photos:
- Directory walking: ~30 seconds
- Hash calculation: ~60-90 seconds (IO bound)
- EXIF extraction: ~30 seconds (parallel)
- Database inserts: ~10 seconds (batched)

**Total:** ~2-3 minutes

Optimizations for later:
- Parallel hash calculation with rayon
- Batch database inserts (100 at a time)
- Skip hash if file path+size+mtime unchanged

---

## Next Phase Preview

**Phase 3: Thumbnail Generation & Timeline UI** will add:
- On-demand thumbnail generation
- Linux-style thumbnail caching (freedesktop spec)
- Virtual scrolling timeline view
- Photo grid component with smooth scrolling

---

## Expected Results & Behavior

> **IMPORTANT:** Do not proceed to Phase 3 until a human has verified ALL of the following behaviors. Each item must be manually tested and confirmed.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **Scan Progress UI** | Progress bar visible with percentage, file count, and current file path |
| **Scan Animation** | Progress bar fills smoothly from 0% to 100% during scan |
| **Scan Complete** | Success message displayed with total photos found and time elapsed |
| **Cancel Button** | Visible during active scan, hidden when not scanning |
| **Error Display** | Unreadable files shown as warnings, not blocking errors |
| **Empty Folder Scan** | Graceful message: "No photos found in selected directory" |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **Start scan on folder with photos** | Progress UI appears, count increments as files are discovered |
| **Cancel scan mid-progress** | Scan stops within 1 second, partial results are saved to database |
| **Scan folder with mixed file types** | Only JPEG, PNG, HEIC, WebP files are indexed; others ignored |
| **Scan folder with nested subdirectories** | All subdirectories traversed recursively, photos found at all depths |
| **Scan folder with hidden files/folders** | Hidden items (prefixed with `.`) are skipped |
| **Scan folder with files < 10KB** | Small files are skipped (likely thumbnails/icons) |
| **Re-scan same folder** | Existing entries updated, no duplicates in database |

### Database Verification

Run after completing a scan:

```bash
# Check photos were inserted
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM photos;"

# Verify EXIF data extracted
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT file_path, date_taken, camera_make, camera_model FROM photos LIMIT 5;"

# Verify SHA256 hashes calculated
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT file_path, sha256_hash FROM photos WHERE sha256_hash IS NOT NULL LIMIT 5;"

# Check GPS coordinates (if photos have GPS)
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT file_path, gps_latitude, gps_longitude FROM photos WHERE gps_latitude IS NOT NULL LIMIT 5;"

# Verify date fallback from filename
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT file_path, date_taken, date_source FROM photos WHERE date_source = 'filename' LIMIT 5;"
```

**Expected:** All queries return results consistent with the scanned photos. No NULL sha256_hash values. Date sources include 'exif' and 'filename' as appropriate.

### Console Verification

When running `cargo run` and initiating a scan:

```
INFO photovault::scanner: Starting scan of /path/to/folder
INFO photovault::scanner: Found 1,234 photo files
INFO photovault::scanner: Processing: /path/to/photo.jpg (1/1234)
INFO photovault::scanner: EXIF extracted: date=2019-03-15, camera=Canon EOS R5
INFO photovault::scanner: Scan complete: 1,234 photos indexed in 45.2s
```

No `ERROR` messages should appear for valid photo files. `WARN` is acceptable for files with missing/corrupt EXIF data.

### Performance Verification

| Metric | Expected |
|--------|----------|
| **10,000 photos scan** | Completes in under 3 minutes |
| **SHA256 hashing** | Processes at least 100 files/second |
| **EXIF extraction** | Processes at least 200 files/second |
| **Database inserts** | Batched, not one-at-a-time |
| **Memory usage during scan** | Stays under 200MB |

### Sign-off Checklist

Before proceeding to Phase 3, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **Scan starts:** Progress UI appears when scan is initiated
- [ ] **Progress updates:** Real-time file count and percentage visible
- [ ] **EXIF extraction works:** Date, camera info, GPS populated in database
- [ ] **Filename date parsing:** Files without EXIF get dates parsed from filename
- [ ] **SHA256 hashing:** All photos have non-NULL hash values
- [ ] **Cancel works:** Scan stops promptly, partial data preserved
- [ ] **Hidden files skipped:** Files/folders starting with `.` are not indexed
- [ ] **Small files skipped:** Files under 10KB are not indexed
- [ ] **Database populated:** `photos` table has correct row count
- [ ] **No console errors:** Clean scan operation logs
- [ ] **SKILL.md followed:** Scan progress UI matches design guidelines

**Signature:** ___________________ **Date:** _______________

---

## Proceed to Phase 3

Only after ALL items above are verified, proceed to:

📁 `plan/PHASE_3_THUMBNAILS_TIMELINE.md`
