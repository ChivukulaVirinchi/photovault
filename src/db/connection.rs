//! Database connection management

use std::path::{Path, PathBuf};

use rusqlite::{Connection, Result as SqliteResult};
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
}

impl Drop for Database {
    fn drop(&mut self) {
        // Best-effort PASSIVE checkpoint on drop. The clean-exit path
        // already calls `flush_and_close` (TRUNCATE), so this only
        // catches accidental drops (panics, early-return paths) and
        // never blocks long.
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
    }
}

/// Returns the `.photovault` metadata directory for a drive root.
pub fn photovault_dir(drive_root: &Path) -> PathBuf {
    drive_root.join(".photovault")
}

/// Returns the map tile cache directory for a drive root.
pub fn tile_cache_dir(drive_root: &Path) -> PathBuf {
    photovault_dir(drive_root).join("tile_cache")
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
        let photovault_dir = photovault_dir(&drive_root);
        if !photovault_dir.exists() {
            std::fs::create_dir_all(&photovault_dir)
                .map_err(DatabaseError::DirectoryCreationError)?;
        }

        let db_path = photovault_dir.join("photovault.db");
        let conn = Connection::open(&db_path)?;

        // Configure SQLite for optimal performance
        Self::configure_connection(&conn)?;
        let _ = Self::create_indexes(&conn);

        Ok(Self { conn })
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

        // Wait up to 5 seconds for a busy lock instead of failing
        // immediately. Without this, two writers (e.g. face-processor
        // writer thread + scanner upserts during a re-index) surface
        // SQLITE_BUSY to the user instead of retrying.
        conn.pragma_update(None, "busy_timeout", 5000)?;

        Ok(())
    }

    /// Create recommended indexes for query performance.
    fn create_indexes(conn: &Connection) -> SqliteResult<()> {
        conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_photos_date ON photos(date_taken);
            CREATE INDEX IF NOT EXISTS idx_photos_hash ON photos(file_hash);
            CREATE INDEX IF NOT EXISTS idx_photos_location ON photos(location_country, location_city);
            CREATE INDEX IF NOT EXISTS idx_photos_trashed ON photos(is_trashed);
            CREATE INDEX IF NOT EXISTS idx_photos_path ON photos(file_path);
            CREATE INDEX IF NOT EXISTS idx_photos_faces_trashed ON photos(faces_processed, is_trashed);
            CREATE INDEX IF NOT EXISTS idx_photos_hash_trashed ON photos(file_hash, is_trashed);
            CREATE INDEX IF NOT EXISTS idx_faces_cluster ON faces(cluster_id);
            CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
            CREATE INDEX IF NOT EXISTS idx_faces_photo_cluster ON faces(photo_id, cluster_id);
            CREATE INDEX IF NOT EXISTS idx_clusters_name ON face_clusters(name);
            "#,
        )
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

    /// Run periodic database maintenance.
    ///
    /// Call after bulk operations (scan, trash empty, reindex).
    pub fn run_maintenance(&self) -> SqliteResult<()> {
        // Let SQLite optimize its query planner statistics
        self.conn.execute_batch("PRAGMA optimize;")?;
        // Checkpoint WAL to keep it from growing unbounded
        self.conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
        Ok(())
    }

    /// Aggressively flush WAL to the main DB file and consume the
    /// connection. Use on drive deselect / app exit so a yanked USB
    /// drive doesn't leave unwritten data behind. TRUNCATE is the
    /// strict mode (vs PASSIVE) — it both checkpoints AND empties the
    /// WAL file.
    pub fn flush_and_close(self) {
        if let Err(e) = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
            tracing::debug!("WAL truncate-checkpoint failed: {}", e);
        }
        // Connection drops here, releasing file handles.
    }

    /// Run integrity check on the database.
    ///
    /// Returns Ok(true) if database is healthy, Ok(false) if corrupt.
    pub fn check_integrity(&self) -> SqliteResult<bool> {
        let result: String = self
            .conn
            .query_row("PRAGMA quick_check;", [], |row| row.get(0))?;
        Ok(result == "ok")
    }

    /// Create a backup of the database file.
    ///
    /// Copies the DB to `<db_path>.backup`. Keeps up to `max_backups` copies.
    pub fn backup(drive_root: &Path, max_backups: usize) -> std::io::Result<PathBuf> {
        let db_path = photovault_dir(drive_root).join("photovault.db");
        if !db_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Database file not found",
            ));
        }

        let backup_dir = photovault_dir(drive_root).join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        // Name with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = backup_dir.join(format!("photovault_{}.db", timestamp));

        std::fs::copy(&db_path, &backup_path)?;

        // Clean old backups (keep max_backups newest)
        let mut backups: Vec<_> = std::fs::read_dir(&backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("photovault_") && n.ends_with(".db"))
                    .unwrap_or(false)
            })
            .collect();

        backups.sort_by_key(|e| std::cmp::Reverse(e.path()));

        for old in backups.into_iter().skip(max_backups) {
            let _ = std::fs::remove_file(old.path());
        }

        tracing::info!("Database backed up to {}", backup_path.display());
        Ok(backup_path)
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

        let table_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_count >= 0);
        assert!(temp.path().join(".photovault").exists());
    }
}
