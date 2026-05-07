//! Burst groups database operations

use rusqlite::{params, types::ToSql, Connection, Result as SqliteResult};

use super::MAX_ROWS_PER_INSERT;

/// Burst group record
#[derive(Debug, Clone)]
pub struct BurstGroupRecord {
    pub id: i64,
    pub start_time: String,
    pub end_time: String,
    pub photo_count: i64,
    /// Cover thumbnails (suggested-best first, then by date) — the
    /// listing card renders these as a horizontal filmstrip the user
    /// can click directly to open any photo in the viewer.
    pub cover_thumbnail_paths: Vec<String>,
    /// Photo IDs aligned 1:1 with `cover_thumbnail_paths`. Lets the
    /// frontend wire each filmstrip thumb to a specific photo route
    /// without an extra IPC roundtrip.
    pub cover_photo_ids: Vec<i64>,
    /// Every member's photo_id, in display order. Used to scope
    /// PhotoDetail's prev/next arrows to this burst when the user
    /// clicks a thumb in the listing — they navigate within the burst,
    /// not the whole library.
    pub member_photo_ids: Vec<i64>,
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

    /// Sync burst groups from detection results, preserving user decisions.
    ///
    /// Uses merge-based approach: existing groups whose photo sets match are kept
    /// intact (preserving `is_suggested_best`), new groups are created, and groups
    /// that no longer match any detection result are removed.
    pub fn sync_burst_groups(
        &self,
        groups: &[(String, String, Vec<i64>)], // (start, end, photo_ids)
    ) -> SqliteResult<()> {
        use std::collections::{BTreeSet, HashMap, HashSet};

        let tx = self.conn.unchecked_transaction()?;

        // Load existing groups with their photo_id sets
        let mut existing_sets: HashMap<BTreeSet<i64>, i64> = HashMap::new();
        {
            let mut grp_stmt = self.conn.prepare("SELECT id FROM burst_groups")?;
            let group_ids: Vec<i64> = grp_stmt
                .query_map([], |row| row.get::<_, i64>(0))?
                .filter_map(|r| r.ok())
                .collect();

            for gid in group_ids {
                let mut mem_stmt = self
                    .conn
                    .prepare("SELECT photo_id FROM burst_group_members WHERE group_id = ?1")?;
                let members: BTreeSet<i64> = mem_stmt
                    .query_map(params![gid], |row| row.get::<_, i64>(0))?
                    .filter_map(|r| r.ok())
                    .collect();
                existing_sets.insert(members, gid);
            }
        }

        let mut seen_sets: HashSet<BTreeSet<i64>> = HashSet::new();

        for (start_time, end_time, photo_ids) in groups {
            let set: BTreeSet<i64> = photo_ids.iter().copied().collect();
            seen_sets.insert(set.clone());

            if existing_sets.contains_key(&set) {
                continue; // Group exists — preserve user decisions
            }
            // Call the free helper directly — nested unchecked_transaction
            // would conflict with the outer `tx` we opened above.
            create_group_in_conn(&tx, start_time, end_time, photo_ids)?;
        }

        // Remove groups that no longer match any detection
        for (set, group_id) in &existing_sets {
            if !seen_sets.contains(set) {
                self.delete_group(*group_id)?;
            }
        }

        tx.commit()
    }

    /// If the group has no `is_suggested_best = TRUE` member, pick one.
    /// Targets the earliest member (by photos.date_taken, falling back to
    /// `bgm.rowid`) so the choice is stable across calls. Idempotent —
    /// callers can run it on every read.
    pub fn ensure_suggested_best(&self, group_id: i64) -> SqliteResult<()> {
        let has_best: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM burst_group_members \
             WHERE group_id = ?1 AND is_suggested_best = TRUE",
            params![group_id],
            |r| r.get(0),
        )?;
        if has_best > 0 {
            return Ok(());
        }
        self.conn.execute(
            r#"
            UPDATE burst_group_members
               SET is_suggested_best = TRUE
             WHERE group_id = ?1
               AND photo_id = (
                   SELECT bgm.photo_id
                     FROM burst_group_members bgm
                     JOIN photos p ON p.id = bgm.photo_id
                    WHERE bgm.group_id = ?1
                    ORDER BY p.date_taken ASC, bgm.rowid ASC
                    LIMIT 1
               )
            "#,
            params![group_id],
        )?;
        Ok(())
    }

    /// Set the suggested best photo for a group (atomic)
    pub fn set_suggested_best(&self, group_id: i64, photo_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "UPDATE burst_group_members SET is_suggested_best = FALSE WHERE group_id = ?1",
            params![group_id],
        )?;

        tx.execute(
            "UPDATE burst_group_members SET is_suggested_best = TRUE WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, photo_id],
        )?;

        tx.commit()
    }

    /// Get all burst groups + the first 4 thumbnails per group for the
    /// listing card's 2×2 stack. Two queries (one for groups, one for
    /// thumbs) is fine here — the listing is small and we keep the
    /// SQL plain rather than wrestling a window function.
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
                cover_thumbnail_paths: Vec::new(),
                cover_photo_ids: Vec::new(),
                member_photo_ids: Vec::new(),
            })
        })?;

        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }

        // Cover thumbnails (up to 6 for the filmstrip) — suggested-best
        // first, then by date. Photo ids paired 1:1 so the frontend
        // can route directly to PhotoDetail on click.
        let mut cover_stmt = self.conn.prepare(
            r#"
            SELECT m.photo_id, p.thumbnail_path
              FROM burst_group_members m
              JOIN photos p ON p.id = m.photo_id
             WHERE m.group_id = ?1
               AND p.thumbnail_path IS NOT NULL
          ORDER BY m.is_suggested_best DESC, p.date_taken ASC
             LIMIT 6
            "#,
        )?;
        // Every member's photo_id in the same order — used as the
        // browseContext scope when opening a photo from the listing
        // card. Caps at the natural group size; bursts rarely exceed
        // tens of photos.
        let mut all_stmt = self.conn.prepare(
            r#"
            SELECT m.photo_id
              FROM burst_group_members m
              JOIN photos p ON p.id = m.photo_id
             WHERE m.group_id = ?1
          ORDER BY m.is_suggested_best DESC, p.date_taken ASC
            "#,
        )?;
        for g in groups.iter_mut() {
            let covers: Vec<(i64, String)> = cover_stmt
                .query_map(params![g.id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            g.cover_photo_ids = covers.iter().map(|(id, _)| *id).collect();
            g.cover_thumbnail_paths = covers.into_iter().map(|(_, p)| p).collect();

            let members: Vec<i64> = all_stmt
                .query_map(params![g.id], |row| row.get::<_, i64>(0))?
                .filter_map(|r| r.ok())
                .collect();
            g.member_photo_ids = members;
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

/// Insert a burst group header + its members using the given connection.
/// Caller owns the transaction. Used by both the public `create_group`
/// (which wraps in its own tx) and `sync_burst_groups` (already in a tx).
fn create_group_in_conn(
    conn: &Connection,
    start_time: &str,
    end_time: &str,
    photo_ids: &[i64],
) -> SqliteResult<i64> {
    conn.execute(
        r#"
        INSERT INTO burst_groups (start_time, end_time, photo_count)
        VALUES (?1, ?2, ?3)
        "#,
        params![start_time, end_time, photo_ids.len() as i64],
    )?;
    let group_id = conn.last_insert_rowid();
    insert_group_members(conn, group_id, photo_ids)?;
    // Default-suggest the first member as best so the UI shows a
    // bordered "pick" without forcing the user to choose. The detail
    // view's "Pick this" buttons let them change it.
    if let Some(first) = photo_ids.first() {
        conn.execute(
            "UPDATE burst_group_members SET is_suggested_best = TRUE WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, first],
        )?;
    }
    Ok(group_id)
}

/// Batch-insert members for a burst group via multi-row VALUES inside
/// whatever transaction the caller is holding. ~3× faster than one
/// INSERT per row for large groups and keeps the whole write atomic.
fn insert_group_members(conn: &Connection, group_id: i64, photo_ids: &[i64]) -> SqliteResult<()> {
    if photo_ids.is_empty() {
        return Ok(());
    }
    for chunk in photo_ids.chunks(MAX_ROWS_PER_INSERT) {
        let placeholders: String = (0..chunk.len())
            .map(|_| "(?, ?)")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO burst_group_members (group_id, photo_id) VALUES {}",
            placeholders
        );
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::with_capacity(chunk.len() * 2);
        for pid in chunk {
            params_vec.push(Box::new(group_id));
            params_vec.push(Box::new(*pid));
        }
        let params_refs: Vec<&dyn ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
    }
    Ok(())
}
