//! Incremental re-indexing service.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::{params, Connection, Result as SqliteResult};
use walkdir::WalkDir;

#[derive(Debug, Default, Clone)]
pub struct IndexChanges {
    pub added: Vec<PathBuf>,
    pub removed: Vec<(i64, PathBuf)>,
    pub moved: Vec<(i64, PathBuf, PathBuf)>,
    pub modified: Vec<(i64, PathBuf)>,
}

impl IndexChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.moved.is_empty()
            && self.modified.is_empty()
    }
}

#[derive(Debug, Default, Clone)]
pub struct ApplyResult {
    pub new_files: usize,
    pub moves_applied: usize,
    pub removals_applied: usize,
    pub updates_applied: usize,
}

pub struct Reindexer {
    supported_extensions: HashSet<String>,
    skip_patterns: Vec<String>,
    scan_hidden_folders: bool,
}

impl Default for Reindexer {
    fn default() -> Self {
        Self::new()
    }
}

impl Reindexer {
    pub fn new() -> Self {
        Self::new_with_options(false)
    }

    pub fn new_with_options(scan_hidden_folders: bool) -> Self {
        let mut supported_extensions = HashSet::new();
        for ext in ["jpg", "jpeg", "png", "heic", "heif", "webp"] {
            supported_extensions.insert(ext.to_string());
        }

        let skip_patterns = vec![
            "System Volume Information".to_string(),
            "$RECYCLE.BIN".to_string(),
            ".Trash".to_string(),
            ".photovault".to_string(),
        ];

        Self {
            supported_extensions,
            skip_patterns,
            scan_hidden_folders,
        }
    }

    pub fn detect_changes(
        &self,
        conn: &Connection,
        drive_root: &Path,
    ) -> SqliteResult<IndexChanges> {
        let mut changes = IndexChanges::default();

        // Use a temp table instead of loading everything into a HashMap.
        // This keeps memory usage O(1) in Rust regardless of library size.
        conn.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS found_files (
                path TEXT PRIMARY KEY,
                mtime TEXT
            );
            DELETE FROM found_files;"
        )?;

        let mut insert_stmt = conn.prepare(
            "INSERT OR IGNORE INTO found_files (path, mtime) VALUES (?1, ?2)"
        )?;

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

            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            if !self.supported_extensions.contains(&ext) {
                continue;
            }

            let relative_path = match entry
                .path()
                .strip_prefix(drive_root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string())
            {
                Some(p) => p,
                None => continue,
            };

            let mtime_str = fs::metadata(entry.path())
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| Self::system_time_to_string(t))
                .unwrap_or_default();

            let _ = insert_stmt.execute(params![relative_path, mtime_str]);
        }
        drop(insert_stmt);

        // Added files: on disk but not in DB
        {
            let mut stmt = conn.prepare(
                "SELECT f.path FROM temp.found_files f
                 LEFT JOIN photos p ON p.file_path = f.path AND p.is_trashed = FALSE
                 WHERE p.id IS NULL"
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                let path = row?;
                changes.added.push(drive_root.join(&path));
            }
        }

        // Modified files: matching path but newer mtime
        {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.file_path FROM photos p
                 INNER JOIN temp.found_files f ON f.path = p.file_path
                 WHERE p.is_trashed = FALSE AND f.mtime > COALESCE(p.updated_at, '')"
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, path) = row?;
                changes.modified.push((id, drive_root.join(&path)));
            }
        }

        // Removed files: in DB but not on disk
        // Also check for moves (same hash, different path)
        {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.file_path, p.file_hash FROM photos p
                 LEFT JOIN temp.found_files f ON f.path = p.file_path
                 WHERE p.is_trashed = FALSE AND f.path IS NULL"
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;

            let mut missing: Vec<(i64, String, String)> = Vec::new();
            for row in rows {
                missing.push(row?);
            }

            // Check for moves: find new paths with matching hash
            for (id, old_path, hash) in &missing {
                let mut found_move = false;

                // Look for an added file with the same quick hash
                let mut move_stmt = conn.prepare(
                    "SELECT f.path FROM temp.found_files f
                     LEFT JOIN photos p ON p.file_path = f.path AND p.is_trashed = FALSE
                     WHERE p.id IS NULL"
                )?;
                let candidates = move_stmt.query_map([], |row| row.get::<_, String>(0))?;
                for candidate in candidates {
                    let new_path = candidate?;
                    let new_full_path = drive_root.join(&new_path);
                    if let Ok(new_hash) = Self::quick_hash(&new_full_path) {
                        if &new_hash == hash {
                            changes.moved.push((
                                *id,
                                PathBuf::from(old_path),
                                PathBuf::from(new_path),
                            ));
                            found_move = true;
                            break;
                        }
                    }
                }

                if !found_move {
                    changes.removed.push((*id, PathBuf::from(old_path)));
                }
            }
        }

        conn.execute("DROP TABLE IF EXISTS temp.found_files", [])?;

        Ok(changes)
    }

    pub fn apply_changes(
        &self,
        conn: &Connection,
        changes: &IndexChanges,
    ) -> SqliteResult<ApplyResult> {
        let mut result = ApplyResult::default();
        let tx = conn.unchecked_transaction()?;

        for (photo_id, _old_path, new_path) in &changes.moved {
            let new_relative = new_path.to_string_lossy().to_string();
            tx.execute(
                "UPDATE photos SET file_path = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![new_relative, photo_id],
            )?;
            result.moves_applied += 1;
        }

        for (photo_id, _) in &changes.removed {
            tx.execute(
                "UPDATE photos SET is_trashed = TRUE, trashed_at = CURRENT_TIMESTAMP WHERE id = ?1",
                params![photo_id],
            )?;
            result.removals_applied += 1;
        }

        for (photo_id, _) in &changes.modified {
            tx.execute(
                "UPDATE photos SET faces_processed = FALSE, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                params![photo_id],
            )?;
            result.updates_applied += 1;
        }

        tx.commit()?;
        result.new_files = changes.added.len();
        Ok(result)
    }

    fn should_skip(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !self.scan_hidden_folders && name.starts_with('.') {
            return true;
        }
        self.skip_patterns.iter().any(|p| name.starts_with(p))
    }

    fn quick_hash(path: &Path) -> std::io::Result<String> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let mut file = fs::File::open(path)?;
        let mut buffer = vec![0u8; 65536];
        let n = file.read(&mut buffer)?;
        buffer.truncate(n);
        let hash = Sha256::digest(&buffer);
        Ok(format!("{:x}", hash))
    }

    fn system_time_to_string(time: SystemTime) -> String {
        use chrono::{DateTime, Utc};

        let datetime: DateTime<Utc> = time.into();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}
