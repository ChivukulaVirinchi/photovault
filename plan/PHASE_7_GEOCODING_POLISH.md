# Phase 7: Offline Geocoding & Polish

## Overview

This final MVP phase adds offline reverse geocoding (converting GPS coordinates to city/country names), incremental re-indexing to detect file changes, a settings view, and overall polish including error handling and performance optimization.

**Estimated Time:** 4-5 days  
**Difficulty:** Intermediate  
**Prerequisites:** All previous phases complete

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

- [ ] Bundle GeoNames database with app
- [ ] Implement reverse geocoding (GPS → city/country)
- [ ] Build incremental re-indexing (detect new/moved/deleted files)
- [ ] Create Settings view
- [ ] Add comprehensive error handling
- [ ] Optimize performance for 100k+ photos
- [ ] Polish UI interactions and animations
- [ ] Final testing and bug fixes

---

## New Files

```
src/
├── services/
│   ├── geocoding.rs            # Reverse geocoding service
│   └── reindexer.rs            # Incremental re-indexing
├── db/
│   └── geonames.rs             # GeoNames database access
├── views/
│   └── settings.rs             # Settings view
└── config/
    └── mod.rs                  # App configuration

data/
└── geonames.db                 # Bundled GeoNames database (~50MB)
```

---

## Step 1: GeoNames Database

### Database Preparation

The GeoNames dataset needs to be preprocessed and bundled with the app.

**Source:** [GeoNames Data](http://download.geonames.org/export/dump/)

Download `cities1000.zip` (cities with population > 1000) and convert to SQLite:

```sql
-- geonames.db schema
CREATE TABLE cities (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    ascii_name TEXT NOT NULL,
    latitude REAL NOT NULL,
    longitude REAL NOT NULL,
    country_code TEXT NOT NULL,
    country_name TEXT NOT NULL,
    population INTEGER,
    timezone TEXT
);

-- Spatial index for fast lookups
CREATE INDEX idx_cities_coords ON cities(latitude, longitude);

-- Country lookup
CREATE TABLE countries (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL
);
```

**Build Script:** Create a script to prepare the database:

```rust
// build_geonames.rs (build script or separate tool)
use std::fs::File;
use std::io::{BufRead, BufReader};
use rusqlite::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open("data/geonames.db")?;
    
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS cities (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            ascii_name TEXT NOT NULL,
            latitude REAL NOT NULL,
            longitude REAL NOT NULL,
            country_code TEXT NOT NULL,
            country_name TEXT NOT NULL,
            population INTEGER,
            timezone TEXT
        );
        
        CREATE TABLE IF NOT EXISTS countries (
            code TEXT PRIMARY KEY,
            name TEXT NOT NULL
        );
        
        CREATE INDEX IF NOT EXISTS idx_cities_coords ON cities(latitude, longitude);
    "#)?;
    
    // Load country codes
    let countries = include_str!("../data/country_codes.txt");
    for line in countries.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            conn.execute(
                "INSERT OR IGNORE INTO countries (code, name) VALUES (?1, ?2)",
                [parts[0], parts[1]],
            )?;
        }
    }
    
    // Load cities from cities1000.txt
    let file = File::open("data/cities1000.txt")?;
    let reader = BufReader::new(file);
    
    let mut stmt = conn.prepare(r#"
        INSERT INTO cities (id, name, ascii_name, latitude, longitude, country_code, country_name, population, timezone)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, (SELECT name FROM countries WHERE code = ?6), ?7, ?8)
    "#)?;
    
    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();
        
        if parts.len() >= 15 {
            let id: i64 = parts[0].parse().unwrap_or(0);
            let name = parts[1];
            let ascii_name = parts[2];
            let lat: f64 = parts[4].parse().unwrap_or(0.0);
            let lon: f64 = parts[5].parse().unwrap_or(0.0);
            let country_code = parts[8];
            let population: i64 = parts[14].parse().unwrap_or(0);
            let timezone = parts[17];
            
            stmt.execute(rusqlite::params![
                id, name, ascii_name, lat, lon, country_code, population, timezone
            ])?;
        }
    }
    
    println!("GeoNames database created successfully");
    Ok(())
}
```

---

## Step 2: Geocoding Service

### File: `src/services/geocoding.rs`

```rust
//! Offline reverse geocoding service
//!
//! Converts GPS coordinates to city/country names using bundled GeoNames data.

use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};

/// A geocoding result
#[derive(Debug, Clone)]
pub struct GeocodingResult {
    /// City name
    pub city: String,
    
    /// Country name
    pub country: String,
    
    /// Country code (ISO 3166-1)
    pub country_code: String,
    
    /// Distance from query point in km
    pub distance_km: f64,
}

/// Offline geocoding service using GeoNames data
pub struct GeocodingService {
    conn: Connection,
}

impl GeocodingService {
    /// Open the geocoding database
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;
        
        // Optimize for read-only queries
        conn.execute_batch(r#"
            PRAGMA query_only = ON;
            PRAGMA cache_size = -10000;
            PRAGMA mmap_size = 268435456;
        "#)?;
        
        Ok(Self { conn })
    }

    /// Reverse geocode a GPS coordinate
    /// 
    /// Returns the nearest city within the search radius, or None if no city found.
    pub fn reverse_geocode(&self, lat: f64, lon: f64) -> Option<GeocodingResult> {
        // Validate coordinates
        if !Self::is_valid_coordinate(lat, lon) {
            return None;
        }

        // First, try a bounding box search for nearby cities
        // This is faster than calculating distance for all cities
        let result = self.search_bounding_box(lat, lon, 1.0); // ~111km
        
        if result.is_some() {
            return result;
        }

        // If no cities within 1 degree, expand search
        self.search_bounding_box(lat, lon, 3.0)
    }

    /// Search within a bounding box and return nearest city
    fn search_bounding_box(&self, lat: f64, lon: f64, radius_deg: f64) -> Option<GeocodingResult> {
        let min_lat = lat - radius_deg;
        let max_lat = lat + radius_deg;
        let min_lon = lon - radius_deg;
        let max_lon = lon + radius_deg;

        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                name,
                country_name,
                country_code,
                latitude,
                longitude
            FROM cities
            WHERE latitude BETWEEN ?1 AND ?2
              AND longitude BETWEEN ?3 AND ?4
            ORDER BY population DESC
            LIMIT 100
            "#,
        ).ok()?;

        let cities: Vec<(String, String, String, f64, f64)> = stmt
            .query_map(params![min_lat, max_lat, min_lon, max_lon], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .ok()?
            .filter_map(|r| r.ok())
            .collect();

        // Find nearest city by Haversine distance
        let mut nearest: Option<(GeocodingResult, f64)> = None;

        for (city_name, country_name, country_code, city_lat, city_lon) in cities {
            let distance = Self::haversine_distance(lat, lon, city_lat, city_lon);

            match &nearest {
                None => {
                    nearest = Some((
                        GeocodingResult {
                            city: city_name,
                            country: country_name,
                            country_code,
                            distance_km: distance,
                        },
                        distance,
                    ));
                }
                Some((_, min_dist)) if distance < *min_dist => {
                    nearest = Some((
                        GeocodingResult {
                            city: city_name,
                            country: country_name,
                            country_code,
                            distance_km: distance,
                        },
                        distance,
                    ));
                }
                _ => {}
            }
        }

        nearest.map(|(result, _)| result)
    }

    /// Calculate Haversine distance between two points in km
    fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        const EARTH_RADIUS_KM: f64 = 6371.0;

        let lat1_rad = lat1.to_radians();
        let lat2_rad = lat2.to_radians();
        let delta_lat = (lat2 - lat1).to_radians();
        let delta_lon = (lon2 - lon1).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);

        let c = 2.0 * a.sqrt().asin();

        EARTH_RADIUS_KM * c
    }

    /// Validate GPS coordinates
    fn is_valid_coordinate(lat: f64, lon: f64) -> bool {
        // Check for valid range
        if lat < -90.0 || lat > 90.0 || lon < -180.0 || lon > 180.0 {
            return false;
        }

        // Check for null island (0, 0) - often indicates invalid GPS
        if (lat.abs() < 0.001) && (lon.abs() < 0.001) {
            return false;
        }

        true
    }

    /// Get country name from country code
    pub fn get_country_name(&self, code: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT name FROM countries WHERE code = ?1",
                params![code],
                |row| row.get(0),
            )
            .ok()
    }

    /// Batch geocode multiple coordinates
    pub fn batch_geocode(&self, coords: &[(f64, f64)]) -> Vec<Option<GeocodingResult>> {
        coords
            .iter()
            .map(|(lat, lon)| self.reverse_geocode(*lat, *lon))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_distance() {
        // Tokyo to New York should be about 10,850 km
        let distance = GeocodingService::haversine_distance(
            35.6762, 139.6503,  // Tokyo
            40.7128, -74.0060,  // New York
        );
        
        assert!((distance - 10850.0).abs() < 100.0);
    }

    #[test]
    fn test_valid_coordinates() {
        assert!(GeocodingService::is_valid_coordinate(35.6762, 139.6503));
        assert!(!GeocodingService::is_valid_coordinate(0.0, 0.0)); // Null island
        assert!(!GeocodingService::is_valid_coordinate(91.0, 0.0)); // Invalid lat
    }
}
```

---

## Step 3: Incremental Re-indexer

### File: `src/services/reindexer.rs`

```rust
//! Incremental re-indexing service
//!
//! Detects new, moved, and deleted files without full rescan.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::{params, Connection, Result as SqliteResult};
use walkdir::WalkDir;

/// Changes detected during re-indexing
#[derive(Debug, Default)]
pub struct IndexChanges {
    /// New files to index
    pub added: Vec<PathBuf>,
    
    /// Files that were deleted from disk
    pub removed: Vec<(i64, PathBuf)>,  // (photo_id, path)
    
    /// Files that were moved (old_path → new_path with same hash)
    pub moved: Vec<(i64, PathBuf, PathBuf)>,  // (photo_id, old_path, new_path)
    
    /// Files that were modified (need re-extraction)
    pub modified: Vec<(i64, PathBuf)>,  // (photo_id, path)
}

impl IndexChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() 
            && self.removed.is_empty() 
            && self.moved.is_empty() 
            && self.modified.is_empty()
    }

    pub fn total_changes(&self) -> usize {
        self.added.len() + self.removed.len() + self.moved.len() + self.modified.len()
    }
}

/// Incremental re-indexer
pub struct Reindexer {
    supported_extensions: HashSet<String>,
    skip_patterns: Vec<String>,
}

impl Default for Reindexer {
    fn default() -> Self {
        Self::new()
    }
}

impl Reindexer {
    pub fn new() -> Self {
        let mut supported_extensions = HashSet::new();
        for ext in &["jpg", "jpeg", "png", "heic", "heif", "webp"] {
            supported_extensions.insert(ext.to_string());
        }

        let skip_patterns = vec![
            ".".to_string(),
            "System Volume Information".to_string(),
            "$RECYCLE.BIN".to_string(),
            ".Trash".to_string(),
            ".photovault".to_string(),
        ];

        Self {
            supported_extensions,
            skip_patterns,
        }
    }

    /// Scan for changes without modifying the database
    pub fn detect_changes(
        &self,
        conn: &Connection,
        drive_root: &Path,
    ) -> SqliteResult<IndexChanges> {
        let mut changes = IndexChanges::default();

        // Get all indexed files from database
        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_hash, updated_at FROM photos WHERE is_trashed = FALSE"
        )?;

        let indexed_files: HashMap<String, (i64, String, Option<String>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,  // file_path
                    (
                        row.get::<_, i64>(0)?,      // id
                        row.get::<_, String>(2)?,   // file_hash
                        row.get::<_, Option<String>>(3)?, // updated_at
                    ),
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Collect hash -> path for move detection
        let hash_to_paths: HashMap<String, Vec<&str>> = indexed_files
            .iter()
            .fold(HashMap::new(), |mut acc, (path, (_, hash, _))| {
                acc.entry(hash.clone()).or_default().push(path);
                acc
            });

        // Scan filesystem
        let mut found_paths: HashSet<String> = HashSet::new();

        for entry in WalkDir::new(drive_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.should_skip(e.path()))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_file() {
                continue;
            }

            // Check extension
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            if !self.supported_extensions.contains(&ext) {
                continue;
            }

            // Get relative path
            let relative_path = entry
                .path()
                .strip_prefix(drive_root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string());

            let relative_path = match relative_path {
                Some(p) => p,
                None => continue,
            };

            found_paths.insert(relative_path.clone());

            // Check if this file is already indexed
            if let Some((id, _hash, updated_at)) = indexed_files.get(&relative_path) {
                // Check if modified
                if let Some(updated) = updated_at {
                    if let Ok(metadata) = fs::metadata(entry.path()) {
                        if let Ok(mtime) = metadata.modified() {
                            let file_time = Self::system_time_to_string(mtime);
                            if file_time > *updated {
                                changes.modified.push((*id, entry.path().to_path_buf()));
                            }
                        }
                    }
                }
            } else {
                // New file
                changes.added.push(entry.path().to_path_buf());
            }
        }

        // Find removed files
        for (path, (id, hash, _)) in &indexed_files {
            if !found_paths.contains(path) {
                // Check if file was moved (same hash exists at different path)
                let mut was_moved = false;

                for found_path in &found_paths {
                    if !indexed_files.contains_key(found_path) {
                        // This is a new file - check if it has the same hash
                        let new_full_path = drive_root.join(found_path);
                        if let Ok(new_hash) = Self::quick_hash(&new_full_path) {
                            if &new_hash == hash {
                                // Found! This is a move
                                changes.moved.push((
                                    *id,
                                    PathBuf::from(path),
                                    new_full_path,
                                ));
                                was_moved = true;
                                break;
                            }
                        }
                    }
                }

                if !was_moved {
                    changes.removed.push((*id, PathBuf::from(path)));
                }
            }
        }

        Ok(changes)
    }

    /// Apply detected changes to the database
    pub fn apply_changes(
        &self,
        conn: &Connection,
        changes: &IndexChanges,
    ) -> SqliteResult<ApplyResult> {
        let mut result = ApplyResult::default();
        let tx = conn.unchecked_transaction()?;

        // Handle moved files
        for (photo_id, old_path, new_path) in &changes.moved {
            let new_relative = new_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            conn.execute(
                r#"
                UPDATE photos 
                SET file_path = ?1, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?2
                "#,
                params![new_relative, photo_id],
            )?;
            result.moves_applied += 1;
        }

        // Handle removed files (mark as deleted or remove)
        for (photo_id, _path) in &changes.removed {
            // Option 1: Mark as missing (soft delete)
            conn.execute(
                r#"
                UPDATE photos 
                SET is_trashed = TRUE, trashed_at = CURRENT_TIMESTAMP
                WHERE id = ?1
                "#,
                params![photo_id],
            )?;
            result.removals_applied += 1;
        }

        // Handle modified files (re-extract metadata)
        for (photo_id, _path) in &changes.modified {
            // Mark for re-processing
            conn.execute(
                r#"
                UPDATE photos 
                SET faces_processed = FALSE, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?1
                "#,
                params![photo_id],
            )?;
            result.updates_applied += 1;
        }

        tx.commit()?;

        // Note: New files (changes.added) should be processed by the scanner
        result.new_files = changes.added.len();

        Ok(result)
    }

    /// Check if a path should be skipped
    fn should_skip(&self, path: &Path) -> bool {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        for pattern in &self.skip_patterns {
            if name.starts_with(pattern) {
                return true;
            }
        }

        false
    }

    /// Quick hash for move detection (first 64KB only)
    fn quick_hash(path: &Path) -> std::io::Result<String> {
        use sha2::{Sha256, Digest};
        use std::io::Read;

        let mut file = fs::File::open(path)?;
        let mut buffer = vec![0u8; 65536]; // 64KB
        let n = file.read(&mut buffer)?;
        buffer.truncate(n);

        let hash = Sha256::digest(&buffer);
        Ok(format!("{:x}", hash))
    }

    /// Convert SystemTime to SQLite-compatible string
    fn system_time_to_string(time: SystemTime) -> String {
        use chrono::{DateTime, Utc};
        
        let datetime: DateTime<Utc> = time.into();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

/// Result of applying changes
#[derive(Debug, Default)]
pub struct ApplyResult {
    pub new_files: usize,
    pub moves_applied: usize,
    pub removals_applied: usize,
    pub updates_applied: usize,
}

impl ApplyResult {
    pub fn total(&self) -> usize {
        self.new_files + self.moves_applied + self.removals_applied + self.updates_applied
    }
}
```

---

## Step 4: App Configuration

### File: `src/config/mod.rs`

```rust
//! Application configuration

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// UI theme
    pub theme: Theme,
    
    /// Thumbnail size in pixels
    pub thumbnail_size: u32,
    
    /// Face detection confidence threshold (0.0-1.0)
    pub face_detection_confidence: f32,
    
    /// Face clustering similarity threshold (0.0-1.0)
    pub face_clustering_threshold: f32,
    
    /// Burst detection time window in seconds
    pub burst_time_window_seconds: i64,
    
    /// Auto-delete trash after N days (0 = disabled)
    pub trash_auto_delete_days: u32,
    
    /// Scan hidden folders
    pub scan_hidden_folders: bool,
    
    /// Date display format
    pub date_format: DateFormat,
    
    /// Previously opened drives
    pub remembered_drives: Vec<PathBuf>,
    
    /// Window size
    pub window_width: u32,
    pub window_height: u32,
    
    /// Sidebar collapsed state
    pub sidebar_collapsed: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            thumbnail_size: 300,
            face_detection_confidence: 0.5,
            face_clustering_threshold: 0.6,
            burst_time_window_seconds: 3,
            trash_auto_delete_days: 30,
            scan_hidden_folders: false,
            date_format: DateFormat::Locale,
            remembered_drives: Vec::new(),
            window_width: 1400,
            window_height: 900,
            sidebar_collapsed: false,
        }
    }
}

impl AppConfig {
    /// Load config from file
    pub fn load() -> Self {
        let config_path = Self::config_path();
        
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(config) => return config,
                        Err(e) => {
                            tracing::warn!("Failed to parse config: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read config: {}", e);
                }
            }
        }

        Self::default()
    }

    /// Save config to file
    pub fn save(&self) -> std::io::Result<()> {
        let config_path = Self::config_path();
        
        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    /// Get config file path
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("photovault")
            .join("config.json")
    }

    /// Add a drive to remembered list
    pub fn remember_drive(&mut self, path: PathBuf) {
        if !self.remembered_drives.contains(&path) {
            self.remembered_drives.push(path);
        }
    }

    /// Remove a drive from remembered list
    pub fn forget_drive(&mut self, path: &PathBuf) {
        self.remembered_drives.retain(|p| p != path);
    }
}

/// UI theme options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
    System,
}

/// Date format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    Locale,
    Iso,
    Us,
    Eu,
}

impl DateFormat {
    /// Format a date according to this format
    pub fn format_date(&self, year: i32, month: u32, day: u32) -> String {
        match self {
            DateFormat::Locale => format!("{:04}-{:02}-{:02}", year, month, day),
            DateFormat::Iso => format!("{:04}-{:02}-{:02}", year, month, day),
            DateFormat::Us => format!("{:02}/{:02}/{:04}", month, day, year),
            DateFormat::Eu => format!("{:02}/{:02}/{:04}", day, month, year),
        }
    }
}
```

---

## Step 5: Settings View

### File: `src/views/settings.rs`

```rust
//! Settings view

use iced::widget::{button, column, container, pick_list, row, slider, text, toggler, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::{AppConfig, DateFormat, Theme};
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Settings view
pub struct SettingsView;

impl SettingsView {
    /// Render the settings view
    pub fn view(config: &AppConfig) -> Element<'static, Message> {
        let title = text("Settings")
            .size(28)
            .color(Text::PRIMARY);

        let content = column![
            title,
            Space::with_height(32),
            
            // Appearance section
            Self::section_header("Appearance"),
            Self::theme_setting(config.theme),
            Space::with_height(24),
            
            // Indexing section
            Self::section_header("Indexing"),
            Self::thumbnail_size_setting(config.thumbnail_size),
            Self::hidden_folders_setting(config.scan_hidden_folders),
            Space::with_height(24),
            
            // Face Recognition section
            Self::section_header("Face Recognition"),
            Self::face_confidence_setting(config.face_detection_confidence),
            Self::clustering_threshold_setting(config.face_clustering_threshold),
            Space::with_height(24),
            
            // Burst Detection section
            Self::section_header("Burst Detection"),
            Self::burst_window_setting(config.burst_time_window_seconds),
            Space::with_height(24),
            
            // Trash section
            Self::section_header("Trash"),
            Self::auto_delete_setting(config.trash_auto_delete_days),
            Space::with_height(24),
            
            // Date & Time section
            Self::section_header("Date & Time"),
            Self::date_format_setting(config.date_format),
            Space::with_height(32),
            
            // Actions
            Self::actions_section(),
        ]
        .padding(32)
        .spacing(8);

        container(
            iced::widget::scrollable(content)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Backgrounds::PRIMARY.into()),
            ..Default::default()
        })
        .into()
    }

    /// Section header
    fn section_header(title: &str) -> Element<'static, Message> {
        column![
            text(title)
                .size(16)
                .color(Text::PRIMARY),
            container(Space::new(Length::Fill, Length::Fixed(1.0)))
                .style(|_theme| container::Style {
                    background: Some(Border::SUBTLE.into()),
                    ..Default::default()
                }),
        ]
        .spacing(8)
        .into()
    }

    /// Theme setting
    fn theme_setting(current: Theme) -> Element<'static, Message> {
        let options = vec![
            ("Dark", Theme::Dark),
            ("Light", Theme::Light),
            ("System", Theme::System),
        ];

        Self::setting_row(
            "Theme",
            "Choose the app color scheme",
            Self::option_buttons(&options, current, |t| Message::SetTheme(t)),
        )
    }

    /// Thumbnail size setting
    fn thumbnail_size_setting(current: u32) -> Element<'static, Message> {
        let sizes = vec![
            ("Small (200px)", 200u32),
            ("Medium (300px)", 300),
            ("Large (400px)", 400),
        ];

        Self::setting_row(
            "Thumbnail Size",
            "Size of generated thumbnails",
            Self::option_buttons(&sizes, current, |s| Message::SetThumbnailSize(s)),
        )
    }

    /// Hidden folders setting
    fn hidden_folders_setting(enabled: bool) -> Element<'static, Message> {
        Self::setting_row(
            "Scan Hidden Folders",
            "Include folders starting with '.'",
            toggler(enabled)
                .on_toggle(Message::SetScanHiddenFolders)
                .into(),
        )
    }

    /// Face detection confidence setting
    fn face_confidence_setting(current: f32) -> Element<'static, Message> {
        let percentage = (current * 100.0) as u32;
        
        Self::setting_row(
            "Face Detection Confidence",
            &format!("Minimum confidence for face detection ({}%)", percentage),
            row![
                slider(30..=90, percentage, |v| Message::SetFaceConfidence(v as f32 / 100.0))
                    .width(200),
                Space::with_width(16),
                text(format!("{}%", percentage))
                    .size(14)
                    .color(Text::SECONDARY),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    /// Face clustering threshold setting
    fn clustering_threshold_setting(current: f32) -> Element<'static, Message> {
        let percentage = (current * 100.0) as u32;
        
        Self::setting_row(
            "Face Grouping Similarity",
            &format!("How similar faces must be to group together ({}%)", percentage),
            row![
                slider(40..=80, percentage, |v| Message::SetClusteringThreshold(v as f32 / 100.0))
                    .width(200),
                Space::with_width(16),
                text(format!("{}%", percentage))
                    .size(14)
                    .color(Text::SECONDARY),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    /// Burst detection window setting
    fn burst_window_setting(current: i64) -> Element<'static, Message> {
        let options = vec![
            ("2 seconds", 2i64),
            ("3 seconds", 3),
            ("5 seconds", 5),
            ("10 seconds", 10),
        ];

        Self::setting_row(
            "Burst Time Window",
            "Maximum gap between photos in a burst",
            Self::option_buttons(&options, current, |s| Message::SetBurstWindow(s)),
        )
    }

    /// Auto delete trash setting
    fn auto_delete_setting(current: u32) -> Element<'static, Message> {
        let options = vec![
            ("Never", 0u32),
            ("7 days", 7),
            ("30 days", 30),
            ("90 days", 90),
        ];

        Self::setting_row(
            "Auto-Delete Trash",
            "Permanently delete trashed photos after",
            Self::option_buttons(&options, current, |d| Message::SetTrashAutoDelete(d)),
        )
    }

    /// Date format setting
    fn date_format_setting(current: DateFormat) -> Element<'static, Message> {
        let options = vec![
            ("ISO (2019-03-15)", DateFormat::Iso),
            ("US (03/15/2019)", DateFormat::Us),
            ("EU (15/03/2019)", DateFormat::Eu),
        ];

        Self::setting_row(
            "Date Format",
            "How dates are displayed",
            Self::option_buttons(&options, current, |f| Message::SetDateFormat(f)),
        )
    }

    /// Actions section
    fn actions_section() -> Element<'static, Message> {
        column![
            Self::section_header("Advanced"),
            Space::with_height(16),
            
            row![
                button(
                    text("Re-scan Library")
                        .size(14)
                        .color(Text::PRIMARY)
                )
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: Border::SUBTLE,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::RescanLibrary),
                
                Space::with_width(16),
                
                button(
                    text("Rebuild Face Clusters")
                        .size(14)
                        .color(Text::PRIMARY)
                )
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: Border::SUBTLE,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::RebuildFaceClusters),
            ],
            
            Space::with_height(32),
            
            row![
                text("PhotoVault v0.1.0")
                    .size(12)
                    .color(Text::TERTIARY),
                Space::with_width(Length::Fill),
            ],
        ]
        .into()
    }

    /// Generic setting row
    fn setting_row<'a>(
        label: &'a str,
        description: &'a str,
        control: Element<'static, Message>,
    ) -> Element<'static, Message> {
        container(
            row![
                column![
                    text(label)
                        .size(14)
                        .color(Text::PRIMARY),
                    text(description)
                        .size(12)
                        .color(Text::TERTIARY),
                ]
                .width(Length::FillPortion(2)),
                
                container(control)
                    .width(Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .align_y(Alignment::Center)
        )
        .padding(Padding::from([12, 0]))
        .into()
    }

    /// Create option buttons for a setting
    fn option_buttons<T: PartialEq + Copy + 'static>(
        options: &[(&str, T)],
        current: T,
        on_select: impl Fn(T) -> Message + 'static + Clone,
    ) -> Element<'static, Message> {
        let buttons: Vec<Element<'static, Message>> = options
            .iter()
            .map(|(label, value)| {
                let is_selected = *value == current;
                let value = *value;
                let on_select = on_select.clone();
                
                button(
                    text(*label)
                        .size(12)
                        .color(if is_selected { Accent::PRIMARY } else { Text::PRIMARY })
                )
                .padding(Padding::from([6, 12]))
                .style(move |_theme, status| {
                    let background = if is_selected {
                        Some(Accent::MUTED.into())
                    } else {
                        match status {
                            button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                            _ => Some(Backgrounds::ELEVATED.into()),
                        }
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: if is_selected { Accent::PRIMARY } else { Border::SUBTLE },
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(on_select(value))
                .into()
            })
            .collect();

        Row::with_children(buttons)
            .spacing(8)
            .into()
    }
}
```

---

## Step 6: Add Messages to App

Add these messages to `src/app.rs`:

```rust
/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing messages ...

    // Settings
    SetTheme(Theme),
    SetThumbnailSize(u32),
    SetScanHiddenFolders(bool),
    SetFaceConfidence(f32),
    SetClusteringThreshold(f32),
    SetBurstWindow(i64),
    SetTrashAutoDelete(u32),
    SetDateFormat(DateFormat),
    
    // Advanced actions
    RescanLibrary,
    RebuildFaceClusters,
    
    // Re-indexing
    CheckForChanges,
    ChangesDetected(IndexChanges),
    ApplyChanges,
    ChangesApplied(ApplyResult),
    
    // Geocoding
    RunGeocoding,
    GeocodingProgress { processed: usize, total: usize },
    GeocodingComplete,
}
```

---

## Step 7: Error Handling Module

### File: `src/error.rs`

```rust
//! Application error handling

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Application error types
#[derive(Debug)]
pub enum AppError {
    /// Database error
    Database(rusqlite::Error),
    
    /// File system error
    Io(io::Error),
    
    /// Image processing error
    Image(image::ImageError),
    
    /// EXIF parsing error
    Exif(String),
    
    /// ML inference error
    Ml(String),
    
    /// File not found
    FileNotFound(PathBuf),
    
    /// Drive not mounted
    DriveNotMounted(PathBuf),
    
    /// Invalid configuration
    Config(String),
    
    /// Operation cancelled by user
    Cancelled,
    
    /// Generic error with message
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Database(e) => write!(f, "Database error: {}", e),
            AppError::Io(e) => write!(f, "I/O error: {}", e),
            AppError::Image(e) => write!(f, "Image error: {}", e),
            AppError::Exif(msg) => write!(f, "EXIF error: {}", msg),
            AppError::Ml(msg) => write!(f, "ML error: {}", msg),
            AppError::FileNotFound(path) => write!(f, "File not found: {}", path.display()),
            AppError::DriveNotMounted(path) => write!(f, "Drive not mounted: {}", path.display()),
            AppError::Config(msg) => write!(f, "Configuration error: {}", msg),
            AppError::Cancelled => write!(f, "Operation cancelled"),
            AppError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e)
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<image::ImageError> for AppError {
    fn from(e: image::ImageError) -> Self {
        AppError::Image(e)
    }
}

/// Result type for app operations
pub type AppResult<T> = Result<T, AppError>;

/// Error recovery suggestions
impl AppError {
    /// Get a user-friendly recovery suggestion
    pub fn recovery_suggestion(&self) -> &str {
        match self {
            AppError::Database(_) => "Try restarting the app. If the problem persists, the database may be corrupted.",
            AppError::Io(_) => "Check file permissions and available disk space.",
            AppError::Image(_) => "This image file may be corrupted or in an unsupported format.",
            AppError::Exif(_) => "Could not read photo metadata. The photo will still be indexed.",
            AppError::Ml(_) => "Face detection failed. Try restarting the app.",
            AppError::FileNotFound(_) => "The file may have been moved or deleted.",
            AppError::DriveNotMounted(_) => "Please connect the drive and try again.",
            AppError::Config(_) => "Reset settings to defaults in the Settings menu.",
            AppError::Cancelled => "",
            AppError::Other(_) => "Please try again.",
        }
    }

    /// Check if this error should be shown to user
    pub fn should_notify_user(&self) -> bool {
        match self {
            AppError::Cancelled => false,
            AppError::Exif(_) => false, // Silent fail for EXIF
            _ => true,
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            AppError::Database(_) => false,
            AppError::DriveNotMounted(_) => true,
            AppError::Cancelled => true,
            _ => true,
        }
    }
}

/// Error notification for UI
#[derive(Debug, Clone)]
pub struct ErrorNotification {
    pub title: String,
    pub message: String,
    pub suggestion: String,
    pub is_critical: bool,
}

impl From<&AppError> for ErrorNotification {
    fn from(error: &AppError) -> Self {
        let title = match error {
            AppError::Database(_) => "Database Error",
            AppError::Io(_) => "File Error",
            AppError::Image(_) => "Image Error",
            AppError::Exif(_) => "Metadata Error",
            AppError::Ml(_) => "Face Detection Error",
            AppError::FileNotFound(_) => "File Not Found",
            AppError::DriveNotMounted(_) => "Drive Not Connected",
            AppError::Config(_) => "Settings Error",
            AppError::Cancelled => "Cancelled",
            AppError::Other(_) => "Error",
        };

        ErrorNotification {
            title: title.to_string(),
            message: error.to_string(),
            suggestion: error.recovery_suggestion().to_string(),
            is_critical: !error.is_recoverable(),
        }
    }
}
```

---

## Step 8: Performance Optimizations

### Database Optimizations

Add to database initialization:

```rust
/// Optimize database for photo library operations
pub fn optimize_database(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(r#"
        -- Write-ahead logging for better concurrency
        PRAGMA journal_mode = WAL;
        
        -- Normal synchronous for balance of speed and safety
        PRAGMA synchronous = NORMAL;
        
        -- 64MB cache
        PRAGMA cache_size = -65536;
        
        -- Store temp tables in memory
        PRAGMA temp_store = MEMORY;
        
        -- Memory-map up to 256MB
        PRAGMA mmap_size = 268435456;
        
        -- Analyze tables for query optimization
        ANALYZE;
    "#)?;

    Ok(())
}

/// Create all recommended indexes
pub fn create_indexes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(r#"
        CREATE INDEX IF NOT EXISTS idx_photos_date ON photos(date_taken);
        CREATE INDEX IF NOT EXISTS idx_photos_hash ON photos(file_hash);
        CREATE INDEX IF NOT EXISTS idx_photos_location ON photos(location_country, location_city);
        CREATE INDEX IF NOT EXISTS idx_photos_trashed ON photos(is_trashed);
        CREATE INDEX IF NOT EXISTS idx_faces_cluster ON faces(cluster_id);
        CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
        CREATE INDEX IF NOT EXISTS idx_clusters_name ON face_clusters(name);
    "#)?;

    Ok(())
}
```

### Thumbnail Cache Optimization

```rust
/// LRU cache for loaded thumbnails
pub struct ThumbnailCache {
    cache: lru::LruCache<String, iced::widget::image::Handle>,
    max_memory_bytes: usize,
    current_memory_bytes: usize,
}

impl ThumbnailCache {
    pub fn new(max_memory_mb: usize) -> Self {
        Self {
            cache: lru::LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
            max_memory_bytes: max_memory_mb * 1024 * 1024,
            current_memory_bytes: 0,
        }
    }

    pub fn get(&mut self, hash: &str) -> Option<&iced::widget::image::Handle> {
        self.cache.get(hash)
    }

    pub fn insert(&mut self, hash: String, handle: iced::widget::image::Handle, size_bytes: usize) {
        // Evict if necessary
        while self.current_memory_bytes + size_bytes > self.max_memory_bytes {
            if let Some((_, _)) = self.cache.pop_lru() {
                // Approximate memory freed
                self.current_memory_bytes = self.current_memory_bytes.saturating_sub(size_bytes);
            } else {
                break;
            }
        }

        self.cache.put(hash, handle);
        self.current_memory_bytes += size_bytes;
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_memory_bytes = 0;
    }
}
```

---

## UI Design: Settings View

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  Settings                                          │
│             │                                                    │
│  Timeline   │  Appearance                                        │
│  People     │  ───────────────────────────────────────────────   │
│  Search     │  Theme                          [Dark] [Light]     │
│  Duplicates │  Choose the app color scheme                       │
│  Bursts     │                                                    │
│             │  Indexing                                          │
│  ─────────  │  ───────────────────────────────────────────────   │
│             │  Thumbnail Size                 [S] [M] [L]        │
│  Trash (5)  │  Size of generated thumbnails                      │
│  Settings ● │                                                    │
│             │  Scan Hidden Folders            [  OFF  ]          │
│             │  Include folders starting with '.'                 │
│             │                                                    │
│             │  Face Recognition                                  │
│             │  ───────────────────────────────────────────────   │
│             │  Detection Confidence           ━━━━●━━━  60%      │
│             │  Minimum confidence for face detection             │
│             │                                                    │
│             │  Trash                                             │
│             │  ───────────────────────────────────────────────   │
│             │  Auto-Delete              [Never] [7d] [30d] [90d] │
│             │  Permanently delete trashed photos after           │
│             │                                                    │
│             │  Advanced                                          │
│             │  ───────────────────────────────────────────────   │
│             │  [Re-scan Library]  [Rebuild Face Clusters]        │
│             │                                                    │
│             │  PhotoVault v0.1.0                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Verification Checklist

### Geocoding
- [ ] GeoNames database bundled correctly (~50MB)
- [ ] Reverse geocoding returns nearest city
- [ ] Null island (0,0) coordinates handled
- [ ] Ocean coordinates return nearest coastal city
- [ ] Batch geocoding works for multiple coordinates

### Re-indexing
- [ ] New files detected correctly
- [ ] Deleted files marked as removed
- [ ] Moved files (same hash, different path) detected
- [ ] Modified files (newer mtime) flagged for re-processing
- [ ] Hidden folders skipped by default
- [ ] System folders always skipped

### Settings
- [ ] All settings persist across app restarts
- [ ] Theme switching works
- [ ] Threshold sliders update in real-time
- [ ] Re-scan library triggers full scan
- [ ] Rebuild face clusters re-runs clustering

### Error Handling
- [ ] Database errors show user-friendly message
- [ ] File not found errors handled gracefully
- [ ] Drive disconnect detected and handled
- [ ] Corrupted images skipped with warning
- [ ] EXIF failures don't block indexing

### Performance
- [ ] Timeline scrolls at 60 FPS with 100k photos
- [ ] Thumbnail cache limits memory usage
- [ ] Database queries use indexes
- [ ] Virtual scrolling renders only visible items
- [ ] Background processing doesn't block UI

---

## Final Integration Checklist

### Before Release
- [ ] All phases implemented and tested
- [ ] Database migrations work correctly
- [ ] App launches in < 3 seconds
- [ ] Memory usage stays under 500MB
- [ ] No crashes on edge cases
- [ ] All keyboard shortcuts work
- [ ] Error messages are helpful

### Documentation
- [ ] README with installation instructions
- [ ] Keyboard shortcuts reference
- [ ] FAQ for common issues
- [ ] Contributing guidelines

---

## MVP Complete!

With Phase 7 complete, PhotoVault MVP includes:

1. **Directory Scanning** - Fast recursive scan with hash calculation
2. **EXIF Extraction** - Metadata extraction with fallbacks
3. **Thumbnails** - On-demand generation with LRU cache
4. **Timeline View** - Chronological browsing with virtual scroll
5. **Face Detection** - SCRFD-based face detection
6. **Face Clustering** - DBSCAN clustering with ArcFace embeddings
7. **People View** - Browse and name face clusters
8. **Duplicate Detection** - SHA256-based exact matching
9. **Burst Detection** - Group rapid-fire photos
10. **Best-Pick Scoring** - Sharpness and blur analysis
11. **Search** - Natural language date, location, person search
12. **Quick Cull** - Keyboard-driven photo review
13. **Trash** - Soft delete with recovery
14. **Offline Geocoding** - GPS to city/country conversion
15. **Settings** - Configurable thresholds and preferences

**Post-MVP features** (Phase 2) documented in `docs/PHASE_2_POST_MVP.md`:
- Map view
- Memories/flashbacks
- Albums
- Video support
- Cloud backup integration
- Mobile companion app

---

## Expected Results & Behavior

> **IMPORTANT:** This is the final MVP phase. ALL of the following must be verified before considering the MVP complete.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **Location Labels** | City and country names displayed on photos with GPS data |
| **Settings View** | All configuration options displayed in organized sections |
| **Theme Setting** | Dark mode active, matches design spec throughout |
| **Thumbnail Size Setting** | Dropdown/slider for cache size limit |
| **Threshold Settings** | Burst detection window, duplicate sensitivity visible |
| **Error Messages** | User-friendly error dialogs with recovery suggestions |
| **Re-index Progress** | Progress indicator during incremental re-index |
| **App Startup** | Launches in under 3 seconds, previous state restored |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **View photo with GPS data** | City and country name displayed in metadata panel |
| **View photo without GPS data** | Location field shows "Unknown" or is hidden |
| **Open Settings** | All config sections visible and editable |
| **Change thumbnail cache size** | Setting saved, takes effect on next thumbnail operation |
| **Change burst detection window** | Setting saved, re-detection uses new threshold |
| **Trigger re-index** | Detects new files added since last scan, indexes them |
| **Trigger re-index with deleted files** | Detects files removed from disk, marks them in database |
| **Trigger re-index with moved files** | Detects files at new paths (same hash), updates paths |
| **Error: corrupt photo file** | Friendly error shown, scan continues with other files |
| **Error: database locked** | Retry with message, not a crash |
| **Close and reopen app** | Settings persisted, database state preserved |

### Technical Verification

```bash
# Verify GeoNames database bundled
ls -la /path/to/photovault/data/geonames*.txt

# Check reverse geocoding populated location fields
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT file_path, gps_latitude, gps_longitude, location_city, location_country FROM photos WHERE location_city IS NOT NULL LIMIT 10;"

# Verify settings file persisted
cat /path/to/drive/.photovault/config.json

# Check database WAL mode enabled
sqlite3 /path/to/drive/.photovault/photovault.db "PRAGMA journal_mode;"
# Expected: wal

# Verify indexes exist for performance
sqlite3 /path/to/drive/.photovault/photovault.db ".indices photos"

# Check incremental re-index detected changes
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM photos WHERE indexed_at > datetime('now', '-1 hour');"

# Verify app config structure
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT * FROM schema_version;"
```

**Expected:** GeoNames data converts GPS coordinates to accurate city/country names. Settings persist in JSON config file. Database uses WAL mode with proper indexes. Re-indexing detects file changes.

### Performance Verification

| Metric | Expected |
|--------|----------|
| **Reverse geocoding** | < 10ms per coordinate lookup (Haversine) |
| **Incremental re-index (1000 new files)** | < 30 seconds |
| **App startup time** | < 3 seconds to usable state |
| **Memory usage (idle)** | Under 500MB |
| **Database query time** | < 100ms for any single query |
| **Settings save** | Instant (< 50ms) |

### Sign-off Checklist

Before considering the MVP complete, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **Geocoding works:** GPS coordinates converted to city/country names
- [ ] **GeoNames bundled:** ~50MB database included with app
- [ ] **Haversine distance:** Nearest city found correctly (within ~50km accuracy)
- [ ] **Incremental re-index:** New files detected and indexed
- [ ] **Moved files detected:** Same hash at new path updates record
- [ ] **Deleted files detected:** Missing files flagged in database
- [ ] **Settings view complete:** All config options displayed and editable
- [ ] **Settings persist:** JSON config saved and loaded on restart
- [ ] **Error handling:** AppError enum covers all error types with recovery suggestions
- [ ] **Database optimized:** WAL mode, indexes on key columns, cache settings tuned
- [ ] **App startup fast:** Under 3 seconds to usable state
- [ ] **Memory stable:** Under 500MB during normal operation with 100k+ photos
- [ ] **No console errors:** Clean operation throughout all features
- [ ] **SKILL.md followed:** Settings and error UIs match design guidelines
- [ ] **All 15 MVP features working:** Scan, EXIF, thumbnails, timeline, faces, clustering, people, duplicates, bursts, best-pick, search, cull, trash, geocoding, settings

**Signature:** ___________________ **Date:** _______________

---

## MVP Complete!

All 7 phases are implemented and verified. PhotoVault MVP is ready for use.

See `docs/PHASE_2_POST_MVP.md` for future feature plans.
