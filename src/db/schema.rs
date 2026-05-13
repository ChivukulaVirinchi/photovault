//! Database schema creation
//!
//! Creates all tables needed for Smriti. The schema is designed to:
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

INSERT INTO schema_version (version) VALUES (20);

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
    metadata_extracted BOOLEAN DEFAULT FALSE,
    thumbnailed BOOLEAN DEFAULT FALSE,
    brightness REAL,                   -- Average luma in [0,1], cached from face pipeline
    phash INTEGER,                     -- 64-bit DCT perceptual hash for near-duplicate detection
    content_category TEXT DEFAULT 'photo', -- 'photo' | 'document' | 'screenshot' | 'presentation' | 'whiteboard' | 'receipt'
    ocr_text TEXT,
    ocr_processed BOOLEAN DEFAULT FALSE,
    ocr_confidence REAL,

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

    -- Interactive feedback: 0=untouched, 1=user confirmed, -1=user rejected from a candidate
    user_confirmed INTEGER DEFAULT 0,

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
    photo_count INTEGER DEFAULT 0,

    -- 1 if the user has explicitly named/confirmed this cluster
    is_user_named INTEGER DEFAULT 0,

    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (representative_face_id) REFERENCES faces(id) ON DELETE SET NULL
);

-- ============================================================
-- INFERRED IDENTITIES
-- Contextual person links for photos without visible faces
-- ============================================================

CREATE TABLE IF NOT EXISTS photo_inferred_identities (
    id INTEGER PRIMARY KEY,
    photo_id INTEGER NOT NULL,
    cluster_id INTEGER NOT NULL,
    source_photo_id INTEGER NOT NULL,
    confidence REAL NOT NULL,
    is_inferred BOOLEAN DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
    FOREIGN KEY (source_photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(photo_id, cluster_id)
);

CREATE TABLE IF NOT EXISTS person_gallery_embeddings (
    id INTEGER PRIMARY KEY,
    cluster_id INTEGER NOT NULL,
    face_id INTEGER NOT NULL,
    embedding BLOB NOT NULL,
    pose_label TEXT,
    quality_score REAL,
    -- 'auto' = chosen by diversity sampling, 'user_confirmed' = sticky user decision
    source TEXT DEFAULT 'auto',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
    FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE,
    UNIQUE(cluster_id, face_id)
);

-- ============================================================
-- INTERACTIVE FEEDBACK
-- User-driven constraints and review queue for face recognition
-- ============================================================

CREATE TABLE IF NOT EXISTS cluster_cannot_merge (
    id INTEGER PRIMARY KEY,
    cluster_a_id INTEGER NOT NULL,
    cluster_b_id INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (cluster_a_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_b_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
    UNIQUE(cluster_a_id, cluster_b_id)
);

CREATE TABLE IF NOT EXISTS face_review_queue (
    id INTEGER PRIMARY KEY,
    face_id INTEGER NOT NULL,
    candidate_cluster_id INTEGER NOT NULL,
    score REAL NOT NULL,               -- top match score (higher = better)
    ambiguity REAL,                    -- score margin to 2nd candidate (lower = more ambiguous)
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    resolved_at DATETIME,
    resolved_as TEXT,                  -- 'same' | 'different' | 'skipped'

    FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE,
    FOREIGN KEY (candidate_cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
    UNIQUE(face_id, candidate_cluster_id)
);

CREATE TABLE IF NOT EXISTS face_negatives (
    face_id        INTEGER NOT NULL,
    not_cluster_id INTEGER NOT NULL,
    created_at     INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    PRIMARY KEY (face_id, not_cluster_id),
    FOREIGN KEY (face_id)        REFERENCES faces(id)         ON DELETE CASCADE,
    FOREIGN KEY (not_cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE
);

-- ============================================================
-- MEMORY BLOCKS
-- User preferences for the Memories feature: hide memories involving
-- specific people, or (future) specific date ranges.
-- ============================================================

CREATE TABLE IF NOT EXISTS memory_blocks (
    id              INTEGER PRIMARY KEY,
    kind            TEXT NOT NULL,      -- 'person' (v1); 'date_range' future
    target_key      TEXT NOT NULL,      -- cluster_id as string, or "MM-DD..MM-DD"
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(kind, target_key)
);

-- ============================================================
-- ALBUMS
-- User-created photo collections
-- ============================================================

CREATE TABLE IF NOT EXISTS albums (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    cover_photo_id INTEGER,
    cover_auto_picked BOOLEAN DEFAULT TRUE,
    photo_count INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS album_photos (
    id INTEGER PRIMARY KEY,
    album_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    added_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(album_id, photo_id)
);

-- ============================================================
-- ALBUM SUGGESTIONS
-- Auto-detected trip/event album proposals
-- ============================================================

CREATE TABLE IF NOT EXISTS album_suggestions (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,                 -- 'trip' | 'event'
    title TEXT NOT NULL,
    photo_ids_json TEXT NOT NULL,       -- JSON array of photo IDs
    cover_photo_id INTEGER,
    fingerprint TEXT NOT NULL,          -- Hash of sorted photo IDs
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'accepted' | 'dismissed'
    seen_count INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
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
-- OCR SEARCH INDEX
-- FTS5 index over extracted OCR text
-- ============================================================

CREATE VIRTUAL TABLE IF NOT EXISTS photos_fts USING fts5(
    ocr_text,
    content='photos',
    content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS photos_fts_insert AFTER INSERT ON photos BEGIN
    INSERT INTO photos_fts(rowid, ocr_text) VALUES (new.id, COALESCE(new.ocr_text, ''));
END;

CREATE TRIGGER IF NOT EXISTS photos_fts_update AFTER UPDATE OF ocr_text ON photos BEGIN
    UPDATE photos_fts SET ocr_text = COALESCE(new.ocr_text, '') WHERE rowid = new.id;
END;

CREATE TRIGGER IF NOT EXISTS photos_fts_delete AFTER DELETE ON photos BEGIN
    DELETE FROM photos_fts WHERE rowid = old.id;
END;

-- ============================================================
-- RECENT SEARCHES
-- Per-library search history (last N queries)
-- ============================================================

CREATE TABLE IF NOT EXISTS recent_searches (
    id INTEGER PRIMARY KEY,
    query TEXT NOT NULL,
    last_used DATETIME DEFAULT CURRENT_TIMESTAMP,
    use_count INTEGER DEFAULT 1,
    UNIQUE(query)
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
CREATE INDEX IF NOT EXISTS idx_photos_content_category ON photos(content_category);
CREATE INDEX IF NOT EXISTS idx_photos_ocr_processed ON photos(ocr_processed);
CREATE INDEX IF NOT EXISTS idx_photos_metadata_extracted
    ON photos(metadata_extracted) WHERE metadata_extracted = FALSE;
CREATE INDEX IF NOT EXISTS idx_photos_thumbnailed
    ON photos(thumbnailed) WHERE thumbnailed = FALSE;

-- Faces
CREATE INDEX IF NOT EXISTS idx_faces_cluster ON faces(cluster_id);
CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_face_clusters_name ON face_clusters(name);
CREATE INDEX IF NOT EXISTS idx_inferred_photo ON photo_inferred_identities(photo_id);
CREATE INDEX IF NOT EXISTS idx_inferred_cluster ON photo_inferred_identities(cluster_id);
CREATE INDEX IF NOT EXISTS idx_inferred_is_inferred ON photo_inferred_identities(is_inferred);
CREATE INDEX IF NOT EXISTS idx_gallery_cluster ON person_gallery_embeddings(cluster_id);
CREATE INDEX IF NOT EXISTS idx_gallery_face ON person_gallery_embeddings(face_id);
CREATE INDEX IF NOT EXISTS idx_gallery_source ON person_gallery_embeddings(source);
CREATE INDEX IF NOT EXISTS idx_cannot_merge_a ON cluster_cannot_merge(cluster_a_id);
CREATE INDEX IF NOT EXISTS idx_cannot_merge_b ON cluster_cannot_merge(cluster_b_id);
CREATE INDEX IF NOT EXISTS idx_review_queue_face ON face_review_queue(face_id);
CREATE INDEX IF NOT EXISTS idx_review_queue_cluster ON face_review_queue(candidate_cluster_id);
CREATE INDEX IF NOT EXISTS idx_review_queue_unresolved ON face_review_queue(resolved_at)
    WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_face_negatives_cluster ON face_negatives(not_cluster_id);
CREATE INDEX IF NOT EXISTS idx_memory_blocks_kind ON memory_blocks(kind);
-- Duplicate and burst group members
CREATE INDEX IF NOT EXISTS idx_dup_members_group ON duplicate_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_dup_members_photo ON duplicate_group_members(photo_id);
CREATE INDEX IF NOT EXISTS idx_burst_members_group ON burst_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_burst_members_photo ON burst_group_members(photo_id);
CREATE INDEX IF NOT EXISTS idx_trash_trashed_at ON trash(trashed_at);

-- Albums
CREATE INDEX IF NOT EXISTS idx_album_photos_album ON album_photos(album_id);
CREATE INDEX IF NOT EXISTS idx_album_photos_photo ON album_photos(photo_id);

-- Album suggestions
CREATE INDEX IF NOT EXISTS idx_album_suggestions_status ON album_suggestions(status);
CREATE INDEX IF NOT EXISTS idx_album_suggestions_fingerprint ON album_suggestions(fingerprint);

-- Recent searches
CREATE INDEX IF NOT EXISTS idx_recent_searches_used ON recent_searches(last_used DESC);
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
