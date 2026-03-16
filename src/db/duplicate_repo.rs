//! Duplicate groups database operations

use rusqlite::{params, Connection, Result as SqliteResult};

/// Duplicate group record
#[derive(Debug, Clone)]
pub struct DuplicateGroupRecord {
    pub id: i64,
    pub group_hash: String,
    pub duplicate_type: String,
    pub member_count: i64,
}

/// Duplicate group member record
#[derive(Debug, Clone)]
pub struct DuplicateGroupMemberRecord {
    pub id: i64,
    pub group_id: i64,
    pub photo_id: i64,
    pub is_suggested_keep: bool,

    // Joined from photos table
    pub file_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub file_size: Option<i64>,
    pub date_taken: Option<String>,
}

/// Duplicate repository
pub struct DuplicateRepo<'a> {
    conn: &'a Connection,
}

impl<'a> DuplicateRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Create or update duplicate groups from detection results
    pub fn sync_duplicate_groups(
        &self,
        groups: &[(String, Vec<i64>, Option<i64>)], // (hash, photo_ids, suggested_keep)
    ) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Clear existing groups (full resync for now)
        self.conn
            .execute("DELETE FROM duplicate_group_members", [])?;
        self.conn.execute("DELETE FROM duplicate_groups", [])?;

        for (hash, photo_ids, suggested_keep) in groups {
            // Create group
            self.conn.execute(
                r#"
                INSERT INTO duplicate_groups (group_hash, duplicate_type)
                VALUES (?1, 'exact')
                "#,
                params![hash],
            )?;

            let group_id = self.conn.last_insert_rowid();

            // Add members
            for photo_id in photo_ids {
                let is_suggested = suggested_keep.map(|s| s == *photo_id).unwrap_or(false);

                self.conn.execute(
                    r#"
                    INSERT INTO duplicate_group_members (group_id, photo_id, is_suggested_keep)
                    VALUES (?1, ?2, ?3)
                    "#,
                    params![group_id, photo_id, is_suggested],
                )?;
            }
        }

        tx.commit()
    }

    /// Get all duplicate groups with member counts
    pub fn get_all_groups(&self) -> SqliteResult<Vec<DuplicateGroupRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                dg.id,
                dg.group_hash,
                dg.duplicate_type,
                COUNT(dgm.id) as member_count
            FROM duplicate_groups dg
            LEFT JOIN duplicate_group_members dgm ON dg.id = dgm.group_id
            GROUP BY dg.id
            ORDER BY member_count DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(DuplicateGroupRecord {
                id: row.get(0)?,
                group_hash: row.get(1)?,
                duplicate_type: row.get(2)?,
                member_count: row.get(3)?,
            })
        })?;

        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }

        Ok(groups)
    }

    /// Get members of a specific group
    pub fn get_group_members(
        &self,
        group_id: i64,
    ) -> SqliteResult<Vec<DuplicateGroupMemberRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                dgm.id,
                dgm.group_id,
                dgm.photo_id,
                dgm.is_suggested_keep,
                p.file_path,
                p.thumbnail_path,
                p.file_size,
                p.date_taken
            FROM duplicate_group_members dgm
            JOIN photos p ON dgm.photo_id = p.id
            WHERE dgm.group_id = ?1
            ORDER BY dgm.is_suggested_keep DESC, p.date_taken ASC
            "#,
        )?;

        let rows = stmt.query_map(params![group_id], |row| {
            Ok(DuplicateGroupMemberRecord {
                id: row.get(0)?,
                group_id: row.get(1)?,
                photo_id: row.get(2)?,
                is_suggested_keep: row.get(3)?,
                file_path: row.get(4)?,
                thumbnail_path: row.get(5)?,
                file_size: row.get(6)?,
                date_taken: row.get(7)?,
            })
        })?;

        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }

        Ok(members)
    }

    /// Mark a photo as the one to keep in a group
    pub fn set_keep_photo(&self, group_id: i64, photo_id: i64) -> SqliteResult<()> {
        // Clear existing
        self.conn.execute(
            "UPDATE duplicate_group_members SET is_suggested_keep = FALSE WHERE group_id = ?1",
            params![group_id],
        )?;

        // Set new
        self.conn.execute(
            "UPDATE duplicate_group_members SET is_suggested_keep = TRUE WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, photo_id],
        )?;

        Ok(())
    }

    /// Delete a duplicate group (after resolution)
    pub fn delete_group(&self, group_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "DELETE FROM duplicate_group_members WHERE group_id = ?1",
            params![group_id],
        )?;
        self.conn.execute(
            "DELETE FROM duplicate_groups WHERE id = ?1",
            params![group_id],
        )?;
        Ok(())
    }

    /// Get photos to trash (all members except the one to keep)
    pub fn get_photos_to_trash(&self, group_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT photo_id
            FROM duplicate_group_members
            WHERE group_id = ?1 AND is_suggested_keep = FALSE
            "#,
        )?;

        let rows = stmt.query_map(params![group_id], |row| row.get(0))?;

        let mut photo_ids = Vec::new();
        for row in rows {
            photo_ids.push(row?);
        }

        Ok(photo_ids)
    }
}
