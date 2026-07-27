//! Durable bookkeeping for Google Photos Takeout imports.

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

pub struct TakeoutImportRepo<'a> {
    conn: &'a Connection,
}

#[derive(Debug, Clone)]
pub struct TakeoutLedgerItem {
    pub content_hash: String,
    pub file_path: String,
    pub metadata_json: Option<String>,
    pub albums: BTreeSet<String>,
}

impl<'a> TakeoutImportRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn path_for_hash(&self, content_hash: &str) -> SqliteResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT file_path FROM google_takeout_items WHERE content_hash = ?1",
                params![content_hash],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn upsert_item(
        &self,
        content_hash: &str,
        file_path: &str,
        metadata_json: Option<&str>,
        albums: &BTreeSet<String>,
    ) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            r#"
            INSERT INTO google_takeout_items (content_hash, file_path, metadata_json)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(content_hash) DO UPDATE SET
                file_path = excluded.file_path,
                metadata_json = COALESCE(excluded.metadata_json, google_takeout_items.metadata_json),
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![content_hash, file_path, metadata_json],
        )?;
        for album in albums {
            tx.execute(
                "INSERT OR IGNORE INTO google_takeout_albums (content_hash, album_name) VALUES (?1, ?2)",
                params![content_hash, album],
            )?;
        }
        tx.commit()
    }

    pub fn upsert_items(&self, items: &[TakeoutLedgerItem]) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        for item in items {
            tx.execute(
                r#"
                INSERT INTO google_takeout_items (content_hash, file_path, metadata_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(content_hash) DO UPDATE SET
                    file_path = excluded.file_path,
                    metadata_json = COALESCE(excluded.metadata_json, google_takeout_items.metadata_json),
                    updated_at = CURRENT_TIMESTAMP
                "#,
                params![item.content_hash, item.file_path, item.metadata_json],
            )?;
            for album in &item.albums {
                tx.execute(
                    "INSERT OR IGNORE INTO google_takeout_albums (content_hash, album_name) VALUES (?1, ?2)",
                    params![item.content_hash, album],
                )?;
            }
        }
        tx.commit()
    }

    pub fn candidate_existing_files(
        &self,
        sizes: &std::collections::HashSet<i64>,
    ) -> SqliteResult<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_path, file_size FROM photos WHERE is_trashed = FALSE")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut result = Vec::new();
        for row in rows {
            let row = row?;
            if sizes.contains(&row.1) {
                result.push(row);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::tempdir;

    #[test]
    fn upsert_is_idempotent_and_unions_albums() {
        let dir = tempdir().unwrap();
        let db = Database::open_for_drive(dir.path()).unwrap();
        crate::db::create_schema(&db.conn).unwrap();
        let repo = TakeoutImportRepo::new(&db.conn);
        repo.upsert_item(
            "abc",
            "Imported from Google Photos/2020/a.jpg",
            Some(r#"{"favorited":true}"#),
            &BTreeSet::from(["Trip".to_string()]),
        )
        .unwrap();
        repo.upsert_item(
            "abc",
            "Imported from Google Photos/2020/a.jpg",
            None,
            &BTreeSet::from(["Family".to_string()]),
        )
        .unwrap();

        assert_eq!(
            repo.path_for_hash("abc").unwrap().as_deref(),
            Some("Imported from Google Photos/2020/a.jpg")
        );
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM google_takeout_albums WHERE content_hash = 'abc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
}
