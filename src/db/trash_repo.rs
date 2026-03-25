//! Trash database operations.

use rusqlite::{Connection, Result as SqliteResult};

/// Trashed photo record.
#[derive(Debug, Clone)]
pub struct TrashedPhotoRecord {
    pub photo_id: i64,
    pub original_path: String,
    pub trashed_at: String,
    pub file_size: Option<i64>,
    pub thumbnail_path: Option<String>,
}

/// Trash repository.
pub struct TrashRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TrashRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_all(&self) -> SqliteResult<Vec<TrashedPhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                t.id,
                t.photo_id,
                t.original_path,
                t.trashed_at,
                p.file_size,
                p.date_taken,
                p.thumbnail_path
            FROM trash t
            JOIN photos p ON t.photo_id = p.id
            ORDER BY t.trashed_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(TrashedPhotoRecord {
                photo_id: row.get(1)?,
                original_path: row.get(2)?,
                trashed_at: row.get(3)?,
                file_size: row.get(4)?,
                thumbnail_path: row.get(6)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }
}
