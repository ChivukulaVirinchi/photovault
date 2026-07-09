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
        let mut existing_sets: HashMap<BTreeSet<i64>, (i64, bool)> = HashMap::new();
        {
            let mut grp_stmt = self.conn.prepare("SELECT id, resolved FROM burst_groups")?;
            let group_ids: Vec<(i64, bool)> = grp_stmt
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?))
                })?
                .collect::<SqliteResult<Vec<_>>>()?;

            for (gid, resolved) in group_ids {
                let mut mem_stmt = self
                    .conn
                    .prepare("SELECT photo_id FROM burst_group_members WHERE group_id = ?1")?;
                let members: BTreeSet<i64> = mem_stmt
                    .query_map(params![gid], |row| row.get::<_, i64>(0))?
                    .collect::<SqliteResult<BTreeSet<_>>>()?;
                existing_sets.insert(members, (gid, resolved));
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
        for (set, (group_id, _)) in &existing_sets {
            if !seen_sets.contains(set) {
                delete_group_in_conn(&tx, *group_id)?;
            }
        }

        tx.commit()
    }

    /// Insert any supplied burst groups that do not already exist by
    /// member set. Does not delete old groups. Used for live streaming
    /// during detection; a full completed run should still call
    /// `sync_burst_groups` to prune stale results.
    pub fn upsert_burst_groups(
        &self,
        groups: &[(String, String, Vec<i64>)],
    ) -> SqliteResult<usize> {
        Ok(self.upsert_burst_groups_collecting_inserted(groups)?.len())
    }

    pub fn upsert_burst_groups_collecting_inserted(
        &self,
        groups: &[(String, String, Vec<i64>)],
    ) -> SqliteResult<Vec<Vec<i64>>> {
        use std::collections::{BTreeSet, HashMap};

        let mut existing_sets: HashMap<BTreeSet<i64>, (i64, bool)> = HashMap::new();
        {
            let mut grp_stmt = self.conn.prepare("SELECT id, resolved FROM burst_groups")?;
            let group_ids: Vec<(i64, bool)> = grp_stmt
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?))
                })?
                .collect::<SqliteResult<Vec<_>>>()?;

            for (gid, resolved) in group_ids {
                let mut mem_stmt = self
                    .conn
                    .prepare("SELECT photo_id FROM burst_group_members WHERE group_id = ?1")?;
                let members: BTreeSet<i64> = mem_stmt
                    .query_map(params![gid], |row| row.get::<_, i64>(0))?
                    .collect::<SqliteResult<BTreeSet<_>>>()?;
                existing_sets.insert(members, (gid, resolved));
            }
        }

        let tx = self.conn.unchecked_transaction()?;
        let mut inserted_sets = Vec::new();
        for (start_time, end_time, photo_ids) in groups {
            let set: BTreeSet<i64> = photo_ids.iter().copied().collect();
            if existing_sets.contains_key(&set) {
                continue;
            }
            let group_id = create_group_in_conn(&tx, start_time, end_time, photo_ids)?;
            inserted_sets.push(set.iter().copied().collect());
            existing_sets.insert(set, (group_id, false));
        }
        tx.commit()?;
        Ok(inserted_sets)
    }

    pub fn delete_unresolved_groups_by_member_sets(
        &self,
        groups: &[Vec<i64>],
    ) -> SqliteResult<usize> {
        use std::collections::{BTreeSet, HashMap, HashSet};

        if groups.is_empty() {
            return Ok(0);
        }
        let targets: HashSet<BTreeSet<i64>> = groups
            .iter()
            .map(|ids| ids.iter().copied().collect())
            .collect();
        let mut existing_sets: HashMap<BTreeSet<i64>, i64> = HashMap::new();
        {
            let mut grp_stmt = self
                .conn
                .prepare("SELECT id FROM burst_groups WHERE resolved = FALSE")?;
            let group_ids: Vec<i64> = grp_stmt
                .query_map([], |row| row.get::<_, i64>(0))?
                .collect::<SqliteResult<Vec<_>>>()?;

            for gid in group_ids {
                let mut mem_stmt = self
                    .conn
                    .prepare("SELECT photo_id FROM burst_group_members WHERE group_id = ?1")?;
                let members: BTreeSet<i64> = mem_stmt
                    .query_map(params![gid], |row| row.get::<_, i64>(0))?
                    .collect::<SqliteResult<BTreeSet<_>>>()?;
                existing_sets.insert(members, gid);
            }
        }

        let tx = self.conn.unchecked_transaction()?;
        let mut deleted = 0usize;
        for set in targets {
            if let Some(group_id) = existing_sets.get(&set) {
                delete_group_in_conn(&tx, *group_id)?;
                deleted += 1;
            }
        }
        tx.commit()?;
        Ok(deleted)
    }

    /// If the group has no `is_suggested_best = TRUE` member, pick one.
    /// Targets the earliest member (by photos.date_taken, falling back to
    /// `bgm.rowid`) so the choice is stable across calls. Idempotent —
    /// callers can run it on every read.
    pub fn ensure_suggested_best(&self, group_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let has_best: i64 = tx.query_row(
            "SELECT COUNT(*) FROM burst_group_members bgm \
             JOIN photos p ON p.id = bgm.photo_id \
             WHERE bgm.group_id = ?1 \
               AND bgm.is_suggested_best = TRUE \
               AND p.is_trashed = FALSE",
            params![group_id],
            |r| r.get(0),
        )?;
        if has_best > 0 {
            tx.commit()?;
            return Ok(());
        }
        tx.execute(
            "UPDATE burst_group_members SET is_suggested_best = FALSE WHERE group_id = ?1",
            params![group_id],
        )?;
        tx.execute(
            r#"
            UPDATE burst_group_members
               SET is_suggested_best = TRUE
             WHERE group_id = ?1
               AND photo_id = (
                   SELECT bgm.photo_id
                     FROM burst_group_members bgm
                     JOIN photos p ON p.id = bgm.photo_id
                    WHERE bgm.group_id = ?1
                      AND p.is_trashed = FALSE
                    ORDER BY p.date_taken ASC, bgm.rowid ASC
                    LIMIT 1
               )
            "#,
            params![group_id],
        )?;
        tx.commit()
    }

    /// Set the suggested best photo for a group (atomic)
    pub fn set_suggested_best(&self, group_id: i64, photo_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        let exists: i64 = tx.query_row(
            "SELECT COUNT(*)
               FROM burst_group_members bgm
               JOIN burst_groups bg ON bg.id = bgm.group_id
              WHERE bgm.group_id = ?1
                AND bgm.photo_id = ?2
                AND bg.resolved = FALSE",
            params![group_id, photo_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

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
        self.get_groups(i64::MAX, 0)
    }

    pub fn get_groups(&self, limit: i64, offset: i64) -> SqliteResult<Vec<BurstGroupRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT bg.id, bg.start_time, bg.end_time, COUNT(p.id) AS live_photo_count
              FROM burst_groups bg
              JOIN burst_group_members bgm ON bgm.group_id = bg.id
              JOIN photos p ON p.id = bgm.photo_id
             WHERE bg.resolved = FALSE
               AND p.is_trashed = FALSE
          GROUP BY bg.id
            HAVING COUNT(p.id) > 1
          ORDER BY bg.start_time DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit.max(0), offset.max(0)], |row| {
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
               AND p.is_trashed = FALSE
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
               AND p.is_trashed = FALSE
          ORDER BY m.is_suggested_best DESC, p.date_taken ASC
            "#,
        )?;
        for g in groups.iter_mut() {
            let covers: Vec<(i64, String)> = cover_stmt
                .query_map(params![g.id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<SqliteResult<Vec<_>>>()?;
            g.cover_photo_ids = covers.iter().map(|(id, _)| *id).collect();
            g.cover_thumbnail_paths = covers.into_iter().map(|(_, p)| p).collect();

            let members: Vec<i64> = all_stmt
                .query_map(params![g.id], |row| row.get::<_, i64>(0))?
                .collect::<SqliteResult<Vec<_>>>()?;
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
            JOIN burst_groups bg ON bg.id = bgm.group_id
            JOIN photos p ON bgm.photo_id = p.id
            WHERE bgm.group_id = ?1
              AND bg.resolved = FALSE
              AND p.is_trashed = FALSE
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
            SELECT bgm.photo_id
            FROM burst_group_members bgm
            JOIN burst_groups bg ON bg.id = bgm.group_id
            JOIN photos p ON p.id = bgm.photo_id
            WHERE bgm.group_id = ?1
              AND bgm.is_suggested_best = FALSE
              AND bg.resolved = FALSE
              AND p.is_trashed = FALSE
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
        delete_group_in_conn(self.conn, group_id)
    }

    /// Mark a burst group as handled without deleting its member set.
    pub fn dismiss_group(&self, group_id: i64) -> SqliteResult<()> {
        let updated = self.conn.execute(
            "UPDATE burst_groups SET resolved = TRUE WHERE id = ?1",
            params![group_id],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }
}

fn delete_group_in_conn(conn: &Connection, group_id: i64) -> SqliteResult<()> {
    conn.execute(
        "DELETE FROM burst_group_members WHERE group_id = ?1",
        params![group_id],
    )?;
    conn.execute("DELETE FROM burst_groups WHERE id = ?1", params![group_id])?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_schema;

    #[test]
    fn invalid_best_photo_does_not_clear_existing_best() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z'),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z')",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        repo.upsert_burst_groups(&[(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            vec![1, 2],
        )])
        .unwrap();
        let group_id = repo.get_all_groups().unwrap()[0].id;

        assert!(repo.set_suggested_best(group_id, 999).is_err());

        let best_id: i64 = conn
            .query_row(
                "SELECT photo_id FROM burst_group_members
                 WHERE group_id = ?1 AND is_suggested_best = TRUE",
                params![group_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(best_id, 1);
    }

    #[test]
    fn groups_ignore_trashed_members() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken, is_trashed)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z', FALSE),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z', FALSE),
                    (3, 'c.jpg', 'c.jpg', 'c', 10, '2026-01-01T00:00:02Z', TRUE)",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        repo.upsert_burst_groups(&[(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:02Z".into(),
            vec![1, 2, 3],
        )])
        .unwrap();

        let group = repo.get_all_groups().unwrap().pop().unwrap();
        assert_eq!(group.photo_count, 2);
        assert_eq!(group.member_photo_ids, vec![1, 2]);
        assert_eq!(
            repo.get_group_members(group.id)
                .unwrap()
                .into_iter()
                .map(|m| m.photo_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn ensure_suggested_best_replaces_trashed_best() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken, is_trashed)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z', TRUE),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z', FALSE),
                    (3, 'c.jpg', 'c.jpg', 'c', 10, '2026-01-01T00:00:02Z', FALSE)",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        repo.upsert_burst_groups(&[(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:02Z".into(),
            vec![1, 2, 3],
        )])
        .unwrap();
        let group_id = conn
            .query_row("SELECT id FROM burst_groups", [], |row| row.get(0))
            .unwrap();

        repo.ensure_suggested_best(group_id).unwrap();

        assert_eq!(repo.get_photos_to_trash(group_id).unwrap(), vec![3]);
        let best_ids = conn
            .prepare(
                "SELECT photo_id FROM burst_group_members
                 WHERE group_id = ?1 AND is_suggested_best = TRUE
                 ORDER BY photo_id",
            )
            .unwrap()
            .query_map(params![group_id], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<SqliteResult<Vec<_>>>()
            .unwrap();
        assert_eq!(best_ids, vec![2]);
    }

    #[test]
    fn dismissed_group_does_not_reappear_on_next_detection() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z'),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z')",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        let detected = [(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            vec![1, 2],
        )];
        repo.sync_burst_groups(&detected).unwrap();
        let group_id = repo.get_all_groups().unwrap()[0].id;

        repo.dismiss_group(group_id).unwrap();
        assert!(repo.get_all_groups().unwrap().is_empty());

        assert_eq!(repo.upsert_burst_groups(&detected).unwrap(), 0);
        assert!(repo.get_all_groups().unwrap().is_empty());

        repo.sync_burst_groups(&detected).unwrap();
        assert!(repo.get_all_groups().unwrap().is_empty());
    }

    #[test]
    fn sync_prunes_stale_groups_inside_transaction() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z'),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z'),
                    (3, 'c.jpg', 'c.jpg', 'c', 10, '2026-01-01T00:00:02Z'),
                    (4, 'd.jpg', 'd.jpg', 'd', 10, '2026-01-01T00:00:03Z')",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        repo.upsert_burst_groups(&[
            (
                "2026-01-01T00:00:00Z".into(),
                "2026-01-01T00:00:01Z".into(),
                vec![1, 2],
            ),
            (
                "2026-01-01T00:00:02Z".into(),
                "2026-01-01T00:00:03Z".into(),
                vec![3, 4],
            ),
        ])
        .unwrap();

        repo.sync_burst_groups(&[(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            vec![1, 2],
        )])
        .unwrap();

        let group_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM burst_groups", [], |row| row.get(0))
            .unwrap();
        let member_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM burst_group_members", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(group_count, 1);
        assert_eq!(member_count, 2);
    }

    #[test]
    fn live_insert_cleanup_removes_only_new_unresolved_groups() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z'),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z'),
                    (3, 'c.jpg', 'c.jpg', 'c', 10, '2026-01-01T00:00:02Z'),
                    (4, 'd.jpg', 'd.jpg', 'd', 10, '2026-01-01T00:00:03Z')",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        repo.upsert_burst_groups(&[(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            vec![1, 2],
        )])
        .unwrap();

        let inserted = repo
            .upsert_burst_groups_collecting_inserted(&[
                (
                    "2026-01-01T00:00:00Z".into(),
                    "2026-01-01T00:00:01Z".into(),
                    vec![1, 2],
                ),
                (
                    "2026-01-01T00:00:02Z".into(),
                    "2026-01-01T00:00:03Z".into(),
                    vec![3, 4],
                ),
            ])
            .unwrap();

        assert_eq!(inserted, vec![vec![3, 4]]);
        assert_eq!(
            repo.delete_unresolved_groups_by_member_sets(&inserted)
                .unwrap(),
            1
        );
        let remaining = repo.get_all_groups().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].member_photo_ids, vec![1, 2]);
    }

    #[test]
    fn dismissed_group_cannot_be_mutated_or_trashed_from_stale_detail() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, date_taken)
             VALUES (1, 'a.jpg', 'a.jpg', 'a', 10, '2026-01-01T00:00:00Z'),
                    (2, 'b.jpg', 'b.jpg', 'b', 10, '2026-01-01T00:00:01Z')",
            [],
        )
        .unwrap();

        let repo = BurstRepo::new(&conn);
        repo.sync_burst_groups(&[(
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            vec![1, 2],
        )])
        .unwrap();
        let group_id = repo.get_all_groups().unwrap()[0].id;

        repo.dismiss_group(group_id).unwrap();

        assert!(repo.set_suggested_best(group_id, 2).is_err());
        assert!(repo.get_photos_to_trash(group_id).unwrap().is_empty());
    }
}
