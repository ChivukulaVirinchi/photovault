//! Trash management service.

use std::fs;
use std::io::Write;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

use crate::db::{album_repo::AlbumRepo, FaceRepo, PhotoStackRepo};
use crate::services::path_util::{safe_existing_path_under_root, safe_join_relative};

/// Result of a permanent delete operation.
#[derive(Debug, Default, Clone)]
pub struct DeleteResult {
    pub files_deleted: usize,
    pub db_records_deleted: usize,
    pub errors: Vec<String>,
}

/// Trash statistics.
#[derive(Debug, Default, Clone)]
pub struct TrashStats {
    pub count: usize,
    pub total_size: u64,
}

/// Trash service.
pub struct TrashService;

#[derive(serde::Serialize, serde::Deserialize)]
struct DeletionIntent {
    photo_id: i64,
    relative_path: String,
}

impl TrashService {
    pub fn trash_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<usize> {
        let tx = conn.unchecked_transaction()?;
        let count = Self::trash_photos_tx(&tx, photo_ids)?;
        tx.commit()?;
        Ok(count)
    }

    pub(crate) fn trash_photos_tx(
        tx: &rusqlite::Transaction<'_>,
        photo_ids: &[i64],
    ) -> SqliteResult<usize> {
        let mut count = 0usize;

        for photo_id in photo_ids {
            let path: Option<String> = tx
                .query_row(
                    "SELECT file_path FROM photos WHERE id = ?1 AND is_trashed = FALSE",
                    params![photo_id],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(path) = path {
                tx.execute(
                    "INSERT OR IGNORE INTO trash (photo_id, original_path) VALUES (?1, ?2)",
                    params![photo_id, path],
                )?;
                tx.execute(
                    "UPDATE photos SET is_trashed = TRUE, trashed_at = CURRENT_TIMESTAMP WHERE id = ?1",
                    params![photo_id],
                )?;
                count += 1;
            }
        }

        PhotoStackRepo::new(tx).reconcile_after_photos_trashed(photo_ids)?;
        refresh_album_state_for_photos(tx, photo_ids)?;
        refresh_face_state_for_photos(tx, photo_ids)?;
        Ok(count)
    }

    pub fn restore_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<usize> {
        let tx = conn.unchecked_transaction()?;
        let mut count = 0usize;

        for photo_id in photo_ids {
            let removed = tx.execute("DELETE FROM trash WHERE photo_id = ?1", params![photo_id])?;
            let updated = tx.execute(
                "UPDATE photos SET is_trashed = FALSE, trashed_at = NULL WHERE id = ?1 AND is_trashed = TRUE",
                params![photo_id],
            )?;
            if removed > 0 || updated > 0 {
                count += 1;
            }
        }

        refresh_album_state_for_photos(&tx, photo_ids)?;
        refresh_face_state_for_photos(&tx, photo_ids)?;
        tx.commit()?;
        Ok(count)
    }

    pub fn permanent_delete(
        conn: &Connection,
        photo_ids: &[i64],
        drive_root: &Path,
    ) -> SqliteResult<DeleteResult> {
        Self::recover_deletions(conn, drive_root)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        // The deletion commit must reach durable storage before staged originals
        // can be unlinked. Normal indexing uses the cheaper NORMAL setting.
        let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        let operation = (|| {
            let tx = conn.unchecked_transaction()?;
            let mut result = DeleteResult::default();

            for photo_id in photo_ids {
                let row: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT file_path, thumbnail_path FROM photos WHERE id = ?1 AND is_trashed = TRUE",
                    params![photo_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

                let Some((relative_path, thumbnail_path)) = row else {
                    continue;
                };

                let full_path = match safe_join_relative(drive_root, &relative_path) {
                    Ok(path) => path,
                    Err(e) => {
                        result.errors.push(format!("{}: {}", relative_path, e));
                        continue;
                    }
                };
                if full_path.exists() {
                    let delete_path =
                        match safe_existing_path_under_root(drive_root, &relative_path) {
                            Ok(path) => path,
                            Err(e) => {
                                result.errors.push(format!("{}: {}", relative_path, e));
                                continue;
                            }
                        };
                    if let Err(e) =
                        stage_deletion(drive_root, *photo_id, &relative_path, &delete_path)
                    {
                        result.errors.push(format!("{}: {}", relative_path, e));
                        continue;
                    }
                    result.files_deleted += 1;
                }

                // Cache files are disposable, but untrusted database paths must never
                // authorize deletion of originals or files outside the cache.
                if let Some(tp) = thumbnail_path {
                    if let Ok(cache_relative) =
                        Path::new(&tp).strip_prefix(".photovault/thumbnails")
                    {
                        if let Ok(cache_root) =
                            safe_existing_path_under_root(drive_root, ".photovault/thumbnails")
                        {
                            if let Ok(thumb_full) = safe_existing_path_under_root(
                                &cache_root,
                                &cache_relative.to_string_lossy(),
                            ) {
                                let _ = fs::remove_file(thumb_full);
                            }
                        }
                    }
                }

                refresh_album_state_for_photos(&tx, &[*photo_id])?;
                refresh_face_state_for_photos(&tx, &[*photo_id])?;
                tx.execute("DELETE FROM trash WHERE photo_id = ?1", params![photo_id])?;
                tx.execute("DELETE FROM photos WHERE id = ?1", params![photo_id])?;
                result.db_records_deleted += 1;
            }

            refresh_album_state_for_photos(&tx, photo_ids)?;
            refresh_face_state_for_photos(&tx, photo_ids)?;
            tx.commit()?;
            Ok(result)
        })();
        let recovery = Self::recover_deletions(conn, drive_root);
        if let Err(error) = conn.pragma_update(None, "synchronous", synchronous) {
            tracing::warn!("Could not restore SQLite synchronous mode: {error}");
        }
        match operation {
            Err(error) => {
                if let Err(recovery_error) = recovery {
                    tracing::error!(
                        "Originals retained in .photovault/delete-* for recovery: {recovery_error}"
                    );
                }
                Err(error)
            }
            Ok(mut result) => {
                if let Err(error) = recovery {
                    result
                        .errors
                        .push(format!("Staged deletion retained for recovery: {error}"));
                }
                Ok(result)
            }
        }
    }

    /// Finish committed deletes, or restore originals after a rollback/crash.
    pub fn recover_deletions(conn: &Connection, drive_root: &Path) -> std::io::Result<()> {
        if !drive_root.join(".photovault").exists() {
            return Ok(());
        }
        let metadata = safe_existing_path_under_root(drive_root, ".photovault")
            .map_err(std::io::Error::other)?;
        for entry in fs::read_dir(metadata)? {
            let entry = entry?;
            if !entry.file_name().to_string_lossy().starts_with("delete-")
                || !entry.file_type()?.is_dir()
            {
                continue;
            }
            let directory = entry.path();
            let manifest = directory.join("intent.json");
            if !manifest.exists() {
                continue;
            }
            let intent: DeletionIntent = serde_json::from_slice(&fs::read(&manifest)?)?;
            let staged = directory.join("original");
            if staged.exists() {
                let retained: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM photos WHERE id = ?1)",
                        [intent.photo_id],
                        |row| row.get(0),
                    )
                    .map_err(std::io::Error::other)?;
                if retained {
                    let original = safe_join_relative(drive_root, &intent.relative_path)
                        .map_err(std::io::Error::other)?;
                    let parent = original
                        .parent()
                        .ok_or_else(|| std::io::Error::other("missing original parent"))?;
                    if !parent
                        .canonicalize()?
                        .starts_with(drive_root.canonicalize()?)
                    {
                        return Err(std::io::Error::other("original parent escaped library"));
                    }
                    // Use a no-replace rename where supported (including FAT on
                    // Linux), and never let the temporary-path guard delete data.
                    let mut source = tempfile::TempPath::try_from_path(staged.clone())?;
                    source.disable_cleanup(true);
                    source
                        .persist_noclobber(&original)
                        .map_err(|error| error.error)?;
                    sync_directory(parent)?;
                }
                if staged.exists() {
                    fs::remove_file(&staged)?;
                }
            }
            fs::remove_file(manifest)?;
            fs::remove_dir(&directory)?;
            sync_directory(directory.parent().expect("journal parent"))?;
        }
        Ok(())
    }

    pub fn empty_trash(conn: &Connection, drive_root: &Path) -> SqliteResult<DeleteResult> {
        let mut stmt = conn.prepare("SELECT photo_id FROM trash")?;
        let ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<SqliteResult<Vec<_>>>()?;
        drop(stmt);
        Self::permanent_delete(conn, &ids, drive_root)
    }

    pub fn get_stats(conn: &Connection) -> SqliteResult<TrashStats> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = TRUE",
            [],
            |row| row.get(0),
        )?;

        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM photos WHERE is_trashed = TRUE",
            [],
            |row| row.get(0),
        )?;

        Ok(TrashStats {
            count: count as usize,
            total_size: total.max(0) as u64,
        })
    }
}

fn stage_deletion(
    root: &Path,
    photo_id: i64,
    relative_path: &str,
    original: &Path,
) -> std::io::Result<()> {
    fs::create_dir_all(root.join(".photovault"))?;
    let metadata =
        safe_existing_path_under_root(root, ".photovault").map_err(std::io::Error::other)?;
    let directory = tempfile::Builder::new()
        .prefix("delete-")
        .tempdir_in(metadata)?;
    let intent = DeletionIntent {
        photo_id,
        relative_path: relative_path.into(),
    };
    let mut manifest = fs::File::create(directory.path().join("intent.json"))?;
    manifest.write_all(&serde_json::to_vec(&intent)?)?;
    manifest.sync_all()?;
    sync_directory(directory.path())?;
    sync_directory(directory.path().parent().expect("journal parent"))?;
    // Keep the intent before moving anything: a process crash is recoverable.
    let directory = directory.keep();
    fs::rename(original, directory.join("original"))?;
    sync_directory(&directory)?;
    sync_directory(original.parent().expect("original parent"))?;
    Ok(())
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn refresh_album_state_for_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<()> {
    if photo_ids.is_empty() {
        return Ok(());
    }

    let mut album_ids = Vec::new();
    for chunk in photo_ids.chunks(900) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT album_id FROM album_photos WHERE photo_id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
            row.get(0)
        })?;
        for row in rows {
            album_ids.push(row?);
        }
    }
    album_ids.sort_unstable();
    album_ids.dedup();

    let repo = AlbumRepo::new(conn);
    for album_id in album_ids {
        repo.refresh_stats(album_id)?;
    }
    Ok(())
}

fn refresh_face_state_for_photos(
    tx: &rusqlite::Transaction<'_>,
    photo_ids: &[i64],
) -> SqliteResult<()> {
    if photo_ids.is_empty() {
        return Ok(());
    }

    let mut cluster_ids = Vec::new();
    for chunk in photo_ids.chunks(900) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT cluster_id FROM faces
             WHERE photo_id IN ({0}) AND cluster_id IS NOT NULL
             UNION
             SELECT DISTINCT cluster_id FROM photo_inferred_identities
             WHERE photo_id IN ({0})",
            placeholders
        );
        let mut params = Vec::with_capacity(chunk.len() * 2);
        params.extend(chunk.iter().copied());
        params.extend(chunk.iter().copied());
        let mut stmt = tx.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| row.get(0))?;
        for row in rows {
            cluster_ids.push(row?);
        }
    }

    cluster_ids.sort_unstable();
    cluster_ids.dedup();
    for cluster_id in cluster_ids {
        FaceRepo::refresh_cluster_stats_tx(tx, cluster_id)?;
        FaceRepo::refresh_gallery_tx(tx, cluster_id)?;
    }
    // Empty active galleries still own identities for trashed photos.
    // Keep those associations so restore remains reversible.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_schema;
    use rusqlite::Connection;

    #[test]
    fn permanent_delete_ignores_photos_not_in_trash() {
        let temp = tempfile::tempdir().unwrap();
        let photo_path = temp.path().join("keep.jpg");
        std::fs::write(&photo_path, b"original").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, is_trashed)
             VALUES (1, 'keep.jpg', 'keep.jpg', 'hash', 8, FALSE)",
            [],
        )
        .unwrap();

        let result = TrashService::permanent_delete(&conn, &[1], temp.path()).unwrap();

        assert_eq!(result.files_deleted, 0);
        assert_eq!(result.db_records_deleted, 0);
        assert!(photo_path.exists());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM photos WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn empty_trash_deletes_collected_rows() {
        let temp = tempfile::tempdir().unwrap();
        let photo_path = temp.path().join("gone.jpg");
        std::fs::write(&photo_path, b"original").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, is_trashed)
             VALUES (1, 'gone.jpg', 'gone.jpg', 'hash', 8, TRUE)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trash (photo_id, original_path) VALUES (1, 'gone.jpg')",
            [],
        )
        .unwrap();

        let result = TrashService::empty_trash(&conn, temp.path()).unwrap();

        assert_eq!(result.files_deleted, 1);
        assert_eq!(result.db_records_deleted, 1);
        assert!(!photo_path.exists());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn permanent_delete_refuses_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.jpg");
        std::fs::write(&outside_file, b"secret").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), temp.path().join("link")).unwrap();

        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(outside.path(), temp.path().join("link")).is_err() {
            return;
        }

        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, is_trashed)
             VALUES (1, 'link/secret.jpg', 'secret.jpg', 'hash', 6, TRUE)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trash (photo_id, original_path) VALUES (1, 'link/secret.jpg')",
            [],
        )
        .unwrap();

        let result = TrashService::permanent_delete(&conn, &[1], temp.path()).unwrap();

        assert_eq!(result.files_deleted, 0);
        assert_eq!(result.db_records_deleted, 0);
        assert!(!result.errors.is_empty());
        assert!(outside_file.exists());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM photos WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn trash_and_restore_refresh_face_cluster_stats() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size)
             VALUES (1, 'one.jpg', 'one.jpg', 'h1', 8),
                    (2, 'two.jpg', 'two.jpg', 'h2', 8)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO face_clusters (id, face_count, photo_count)
             VALUES (10, 2, 2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO faces
                (id, photo_id, bbox_x, bbox_y, bbox_width, bbox_height, embedding, cluster_id, confidence)
             VALUES
                (1, 1, 0.1, 0.1, 0.2, 0.2, zeroblob(16), 10, 0.5),
                (2, 2, 0.1, 0.1, 0.2, 0.2, zeroblob(16), 10, 0.9)",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE face_clusters SET representative_face_id = 2 WHERE id = 10",
            [],
        )
        .unwrap();

        assert_eq!(TrashService::trash_photos(&conn, &[2]).unwrap(), 1);
        let after_trash: (i64, i64, i64) = conn
            .query_row(
                "SELECT face_count, photo_count, representative_face_id FROM face_clusters WHERE id = 10",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(after_trash, (1, 1, 1));

        assert_eq!(TrashService::restore_photos(&conn, &[2]).unwrap(), 1);
        let after_restore: (i64, i64, i64) = conn
            .query_row(
                "SELECT face_count, photo_count, representative_face_id FROM face_clusters WHERE id = 10",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(after_restore, (2, 2, 2));
    }
}
