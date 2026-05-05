//! Duplicate groups database operations

use rusqlite::{params, types::ToSql, Connection, Result as SqliteResult};

use super::MAX_ROWS_PER_INSERT;

/// Duplicate group record
#[derive(Debug, Clone)]
pub struct DuplicateGroupRecord {
    pub id: i64,
    pub member_count: i64,
}

/// Duplicate group member record
#[derive(Debug, Clone)]
pub struct DuplicateGroupMemberRecord {
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

    /// Create or update duplicate groups from detection results, preserving user decisions.
    ///
    /// Uses merge-based approach: existing groups whose hash matches are kept
    /// intact (preserving `is_suggested_keep`), new groups are created, and groups
    /// whose hash no longer has duplicates are removed.
    pub fn sync_duplicate_groups(
        &self,
        groups: &[(String, Vec<i64>, Option<i64>, &'static str)], // (hash, photo_ids, suggested_keep, duplicate_type)
    ) -> SqliteResult<()> {
        use std::collections::{HashMap, HashSet};

        let tx = self.conn.unchecked_transaction()?;

        // Load existing groups by hash
        let mut existing_hashes: HashMap<String, i64> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT id, group_hash FROM duplicate_groups")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, hash) = row?;
                existing_hashes.insert(hash, id);
            }
        }

        let mut seen_hashes: HashSet<String> = HashSet::new();

        for (hash, photo_ids, suggested_keep, dup_type) in groups {
            seen_hashes.insert(hash.clone());

            if existing_hashes.contains_key(hash) {
                continue; // Group exists — keep user's keep/dismiss choices
            }

            // Create new duplicate group. Use the tx handle (already open
            // above); nesting unchecked_transaction would conflict.
            tx.execute(
                r#"
                INSERT INTO duplicate_groups (group_hash, duplicate_type)
                VALUES (?1, ?2)
                "#,
                params![hash, dup_type],
            )?;

            let group_id = tx.last_insert_rowid();
            insert_duplicate_members(&tx, group_id, photo_ids, *suggested_keep)?;
        }

        // Remove groups whose hash no longer has duplicates
        for (hash, group_id) in &existing_hashes {
            if !seen_hashes.contains(hash) {
                self.delete_group(*group_id)?;
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
                member_count: row.get(1)?,
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
                photo_id: row.get(0)?,
                is_suggested_keep: row.get(1)?,
                file_path: row.get(2)?,
                thumbnail_path: row.get(3)?,
                file_size: row.get(4)?,
                date_taken: row.get(5)?,
            })
        })?;

        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }

        Ok(members)
    }

    /// Mark a photo as the one to keep in a group (atomic)
    pub fn set_keep_photo(&self, group_id: i64, photo_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "UPDATE duplicate_group_members SET is_suggested_keep = FALSE WHERE group_id = ?1",
            params![group_id],
        )?;

        tx.execute(
            "UPDATE duplicate_group_members SET is_suggested_keep = TRUE WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, photo_id],
        )?;

        tx.commit()
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

/// Batch-insert duplicate-group members via multi-row VALUES inside
/// the caller's transaction. ~3× faster than one INSERT per row for
/// large duplicate groups.
fn insert_duplicate_members(
    conn: &Connection,
    group_id: i64,
    photo_ids: &[i64],
    suggested_keep: Option<i64>,
) -> SqliteResult<()> {
    if photo_ids.is_empty() {
        return Ok(());
    }
    for chunk in photo_ids.chunks(MAX_ROWS_PER_INSERT) {
        let placeholders: String = (0..chunk.len())
            .map(|_| "(?, ?, ?)")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO duplicate_group_members (group_id, photo_id, is_suggested_keep) VALUES {}",
            placeholders
        );
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::with_capacity(chunk.len() * 3);
        for pid in chunk {
            let is_suggested = suggested_keep.map(|s| s == *pid).unwrap_or(false);
            params_vec.push(Box::new(group_id));
            params_vec.push(Box::new(*pid));
            params_vec.push(Box::new(is_suggested));
        }
        let params_refs: Vec<&dyn ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
    }
    Ok(())
}
