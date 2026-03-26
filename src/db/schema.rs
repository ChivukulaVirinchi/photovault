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

INSERT INTO schema_version (version) VALUES (4);

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
    iso INTEGER,                       -- ISO sensitivity
    aperture TEXT,                     -- e.g. "f/2.8"
    shutter_speed TEXT,                -- e.g. "1/125"
    focal_length TEXT,                 -- e.g. "50mm"
    lens_model TEXT,                   -- e.g. "iPhone 15 Pro back camera"
    flash TEXT,                        -- "Fired" or "Off"
    gps_altitude REAL,                 -- meters above/below sea level
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
-- TRASH TABLE
-- Tracks soft-deleted photos for restore/permanent delete
-- ============================================================

CREATE TABLE IF NOT EXISTS trash (
    id INTEGER PRIMARY KEY,
    photo_id INTEGER NOT NULL UNIQUE,
    original_path TEXT NOT NULL,
    trashed_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
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
CREATE INDEX IF NOT EXISTS idx_face_clusters_name ON face_clusters(name);

-- Duplicate and burst group members
CREATE INDEX IF NOT EXISTS idx_dup_members_group ON duplicate_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_dup_members_photo ON duplicate_group_members(photo_id);
CREATE INDEX IF NOT EXISTS idx_burst_members_group ON burst_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_burst_members_photo ON burst_group_members(photo_id);
CREATE INDEX IF NOT EXISTS idx_trash_trashed_at ON trash(trashed_at);
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
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='photos'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
    }
}
