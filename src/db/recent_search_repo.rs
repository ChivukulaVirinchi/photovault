//! Recent searches persistence (per-library history).

use rusqlite::{params, Connection, Result as SqliteResult};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RecentSearch {
    pub query: String,
    pub last_used: String,
    pub use_count: i64,
}

pub struct RecentSearchRepo<'a> {
    conn: &'a Connection,
}

impl<'a> RecentSearchRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Record a search. Bumps use_count + last_used if it exists.
    pub fn record(&self, query: &str) -> SqliteResult<()> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            r#"
            INSERT INTO recent_searches (query, last_used, use_count)
            VALUES (?1, CURRENT_TIMESTAMP, 1)
            ON CONFLICT(query) DO UPDATE SET
                last_used = CURRENT_TIMESTAMP,
                use_count = use_count + 1
            "#,
            params![trimmed],
        )?;
        Ok(())
    }

    /// Get the N most-recently-used searches.
    pub fn get_recent(&self, limit: i64) -> SqliteResult<Vec<RecentSearch>> {
        let mut stmt = self.conn.prepare(
            "SELECT query, last_used, use_count FROM recent_searches
             ORDER BY last_used DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(RecentSearch {
                query: row.get(0)?,
                last_used: row.get(1)?,
                use_count: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Remove a single recent search.
    pub fn remove(&self, query: &str) -> SqliteResult<()> {
        self.conn.execute(
            "DELETE FROM recent_searches WHERE query = ?1",
            params![query],
        )?;
        Ok(())
    }

    /// Clear all recent searches.
    pub fn clear(&self) -> SqliteResult<()> {
        self.conn.execute("DELETE FROM recent_searches", [])?;
        Ok(())
    }
}
