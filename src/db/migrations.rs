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
    let current_version = get_schema_version(conn)?;

    // Add migration functions here as schema evolves
    // if current_version < 2 {
    //     migrate_v1_to_v2(conn)?;
    // }

    tracing::info!("Database at schema version {}", current_version);
    Ok(())
}
