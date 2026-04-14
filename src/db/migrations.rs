//! Database migrations for schema versioning

use rusqlite::{Connection, Result as SqliteResult};

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

/// Run any pending migrations
pub fn run_migrations(conn: &Connection) -> SqliteResult<()> {
    let current_version = get_schema_version(conn).unwrap_or(0);

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

    let updated_version = get_schema_version(conn).unwrap_or(current_version);
    tracing::info!("Database at schema version {}", updated_version);
    Ok(())
}

fn migrate_v3_to_v4(conn: &Connection) -> SqliteResult<()> {
    // Add lens_model, flash, gps_altitude (these were missed in v3)
    let columns = [
        ("lens_model", "TEXT"),
        ("flash", "TEXT"),
        ("gps_altitude", "REAL"),
    ];

    for (col, col_type) in &columns {
        let sql = format!("ALTER TABLE photos ADD COLUMN {} {}", col, col_type);
        match conn.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
    }

    conn.execute("INSERT INTO schema_version (version) VALUES (4)", [])?;
    tracing::info!("Migrated database to schema version 4 (lens, flash, altitude)");
    Ok(())
}

fn migrate_v5_to_v6(conn: &Connection) -> SqliteResult<()> {
    let columns = [
        ("content_category", "TEXT DEFAULT 'photo'"),
        ("ocr_text", "TEXT"),
        ("ocr_processed", "BOOLEAN DEFAULT FALSE"),
        ("ocr_confidence", "REAL"),
    ];

    for (col, col_type) in &columns {
        let sql = format!("ALTER TABLE photos ADD COLUMN {} {}", col, col_type);
        match conn.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
    }

    conn.execute_batch(
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
    // Add EXIF shooting parameters
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
        match conn.execute(&sql, []) {
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

    conn.execute("INSERT INTO schema_version (version) VALUES (3)", [])?;
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
