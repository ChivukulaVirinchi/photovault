//! Trash management service.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};

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

impl TrashService {
    pub fn trash_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<usize> {
        let tx = conn.unchecked_transaction()?;
        let mut count = 0usize;

        for photo_id in photo_ids {
            let path: Option<String> = tx
                .query_row(
                    "SELECT file_path FROM photos WHERE id = ?1 AND is_trashed = FALSE",
                    params![photo_id],
                    |row| row.get(0),
                )
                .ok();

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

        PhotoStackRepo::new(&tx).reconcile_after_photos_trashed(photo_ids)?;
        refresh_album_state_for_photos(&tx, photo_ids)?;
        refresh_face_state_for_photos(&tx, photo_ids)?;
        tx.commit()?;
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
        let tx = conn.unchecked_transaction()?;
        let mut result = DeleteResult::default();

        for photo_id in photo_ids {
            let row: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT file_path, thumbnail_path FROM photos WHERE id = ?1 AND is_trashed = TRUE",
                    params![photo_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

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
                let delete_path = match safe_existing_path_under_root(drive_root, &relative_path) {
                    Ok(path) => path,
                    Err(e) => {
                        result.errors.push(format!("{}: {}", relative_path, e));
                        continue;
                    }
                };
                if let Err(e) = fs::remove_file(&delete_path) {
                    result.errors.push(format!("{}: {}", relative_path, e));
                    continue;
                }
                result.files_deleted += 1;
            }

            if let Some(tp) = thumbnail_path {
                if let Ok(thumb_full) = safe_join_relative(drive_root, &tp) {
                    if thumb_full.exists() {
                        let _ = fs::remove_file(thumb_full);
                    }
                }
            }

            tx.execute("DELETE FROM trash WHERE photo_id = ?1", params![photo_id])?;
            tx.execute("DELETE FROM photos WHERE id = ?1", params![photo_id])?;
            result.db_records_deleted += 1;
        }

        refresh_album_state_for_photos(&tx, photo_ids)?;
        refresh_face_state_for_photos(&tx, photo_ids)?;
        tx.commit()?;
        Ok(result)
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
    tx.execute(
        "DELETE FROM face_clusters WHERE face_count <= 0 AND photo_count <= 0",
        [],
    )?;
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
