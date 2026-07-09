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

    pub fn count_all(&self) -> SqliteResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM trash", [], |row| row.get(0))
    }

    pub fn page_after(
        &self,
        after: Option<(String, i64)>,
        limit: i64,
    ) -> SqliteResult<Vec<TrashedPhotoRecord>> {
        let limit = limit.max(0);
        let sql_without_cursor = r#"
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
            ORDER BY t.trashed_at DESC, t.photo_id DESC
            LIMIT ?1
            "#;
        let sql_with_cursor = r#"
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
            WHERE t.trashed_at < ?1
               OR (t.trashed_at = ?1 AND t.photo_id < ?2)
            ORDER BY t.trashed_at DESC, t.photo_id DESC
            LIMIT ?3
            "#;

        let mut out = Vec::new();
        match after {
            Some((trashed_at, photo_id)) => {
                let mut stmt = self.conn.prepare(sql_with_cursor)?;
                let rows = stmt.query_map((trashed_at, photo_id, limit), trash_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(sql_without_cursor)?;
                let rows = stmt.query_map([limit], trash_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
        }
        Ok(out)
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

fn trash_row(row: &rusqlite::Row<'_>) -> SqliteResult<TrashedPhotoRecord> {
    Ok(TrashedPhotoRecord {
        photo_id: row.get(1)?,
        original_path: row.get(2)?,
        trashed_at: row.get(3)?,
        file_size: row.get(4)?,
        thumbnail_path: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::TrashRepo;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                file_size INTEGER,
                date_taken TEXT,
                thumbnail_path TEXT
            );
            CREATE TABLE trash (
                id INTEGER PRIMARY KEY,
                photo_id INTEGER NOT NULL UNIQUE,
                original_path TEXT NOT NULL,
                trashed_at TEXT NOT NULL
            );
            INSERT INTO photos (id, file_size, date_taken, thumbnail_path) VALUES
                (1, 10, NULL, 'a.jpg'),
                (2, 20, NULL, 'b.jpg'),
                (3, 30, NULL, 'c.jpg');
            INSERT INTO trash (photo_id, original_path, trashed_at) VALUES
                (1, 'one.jpg', '2026-01-03T00:00:00Z'),
                (2, 'two.jpg', '2026-01-02T00:00:00Z'),
                (3, 'three.jpg', '2026-01-01T00:00:00Z');
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn page_after_fetches_only_next_window() {
        let conn = setup();
        let repo = TrashRepo::new(&conn);

        let first = repo.page_after(None, 2).unwrap();
        assert_eq!(
            first.iter().map(|row| row.photo_id).collect::<Vec<_>>(),
            vec![1, 2]
        );

        let second = repo
            .page_after(Some((first[1].trashed_at.clone(), first[1].photo_id)), 2)
            .unwrap();
        assert_eq!(
            second.iter().map(|row| row.photo_id).collect::<Vec<_>>(),
            vec![3]
        );
    }

    #[test]
    fn page_after_survives_missing_cursor_row() {
        let conn = setup();
        let repo = TrashRepo::new(&conn);

        let first = repo.page_after(None, 2).unwrap();
        let cursor = (first[1].trashed_at.clone(), first[1].photo_id);
        conn.execute("DELETE FROM trash WHERE photo_id = 2", [])
            .unwrap();

        let second = repo.page_after(Some(cursor), 2).unwrap();
        assert_eq!(
            second.iter().map(|row| row.photo_id).collect::<Vec<_>>(),
            vec![3]
        );
    }
}
