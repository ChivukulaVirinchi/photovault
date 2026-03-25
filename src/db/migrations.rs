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

    let updated_version = get_schema_version(conn).unwrap_or(current_version);
    tracing::info!("Database at schema version {}", updated_version);
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
