//! Incremental re-indexing service.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, Result as SqliteResult};
use walkdir::WalkDir;

use crate::db::face_repo::FaceRepo;
use crate::services::exclusions::ExclusionMatcher;
use crate::services::path_util::safe_join_relative;
use crate::services::scanner::calculate_fast_hash;

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
        // Share format support with the initial scanner.
        let mut supported_extensions = HashSet::new();
        for ext in crate::services::scanner::SUPPORTED_EXTENSIONS {
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
        let metadata = fs::metadata(drive_root)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        if !metadata.is_dir() {
            return Err(rusqlite::Error::InvalidParameterName(
                "library root is not a directory".into(),
            ));
        }
        let mut changes = IndexChanges::default();
        let exclusions = ExclusionMatcher::from_db(conn)?;

        // Use a temp table instead of loading everything into a HashMap.
        // This keeps memory usage O(1) in Rust regardless of library size.
        conn.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS found_files (
                path TEXT PRIMARY KEY,
                mtime INTEGER,
                size INTEGER
            );
            DELETE FROM found_files;",
        )?;

        let mut insert_stmt = conn
            .prepare("INSERT OR IGNORE INTO found_files (path, mtime, size) VALUES (?1, ?2, ?3)")?;

        for entry in WalkDir::new(drive_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.should_skip(drive_root, e.path(), &exclusions))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(error) => return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
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

            let metadata = match fs::metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(error) => return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
            };
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            insert_stmt.execute(params![relative_path, mtime, metadata.len() as i64])?;
        }
        drop(insert_stmt);

        // Added files: on disk but not in DB
        {
            let mut stmt = conn.prepare(
                "SELECT f.path FROM temp.found_files f
                 LEFT JOIN photos p ON p.file_path = f.path
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
                 WHERE p.is_trashed = FALSE
                   AND (f.mtime IS NOT p.file_mtime OR f.size != p.file_size)",
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
            // Hashes computed here use the same fast hash as the scanner so
            // the result matches the stored `photos.file_hash` column.
            let candidate_paths: Vec<String> = changes
                .added
                .iter()
                .filter_map(|p| p.strip_prefix(drive_root).ok())
                .map(crate::services::path_util::relative_path_for_storage)
                .collect();

            let mut hash_to_candidate: HashMap<String, String> = HashMap::new();
            for relative in &candidate_paths {
                let Ok(full) = safe_join_relative(drive_root, relative) else {
                    continue;
                };
                let Ok(meta) = fs::metadata(&full) else {
                    continue;
                };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                if let Ok(hash) = calculate_fast_hash(&full, meta.len(), mtime) {
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
                        .map(crate::services::path_util::relative_path_for_storage)
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
            let new_relative = crate::services::path_util::relative_path_for_storage(new_path);
            let new_file_name = new_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&new_relative)
                .to_string();
            tx.execute(
                "UPDATE photos
                    SET file_path = ?1,
                        file_name = ?2,
                        updated_at = CURRENT_TIMESTAMP
                  WHERE id = ?3",
                params![new_relative, new_file_name, photo_id],
            )?;
            result.moves_applied += 1;
        }

        let removed_ids: Vec<_> = changes.removed.iter().map(|(id, _)| *id).collect();
        result.removals_applied =
            crate::services::trash::TrashService::trash_photos_tx(&tx, &removed_ids)?;

        for (photo_id, path) in &changes.modified {
            clear_face_derivatives_for_photo(&tx, *photo_id)?;
            let file_facts = fs::metadata(path).ok().and_then(|meta| {
                let size = i64::try_from(meta.len()).ok()?;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                let hash = calculate_fast_hash(path, meta.len(), mtime).ok()?;
                Some((size, mtime, hash))
            });
            let (file_size, file_mtime, file_hash) = match file_facts {
                Some((size, mtime, hash)) => (Some(size), mtime, Some(hash)),
                None => (None, None, None),
            };
            tx.execute(
                "UPDATE photos
                    SET file_size = COALESCE(?2, file_size),
                        file_mtime = COALESCE(?3, file_mtime),
                        file_hash = COALESCE(?4, file_hash),
                        metadata_extracted = FALSE,
                        thumbnailed = FALSE,
                        thumbnail_path = NULL,
                        faces_processed = FALSE,
                        phash = NULL,
                        brightness = NULL,
                        ocr_text = NULL,
                        ocr_processed = FALSE,
                        updated_at = CURRENT_TIMESTAMP
                  WHERE id = ?1",
                params![photo_id, file_size, file_mtime, file_hash],
            )?;
            result.updates_applied += 1;
            tx.execute(
                "DELETE FROM semantic_index_state WHERE photo_id = ?1",
                [photo_id],
            )?;
        }

        if !changes.added.is_empty() {
            let db_path: String = tx.query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get(0),
            )?;
            let root = Path::new(&db_path)
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| {
                    rusqlite::Error::InvalidParameterName(
                        "new files require an on-disk library database".into(),
                    )
                })?;
            let mut insert = tx.prepare(
                "INSERT INTO photos(file_path,file_name,file_hash,file_size,file_mtime,media_type)
                VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(file_path) DO NOTHING",
            )?;
            for path in &changes.added {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                let relative = crate::services::path_util::relative_path_for_storage(relative);
                crate::services::path_util::safe_existing_path_under_root(root, &relative)
                    .map_err(rusqlite::Error::InvalidParameterName)?;
                let metadata = fs::metadata(path)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|time| time.as_secs() as i64);
                let hash = calculate_fast_hash(path, metadata.len(), mtime)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                let media_type =
                    crate::services::scanner::media_type_for_path(path).ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName("unsupported media file".into())
                    })?;
                result.new_files += insert.execute(params![
                    relative,
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    hash,
                    metadata.len() as i64,
                    mtime,
                    media_type.as_str()
                ])?;
            }
        }
        tx.commit()?;
        if !changes.modified.is_empty() {
            FaceRepo::new(conn).normalize_cluster_stats()?;
        }
        Ok(result)
    }

    fn should_skip(&self, drive_root: &Path, path: &Path, exclusions: &ExclusionMatcher) -> bool {
        if path == drive_root {
            return false;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !self.scan_hidden_folders && name.starts_with('.') {
            return true;
        }
        self.skip_patterns.iter().any(|p| name.starts_with(p))
            || exclusions.should_skip_path(drive_root, path)
    }
}

fn clear_face_derivatives_for_photo(
    tx: &rusqlite::Transaction<'_>,
    photo_id: i64,
) -> SqliteResult<()> {
    let face_ids = {
        let mut stmt = tx.prepare("SELECT id FROM faces WHERE photo_id = ?1")?;
        let rows = stmt.query_map(params![photo_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };

    for face_id in face_ids {
        tx.execute(
            "DELETE FROM face_review_queue WHERE face_id = ?1",
            params![face_id],
        )?;
        tx.execute(
            "DELETE FROM face_negatives WHERE face_id = ?1",
            params![face_id],
        )?;
        tx.execute(
            "DELETE FROM person_gallery_embeddings WHERE face_id = ?1",
            params![face_id],
        )?;
        tx.execute("DELETE FROM faces WHERE id = ?1", params![face_id])?;
    }

    tx.execute(
        "DELETE FROM photo_inferred_identities WHERE photo_id = ?1 OR source_photo_id = ?1",
        params![photo_id],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_schema;
    use crate::services::scanner::calculate_hash;
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

    #[test]
    fn reindexer_excludes_folder_descendants() {
        let temp = tempdir().unwrap();
        let excluded = temp.path().join("Trips").join("Goa");
        let similar = temp.path().join("Trips").join("Goa2");
        fs::create_dir_all(&excluded).unwrap();
        fs::create_dir_all(&similar).unwrap();

        let matcher = ExclusionMatcher::new(vec!["Trips/Goa".into()]);
        let reindexer = Reindexer::new();

        assert!(reindexer.should_skip(temp.path(), &excluded, &matcher));
        assert!(!reindexer.should_skip(temp.path(), &similar, &matcher));
    }

    #[test]
    fn apply_removed_files_creates_trash_rows() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, is_trashed)
             VALUES (1, 'missing.jpg', 'missing.jpg', 'hash', 8, FALSE)",
            [],
        )
        .unwrap();

        let changes = IndexChanges {
            removed: vec![(1, PathBuf::from("missing.jpg"))],
            ..IndexChanges::default()
        };
        let result = Reindexer::new().apply_changes(&conn, &changes).unwrap();

        assert_eq!(result.removals_applied, 1);
        let is_trashed: bool = conn
            .query_row("SELECT is_trashed FROM photos WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(is_trashed);
        let original_path: String = conn
            .query_row(
                "SELECT original_path FROM trash WHERE photo_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(original_path, "missing.jpg");
    }

    #[test]
    fn detect_changes_uses_file_mtime_for_modified_files() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("photo.jpg");
        fs::write(&path, vec![0xABu8; 12_000]).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let current_mtime = metadata
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (
                id, file_path, file_name, file_hash, file_size, file_mtime,
                metadata_extracted, thumbnailed, faces_processed, updated_at
             ) VALUES (
                1, 'photo.jpg', 'photo.jpg', 'hash', 12000, ?1,
                TRUE, TRUE, TRUE, datetime('now', '+1 day')
             )",
            [current_mtime - 10],
        )
        .unwrap();

        let changes = Reindexer::new_with_options(true)
            .detect_changes(&conn, temp.path())
            .unwrap();
        assert_eq!(changes.modified.len(), 1);
        assert_eq!(changes.modified[0].0, 1);
    }

    #[test]
    fn detect_changes_identifies_moved_files() {
        let temp = tempdir().unwrap();
        let new_dir = temp.path().join("new");
        fs::create_dir_all(&new_dir).unwrap();
        let new_path = new_dir.join("photo.jpg");
        fs::write(&new_path, vec![0xEFu8; 12_000]).unwrap();
        let meta = fs::metadata(&new_path).unwrap();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let hash = calculate_fast_hash(&new_path, meta.len(), mtime).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, is_trashed)
             VALUES (1, 'old/photo.jpg', 'photo.jpg', ?1, ?2, FALSE)",
            params![hash, meta.len() as i64],
        )
        .unwrap();

        let changes = Reindexer::new_with_options(true)
            .detect_changes(&conn, temp.path())
            .unwrap();

        assert_eq!(changes.moved.len(), 1);
        assert_eq!(changes.moved[0].0, 1);
        assert_eq!(changes.moved[0].1, PathBuf::from("old/photo.jpg"));
        assert_eq!(changes.moved[0].2, PathBuf::from("new/photo.jpg"));
        assert!(changes.added.is_empty());
        assert!(changes.removed.is_empty());
    }

    #[test]
    fn apply_modified_files_resets_stale_processing_state() {
        let temp = tempdir().unwrap();
        let changed_path = temp.path().join("changed.jpg");
        fs::write(&changed_path, vec![0xCDu8; 12_000]).unwrap();
        let meta = fs::metadata(&changed_path).unwrap();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let expected_hash = calculate_fast_hash(&changed_path, meta.len(), mtime).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (
                id, file_path, file_name, file_hash, file_size, thumbnail_path,
                file_mtime,
                metadata_extracted, thumbnailed, faces_processed
             ) VALUES (
                1, 'changed.jpg', 'changed.jpg', 'old-hash', 8, '.photovault/thumbnails/medium/v2/ha/hash.jpg',
                1,
                TRUE, TRUE, TRUE
             )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size)
             VALUES (2, 'other.jpg', 'other.jpg', 'other-hash', 8)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO face_clusters (id, name, representative_face_id, face_count, photo_count)
             VALUES (10, 'Stale', NULL, 1, 2), (20, 'Other', NULL, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO faces (
                id, photo_id, bbox_x, bbox_y, bbox_width, bbox_height,
                embedding, cluster_id, confidence, user_confirmed
             )
             VALUES (100, 1, 0.1, 0.1, 0.2, 0.2, zeroblob(16), 10, 0.99, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE face_clusters SET representative_face_id = 100 WHERE id = 10",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO face_review_queue (face_id, candidate_cluster_id, score)
             VALUES (100, 20, 0.75)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO face_negatives (face_id, not_cluster_id) VALUES (100, 20)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO person_gallery_embeddings (cluster_id, face_id, embedding)
             VALUES (10, 100, zeroblob(16))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photo_inferred_identities (photo_id, cluster_id, source_photo_id, confidence)
             VALUES (1, 10, 2, 0.5)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photo_inferred_identities (photo_id, cluster_id, source_photo_id, confidence)
             VALUES (2, 10, 1, 0.5)",
            [],
        )
        .unwrap();

        let changes = IndexChanges {
            modified: vec![(1, changed_path.clone())],
            ..IndexChanges::default()
        };
        let result = Reindexer::new().apply_changes(&conn, &changes).unwrap();
        assert_eq!(result.updates_applied, 1);

        let row: (bool, bool, bool, Option<String>, i64, Option<i64>, String) = conn
            .query_row(
                "SELECT metadata_extracted, thumbnailed, faces_processed, thumbnail_path,
                        file_size, file_mtime, file_hash
                   FROM photos WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                false,
                false,
                false,
                None,
                meta.len() as i64,
                mtime,
                expected_hash
            )
        );

        for table in [
            "faces",
            "face_review_queue",
            "face_negatives",
            "person_gallery_embeddings",
            "photo_inferred_identities",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} should be cleared");
        }
        let cluster_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM face_clusters WHERE id = 10",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cluster_count, 0, "empty stale cluster should be pruned");
    }

    #[test]
    fn apply_moved_files_updates_file_name() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size)
             VALUES (1, 'old/name.jpg', 'name.jpg', 'hash', 8)",
            [],
        )
        .unwrap();

        let changes = IndexChanges {
            moved: vec![(
                1,
                PathBuf::from("old/name.jpg"),
                PathBuf::from("new/renamed.jpg"),
            )],
            ..IndexChanges::default()
        };
        let result = Reindexer::new().apply_changes(&conn, &changes).unwrap();
        assert_eq!(result.moves_applied, 1);

        let row: (String, String) = conn
            .query_row(
                "SELECT file_path, file_name FROM photos WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("new/renamed.jpg".into(), "renamed.jpg".into()));
    }
}
