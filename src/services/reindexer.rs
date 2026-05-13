//! Incremental re-indexing service.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::{params, Connection, Result as SqliteResult};
use walkdir::WalkDir;

use crate::services::path_util::safe_join_relative;
use crate::services::scanner::calculate_hash;

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
            DELETE FROM found_files;",
        )?;

        let mut insert_stmt =
            conn.prepare("INSERT OR IGNORE INTO found_files (path, mtime) VALUES (?1, ?2)")?;

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

            // Forward-slash-normalized so a Windows write and a Linux
            // write of the same drive produce identical strings.
            let relative_path = match entry.path().strip_prefix(drive_root).ok() {
                Some(p) => crate::services::path_util::relative_path_for_storage(p),
                None => continue,
            };

            let mtime_str = fs::metadata(entry.path())
                .ok()
                .and_then(|m| m.modified().ok())
                .map(Self::system_time_to_string)
                .unwrap_or_default();

            let _ = insert_stmt.execute(params![relative_path, mtime_str]);
        }
        drop(insert_stmt);

        // Added files: on disk but not in DB
        {
            let mut stmt = conn.prepare(
                "SELECT f.path FROM temp.found_files f
                 LEFT JOIN photos p ON p.file_path = f.path AND p.is_trashed = FALSE
                 WHERE p.id IS NULL",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                let path = row?;
                if let Ok(full) = safe_join_relative(drive_root, &path) {
                    changes.added.push(full);
                }
            }
        }

        // Modified files: matching path but newer mtime
        {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.file_path FROM photos p
                 INNER JOIN temp.found_files f ON f.path = p.file_path
                 WHERE p.is_trashed = FALSE AND f.mtime > COALESCE(p.updated_at, '')",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, path) = row?;
                if let Ok(full) = safe_join_relative(drive_root, &path) {
                    changes.modified.push((id, full));
                }
            }
        }

        // Removed files: in DB but not on disk
        // Also check for moves (same hash, different path)
        {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.file_path, p.file_hash FROM photos p
                 LEFT JOIN temp.found_files f ON f.path = p.file_path
                 WHERE p.is_trashed = FALSE AND f.path IS NULL",
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

            // Pre-hash all candidate (added) files exactly once into a hash → path map.
            // Without this, we'd re-hash every candidate for every missing file (N×M).
            // Hashes computed here use the same full-file SHA256 as the scanner so the
            // result matches the stored `photos.file_hash` column. A 64KB-prefix hash
            // (the previous quick_hash) collides on photos sharing camera EXIF headers
            // and never matches the scanner's full-file hash either way, so move
            // detection was effectively broken.
            let candidate_paths: Vec<String> = changes
                .added
                .iter()
                .filter_map(|p| p.strip_prefix(drive_root).ok())
                .filter_map(|p| p.to_str())
                .map(|s| s.to_string())
                .collect();

            let mut hash_to_candidate: HashMap<String, String> = HashMap::new();
            for relative in &candidate_paths {
                let Ok(full) = safe_join_relative(drive_root, relative) else {
                    continue;
                };
                if let Ok(hash) = calculate_hash(&full) {
                    hash_to_candidate
                        .entry(hash)
                        .or_insert_with(|| relative.clone());
                }
            }

            let mut consumed_candidates: HashSet<String> = HashSet::new();
            for (id, old_path, hash) in &missing {
                match hash_to_candidate.get(hash) {
                    Some(new_path) if !consumed_candidates.contains(new_path) => {
                        consumed_candidates.insert(new_path.clone());
                        changes
                            .moved
                            .push((*id, PathBuf::from(old_path), PathBuf::from(new_path)));
                    }
                    _ => {
                        changes.removed.push((*id, PathBuf::from(old_path)));
                    }
                }
            }

            // Files matched as moves are no longer "added"; drop them from the added list.
            if !consumed_candidates.is_empty() {
                changes.added.retain(|p| {
                    let relative = p
                        .strip_prefix(drive_root)
                        .ok()
                        .and_then(|p| p.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    !consumed_candidates.contains(&relative)
                });
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

    fn system_time_to_string(time: SystemTime) -> String {
        use chrono::{DateTime, Utc};

        let datetime: DateTime<Utc> = time.into();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    /// Two files sharing a 64 KB prefix but differing afterwards must hash
    /// differently. The previous quick_hash (64 KB only) collided here, which
    /// caused the reindexer to misidentify distinct files as moves of each
    /// other.
    #[test]
    fn full_hash_distinguishes_files_with_shared_prefix() {
        let temp = tempdir().unwrap();
        let prefix = vec![0xABu8; 65_536];

        let path_a = temp.path().join("a.bin");
        let mut f = fs::File::create(&path_a).unwrap();
        f.write_all(&prefix).unwrap();
        f.write_all(&[0x01u8; 4096]).unwrap();
        drop(f);

        let path_b = temp.path().join("b.bin");
        let mut f = fs::File::create(&path_b).unwrap();
        f.write_all(&prefix).unwrap();
        f.write_all(&[0x02u8; 4096]).unwrap();
        drop(f);

        let hash_a = calculate_hash(&path_a).unwrap();
        let hash_b = calculate_hash(&path_b).unwrap();
        assert_ne!(
            hash_a, hash_b,
            "files differing past the first 64 KB must produce different hashes"
        );
    }
}
