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
