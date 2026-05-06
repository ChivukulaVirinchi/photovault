//! Album database operations

use rusqlite::{params, Connection, Result as SqliteResult};

/// Album record for list views (cover + summary info)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AlbumRecord {
    pub id: i64,
    pub name: String,
    pub cover_photo_id: Option<i64>,
    pub cover_auto_picked: bool,
    pub photo_count: i64,
    /// Earliest date_taken among album photos (ISO string or None)
    pub date_range_start: Option<String>,
    /// Latest date_taken among album photos (ISO string or None)
    pub date_range_end: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Resolved absolute thumbnail path for the cover photo (set during loading, not from DB)
    pub cover_thumbnail_path: Option<String>,
}

pub struct AlbumRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AlbumRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create a new album with the given name. Returns the new album ID.
    pub fn create(&self, name: &str) -> SqliteResult<i64> {
        self.conn
            .execute("INSERT INTO albums (name) VALUES (?1)", params![name])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Rename an existing album.
    pub fn rename(&self, album_id: i64, name: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE albums SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, album_id],
        )?;
        Ok(())
    }

    /// Delete an album and its photo associations. Photos themselves are NOT trashed.
    pub fn delete(&self, album_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "DELETE FROM album_photos WHERE album_id = ?1",
            params![album_id],
        )?;
        self.conn
            .execute("DELETE FROM albums WHERE id = ?1", params![album_id])?;
        Ok(())
    }

    /// Add photos to an album. Returns count of newly added (ignores duplicates).
    pub fn add_photos(&self, album_id: i64, photo_ids: &[i64]) -> SqliteResult<usize> {
        let mut added = 0usize;
        for pid in photo_ids {
            let result = self.conn.execute(
                "INSERT OR IGNORE INTO album_photos (album_id, photo_id) VALUES (?1, ?2)",
                params![album_id, pid],
            )?;
            if result > 0 {
                added += 1;
            }
        }
        self.update_album_stats(album_id)?;
        Ok(added)
    }

    /// Remove photos from an album.
    pub fn remove_photos(&self, album_id: i64, photo_ids: &[i64]) -> SqliteResult<()> {
        for pid in photo_ids {
            self.conn.execute(
                "DELETE FROM album_photos WHERE album_id = ?1 AND photo_id = ?2",
                params![album_id, pid],
            )?;
        }
        self.update_album_stats(album_id)?;
        Ok(())
    }

    /// Get all albums ordered by most recently updated.
    pub fn get_all(&self) -> SqliteResult<Vec<AlbumRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT a.id, a.name, a.cover_photo_id, a.cover_auto_picked,
                   a.photo_count, a.created_at, a.updated_at,
                   MIN(p.date_taken) AS date_range_start,
                   MAX(p.date_taken) AS date_range_end,
                   pcov.thumbnail_path AS cover_thumbnail_path
            FROM albums a
            LEFT JOIN album_photos ap ON a.id = ap.album_id
            LEFT JOIN photos p ON ap.photo_id = p.id
            LEFT JOIN photos pcov ON pcov.id = a.cover_photo_id
            GROUP BY a.id
            ORDER BY a.updated_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(AlbumRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                cover_photo_id: row.get(2)?,
                cover_auto_picked: row.get::<_, bool>(3).unwrap_or(true),
                photo_count: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                date_range_start: row.get(7)?,
                date_range_end: row.get(8)?,
                cover_thumbnail_path: row.get(9)?,
            })
        })?;

        let mut albums = Vec::new();
        for r in rows {
            albums.push(r?);
        }
        Ok(albums)
    }

    /// Get ordered photo IDs for an album.
    pub fn get_album_photo_ids(&self, album_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT ap.photo_id FROM album_photos ap
            JOIN photos p ON ap.photo_id = p.id
            WHERE ap.album_id = ?1
            ORDER BY p.date_taken ASC
            "#,
        )?;

        let rows = stmt.query_map(params![album_id], |row| row.get(0))?;
        let mut ids = Vec::new();
        for r in rows {
            ids.push(r?);
        }
        Ok(ids)
    }

    /// Get album memberships for a photo: (album_id, album_name) pairs.
    pub fn get_albums_for_photo(&self, photo_id: i64) -> SqliteResult<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT a.id, a.name FROM albums a
            JOIN album_photos ap ON a.id = ap.album_id
            WHERE ap.photo_id = ?1
            ORDER BY a.name ASC
            "#,
        )?;

        let rows = stmt.query_map(params![photo_id], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    /// Auto-pick a cover photo for the album (prefers faces, landscape, newest).
    pub fn auto_pick_cover(&self, album_id: i64) -> SqliteResult<()> {
        let cover_id: Option<i64> = self
            .conn
            .query_row(
                r#"
            SELECT ap.photo_id FROM album_photos ap
            JOIN photos p ON ap.photo_id = p.id
            LEFT JOIN faces f ON p.id = f.photo_id
            WHERE ap.album_id = ?1 AND p.is_trashed = FALSE
            GROUP BY ap.photo_id
            ORDER BY
              COUNT(f.id) > 0 DESC,
              p.width > p.height DESC,
              p.date_taken DESC
            LIMIT 1
            "#,
                params![album_id],
                |row| row.get(0),
            )
            .ok();

        self.conn.execute(
            "UPDATE albums SET cover_photo_id = ?1, cover_auto_picked = TRUE, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![cover_id, album_id],
        )?;
        Ok(())
    }

    /// Update photo_count and optionally auto-pick cover.
    fn update_album_stats(&self, album_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            r#"
            UPDATE albums SET
              photo_count = (SELECT COUNT(*) FROM album_photos WHERE album_id = ?1),
              updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![album_id],
        )?;

        // Check if cover is auto-picked
        let auto_picked: bool = self
            .conn
            .query_row(
                "SELECT cover_auto_picked FROM albums WHERE id = ?1",
                params![album_id],
                |row| row.get(0),
            )
            .unwrap_or(true);

        if auto_picked {
            self.auto_pick_cover(album_id)?;
        }

        Ok(())
    }
}
