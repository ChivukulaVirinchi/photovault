//! Trash management service.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::services::path_util::safe_join_relative;

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
                    "SELECT file_path FROM photos WHERE id = ?1",
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

        tx.commit()?;
        Ok(count)
    }

    pub fn restore_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<usize> {
        let tx = conn.unchecked_transaction()?;
        let mut count = 0usize;

        for photo_id in photo_ids {
            tx.execute("DELETE FROM trash WHERE photo_id = ?1", params![photo_id])?;
            tx.execute(
                "UPDATE photos SET is_trashed = FALSE, trashed_at = NULL WHERE id = ?1",
                params![photo_id],
            )?;
            count += 1;
        }

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
                    "SELECT file_path, thumbnail_path FROM photos WHERE id = ?1",
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
                if let Err(e) = fs::remove_file(&full_path) {
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

        tx.commit()?;
        Ok(result)
    }

    pub fn empty_trash(conn: &Connection, drive_root: &Path) -> SqliteResult<DeleteResult> {
        let mut stmt = conn.prepare("SELECT photo_id FROM trash")?;
        let ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
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
