//! Per-library folder exclusions.

use rusqlite::{params, Connection, Result as SqliteResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedFolderRecord {
    pub relative_path: String,
    pub created_at: String,
    pub indexed_count: i64,
}

pub struct ExcludedFolderRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ExcludedFolderRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> SqliteResult<Vec<ExcludedFolderRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.relative_path, e.created_at,
                    (SELECT COUNT(*) FROM photos p
                     WHERE p.is_trashed = FALSE
                       AND (p.file_path = e.relative_path OR substr(p.file_path, 1, length(e.relative_path) + 1) = e.relative_path || '/'))
             FROM excluded_folders e
             ORDER BY e.relative_path COLLATE NOCASE",
        )?;
        let records = stmt
            .query_map([], |row| {
                Ok(ExcludedFolderRecord {
                    relative_path: row.get(0)?,
                    created_at: row.get(1)?,
                    indexed_count: row.get(2)?,
                })
            })?
            .collect();
        records
    }

    pub fn relative_paths(&self) -> SqliteResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT relative_path FROM excluded_folders ORDER BY relative_path")?;
        let paths = stmt.query_map([], |row| row.get(0))?.collect();
        paths
    }

    pub fn count_indexed_under(&self, relative_path: &str) -> SqliteResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos
             WHERE is_trashed = FALSE
               AND (file_path = ?1 OR substr(file_path, 1, length(?1) + 1) = ?1 || '/')",
            params![relative_path],
            |row| row.get(0),
        )
    }

    pub fn insert_and_remove_indexed(
        &self,
        relative_path: &str,
    ) -> SqliteResult<ExcludedFolderRecord> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO excluded_folders (relative_path) VALUES (?1)",
            params![relative_path],
        )?;
        tx.execute(
            "DELETE FROM photos
             WHERE file_path = ?1 OR substr(file_path, 1, length(?1) + 1) = ?1 || '/'",
            params![relative_path],
        )?;
        tx.commit()?;
        self.get(relative_path)
    }

    pub fn remove(&self, relative_path: &str) -> SqliteResult<bool> {
        let removed = self.conn.execute(
            "DELETE FROM excluded_folders WHERE relative_path = ?1",
            params![relative_path],
        )?;
        Ok(removed > 0)
    }

    fn get(&self, relative_path: &str) -> SqliteResult<ExcludedFolderRecord> {
        self.conn.query_row(
            "SELECT e.relative_path, e.created_at,
                    (SELECT COUNT(*) FROM photos p
                     WHERE p.is_trashed = FALSE
                       AND (p.file_path = e.relative_path OR substr(p.file_path, 1, length(e.relative_path) + 1) = e.relative_path || '/'))
             FROM excluded_folders e
             WHERE e.relative_path = ?1",
            params![relative_path],
            |row| {
                Ok(ExcludedFolderRecord {
                    relative_path: row.get(0)?,
                    created_at: row.get(1)?,
                    indexed_count: row.get(2)?,
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn counts_and_removes_only_folder_descendants() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (file_path, file_name, file_hash, file_size)
             VALUES
             ('Trips/Goa/a.jpg', 'a.jpg', 'h1', 12000),
             ('Trips/Goa/Sub/b.jpg', 'b.jpg', 'h2', 12000),
             ('Trips/Goa2/c.jpg', 'c.jpg', 'h3', 12000),
             ('Other/Goa/d.jpg', 'd.jpg', 'h4', 12000)",
            [],
        )
        .unwrap();

        let repo = ExcludedFolderRepo::new(&conn);
        assert_eq!(repo.count_indexed_under("Trips/Goa").unwrap(), 2);

        let record = repo.insert_and_remove_indexed("Trips/Goa").unwrap();
        assert_eq!(record.relative_path, "Trips/Goa");
        assert_eq!(record.indexed_count, 0);

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 2);
    }
}
