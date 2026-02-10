# Phase 1: Project Setup & Core Infrastructure

## Overview

This phase establishes the foundation: project structure, dependencies, database schema, and the Iced UI shell. By the end, you'll have a running desktop app with a sidebar, empty views, and a working SQLite database.

**Estimated Time:** 2-3 days  
**Difficulty:** Beginner-Intermediate  
**Prerequisites:** Rust installed, basic Rust knowledge

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

- [x] Initialize Rust project with correct structure
- [ ] Set up Iced UI framework with custom theme
- [ ] Create navigation shell (sidebar + main content area)
- [ ] Initialize SQLite database with full schema
- [ ] Create database abstraction layer
- [ ] Implement drive detection and selection
- [ ] Build "Welcome" screen for first launch

---

## Project Structure

```
photovault/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Entry point, Iced application
│   ├── app.rs                  # Main application state and messages
│   ├── theme/
│   │   ├── mod.rs              # Theme module
│   │   ├── colors.rs           # Color palette
│   │   └── typography.rs       # Font definitions
│   ├── views/
│   │   ├── mod.rs              # View exports
│   │   ├── welcome.rs          # First-launch welcome screen
│   │   ├── timeline.rs         # Timeline view (stub)
│   │   ├── people.rs           # People view (stub)
│   │   ├── search.rs           # Search view (stub)
│   │   └── settings.rs         # Settings view (stub)
│   ├── components/
│   │   ├── mod.rs              # Component exports
│   │   ├── sidebar.rs          # Navigation sidebar
│   │   ├── photo_grid.rs       # Photo grid (stub)
│   │   └── drive_picker.rs     # Drive selection component
│   ├── db/
│   │   ├── mod.rs              # Database module
│   │   ├── connection.rs       # Connection management
│   │   ├── schema.rs           # Schema creation
│   │   └── migrations.rs       # Schema versioning
│   ├── services/
│   │   ├── mod.rs              # Service exports
│   │   └── drive_detector.rs   # Detect mounted drives
│   └── models/
│       ├── mod.rs              # Model exports
│       └── photo.rs            # Photo struct
├── assets/
│   ├── fonts/
│   │   ├── Inter-Regular.ttf
│   │   ├── Inter-Medium.ttf
│   │   ├── Inter-SemiBold.ttf
│   │   └── JetBrainsMono-Regular.ttf
│   └── icons/
│       ├── sidebar/
│       │   ├── timeline.svg
│       │   ├── people.svg
│       │   ├── search.svg
│       │   └── settings.svg
│       └── actions/
│           ├── folder.svg
│           └── drive.svg
└── plan/
    └── *.md
```

---

## Step 1: Update Cargo.toml

Replace the contents of `Cargo.toml`:

```toml
[package]
name = "photovault"
version = "0.1.0"
edition = "2021"
description = "Offline-first photo library manager"
authors = ["Your Name <your.email@example.com>"]

[dependencies]
# UI Framework
iced = { version = "0.13", features = ["image", "svg", "tokio", "advanced"] }

# Database
rusqlite = { version = "0.32", features = ["bundled", "blob"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Date/Time
chrono = { version = "0.4", features = ["serde"] }

# File system utilities
walkdir = "2"
dirs = "5"                      # Platform-specific directories

# Hashing
sha2 = "0.10"

# Image processing (for later phases)
image = "0.25"

# Error handling
thiserror = "1"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

---

## Step 2: Create Directory Structure

Run these commands:

```bash
cd photovault
mkdir -p src/{theme,views,components,db,services,models}
mkdir -p assets/{fonts,icons/sidebar,icons/actions}
```

---

## Step 3: Theme System

### File: `src/theme/mod.rs`

```rust
//! PhotoVault Theme System
//! 
//! A refined, minimal dark theme with warm accents.
//! Aesthetic: Editorial/Magazine meets Modern Desktop App

pub mod colors;
pub mod typography;

pub use colors::*;
pub use typography::*;
```

### File: `src/theme/colors.rs`

```rust
//! Color Palette for PhotoVault
//! 
//! Design Philosophy:
//! - Dark theme optimized for photo viewing (photos should pop)
//! - Warm neutral background (not pure black, not cold gray)
//! - Subtle accent color for interactive elements
//! - High contrast for text readability

use iced::Color;

/// Background colors - warm dark neutrals
pub struct Backgrounds;

impl Backgrounds {
    /// Main app background - warm charcoal
    pub const PRIMARY: Color = Color::from_rgb(
        0x12 as f32 / 255.0,
        0x12 as f32 / 255.0,
        0x14 as f32 / 255.0,
    ); // #121214
    
    /// Sidebar and panels - slightly lighter
    pub const SECONDARY: Color = Color::from_rgb(
        0x1A as f32 / 255.0,
        0x1A as f32 / 255.0,
        0x1E as f32 / 255.0,
    ); // #1A1A1E
    
    /// Cards and elevated surfaces
    pub const ELEVATED: Color = Color::from_rgb(
        0x24 as f32 / 255.0,
        0x24 as f32 / 255.0,
        0x2A as f32 / 255.0,
    ); // #24242A
    
    /// Hover states
    pub const HOVER: Color = Color::from_rgb(
        0x2E as f32 / 255.0,
        0x2E as f32 / 255.0,
        0x36 as f32 / 255.0,
    ); // #2E2E36
    
    /// Selected/Active states
    pub const ACTIVE: Color = Color::from_rgb(
        0x38 as f32 / 255.0,
        0x38 as f32 / 255.0,
        0x42 as f32 / 255.0,
    ); // #383842
}

/// Text colors - warm whites and grays
pub struct Text;

impl Text {
    /// Primary text - warm white
    pub const PRIMARY: Color = Color::from_rgb(
        0xF5 as f32 / 255.0,
        0xF5 as f32 / 255.0,
        0xF3 as f32 / 255.0,
    ); // #F5F5F3
    
    /// Secondary text - muted
    pub const SECONDARY: Color = Color::from_rgb(
        0xA0 as f32 / 255.0,
        0xA0 as f32 / 255.0,
        0x9C as f32 / 255.0,
    ); // #A0A09C
    
    /// Tertiary/Disabled text
    pub const TERTIARY: Color = Color::from_rgb(
        0x6B as f32 / 255.0,
        0x6B as f32 / 255.0,
        0x67 as f32 / 255.0,
    ); // #6B6B67
}

/// Accent colors - warm amber/gold
pub struct Accent;

impl Accent {
    /// Primary accent - warm amber
    pub const PRIMARY: Color = Color::from_rgb(
        0xE5 as f32 / 255.0,
        0xA8 as f32 / 255.0,
        0x3B as f32 / 255.0,
    ); // #E5A83B
    
    /// Accent hover - lighter
    pub const HOVER: Color = Color::from_rgb(
        0xF0 as f32 / 255.0,
        0xB8 as f32 / 255.0,
        0x4B as f32 / 255.0,
    ); // #F0B84B
    
    /// Accent muted - for subtle highlights
    pub const MUTED: Color = Color::from_rgb(
        0xE5 as f32 / 255.0,
        0xA8 as f32 / 255.0,
        0x3B as f32 / 255.0,
    ).scale_alpha(0.15); // #E5A83B at 15%
}

/// Semantic colors
pub struct Semantic;

impl Semantic {
    /// Success - muted green
    pub const SUCCESS: Color = Color::from_rgb(
        0x4A as f32 / 255.0,
        0xB8 as f32 / 255.0,
        0x7D as f32 / 255.0,
    ); // #4AB87D
    
    /// Warning - warm yellow
    pub const WARNING: Color = Color::from_rgb(
        0xE5 as f32 / 255.0,
        0xC0 as f32 / 255.0,
        0x7B as f32 / 255.0,
    ); // #E5C07B
    
    /// Error - muted red
    pub const ERROR: Color = Color::from_rgb(
        0xE0 as f32 / 255.0,
        0x6C as f32 / 255.0,
        0x75 as f32 / 255.0,
    ); // #E06C75
}

/// Border colors
pub struct Border;

impl Border {
    /// Subtle border for separation
    pub const SUBTLE: Color = Color::from_rgb(
        0x2A as f32 / 255.0,
        0x2A as f32 / 255.0,
        0x30 as f32 / 255.0,
    ); // #2A2A30
    
    /// Visible border for interactive elements
    pub const VISIBLE: Color = Color::from_rgb(
        0x3A as f32 / 255.0,
        0x3A as f32 / 255.0,
        0x42 as f32 / 255.0,
    ); // #3A3A42
}

/// Helper trait for color manipulation
pub trait ColorExt {
    fn scale_alpha(self, factor: f32) -> Color;
}

impl ColorExt for Color {
    fn scale_alpha(self, factor: f32) -> Color {
        Color {
            a: self.a * factor,
            ..self
        }
    }
}
```

### File: `src/theme/typography.rs`

```rust
//! Typography definitions for PhotoVault
//! 
//! Font Stack:
//! - Display/Headers: Inter (clean, modern, highly legible)
//! - Body: Inter
//! - Monospace: JetBrains Mono (for file paths, technical info)

use iced::{Font, font};

/// Font weights as embedded bytes
pub struct Fonts;

impl Fonts {
    /// Inter Regular (400)
    pub const INTER_REGULAR: &'static [u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
    
    /// Inter Medium (500)
    pub const INTER_MEDIUM: &'static [u8] = include_bytes!("../../assets/fonts/Inter-Medium.ttf");
    
    /// Inter SemiBold (600)
    pub const INTER_SEMIBOLD: &'static [u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
    
    /// JetBrains Mono Regular
    pub const JETBRAINS_MONO: &'static [u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
}

/// Font family definitions
pub struct FontFamily;

impl FontFamily {
    pub const INTER: Font = Font::with_name("Inter");
    pub const MONO: Font = Font::with_name("JetBrains Mono");
}

/// Text size scale (in pixels)
pub struct TextSize;

impl TextSize {
    /// Tiny labels, badges
    pub const XS: f32 = 11.0;
    /// Small captions, metadata
    pub const SM: f32 = 12.0;
    /// Body text
    pub const BASE: f32 = 14.0;
    /// Emphasized body, small headers
    pub const LG: f32 = 16.0;
    /// Section headers
    pub const XL: f32 = 20.0;
    /// Page titles
    pub const XXL: f32 = 28.0;
    /// Hero text
    pub const XXXL: f32 = 36.0;
}

/// Line heights
pub struct LineHeight;

impl LineHeight {
    pub const TIGHT: f32 = 1.2;
    pub const NORMAL: f32 = 1.5;
    pub const RELAXED: f32 = 1.75;
}
```

---

## Step 4: Database Layer

### File: `src/db/mod.rs`

```rust
//! Database module for PhotoVault
//! 
//! Uses SQLite with the database stored on the indexed drive itself,
//! making the index fully portable.

pub mod connection;
pub mod schema;
pub mod migrations;

pub use connection::Database;
pub use schema::create_schema;
```

### File: `src/db/connection.rs`

```rust
//! Database connection management

use rusqlite::{Connection, Result as SqliteResult};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Failed to open database: {0}")]
    OpenError(#[from] rusqlite::Error),
    
    #[error("Database path does not exist: {0}")]
    PathNotFound(PathBuf),
    
    #[error("Failed to create .photovault directory: {0}")]
    DirectoryCreationError(std::io::Error),
}

/// Database wrapper with path information
pub struct Database {
    pub conn: Connection,
    pub path: PathBuf,
    pub drive_root: PathBuf,
}

impl Database {
    /// Open or create database on a drive
    /// 
    /// # Arguments
    /// * `drive_root` - Root path of the drive to index (e.g., "/media/photos")
    /// 
    /// # Returns
    /// A Database instance with an open connection
    /// 
    /// # Example
    /// ```ignore
    /// let db = Database::open_for_drive("/media/my-drive")?;
    /// ```
    pub fn open_for_drive<P: AsRef<Path>>(drive_root: P) -> Result<Self, DatabaseError> {
        let drive_root = drive_root.as_ref().to_path_buf();
        
        if !drive_root.exists() {
            return Err(DatabaseError::PathNotFound(drive_root));
        }
        
        // Create .photovault directory if it doesn't exist
        let photovault_dir = drive_root.join(".photovault");
        if !photovault_dir.exists() {
            std::fs::create_dir_all(&photovault_dir)
                .map_err(DatabaseError::DirectoryCreationError)?;
        }
        
        let db_path = photovault_dir.join("photovault.db");
        let conn = Connection::open(&db_path)?;
        
        // Configure SQLite for optimal performance
        Self::configure_connection(&conn)?;
        
        Ok(Self {
            conn,
            path: db_path,
            drive_root,
        })
    }
    
    /// Configure SQLite connection for optimal performance
    fn configure_connection(conn: &Connection) -> SqliteResult<()> {
        // Write-Ahead Logging for better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")?;
        
        // Balance between safety and speed
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        
        // 64MB cache
        conn.pragma_update(None, "cache_size", -64000)?;
        
        // Temp tables in memory
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        
        // Memory-map up to 256MB
        conn.pragma_update(None, "mmap_size", 268435456)?;
        
        // Foreign key enforcement
        conn.pragma_update(None, "foreign_keys", "ON")?;
        
        Ok(())
    }
    
    /// Check if this is a fresh database (needs schema creation)
    pub fn needs_schema(&self) -> SqliteResult<bool> {
        let count: i32 = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='photos'",
            [],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_open_for_drive() {
        let temp = tempdir().unwrap();
        let db = Database::open_for_drive(temp.path()).unwrap();
        
        assert!(db.path.exists());
        assert!(temp.path().join(".photovault").exists());
    }
}
```

### File: `src/db/schema.rs`

```rust
//! Database schema creation
//! 
//! Creates all tables needed for PhotoVault. The schema is designed to:
//! - Support all Phase 1 features
//! - Be extensible for Phase 2
//! - Allow efficient queries with proper indexes

use rusqlite::{Connection, Result as SqliteResult};

/// Create the complete database schema
/// 
/// This should be called once when initializing a new database.
pub fn create_schema(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    tracing::info!("Database schema created successfully");
    Ok(())
}

const SCHEMA_SQL: &str = r#"
-- ============================================================
-- SCHEMA VERSION
-- ============================================================

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO schema_version (version) VALUES (1);

-- ============================================================
-- PHOTOS TABLE
-- Core photo metadata
-- ============================================================

CREATE TABLE IF NOT EXISTS photos (
    id INTEGER PRIMARY KEY,
    
    -- File information
    file_path TEXT NOT NULL,           -- Relative path from drive root
    file_name TEXT NOT NULL,
    file_hash TEXT NOT NULL,           -- SHA256 for duplicate detection
    file_size INTEGER NOT NULL,
    file_mtime INTEGER,                -- File modification time (unix timestamp)
    
    -- EXIF metadata
    date_taken DATETIME,               -- From EXIF, fallback to file mtime
    date_taken_source TEXT,            -- 'exif' | 'filename' | 'mtime'
    gps_latitude REAL,
    gps_longitude REAL,
    location_city TEXT,                -- Reverse geocoded
    location_country TEXT,             -- Reverse geocoded
    camera_make TEXT,
    camera_model TEXT,
    width INTEGER,
    height INTEGER,
    orientation INTEGER DEFAULT 1,
    
    -- Processing state
    thumbnail_path TEXT,               -- Path to cached thumbnail (relative)
    faces_processed BOOLEAN DEFAULT FALSE,
    
    -- Soft delete
    is_trashed BOOLEAN DEFAULT FALSE,
    trashed_at DATETIME,
    
    -- Timestamps
    indexed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(file_path)
);

-- ============================================================
-- FACES TABLE
-- Detected faces in photos
-- ============================================================

CREATE TABLE IF NOT EXISTS faces (
    id INTEGER PRIMARY KEY,
    photo_id INTEGER NOT NULL,
    
    -- Bounding box (normalized 0-1 coordinates)
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_width REAL NOT NULL,
    bbox_height REAL NOT NULL,
    
    -- Face embedding (512-dimensional vector, stored as blob)
    embedding BLOB NOT NULL,
    
    -- Clustering
    cluster_id INTEGER,                -- NULL = unassigned
    confidence REAL,                   -- Detection confidence
    
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE SET NULL
);

-- ============================================================
-- FACE CLUSTERS TABLE  
-- A cluster represents a person
-- ============================================================

CREATE TABLE IF NOT EXISTS face_clusters (
    id INTEGER PRIMARY KEY,
    name TEXT,                         -- NULL = unnamed, user sets this
    representative_face_id INTEGER,    -- Best face for this cluster (for display)
    face_count INTEGER DEFAULT 0,
    
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (representative_face_id) REFERENCES faces(id) ON DELETE SET NULL
);

-- ============================================================
-- DUPLICATE GROUPS
-- Groups of identical or near-identical photos
-- ============================================================

CREATE TABLE IF NOT EXISTS duplicate_groups (
    id INTEGER PRIMARY KEY,
    group_hash TEXT,                   -- Shared hash
    duplicate_type TEXT NOT NULL,      -- 'exact' | 'perceptual'
    resolved BOOLEAN DEFAULT FALSE,    -- User has dealt with this group
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS duplicate_group_members (
    id INTEGER PRIMARY KEY,
    group_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    is_suggested_keep BOOLEAN DEFAULT FALSE,  -- Our recommendation
    
    FOREIGN KEY (group_id) REFERENCES duplicate_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(group_id, photo_id)
);

-- ============================================================
-- BURST GROUPS
-- Photos taken within seconds of each other
-- ============================================================

CREATE TABLE IF NOT EXISTS burst_groups (
    id INTEGER PRIMARY KEY,
    start_time DATETIME NOT NULL,
    end_time DATETIME NOT NULL,
    photo_count INTEGER DEFAULT 0,
    resolved BOOLEAN DEFAULT FALSE,    -- User has reviewed this burst
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS burst_group_members (
    id INTEGER PRIMARY KEY,
    group_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    sharpness_score REAL,             -- Higher = sharper
    blur_score REAL,                  -- Lower = less blur
    face_count INTEGER DEFAULT 0,
    is_suggested_best BOOLEAN DEFAULT FALSE,
    
    FOREIGN KEY (group_id) REFERENCES burst_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(group_id, photo_id)
);

-- ============================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================

-- Photos
CREATE INDEX IF NOT EXISTS idx_photos_date ON photos(date_taken);
CREATE INDEX IF NOT EXISTS idx_photos_hash ON photos(file_hash);
CREATE INDEX IF NOT EXISTS idx_photos_location ON photos(location_country, location_city);
CREATE INDEX IF NOT EXISTS idx_photos_trashed ON photos(is_trashed);
CREATE INDEX IF NOT EXISTS idx_photos_path ON photos(file_path);

-- Faces
CREATE INDEX IF NOT EXISTS idx_faces_cluster ON faces(cluster_id);
CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);

-- Duplicate and burst group members
CREATE INDEX IF NOT EXISTS idx_dup_members_group ON duplicate_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_dup_members_photo ON duplicate_group_members(photo_id);
CREATE INDEX IF NOT EXISTS idx_burst_members_group ON burst_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_burst_members_photo ON burst_group_members(photo_id);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    
    #[test]
    fn test_create_schema() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        
        // Verify photos table exists
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='photos'",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 1);
    }
}
```

### File: `src/db/migrations.rs`

```rust
//! Database migrations for schema versioning

use rusqlite::{Connection, Result as SqliteResult};

/// Get the current schema version
pub fn get_schema_version(conn: &Connection) -> SqliteResult<i32> {
    let result = conn.query_row(
        "SELECT MAX(version) FROM schema_version",
        [],
        |row| row.get(0),
    );
    
    match result {
        Ok(version) => Ok(version),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e),
    }
}

/// Run any pending migrations
pub fn run_migrations(conn: &Connection) -> SqliteResult<()> {
    let current_version = get_schema_version(conn)?;
    
    // Add migration functions here as schema evolves
    // if current_version < 2 {
    //     migrate_v1_to_v2(conn)?;
    // }
    
    tracing::info!("Database at schema version {}", current_version);
    Ok(())
}
```

---

## Step 5: Models

### File: `src/models/mod.rs`

```rust
//! Data models for PhotoVault

pub mod photo;

pub use photo::Photo;
```

### File: `src/models/photo.rs`

```rust
//! Photo data model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a photo in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: i64,
    
    // File info
    pub file_path: String,
    pub file_name: String,
    pub file_hash: String,
    pub file_size: i64,
    
    // EXIF data
    pub date_taken: Option<DateTime<Utc>>,
    pub date_taken_source: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,
    
    // Processing state
    pub thumbnail_path: Option<String>,
    pub faces_processed: bool,
    
    // Soft delete
    pub is_trashed: bool,
    pub trashed_at: Option<DateTime<Utc>>,
    
    // Timestamps
    pub indexed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Photo {
    /// Get the display date for this photo
    pub fn display_date(&self) -> Option<DateTime<Utc>> {
        self.date_taken
    }
    
    /// Check if this photo has GPS coordinates
    pub fn has_location(&self) -> bool {
        self.gps_latitude.is_some() && self.gps_longitude.is_some()
    }
    
    /// Get a human-readable location string
    pub fn location_string(&self) -> Option<String> {
        match (&self.location_city, &self.location_country) {
            (Some(city), Some(country)) => Some(format!("{}, {}", city, country)),
            (Some(city), None) => Some(city.clone()),
            (None, Some(country)) => Some(country.clone()),
            (None, None) => None,
        }
    }
}
```

---

## Step 6: Drive Detection Service

### File: `src/services/mod.rs`

```rust
//! Application services

pub mod drive_detector;

pub use drive_detector::DriveDetector;
```

### File: `src/services/drive_detector.rs`

```rust
//! Drive detection service
//! 
//! Detects mounted external drives and directories that can be indexed.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Information about a detected drive or folder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_removable: bool,
    pub has_photovault_db: bool,
    pub total_size_bytes: Option<u64>,
    pub free_space_bytes: Option<u64>,
}

/// Drive detection service
pub struct DriveDetector;

impl DriveDetector {
    /// Detect available drives/mount points
    pub fn detect() -> Vec<DriveInfo> {
        let mut drives = Vec::new();
        
        #[cfg(target_os = "linux")]
        {
            // Check /media and /mnt for mounted drives
            for base in &["/media", "/mnt", "/run/media"] {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        // For /media and /run/media, we need to go one level deeper (user folder)
                        if base == &"/media" || base.starts_with("/run/media") {
                            if let Ok(user_entries) = std::fs::read_dir(entry.path()) {
                                for user_entry in user_entries.flatten() {
                                    if let Some(drive) = Self::check_path(user_entry.path()) {
                                        drives.push(drive);
                                    }
                                }
                            }
                        } else {
                            if let Some(drive) = Self::check_path(entry.path()) {
                                drives.push(drive);
                            }
                        }
                    }
                }
            }
            
            // Also check home directory as a valid target
            if let Some(home) = dirs::home_dir() {
                if let Some(drive) = Self::check_path(home.join("Pictures")) {
                    drives.push(drive);
                }
            }
        }
        
        #[cfg(target_os = "macos")]
        {
            // Check /Volumes for mounted drives
            if let Ok(entries) = std::fs::read_dir("/Volumes") {
                for entry in entries.flatten() {
                    if let Some(drive) = Self::check_path(entry.path()) {
                        drives.push(drive);
                    }
                }
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            // Check drive letters
            for letter in 'A'..='Z' {
                let path = PathBuf::from(format!("{}:\\", letter));
                if path.exists() {
                    if let Some(drive) = Self::check_path(path) {
                        drives.push(drive);
                    }
                }
            }
        }
        
        drives
    }
    
    /// Check if a path is a valid drive/folder for indexing
    fn check_path(path: PathBuf) -> Option<DriveInfo> {
        if !path.exists() || !path.is_dir() {
            return None;
        }
        
        // Check if it's readable
        if std::fs::read_dir(&path).is_err() {
            return None;
        }
        
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        
        let has_photovault_db = path.join(".photovault").join("photovault.db").exists();
        
        Some(DriveInfo {
            name,
            path,
            is_removable: true, // Simplified - could detect properly
            has_photovault_db,
            total_size_bytes: None, // Could use platform-specific APIs
            free_space_bytes: None,
        })
    }
}
```

---

## Step 7: UI Components

### File: `src/components/mod.rs`

```rust
//! Reusable UI components

pub mod sidebar;
pub mod drive_picker;

pub use sidebar::Sidebar;
pub use drive_picker::DrivePicker;
```

### File: `src/components/sidebar.rs`

```rust
//! Navigation sidebar component
//! 
//! A refined, minimal sidebar with icon-based navigation.
//! Aesthetic: Clean vertical bar with subtle hover states.

use iced::widget::{button, column, container, text, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::colors::{Backgrounds, Text, Accent, Border};
use crate::app::{Message, View};

/// Sidebar navigation component
pub struct Sidebar;

impl Sidebar {
    /// Render the sidebar
    pub fn view(current_view: &View) -> Element<'static, Message> {
        let nav_items = column![
            Self::nav_button("Timeline", View::Timeline, current_view),
            Self::nav_button("People", View::People, current_view),
            Self::nav_button("Search", View::Search, current_view),
            Space::with_height(Length::Fill),
            Self::nav_button("Settings", View::Settings, current_view),
        ]
        .spacing(4)
        .padding(Padding::from([16, 8]));
        
        container(nav_items)
            .width(Length::Fixed(200.0))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::SECONDARY.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
    
    /// Create a navigation button
    fn nav_button(label: &str, target: View, current: &View) -> Element<'static, Message> {
        let is_active = std::mem::discriminant(&target) == std::mem::discriminant(current);
        
        let label_color = if is_active {
            Text::PRIMARY
        } else {
            Text::SECONDARY
        };
        
        let btn = button(
            text(label)
                .size(14)
                .color(label_color)
        )
        .padding(Padding::from([10, 12]))
        .width(Length::Fill)
        .style(move |_theme, status| {
            let background = match status {
                button::Status::Active if is_active => Some(Backgrounds::ACTIVE.into()),
                button::Status::Active => None,
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                button::Status::Pressed => Some(Backgrounds::ACTIVE.into()),
                button::Status::Disabled => None,
            };
            
            button::Style {
                background,
                text_color: label_color,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::NavigateTo(target));
        
        btn.into()
    }
}
```

### File: `src/components/drive_picker.rs`

```rust
//! Drive picker component
//! 
//! Allows users to select a drive or folder to index.

use iced::widget::{button, column, container, row, text, Space, scrollable};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::colors::{Backgrounds, Text, Accent, Border};
use crate::services::DriveInfo;
use crate::app::Message;

/// Drive picker component
pub struct DrivePicker;

impl DrivePicker {
    /// Render the drive picker
    pub fn view(drives: &[DriveInfo]) -> Element<'static, Message> {
        let title = text("Select a folder to index")
            .size(20)
            .color(Text::PRIMARY);
        
        let subtitle = text("Choose a drive or folder containing your photos")
            .size(14)
            .color(Text::SECONDARY);
        
        let drive_list: Element<'static, Message> = if drives.is_empty() {
            container(
                text("No drives detected")
                    .size(14)
                    .color(Text::TERTIARY)
            )
            .padding(20)
            .into()
        } else {
            let items: Vec<Element<'static, Message>> = drives
                .iter()
                .map(|drive| Self::drive_item(drive.clone()))
                .collect();
            
            scrollable(
                column(items).spacing(8)
            )
            .height(Length::Fixed(300.0))
            .into()
        };
        
        let browse_button = button(
            text("Browse for folder...")
                .size(14)
                .color(Text::PRIMARY)
        )
        .padding(Padding::from([10, 16]))
        .style(|_theme, status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                button::Status::Pressed => Some(Backgrounds::ACTIVE.into()),
                _ => Some(Backgrounds::ELEVATED.into()),
            };
            
            button::Style {
                background,
                text_color: Text::PRIMARY,
                border: iced::Border {
                    color: Border::VISIBLE,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::BrowseForFolder);
        
        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(24),
            drive_list,
            Space::with_height(16),
            browse_button,
        ]
        .align_x(Alignment::Center);
        
        container(content)
            .padding(40)
            .into()
    }
    
    /// Render a single drive item
    fn drive_item(drive: DriveInfo) -> Element<'static, Message> {
        let status_text = if drive.has_photovault_db {
            text("Previously indexed")
                .size(12)
                .color(Accent::PRIMARY)
        } else {
            text("Not indexed")
                .size(12)
                .color(Text::TERTIARY)
        };
        
        let info = column![
            text(&drive.name)
                .size(14)
                .color(Text::PRIMARY),
            text(drive.path.to_string_lossy())
                .size(12)
                .color(Text::SECONDARY),
            status_text,
        ]
        .spacing(4);
        
        let path = drive.path.clone();
        
        button(
            container(info)
                .padding(Padding::from([12, 16]))
                .width(Length::Fill)
        )
        .width(Length::Fixed(400.0))
        .style(|_theme, status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                button::Status::Pressed => Some(Backgrounds::ACTIVE.into()),
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
        .on_press(Message::SelectDrive(path))
        .into()
    }
}
```

---

## Step 8: Views

### File: `src/views/mod.rs`

```rust
//! Application views

pub mod welcome;
pub mod timeline;
pub mod people;
pub mod search;
pub mod settings;

pub use welcome::WelcomeView;
pub use timeline::TimelineView;
pub use people::PeopleView;
pub use search::SearchView;
pub use settings::SettingsView;
```

### File: `src/views/welcome.rs`

```rust
//! Welcome view - shown when no drive is selected
//! 
//! A clean, inviting screen that guides users to select their photo library.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::theme::colors::{Backgrounds, Text, Accent};
use crate::components::DrivePicker;
use crate::services::DriveInfo;
use crate::app::Message;

/// Welcome view component
pub struct WelcomeView;

impl WelcomeView {
    /// Render the welcome view
    pub fn view(drives: &[DriveInfo]) -> Element<'static, Message> {
        let logo = text("PhotoVault")
            .size(48)
            .color(Text::PRIMARY);
        
        let tagline = text("Your photos. Your drive. Your privacy.")
            .size(16)
            .color(Text::SECONDARY);
        
        let content = column![
            logo,
            Space::with_height(8),
            tagline,
            Space::with_height(48),
            DrivePicker::view(drives),
        ]
        .align_x(Alignment::Center);
        
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
}
```

### File: `src/views/timeline.rs`

```rust
//! Timeline view - main photo browsing interface
//! 
//! Placeholder for Phase 2 implementation.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::theme::colors::{Backgrounds, Text};
use crate::app::Message;

/// Timeline view component
pub struct TimelineView;

impl TimelineView {
    /// Render the timeline view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("Timeline")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Your photos will appear here, organized by date.")
                .size(14)
                .color(Text::SECONDARY),
            Space::with_height(32),
            text("Select a folder to start indexing...")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);
        
        container(content)
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

### File: `src/views/people.rs`

```rust
//! People view - face clusters
//! 
//! Placeholder for Phase 4 implementation.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::theme::colors::{Backgrounds, Text};
use crate::app::Message;

/// People view component
pub struct PeopleView;

impl PeopleView {
    /// Render the people view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("People")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Recognized faces will appear here.")
                .size(14)
                .color(Text::SECONDARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);
        
        container(content)
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

### File: `src/views/search.rs`

```rust
//! Search view
//! 
//! Placeholder for Phase 6 implementation.

use iced::widget::{column, container, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::theme::colors::{Backgrounds, Text};
use crate::app::Message;

/// Search view component
pub struct SearchView;

impl SearchView {
    /// Render the search view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("Search")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Search by date, location, or people.")
                .size(14)
                .color(Text::SECONDARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);
        
        container(content)
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

### File: `src/views/settings.rs`

```rust
//! Settings view
//! 
//! Application settings and preferences.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::theme::colors::{Backgrounds, Text};
use crate::app::Message;

/// Settings view component
pub struct SettingsView;

impl SettingsView {
    /// Render the settings view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("Settings")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Configure PhotoVault preferences.")
                .size(14)
                .color(Text::SECONDARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);
        
        container(content)
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

## Step 9: Main Application

### File: `src/app.rs`

```rust
//! Main application state and logic

use std::path::PathBuf;
use iced::widget::{column, container, row, Space};
use iced::{Element, Length, Task};

use crate::components::Sidebar;
use crate::services::{DriveDetector, DriveInfo};
use crate::views::{WelcomeView, TimelineView, PeopleView, SearchView, SettingsView};
use crate::theme::colors::Backgrounds;
use crate::db::Database;

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Timeline,
    People,
    Search,
    Settings,
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
    database: Option<Database>,
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
}

impl PhotoVault {
    /// Create new application instance
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            current_view: View::Welcome,
            drives: Vec::new(),
            selected_drive: None,
            database: None,
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
                            if let Err(e) = crate::db::create_schema(&db.conn) {
                                tracing::error!("Failed to create schema: {}", e);
                            }
                        }
                        
                        self.database = Some(db);
                        self.selected_drive = Some(path);
                        self.current_view = View::Timeline;
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database: {}", e);
                    }
                }
                
                Task::none()
            }
            
            Message::BrowseForFolder => {
                // TODO: Implement native folder picker
                // For now, this is a placeholder
                tracing::info!("Browse for folder requested");
                Task::none()
            }
            
            Message::FolderSelected(path) => {
                if let Some(path) = path {
                    return self.update(Message::SelectDrive(path));
                }
                Task::none()
            }
            
            Message::RefreshDrives => {
                Task::perform(
                    async { DriveDetector::detect() },
                    Message::DrivesDetected,
                )
            }
            
            Message::DrivesDetected(drives) => {
                tracing::info!("Detected {} drives", drives.len());
                self.drives = drives;
                Task::none()
            }
        }
    }
    
    /// Render the application
    pub fn view(&self) -> Element<Message> {
        // If no drive selected, show welcome screen
        if self.selected_drive.is_none() {
            return WelcomeView::view(&self.drives);
        }
        
        // Main layout: sidebar + content
        let sidebar = Sidebar::view(&self.current_view);
        
        let content = match self.current_view {
            View::Welcome => WelcomeView::view(&self.drives),
            View::Timeline => TimelineView::view(),
            View::People => PeopleView::view(),
            View::Search => SearchView::view(),
            View::Settings => SettingsView::view(),
        };
        
        let layout = row![
            sidebar,
            content,
        ];
        
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

### File: `src/main.rs`

```rust
//! PhotoVault - Offline Photo Library Manager
//! 
//! A desktop application for organizing and browsing photos from external drives.

mod app;
mod theme;
mod views;
mod components;
mod db;
mod services;
mod models;

use iced::{Application, Settings, Size};

fn main() -> iced::Result {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("photovault=debug,iced=warn")
        .init();
    
    tracing::info!("Starting PhotoVault...");
    
    // Run the application
    iced::application(app::PhotoVault::title, app::PhotoVault::update, app::PhotoVault::view)
        .window_size(Size::new(1200.0, 800.0))
        .antialiasing(true)
        .run_with(app::PhotoVault::new)
}
```

---

## Step 10: Download Fonts

Download these fonts and place in `assets/fonts/`:

1. **Inter** (Google Fonts): https://fonts.google.com/specimen/Inter
   - Inter-Regular.ttf
   - Inter-Medium.ttf
   - Inter-SemiBold.ttf

2. **JetBrains Mono**: https://www.jetbrains.com/lp/mono/
   - JetBrainsMono-Regular.ttf

---

## Verification Checklist

After completing this phase, verify:

- [ ] `cargo build` completes without errors
- [ ] `cargo run` launches the application
- [ ] Welcome screen displays with "PhotoVault" title
- [ ] Drive picker shows detected drives (or "No drives detected")
- [ ] Clicking a drive creates `.photovault/photovault.db`
- [ ] Sidebar navigation works (Timeline, People, Search, Settings)
- [ ] Dark theme is applied consistently
- [ ] Window is resizable and content adapts

---

## Common Issues & Solutions

### Issue: "Font not found"
**Solution:** Ensure fonts are in `assets/fonts/` with exact filenames. The `include_bytes!` macro will fail at compile time if files are missing.

### Issue: "Cannot find crate `iced`"
**Solution:** Ensure `Cargo.toml` has iced with correct features. Run `cargo update`.

### Issue: "No drives detected"
**Solution:** On Linux, check if you have permissions to read `/media`, `/mnt`. Try adding your home Pictures folder as a test.

### Issue: Database error on drive selection
**Solution:** Ensure the target directory is writable. Check console for specific error.

---

## Next Phase Preview

**Phase 2: Directory Scanning & EXIF Extraction** will add:
- Recursive file discovery
- EXIF metadata extraction
- Progress reporting UI
- SHA256 hashing for duplicate detection

---

## Code Quality Notes

- All code follows Rust idioms (ownership, error handling)
- Comments explain "why", not "what"
- Modules are organized by responsibility
- Theme system allows easy customization
- Database layer is abstracted for testability

---

## Expected Results & Behavior

> **IMPORTANT:** Do not proceed to Phase 2 until a human has verified ALL of the following behaviors. Each item must be manually tested and confirmed.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **App Launch** | Window opens at 1200x800, dark theme background visible |
| **Welcome Screen** | "PhotoVault" title centered, tagline visible below |
| **Drive List** | At least one drive or folder option shown (may be empty list with message) |
| **Color Scheme** | Dark background (#121214), warm amber accents (#E5A83B) |
| **Typography** | Inter font renders cleanly, proper hierarchy (title > body > tertiary) |
| **Spacing** | Consistent 8px grid spacing, nothing looks cramped |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **Click drive/folder** | Database created at `<drive>/.photovault/photovault.db`, transitions to Timeline view |
| **Click sidebar "Timeline"** | Timeline view content area shows placeholder text |
| **Click sidebar "People"** | People view content area shows placeholder text |
| **Click sidebar "Search"** | Search view content area shows placeholder text |
| **Click sidebar "Settings"** | Settings view content area shows placeholder text |
| **Resize window** | Content adapts, sidebar remains fixed width |
| **Minimize/restore** | App state preserved, no visual glitches |

### Database Verification

Run this command after selecting a drive:

```bash
sqlite3 /path/to/drive/.photovault/photovault.db ".tables"
```

**Expected output:**
```
burst_group_members   duplicate_groups      photos
burst_groups          face_clusters         schema_version
duplicate_group_members  faces              trash
```

### Console Verification

When running `cargo run`, check the terminal output:

```
INFO photovault: Starting PhotoVault...
INFO photovault: Detected X drives
INFO photovault: Selected drive: /path/to/drive
```

No `ERROR` or `WARN` messages should appear during normal operation.

### Sign-off Checklist

Before proceeding to Phase 2, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **App launches:** Window appears with correct dimensions
- [ ] **Theme correct:** Dark mode with amber accents matches spec
- [ ] **Navigation works:** All sidebar items switch views
- [ ] **Database created:** `.photovault/photovault.db` exists after drive selection
- [ ] **Schema complete:** All 9 tables created in database
- [ ] **No console errors:** Clean startup and operation logs
- [ ] **SKILL.md followed:** UI matches design guidelines (typography, spacing, colors)

**Signature:** ___________________ **Date:** _______________

---

## Proceed to Phase 2

Only after ALL items above are verified, proceed to:

📁 `plan/PHASE_2_SCANNING_EXIF.md`