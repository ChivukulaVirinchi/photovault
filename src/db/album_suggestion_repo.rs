//! Album suggestion database operations
//!
//! Stores detected trip/event suggestions with lifecycle tracking.
//! Suggestions are fingerprinted so dismissed or accepted patterns are
//! never re-surfaced.

use rusqlite::{params, Connection, Result as SqliteResult};

/// A persisted album suggestion.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AlbumSuggestionRecord {
    pub id: i64,
    pub kind: String,           // "trip" | "event"
    pub title: String,
    pub photo_ids_json: String, // JSON array of i64
    pub cover_photo_id: Option<i64>,
    pub fingerprint: String,
    pub status: String,         // "pending" | "accepted" | "dismissed"
    pub seen_count: i64,
    pub created_at: String,
    /// Resolved absolute thumbnail path (set during loading, not from DB)
    pub cover_thumbnail_path: Option<String>,
}

impl AlbumSuggestionRecord {
    /// Deserialise the photo_ids JSON into a Vec<i64>.
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
            r#"SELECT id, kind, title, photo_ids_json, cover_photo_id,
                      fingerprint, status, seen_count, created_at
               FROM album_suggestions
               WHERE status = 'pending'
               ORDER BY created_at DESC"#,
        )?;
        let rows = stmt.query_map([], Self::map_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
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
            cover_thumbnail_path: None,
        })
    }
}
