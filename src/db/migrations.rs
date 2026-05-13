//! Database migrations for schema versioning

use rusqlite::{Connection, Result as SqliteResult};

/// The highest schema version this binary knows how to produce. Bump
/// this in lockstep with each new `migrate_vN_to_vM` (and add the
/// matching `if current_version < N` line in `run_migrations`).
///
/// `run_migrations` refuses to open a DB whose `schema_version` is
/// higher than this — that would mean a newer build wrote it, and
/// blindly reading would expose missing tables / columns to old code.
pub const MAX_KNOWN_SCHEMA_VERSION: i32 = 21;

/// Get the current schema version
pub fn get_schema_version(conn: &Connection) -> SqliteResult<i32> {
    let result = conn.query_row("SELECT MAX(version) FROM schema_version", [], |row| {
        row.get(0)
    });

    match result {
        Ok(version) => Ok(version),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e),
    }
}

/// Distinct error returned by `run_migrations` when the DB is newer
/// than this binary supports. Surfaced to the user with a friendlier
/// message than a generic SQLite error.
#[derive(Debug)]
pub struct SchemaTooNewError {
    pub db_version: i32,
    pub max_supported: i32,
}

impl std::fmt::Display for SchemaTooNewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Library was created by a newer version of Smriti \
             (schema v{}). This build only supports up to v{}. \
             Please update Smriti to open this library.",
            self.db_version, self.max_supported
        )
    }
}

impl std::error::Error for SchemaTooNewError {}

/// Run any pending migrations
pub fn run_migrations(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let current_version = get_schema_version(conn).unwrap_or(0);

    // Forward-compat guard: don't read schemas this binary doesn't
    // know about. Better to refuse opening than to silently
    // mis-interpret unfamiliar columns or miss new tables.
    if current_version > MAX_KNOWN_SCHEMA_VERSION {
        return Err(Box::new(SchemaTooNewError {
            db_version: current_version,
            max_supported: MAX_KNOWN_SCHEMA_VERSION,
        }));
    }

    if current_version < 2 {
        migrate_v1_to_v2(conn)?;
    }
    if current_version < 3 {
        migrate_v2_to_v3(conn)?;
    }
    if current_version < 4 {
        migrate_v3_to_v4(conn)?;
    }
    if current_version < 5 {
        migrate_v4_to_v5(conn)?;
    }
    if current_version < 6 {
        migrate_v5_to_v6(conn)?;
    }
    if current_version < 7 {
        migrate_v6_to_v7(conn)?;
    }
    if current_version < 8 {
        migrate_v7_to_v8(conn)?;
    }
    if current_version < 9 {
        migrate_v8_to_v9(conn)?;
    }
    if current_version < 10 {
        migrate_v9_to_v10(conn)?;
    }
    if current_version < 11 {
        migrate_v10_to_v11(conn)?;
    }
    if current_version < 12 {
        migrate_v11_to_v12(conn)?;
    }
    if current_version < 13 {
        migrate_v12_to_v13(conn)?;
    }
    if current_version < 14 {
        migrate_v13_to_v14(conn)?;
    }
    if current_version < 15 {
        migrate_v14_to_v15(conn)?;
    }
    if current_version < 16 {
        migrate_v15_to_v16(conn)?;
    }
    if current_version < 17 {
        migrate_v16_to_v17(conn)?;
    }
    if current_version < 18 {
        migrate_v17_to_v18(conn)?;
    }
    if current_version < 19 {
        migrate_v18_to_v19(conn)?;
    }
    if current_version < 20 {
        migrate_v19_to_v20(conn)?;
    }
    if current_version < 21 {
        migrate_v20_to_v21(conn)?;
    }
    let updated_version = get_schema_version(conn).unwrap_or(current_version);
    tracing::info!("Database at schema version {}", updated_version);
    Ok(())
}

fn migrate_v18_to_v19(conn: &Connection) -> SqliteResult<()> {
    // Streaming scanner pipeline stage flags.
    // Existing rows: anything inserted by the legacy scanner already
    // has EXIF and (if successful) a thumbnail. Mark them done so the
    // new workers don't re-process them.
    let tx = conn.unchecked_transaction()?;
    for (col, def) in &[
        ("metadata_extracted", "BOOLEAN DEFAULT FALSE"),
        ("thumbnailed", "BOOLEAN DEFAULT FALSE"),
    ] {
        let sql = format!("ALTER TABLE photos ADD COLUMN {} {}", col, def);
        match tx.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
    }
    tx.execute(
        "UPDATE photos SET metadata_extracted = TRUE WHERE date_taken IS NOT NULL OR camera_make IS NOT NULL",
        [],
    )?;
    tx.execute(
        "UPDATE photos SET thumbnailed = TRUE WHERE thumbnail_path IS NOT NULL",
        [],
    )?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_photos_metadata_extracted ON photos(metadata_extracted) WHERE metadata_extracted = FALSE",
        [],
    )?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_photos_thumbnailed ON photos(thumbnailed) WHERE thumbnailed = FALSE",
        [],
    )?;
    tx.execute("INSERT INTO schema_version (version) VALUES (19)", [])?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 19 (scanner pipeline stages)");
    Ok(())
}

fn migrate_v19_to_v20(conn: &Connection) -> SqliteResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS face_negatives (
            face_id        INTEGER NOT NULL,
            not_cluster_id INTEGER NOT NULL,
            created_at     INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            PRIMARY KEY (face_id, not_cluster_id),
            FOREIGN KEY (face_id)        REFERENCES faces(id)         ON DELETE CASCADE,
            FOREIGN KEY (not_cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_face_negatives_cluster ON face_negatives(not_cluster_id);
        INSERT INTO schema_version (version) VALUES (20);
        "#,
    )?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 20 (face_negatives)");
    Ok(())
}

fn migrate_v20_to_v21(conn: &Connection) -> SqliteResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS face_processing_stats (
            id                INTEGER PRIMARY KEY CHECK (id = 1),
            rejected_small    INTEGER NOT NULL DEFAULT 0,
            rejected_lowconf  INTEGER NOT NULL DEFAULT 0,
            rejected_blurry   INTEGER NOT NULL DEFAULT 0,
            rejected_yaw      INTEGER NOT NULL DEFAULT 0,
            completed_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );
        INSERT INTO schema_version (version) VALUES (21);
        "#,
    )?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 21 (face_processing_stats)");
    Ok(())
}

fn migrate_v17_to_v18(conn: &Connection) -> SqliteResult<()> {
    // Perceptual hash for near-duplicate detection. 64-bit DCT phash;
    // populated lazily by the thumbnail pipeline (see services/thumbnail.rs)
    // and read by DuplicateDetector::find_perceptual_duplicates.
    let tx = conn.unchecked_transaction()?;
    match tx.execute("ALTER TABLE photos ADD COLUMN phash INTEGER", []) {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("duplicate column") {
                return Err(e);
            }
        }
    }
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_photos_phash ON photos(phash) WHERE phash IS NOT NULL",
        [],
    )?;
    tx.execute("INSERT INTO schema_version (version) VALUES (18)", [])?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 18 (photos.phash)");
    Ok(())
}

fn migrate_v16_to_v17(conn: &Connection) -> SqliteResult<()> {
    // Persist average brightness per photo. Previously face_processor
    // recomputed it every run; now it's stored, queried via SQL during
    // contextual identity propagation, and reused across runs.
    let tx = conn.unchecked_transaction()?;
    match tx.execute("ALTER TABLE photos ADD COLUMN brightness REAL", []) {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("duplicate column") {
                return Err(e);
            }
        }
    }
    tx.execute("INSERT INTO schema_version (version) VALUES (17)", [])?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 17 (photos.brightness)");
    Ok(())
}

fn migrate_v15_to_v16(conn: &Connection) -> SqliteResult<()> {
    // Path-string normalization: rewrite stored relative paths to use
    // forward slashes only. Backslashes from prior Windows writes are
    // remapped so the same drive opens identically on every OS.
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        r#"
        UPDATE photos SET file_path = REPLACE(file_path, '\', '/');
        UPDATE photos SET thumbnail_path = REPLACE(thumbnail_path, '\', '/')
            WHERE thumbnail_path IS NOT NULL;
        UPDATE trash SET original_path = REPLACE(original_path, '\', '/');

        INSERT INTO schema_version (version) VALUES (16);
        "#,
    )?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 16 (forward-slash paths)");
    Ok(())
}

fn migrate_v14_to_v15(conn: &Connection) -> SqliteResult<()> {
    // Phase 2 Track A4/B2: composite indexes for hot query paths.
    // Single-column indexes exist for these columns already, but the
    // combined access pattern (filter-then-sort) hit the table before.
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_photos_trashed_date
            ON photos(is_trashed, date_taken DESC);

        CREATE INDEX IF NOT EXISTS idx_photos_faces_processed_trashed
            ON photos(faces_processed, is_trashed, date_taken DESC);

        CREATE INDEX IF NOT EXISTS idx_faces_cluster_confidence
            ON faces(cluster_id, confidence DESC, id);

        CREATE INDEX IF NOT EXISTS idx_faces_photo_cluster
            ON faces(photo_id, cluster_id);

        INSERT INTO schema_version (version) VALUES (15);
        "#,
    )?;
    tracing::info!("Migrated database to schema version 15 (composite indexes)");
    Ok(())
}

fn migrate_v13_to_v14(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS recent_searches (
            id INTEGER PRIMARY KEY,
            query TEXT NOT NULL,
            last_used DATETIME DEFAULT CURRENT_TIMESTAMP,
            use_count INTEGER DEFAULT 1,
            UNIQUE(query)
        );

        CREATE INDEX IF NOT EXISTS idx_recent_searches_used
            ON recent_searches(last_used DESC);

        INSERT INTO schema_version (version) VALUES (14);
        "#,
    )?;
    tracing::info!("Migrated database to schema version 14 (recent searches)");
    Ok(())
}

fn migrate_v12_to_v13(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS album_suggestions (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            photo_ids_json TEXT NOT NULL,
            cover_photo_id INTEGER,
            fingerprint TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            seen_count INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_album_suggestions_status ON album_suggestions(status);
        CREATE INDEX IF NOT EXISTS idx_album_suggestions_fingerprint ON album_suggestions(fingerprint);

        INSERT INTO schema_version (version) VALUES (13);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 13 (album suggestions)");
    Ok(())
}

fn migrate_v11_to_v12(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
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

        CREATE INDEX IF NOT EXISTS idx_album_photos_album ON album_photos(album_id);
        CREATE INDEX IF NOT EXISTS idx_album_photos_photo ON album_photos(photo_id);

        INSERT INTO schema_version (version) VALUES (12);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 12 (albums)");
    Ok(())
}

fn migrate_v10_to_v11(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS memory_blocks (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            target_key TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(kind, target_key)
        );

        CREATE INDEX IF NOT EXISTS idx_memory_blocks_kind ON memory_blocks(kind);

        INSERT INTO schema_version (version) VALUES (11);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 11 (memory blocks)");
    Ok(())
}

fn migrate_v3_to_v4(conn: &Connection) -> SqliteResult<()> {
    // Add lens_model, flash, gps_altitude (these were missed in v3).
    // Wrap in an explicit transaction so a crash mid-loop doesn't leave
    // some columns added without bumping schema_version.
    let tx = conn.unchecked_transaction()?;
    let columns = [
        ("lens_model", "TEXT"),
        ("flash", "TEXT"),
        ("gps_altitude", "REAL"),
    ];

    for (col, col_type) in &columns {
        let sql = format!("ALTER TABLE photos ADD COLUMN {} {}", col, col_type);
        match tx.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
    }

    tx.execute("INSERT INTO schema_version (version) VALUES (4)", [])?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 4 (lens, flash, altitude)");
    Ok(())
}

fn migrate_v5_to_v6(conn: &Connection) -> SqliteResult<()> {
    // Atomic: ALTER loop + FTS table + triggers + version bump all in
    // one transaction. Without this, a kill mid-ALTER could land us
    // with new columns but no FTS index — confusing on next launch.
    let tx = conn.unchecked_transaction()?;

    let columns = [
        ("content_category", "TEXT DEFAULT 'photo'"),
        ("ocr_text", "TEXT"),
        ("ocr_processed", "BOOLEAN DEFAULT FALSE"),
        ("ocr_confidence", "REAL"),
    ];

    for (col, col_type) in &columns {
        let sql = format!("ALTER TABLE photos ADD COLUMN {} {}", col, col_type);
        match tx.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
    }

    tx.execute_batch(
        r#"
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

        CREATE INDEX IF NOT EXISTS idx_photos_content_category ON photos(content_category);
        CREATE INDEX IF NOT EXISTS idx_photos_ocr_processed ON photos(ocr_processed);

        INSERT INTO schema_version (version) VALUES (6);
        "#,
    )?;
    tx.commit()?;

    tracing::info!("Migrated database to schema version 6 (documents + OCR fields)");
    Ok(())
}

fn migrate_v6_to_v7(conn: &Connection) -> SqliteResult<()> {
    match conn.execute(
        "ALTER TABLE face_clusters ADD COLUMN photo_count INTEGER DEFAULT 0",
        [],
    ) {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("duplicate column") {
                return Err(e);
            }
        }
    }

    conn.execute_batch(
        r#"
        UPDATE face_clusters
        SET
            face_count = (SELECT COUNT(*) FROM faces WHERE cluster_id = face_clusters.id),
            photo_count = (
                SELECT COUNT(DISTINCT photo_id)
                FROM (
                    SELECT photo_id FROM faces WHERE cluster_id = face_clusters.id
                    UNION
                    SELECT photo_id FROM photo_inferred_identities WHERE cluster_id = face_clusters.id
                )
            ),
            representative_face_id = (
                SELECT id
                FROM faces
                WHERE cluster_id = face_clusters.id
                ORDER BY confidence DESC
                LIMIT 1
            ),
            updated_at = CURRENT_TIMESTAMP;

        INSERT INTO schema_version (version) VALUES (7);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 7 (face cluster photo counts)");
    Ok(())
}

fn migrate_v7_to_v8(conn: &Connection) -> SqliteResult<()> {
    match conn.execute(
        "ALTER TABLE photo_inferred_identities ADD COLUMN is_inferred BOOLEAN DEFAULT TRUE",
        [],
    ) {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("duplicate column") {
                return Err(e);
            }
        }
    }

    conn.execute_batch(
        r#"
        UPDATE photo_inferred_identities
        SET is_inferred = TRUE
        WHERE is_inferred IS NULL;

        CREATE INDEX IF NOT EXISTS idx_inferred_is_inferred ON photo_inferred_identities(is_inferred);

        INSERT INTO schema_version (version) VALUES (8);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 8 (inferred identity flag)");
    Ok(())
}

fn migrate_v9_to_v10(conn: &Connection) -> SqliteResult<()> {
    let add_col = |sql: &str| -> SqliteResult<()> {
        match conn.execute(sql, []) {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("duplicate column") {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    };

    add_col("ALTER TABLE faces ADD COLUMN user_confirmed INTEGER DEFAULT 0")?;
    add_col("ALTER TABLE face_clusters ADD COLUMN is_user_named INTEGER DEFAULT 0")?;
    add_col("ALTER TABLE person_gallery_embeddings ADD COLUMN source TEXT DEFAULT 'auto'")?;

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS cluster_cannot_merge (
            id INTEGER PRIMARY KEY,
            cluster_a_id INTEGER NOT NULL,
            cluster_b_id INTEGER NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (cluster_a_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
            FOREIGN KEY (cluster_b_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
            UNIQUE(cluster_a_id, cluster_b_id)
        );

        CREATE INDEX IF NOT EXISTS idx_cannot_merge_a ON cluster_cannot_merge(cluster_a_id);
        CREATE INDEX IF NOT EXISTS idx_cannot_merge_b ON cluster_cannot_merge(cluster_b_id);

        CREATE TABLE IF NOT EXISTS face_review_queue (
            id INTEGER PRIMARY KEY,
            face_id INTEGER NOT NULL,
            candidate_cluster_id INTEGER NOT NULL,
            score REAL NOT NULL,
            ambiguity REAL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            resolved_at DATETIME,
            resolved_as TEXT,
            FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE,
            FOREIGN KEY (candidate_cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
            UNIQUE(face_id, candidate_cluster_id)
        );

        CREATE INDEX IF NOT EXISTS idx_review_queue_face ON face_review_queue(face_id);
        CREATE INDEX IF NOT EXISTS idx_review_queue_cluster ON face_review_queue(candidate_cluster_id);
        CREATE INDEX IF NOT EXISTS idx_review_queue_unresolved
            ON face_review_queue(resolved_at) WHERE resolved_at IS NULL;

        CREATE INDEX IF NOT EXISTS idx_gallery_source ON person_gallery_embeddings(source);

        INSERT INTO schema_version (version) VALUES (10);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 10 (face feedback tables)");
    Ok(())
}

fn migrate_v8_to_v9(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS person_gallery_embeddings (
            id INTEGER PRIMARY KEY,
            cluster_id INTEGER NOT NULL,
            face_id INTEGER NOT NULL,
            embedding BLOB NOT NULL,
            pose_label TEXT,
            quality_score REAL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

            FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
            FOREIGN KEY (face_id) REFERENCES faces(id) ON DELETE CASCADE,
            UNIQUE(cluster_id, face_id)
        );

        CREATE INDEX IF NOT EXISTS idx_gallery_cluster ON person_gallery_embeddings(cluster_id);
        CREATE INDEX IF NOT EXISTS idx_gallery_face ON person_gallery_embeddings(face_id);

        INSERT INTO schema_version (version) VALUES (9);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 9 (person gallery embeddings)");
    Ok(())
}

fn migrate_v4_to_v5(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS photo_inferred_identities (
            id INTEGER PRIMARY KEY,
            photo_id INTEGER NOT NULL,
            cluster_id INTEGER NOT NULL,
            source_photo_id INTEGER NOT NULL,
            confidence REAL NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

            FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
            FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE CASCADE,
            FOREIGN KEY (source_photo_id) REFERENCES photos(id) ON DELETE CASCADE,
            UNIQUE(photo_id, cluster_id)
        );

        CREATE INDEX IF NOT EXISTS idx_inferred_photo ON photo_inferred_identities(photo_id);
        CREATE INDEX IF NOT EXISTS idx_inferred_cluster ON photo_inferred_identities(cluster_id);

        INSERT INTO schema_version (version) VALUES (5);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 5 (inferred identities)");
    Ok(())
}

fn migrate_v2_to_v3(conn: &Connection) -> SqliteResult<()> {
    // Add EXIF shooting parameters. Wrap the ALTER loop + version
    // bump in one transaction so partial application can't leave us
    // with some columns added but schema_version still at 2.
    let tx = conn.unchecked_transaction()?;
    let columns = [
        ("iso", "INTEGER"),
        ("aperture", "TEXT"),
        ("shutter_speed", "TEXT"),
        ("focal_length", "TEXT"),
        ("lens_model", "TEXT"),
        ("flash", "TEXT"),
        ("gps_altitude", "REAL"),
    ];

    for (col, col_type) in &columns {
        let sql = format!("ALTER TABLE photos ADD COLUMN {} {}", col, col_type);
        match tx.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                // Column may already exist if migration was partially applied
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
    }

    tx.execute("INSERT INTO schema_version (version) VALUES (3)", [])?;
    tx.commit()?;
    tracing::info!("Migrated database to schema version 3 (EXIF shooting params)");
    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS trash (
            id INTEGER PRIMARY KEY,
            photo_id INTEGER NOT NULL UNIQUE,
            original_path TEXT NOT NULL,
            trashed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_trash_trashed_at ON trash(trashed_at);
        CREATE INDEX IF NOT EXISTS idx_face_clusters_name ON face_clusters(name);

        INSERT INTO schema_version (version) VALUES (2);
        "#,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_migrate_v18_to_v19() -> rusqlite::Result<()> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY, applied_at DATETIME DEFAULT CURRENT_TIMESTAMP);
            INSERT INTO schema_version (version) VALUES (1);

            CREATE TABLE IF NOT EXISTS photos (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                file_mtime INTEGER,
                date_taken DATETIME,
                date_taken_source TEXT,
                gps_latitude REAL,
                gps_longitude REAL,
                location_city TEXT,
                location_country TEXT,
                camera_make TEXT,
                camera_model TEXT,
                iso INTEGER,
                aperture TEXT,
                shutter_speed TEXT,
                focal_length TEXT,
                lens_model TEXT,
                flash TEXT,
                gps_altitude REAL,
                width INTEGER,
                height INTEGER,
                orientation INTEGER DEFAULT 1,
                thumbnail_path TEXT,
                faces_processed BOOLEAN DEFAULT FALSE,
                content_category TEXT DEFAULT 'photo',
                ocr_text TEXT,
                ocr_processed BOOLEAN DEFAULT FALSE,
                ocr_confidence REAL,
                brightness REAL,
                phash INTEGER,
                is_trashed BOOLEAN DEFAULT FALSE,
                trashed_at DATETIME,
                indexed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(file_path)
            );
            "#,
        ).unwrap();

        // Insert a row that already has EXIF data (simulates legacy scanner)
        conn.execute(
            "INSERT INTO photos (file_path, file_name, file_hash, file_size, date_taken, camera_make, thumbnail_path, faces_processed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            rusqlite::params!["/photos/img1.jpg", "img1.jpg", "abc123", 1000000, "2024-01-01T00:00:00Z", "Canon", "thumb/img1.jpg"],
        ).unwrap();

        // Insert a row with no EXIF and no thumbnail
        conn.execute(
            "INSERT INTO photos (file_path, file_name, file_hash, file_size, faces_processed) VALUES (?1, ?2, ?3, ?4, 0)",
            rusqlite::params!["/photos/img2.png", "img2.png", "def456", 500000],
        ).unwrap();

        // Advance schema version to 18 (simulate all prior migrations done)
        conn.execute("INSERT INTO schema_version (version) VALUES (18)", [])
            .unwrap();

        // Run v18->v19 migration
        migrate_v18_to_v19(&conn).unwrap();

        // Verify columns exist
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(photos)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|c| c.ok())
            .collect();
        assert!(columns.iter().any(|c| c == "metadata_extracted"));
        assert!(columns.iter().any(|c| c == "thumbnailed"));

        // Row with EXIF data should have metadata_extracted = TRUE
        let meta_flag: bool = conn
            .query_row(
                "SELECT metadata_extracted FROM photos WHERE file_path = '/photos/img1.jpg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(meta_flag);

        // Row without EXIF should have metadata_extracted = FALSE
        let meta_flag2: bool = conn
            .query_row(
                "SELECT metadata_extracted FROM photos WHERE file_path = '/photos/img2.png'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!meta_flag2);

        // Row with thumbnail should have thumbnailed = TRUE
        let thumb_flag: bool = conn
            .query_row(
                "SELECT thumbnailed FROM photos WHERE file_path = '/photos/img1.jpg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(thumb_flag);

        // Row without thumbnail should have thumbnailed = FALSE
        let thumb_flag2: bool = conn
            .query_row(
                "SELECT thumbnailed FROM photos WHERE file_path = '/photos/img2.png'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!thumb_flag2);

        // Verify partial indexes exist
        let idx_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_photos_metadata_extracted', 'idx_photos_thumbnailed')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(idx_count, 2);
        Ok(())
    }
}
