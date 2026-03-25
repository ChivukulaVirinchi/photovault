//! Incremental re-indexing service.

use std::collections::{HashMap, HashSet};
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

        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_hash, updated_at FROM photos WHERE is_trashed = FALSE",
        )?;

        let indexed_files: HashMap<String, (i64, String, Option<String>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    (
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ),
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut found_paths: HashSet<String> = HashSet::new();

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

            let relative_path = entry
                .path()
                .strip_prefix(drive_root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string());

            let relative_path = match relative_path {
                Some(p) => p,
                None => continue,
            };

            found_paths.insert(relative_path.clone());

            if let Some((id, _hash, updated_at)) = indexed_files.get(&relative_path) {
                if let Some(updated) = updated_at {
                    if let Ok(metadata) = fs::metadata(entry.path()) {
                        if let Ok(mtime) = metadata.modified() {
                            let file_time = Self::system_time_to_string(mtime);
                            if file_time > *updated {
                                changes.modified.push((*id, entry.path().to_path_buf()));
                            }
                        }
                    }
                }
            } else {
                changes.added.push(entry.path().to_path_buf());
            }
        }

        for (path, (id, hash, _)) in &indexed_files {
            if !found_paths.contains(path) {
                let mut was_moved = false;

                for found_path in &found_paths {
                    if !indexed_files.contains_key(found_path) {
                        let new_full_path = drive_root.join(found_path);
                        if let Ok(new_hash) = Self::quick_hash(&new_full_path) {
                            if &new_hash == hash {
                                changes.moved.push((
                                    *id,
                                    PathBuf::from(path),
                                    PathBuf::from(found_path),
                                ));
                                was_moved = true;
                                break;
                            }
                        }
                    }
                }

                if !was_moved {
                    changes.removed.push((*id, PathBuf::from(path)));
                }
            }
        }

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
