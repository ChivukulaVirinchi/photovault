//! Album database operations

use rusqlite::{params, types::ToSql, Connection, Result as SqliteResult};

use super::MAX_ROWS_PER_INSERT;

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
    pub created_by: String,
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
        self.create_with_source(name, "user")
    }

    /// Create a new album and mark who created it.
    pub fn create_with_source(&self, name: &str, created_by: &str) -> SqliteResult<i64> {
        self.conn.execute(
            "INSERT INTO albums (name, created_by) VALUES (?1, ?2)",
            params![name, created_by],
        )?;
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
        if photo_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut added = 0usize;
        for chunk in photo_ids.chunks(MAX_ROWS_PER_INSERT) {
            let candidate_sql = chunk
                .iter()
                .enumerate()
                .map(|(idx, _)| {
                    if idx == 0 {
                        "SELECT ? AS photo_id"
                    } else {
                        "UNION ALL SELECT ?"
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let sql = format!(
                r#"
                INSERT OR IGNORE INTO album_photos (album_id, photo_id)
                SELECT ?, candidate.photo_id
                  FROM ({candidate_sql}) AS candidate
                  JOIN photos p ON p.id = candidate.photo_id
                 WHERE p.is_trashed = FALSE
                "#
            );
            let mut values: Vec<Box<dyn ToSql>> = Vec::with_capacity(chunk.len() + 1);
            values.push(Box::new(album_id));
            for pid in chunk {
                values.push(Box::new(*pid));
            }
            let refs: Vec<&dyn ToSql> = values.iter().map(|v| v.as_ref()).collect();
            added += tx.execute(&sql, refs.as_slice())?;
        }
        update_album_stats_in_conn(&tx, album_id)?;
        tx.commit()?;
        Ok(added)
    }

    /// Remove photos from an album. Returns the number actually removed.
    pub fn remove_photos(&self, album_id: i64, photo_ids: &[i64]) -> SqliteResult<usize> {
        if photo_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut removed = 0usize;
        for chunk in photo_ids.chunks(MAX_ROWS_PER_INSERT) {
            let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "DELETE FROM album_photos WHERE album_id = ? AND photo_id IN ({placeholders})"
            );
            let mut values: Vec<Box<dyn ToSql>> = Vec::with_capacity(chunk.len() + 1);
            values.push(Box::new(album_id));
            for pid in chunk {
                values.push(Box::new(*pid));
            }
            let refs: Vec<&dyn ToSql> = values.iter().map(|v| v.as_ref()).collect();
            removed += tx.execute(&sql, refs.as_slice())?;
        }
        update_album_stats_in_conn(&tx, album_id)?;
        tx.commit()?;
        Ok(removed)
    }

    /// Get all albums ordered by most recently updated.
    pub fn get_all(&self) -> SqliteResult<Vec<AlbumRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT a.id, a.name, a.cover_photo_id, a.cover_auto_picked,
                   COUNT(p.id) AS live_photo_count, a.created_at, a.updated_at,
                   MIN(p.date_taken) AS date_range_start,
                   MAX(p.date_taken) AS date_range_end,
                   pcov.id AS live_cover_photo_id,
                   COALESCE(a.created_by, 'user') AS created_by,
                   pcov.thumbnail_path AS cover_thumbnail_path
            FROM albums a
            LEFT JOIN album_photos ap ON a.id = ap.album_id
            LEFT JOIN photos p ON ap.photo_id = p.id AND p.is_trashed = FALSE
            LEFT JOIN photos pcov ON pcov.id = a.cover_photo_id AND pcov.is_trashed = FALSE
            GROUP BY a.id
            ORDER BY a.updated_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(AlbumRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                cover_photo_id: row.get(9)?,
                cover_auto_picked: row.get::<_, bool>(3).unwrap_or(true),
                photo_count: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                date_range_start: row.get(7)?,
                date_range_end: row.get(8)?,
                cover_thumbnail_path: row.get(11)?,
                created_by: row.get(10)?,
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
              AND p.is_trashed = FALSE
            ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
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
            JOIN photos p ON p.id = ap.photo_id
            WHERE ap.photo_id = ?1
              AND p.is_trashed = FALSE
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
        auto_pick_cover_in_conn(self.conn, album_id)
    }

    /// Recompute persisted album counters and refresh auto-picked cover.
    pub fn refresh_stats(&self, album_id: i64) -> SqliteResult<()> {
        self.update_album_stats(album_id)
    }

    /// Update photo_count and optionally auto-pick cover.
    fn update_album_stats(&self, album_id: i64) -> SqliteResult<()> {
        update_album_stats_in_conn(self.conn, album_id)
    }
}

fn update_album_stats_in_conn(conn: &Connection, album_id: i64) -> SqliteResult<()> {
    conn.execute(
        r#"
        UPDATE albums SET
          photo_count = (
            SELECT COUNT(*)
              FROM album_photos ap
              JOIN photos p ON p.id = ap.photo_id
             WHERE ap.album_id = ?1
               AND p.is_trashed = FALSE
          ),
          updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
        params![album_id],
    )?;

    // Check if cover is auto-picked
    let auto_picked: bool = conn
        .query_row(
            "SELECT cover_auto_picked FROM albums WHERE id = ?1",
            params![album_id],
            |row| row.get(0),
        )
        .unwrap_or(true);

    if auto_picked {
        auto_pick_cover_in_conn(conn, album_id)?;
    }

    Ok(())
}

fn auto_pick_cover_in_conn(conn: &Connection, album_id: i64) -> SqliteResult<()> {
    let cover_id: Option<i64> = conn
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

    conn.execute(
        "UPDATE albums SET cover_photo_id = ?1, cover_auto_picked = TRUE, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![cover_id, album_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        crate::db::create_schema(&conn).expect("create schema");
        conn
    }

    fn insert_photo(conn: &Connection, id: i64, trashed: bool) {
        conn.execute(
            r#"
            INSERT INTO photos
                (id, file_path, file_name, file_hash, file_size, date_taken, is_trashed)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id,
                format!("IMG_{id:04}.jpg"),
                format!("IMG_{id:04}.jpg"),
                format!("hash-{id}"),
                1000 + id,
                format!("2024-01-01T12:0{id}:00Z"),
                trashed,
            ],
        )
        .expect("insert photo");
    }

    #[test]
    fn add_photos_batches_live_rows_and_ignores_duplicates_or_trashed() {
        let conn = setup();
        for id in 1..=3 {
            insert_photo(&conn, id, id == 3);
        }
        let repo = AlbumRepo::new(&conn);
        let album_id = repo.create("Trip").unwrap();

        let added = repo.add_photos(album_id, &[1, 2, 2, 3]).unwrap();

        assert_eq!(added, 2);
        let album = repo.get_all().unwrap().pop().unwrap();
        assert_eq!(album.photo_count, 2);
        assert!(album.cover_photo_id.is_some());
    }

    #[test]
    fn remove_photos_reports_actual_removed_count_and_refreshes_stats() {
        let conn = setup();
        for id in 1..=3 {
            insert_photo(&conn, id, false);
        }
        let repo = AlbumRepo::new(&conn);
        let album_id = repo.create("Trip").unwrap();
        repo.add_photos(album_id, &[1, 2, 3]).unwrap();

        let removed = repo.remove_photos(album_id, &[2, 999]).unwrap();

        assert_eq!(removed, 1);
        let album = repo.get_all().unwrap().pop().unwrap();
        assert_eq!(album.photo_count, 2);
        assert_eq!(repo.get_album_photo_ids(album_id).unwrap(), vec![3, 1]);
    }
}
