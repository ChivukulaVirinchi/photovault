//! Burst groups database operations

use rusqlite::{params, Connection, Result as SqliteResult};

/// Burst group record
#[derive(Debug, Clone)]
pub struct BurstGroupRecord {
    pub id: i64,
    pub start_time: String,
    pub end_time: String,
    pub photo_count: i64,
}

/// Burst group member record
#[derive(Debug, Clone)]
pub struct BurstGroupMemberRecord {
    pub photo_id: i64,
    pub sharpness_score: Option<f32>,
    pub blur_score: Option<f32>,
    pub is_suggested_best: bool,
}

/// Burst repository
pub struct BurstRepo<'a> {
    conn: &'a Connection,
}

impl<'a> BurstRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create a new burst group
    pub fn create_group(
        &self,
        start_time: &str,
        end_time: &str,
        photo_ids: &[i64],
    ) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO burst_groups (start_time, end_time, photo_count)
            VALUES (?1, ?2, ?3)
            "#,
            params![start_time, end_time, photo_ids.len() as i64],
        )?;

        let group_id = self.conn.last_insert_rowid();

        // Add members
        for photo_id in photo_ids {
            self.conn.execute(
                r#"
                INSERT INTO burst_group_members (group_id, photo_id)
                VALUES (?1, ?2)
                "#,
                params![group_id, photo_id],
            )?;
        }

        Ok(group_id)
    }

    /// Sync burst groups from detection results
    pub fn sync_burst_groups(
        &self,
        groups: &[(String, String, Vec<i64>)], // (start, end, photo_ids)
    ) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Clear existing
        self.conn.execute("DELETE FROM burst_group_members", [])?;
        self.conn.execute("DELETE FROM burst_groups", [])?;

        for (start_time, end_time, photo_ids) in groups {
            self.create_group(start_time, end_time, photo_ids)?;
        }

        tx.commit()
    }

    /// Set the suggested best photo for a group
    pub fn set_suggested_best(&self, group_id: i64, photo_id: i64) -> SqliteResult<()> {
        // Clear existing
        self.conn.execute(
            "UPDATE burst_group_members SET is_suggested_best = FALSE WHERE group_id = ?1",
            params![group_id],
        )?;

        // Set new
        self.conn.execute(
            "UPDATE burst_group_members SET is_suggested_best = TRUE WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, photo_id],
        )?;

        Ok(())
    }

    /// Get all burst groups
    pub fn get_all_groups(&self) -> SqliteResult<Vec<BurstGroupRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, start_time, end_time, photo_count
            FROM burst_groups
            ORDER BY start_time DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(BurstGroupRecord {
                id: row.get(0)?,
                start_time: row.get(1)?,
                end_time: row.get(2)?,
                photo_count: row.get(3)?,
            })
        })?;

        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }

        Ok(groups)
    }

    /// Get members of a burst group
    pub fn get_group_members(&self, group_id: i64) -> SqliteResult<Vec<BurstGroupMemberRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                bgm.photo_id,
                bgm.sharpness_score,
                bgm.blur_score,
                bgm.face_count,
                bgm.is_suggested_best,
                p.file_path
            FROM burst_group_members bgm
            JOIN photos p ON bgm.photo_id = p.id
            WHERE bgm.group_id = ?1
            ORDER BY bgm.is_suggested_best DESC, p.date_taken ASC
            "#,
        )?;

        let rows = stmt.query_map(params![group_id], |row| {
            Ok(BurstGroupMemberRecord {
                photo_id: row.get(0)?,
                sharpness_score: row.get(1)?,
                blur_score: row.get(2)?,
                is_suggested_best: row.get(4)?,
            })
        })?;

        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }

        Ok(members)
    }

    /// Get non-best photos to potentially trash
    pub fn get_photos_to_trash(&self, group_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT photo_id
            FROM burst_group_members
            WHERE group_id = ?1 AND is_suggested_best = FALSE
            "#,
        )?;

        let rows = stmt.query_map(params![group_id], |row| row.get(0))?;

        let mut photo_ids = Vec::new();
        for row in rows {
            photo_ids.push(row?);
        }

        Ok(photo_ids)
    }

    /// Delete a burst group
    pub fn delete_group(&self, group_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "DELETE FROM burst_group_members WHERE group_id = ?1",
            params![group_id],
        )?;
        self.conn
            .execute("DELETE FROM burst_groups WHERE id = ?1", params![group_id])?;
        Ok(())
    }
}
