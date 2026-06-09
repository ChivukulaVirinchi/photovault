//! Album suggestion database operations
//!
//! Stores detected trip/event suggestions with lifecycle tracking.
//! Suggestions are fingerprinted so dismissed or accepted patterns are
//! never re-surfaced.

use rusqlite::{params, Connection, Result as SqliteResult};
use std::collections::{HashMap, HashSet};

/// A persisted album suggestion.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AlbumSuggestionRecord {
    pub id: i64,
    pub kind: String, // "trip" | "event"
    pub title: String,
    pub photo_ids_json: String, // JSON array of i64
    pub cover_photo_id: Option<i64>,
    pub fingerprint: String,
    pub status: String, // "pending" | "accepted" | "dismissed"
    pub seen_count: i64,
    pub created_at: String,
    /// Resolved absolute thumbnail path (set during loading, not from DB)
    pub cover_thumbnail_path: Option<String>,
}

impl AlbumSuggestionRecord {
    /// Deserialise the photo_ids JSON into a `Vec<i64>`.
    pub fn photo_ids(&self) -> Vec<i64> {
        serde_json::from_str(&self.photo_ids_json).unwrap_or_default()
    }

    pub fn photo_count(&self) -> usize {
        self.photo_ids().len()
    }
}

pub struct AlbumSuggestionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AlbumSuggestionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert a new suggestion. Returns the new row id.
    pub fn insert(
        &self,
        kind: &str,
        title: &str,
        photo_ids: &[i64],
        cover_photo_id: Option<i64>,
        fingerprint: &str,
    ) -> SqliteResult<i64> {
        let json = serde_json::to_string(photo_ids).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            r#"INSERT INTO album_suggestions
               (kind, title, photo_ids_json, cover_photo_id, fingerprint, status, seen_count)
               VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0)"#,
            params![kind, title, json, cover_photo_id, fingerprint],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Get all pending suggestions, ordered by newest first.
    pub fn get_pending(&self) -> SqliteResult<Vec<AlbumSuggestionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT s.id, s.kind, s.title, s.photo_ids_json, s.cover_photo_id,
                      s.fingerprint, s.status, s.seen_count, s.created_at,
                      pcov.thumbnail_path
               FROM album_suggestions s
               LEFT JOIN photos pcov
                      ON pcov.id = s.cover_photo_id
                     AND pcov.is_trashed = FALSE
               WHERE s.status = 'pending'
               ORDER BY s.created_at DESC"#,
        )?;
        let rows = stmt.query_map([], Self::map_row)?;
        let mut out = Vec::new();
        for r in rows {
            if let Some(record) = self.with_active_photos_only(r?)? {
                out.push(record);
            }
        }
        Ok(out)
    }

    /// Mark a suggestion as accepted.
    pub fn accept(&self, id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE album_suggestions SET status = 'accepted' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Mark a suggestion as dismissed (will never re-surface due to fingerprint).
    pub fn dismiss(&self, id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE album_suggestions SET status = 'dismissed' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Increment seen_count for all pending suggestions by 1.
    /// Suggestions with seen_count > 10 are auto-expired to 'dismissed'.
    pub fn increment_seen_counts(&self) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE album_suggestions SET seen_count = seen_count + 1 WHERE status = 'pending'",
            [],
        )?;
        self.conn.execute(
            "UPDATE album_suggestions SET status = 'dismissed' WHERE status = 'pending' AND seen_count > 10",
            [],
        )?;
        Ok(())
    }

    /// Get all fingerprints (accepted + dismissed) to avoid re-suggesting.
    pub fn get_all_fingerprints(&self) -> SqliteResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT fingerprint FROM album_suggestions WHERE status IN ('accepted', 'dismissed', 'pending')",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Delete suggestions older than `days` that are not pending.
    pub fn cleanup_old(&self, days: u32) -> SqliteResult<usize> {
        let affected = self.conn.execute(
            r#"DELETE FROM album_suggestions
               WHERE status != 'pending'
                 AND created_at < datetime('now', ?1)"#,
            params![format!("-{} days", days)],
        )?;
        Ok(affected)
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AlbumSuggestionRecord> {
        Ok(AlbumSuggestionRecord {
            id: row.get(0)?,
            kind: row.get(1)?,
            title: row.get(2)?,
            photo_ids_json: row.get(3)?,
            cover_photo_id: row.get(4)?,
            fingerprint: row.get(5)?,
            status: row.get(6)?,
            seen_count: row.get(7)?,
            created_at: row.get(8)?,
            cover_thumbnail_path: row.get(9)?,
        })
    }

    fn with_active_photos_only(
        &self,
        mut record: AlbumSuggestionRecord,
    ) -> SqliteResult<Option<AlbumSuggestionRecord>> {
        let ids = record.photo_ids();
        if ids.is_empty() {
            return Ok(None);
        }
        let active = self.active_photo_rows(&ids)?;
        if active.is_empty() {
            return Ok(None);
        }
        let active_set: HashSet<i64> = active.iter().map(|p| p.id).collect();
        let active_by_id: HashMap<i64, ActivePhoto> =
            active.into_iter().map(|p| (p.id, p)).collect();
        let filtered: Vec<i64> = ids
            .into_iter()
            .filter(|id| active_set.contains(id))
            .collect();
        record.photo_ids_json = serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".into());
        let cover_is_active = record
            .cover_photo_id
            .map(|id| active_set.contains(&id))
            .unwrap_or(false);
        let cover_is_renderable = record
            .cover_photo_id
            .and_then(|id| active_by_id.get(&id))
            .map(|p| p.media_type == "photo" || p.thumbnail_path.is_some())
            .unwrap_or(false);
        if !cover_is_active || !cover_is_renderable {
            record.cover_photo_id = filtered
                .iter()
                .copied()
                .find(|id| {
                    active_by_id
                        .get(id)
                        .map(|p| p.media_type == "photo")
                        .unwrap_or(false)
                })
                .or_else(|| {
                    filtered.iter().copied().find(|id| {
                        active_by_id
                            .get(id)
                            .and_then(|p| p.thumbnail_path.as_ref())
                            .is_some()
                    })
                })
                .or_else(|| filtered.first().copied());
        }
        record.cover_thumbnail_path = record
            .cover_photo_id
            .and_then(|id| active_by_id.get(&id))
            .and_then(|p| p.thumbnail_path.clone());
        let cover = record.cover_photo_id.and_then(|id| active_by_id.get(&id));
        let has_renderable_cover = cover
            .map(|p| p.media_type == "photo" || p.thumbnail_path.is_some())
            .unwrap_or(false);
        if has_renderable_cover {
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    fn active_photo_rows(&self, ids: &[i64]) -> SqliteResult<Vec<ActivePhoto>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(900) {
            let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id, media_type, thumbnail_path FROM photos WHERE is_trashed = FALSE AND id IN ({})",
                placeholders
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows =
                stmt.query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
                    Ok(ActivePhoto {
                        id: row.get(0)?,
                        media_type: row.get(1)?,
                        thumbnail_path: row.get(2)?,
                    })
                })?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
struct ActivePhoto {
    id: i64,
    media_type: String,
    thumbnail_path: Option<String>,
}
