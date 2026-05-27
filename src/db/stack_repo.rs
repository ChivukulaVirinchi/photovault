//! Photo stack persistence.

use rusqlite::{params, types::ToSql, Connection, Result as SqliteResult};

use super::MAX_ROWS_PER_INSERT;

#[derive(Debug, Clone)]
pub struct PhotoStackRecord {
    pub id: i64,
    pub kind: String,
    pub source_group_id: i64,
    pub cover_photo_id: i64,
    pub member_count: i64,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct PhotoStackMemberRecord {
    pub photo_id: i64,
    pub thumbnail_path: Option<String>,
    pub date_taken: Option<String>,
    pub quality_score: f32,
    pub score_reasons: Option<String>,
    pub is_cover: bool,
}

#[derive(Debug, Clone)]
pub struct StackCandidate {
    pub kind: String,
    pub source_group_id: i64,
    pub source_group_hash: Option<String>,
    pub photo_ids: Vec<i64>,
    pub cover_photo_id: i64,
    pub confidence: f32,
    pub member_scores: Vec<(i64, f32, String)>,
}

pub struct PhotoStackRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PhotoStackRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn sync_stacks(&self, candidates: &[StackCandidate]) -> SqliteResult<()> {
        use std::collections::HashSet;

        let tx = self.conn.unchecked_transaction()?;
        let mut seen: HashSet<(String, i64)> = HashSet::new();

        for c in candidates {
            if c.photo_ids.len() < 2 || !c.photo_ids.contains(&c.cover_photo_id) {
                continue;
            }
            seen.insert((c.kind.clone(), c.source_group_id));

            let existing = tx.query_row(
                "SELECT id, dismissed, cover_photo_id FROM photo_stacks WHERE kind = ?1 AND source_group_id = ?2",
                params![c.kind, c.source_group_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?, r.get::<_, i64>(2)?)),
            );

            let (stack_id, dismissed, existing_cover) = match existing {
                Ok(row) => row,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    tx.execute(
                        r#"
                        INSERT INTO photo_stacks
                            (kind, source_group_id, source_group_hash, cover_photo_id, confidence)
                        VALUES (?1, ?2, ?3, ?4, ?5)
                        "#,
                        params![
                            c.kind,
                            c.source_group_id,
                            c.source_group_hash,
                            c.cover_photo_id,
                            c.confidence
                        ],
                    )?;
                    (tx.last_insert_rowid(), false, c.cover_photo_id)
                }
                Err(e) => return Err(e),
            };

            if dismissed {
                continue;
            }

            let cover = if c.photo_ids.contains(&existing_cover) {
                existing_cover
            } else {
                c.cover_photo_id
            };

            tx.execute(
                r#"
                UPDATE photo_stacks
                   SET source_group_hash = ?3,
                       cover_photo_id = ?4,
                       confidence = ?5,
                       updated_at = CURRENT_TIMESTAMP
                 WHERE kind = ?1 AND source_group_id = ?2
                "#,
                params![
                    c.kind,
                    c.source_group_id,
                    c.source_group_hash,
                    cover,
                    c.confidence
                ],
            )?;
            tx.execute(
                "DELETE FROM photo_stack_members WHERE stack_id = ?1",
                params![stack_id],
            )?;
            insert_members(&tx, stack_id, cover, &c.member_scores)?;
        }

        let mut stmt = tx.prepare("SELECT id, kind, source_group_id FROM photo_stacks")?;
        let existing: Vec<(i64, String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<SqliteResult<Vec<_>>>()?;
        drop(stmt);
        for (id, kind, source_id) in existing {
            if !seen.contains(&(kind, source_id)) {
                tx.execute("DELETE FROM photo_stacks WHERE id = ?1", params![id])?;
            }
        }

        tx.commit()
    }

    pub fn get_stack(&self, stack_id: i64) -> SqliteResult<Option<PhotoStackRecord>> {
        match self.conn.query_row(
            r#"
            SELECT s.id, s.kind, s.source_group_id, s.cover_photo_id,
                   COUNT(m.id), s.confidence
              FROM photo_stacks s
              JOIN photo_stack_members m ON m.stack_id = s.id
             WHERE s.id = ?1 AND s.dismissed = FALSE
          GROUP BY s.id
            "#,
            params![stack_id],
            row_to_stack,
        ) {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_stack_for_photo(&self, photo_id: i64) -> SqliteResult<Option<PhotoStackRecord>> {
        match self.conn.query_row(
            r#"
            SELECT s.id, s.kind, s.source_group_id, s.cover_photo_id,
                   COUNT(all_m.id), s.confidence
              FROM photo_stack_members m
              JOIN photo_stacks s ON s.id = m.stack_id
              JOIN photo_stack_members all_m ON all_m.stack_id = s.id
             WHERE m.photo_id = ?1 AND s.dismissed = FALSE
          GROUP BY s.id
            "#,
            params![photo_id],
            row_to_stack,
        ) {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_members(&self, stack_id: i64) -> SqliteResult<Vec<PhotoStackMemberRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT m.photo_id, p.thumbnail_path, p.date_taken,
                   m.quality_score, m.score_reasons, m.is_cover
              FROM photo_stack_members m
              JOIN photos p ON p.id = m.photo_id
              JOIN photo_stacks s ON s.id = m.stack_id
             WHERE m.stack_id = ?1 AND s.dismissed = FALSE AND p.is_trashed = FALSE
          ORDER BY m.is_cover DESC, m.quality_score DESC, p.date_taken ASC, m.photo_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![stack_id], |r| {
            Ok(PhotoStackMemberRecord {
                photo_id: r.get(0)?,
                thumbnail_path: r.get(1)?,
                date_taken: r.get(2)?,
                quality_score: r.get(3)?,
                score_reasons: r.get(4)?,
                is_cover: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn set_cover(&self, stack_id: i64, photo_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM photo_stack_members WHERE stack_id = ?1 AND photo_id = ?2",
            params![stack_id, photo_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        tx.execute(
            "UPDATE photo_stack_members SET is_cover = FALSE WHERE stack_id = ?1",
            params![stack_id],
        )?;
        tx.execute(
            "UPDATE photo_stack_members SET is_cover = TRUE WHERE stack_id = ?1 AND photo_id = ?2",
            params![stack_id, photo_id],
        )?;
        tx.execute(
            "UPDATE photo_stacks SET cover_photo_id = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![stack_id, photo_id],
        )?;
        tx.commit()
    }

    pub fn remove_member(&self, stack_id: i64, photo_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM photo_stack_members WHERE stack_id = ?1 AND photo_id = ?2",
            params![stack_id, photo_id],
        )?;
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM photo_stack_members WHERE stack_id = ?1",
            params![stack_id],
            |r| r.get(0),
        )?;
        if count < 2 {
            tx.execute("DELETE FROM photo_stacks WHERE id = ?1", params![stack_id])?;
            tx.commit()?;
            return Ok(());
        }
        let cover_exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM photo_stack_members WHERE stack_id = ?1 AND is_cover = TRUE",
            params![stack_id],
            |r| r.get(0),
        )?;
        if cover_exists == 0 {
            let next_cover: i64 = tx.query_row(
                "SELECT photo_id FROM photo_stack_members WHERE stack_id = ?1 ORDER BY quality_score DESC, photo_id ASC LIMIT 1",
                params![stack_id],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE photo_stack_members SET is_cover = TRUE WHERE stack_id = ?1 AND photo_id = ?2",
                params![stack_id, next_cover],
            )?;
            tx.execute(
                "UPDATE photo_stacks SET cover_photo_id = ?2 WHERE id = ?1",
                params![stack_id, next_cover],
            )?;
        }
        tx.commit()
    }

    pub fn unstack(&self, stack_id: i64) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE photo_stacks SET dismissed = TRUE, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![stack_id],
        )?;
        Ok(())
    }

    pub fn photos_to_trash_except_cover(&self, stack_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT m.photo_id
              FROM photo_stack_members m
              JOIN photo_stacks s ON s.id = m.stack_id
             WHERE m.stack_id = ?1 AND m.photo_id != s.cover_photo_id
            "#,
        )?;
        let rows = stmt.query_map(params![stack_id], |r| r.get(0))?;
        rows.collect()
    }

    pub fn delete_stack(&self, stack_id: i64) -> SqliteResult<()> {
        self.conn
            .execute("DELETE FROM photo_stacks WHERE id = ?1", params![stack_id])?;
        Ok(())
    }
}

fn row_to_stack(row: &rusqlite::Row) -> SqliteResult<PhotoStackRecord> {
    Ok(PhotoStackRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        source_group_id: row.get(2)?,
        cover_photo_id: row.get(3)?,
        member_count: row.get(4)?,
        confidence: row.get(5)?,
    })
}

fn insert_members(
    conn: &Connection,
    stack_id: i64,
    cover_photo_id: i64,
    members: &[(i64, f32, String)],
) -> SqliteResult<()> {
    for chunk in members.chunks(MAX_ROWS_PER_INSERT) {
        let placeholders = (0..chunk.len())
            .map(|_| "(?, ?, ?, ?, ?)")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO photo_stack_members \
             (stack_id, photo_id, quality_score, score_reasons, is_cover) VALUES {}",
            placeholders
        );
        let mut values: Vec<Box<dyn ToSql>> = Vec::with_capacity(chunk.len() * 5);
        for (photo_id, score, reasons) in chunk {
            values.push(Box::new(stack_id));
            values.push(Box::new(*photo_id));
            values.push(Box::new(*score));
            values.push(Box::new(reasons.clone()));
            values.push(Box::new(*photo_id == cover_photo_id));
        }
        let refs: Vec<&dyn ToSql> = values.iter().map(|v| v.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        crate::db::create_schema(&conn).expect("create schema");
        conn
    }

    fn insert_photo(conn: &Connection, id: i64) {
        conn.execute(
            r#"
            INSERT INTO photos
                (id, file_path, file_name, file_hash, file_size, date_taken, thumbnail_path)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id,
                format!("IMG_{id:04}.jpg"),
                format!("IMG_{id:04}.jpg"),
                format!("hash-{id}"),
                1000 + id,
                format!("2024-01-01T12:0{id}:00Z"),
                format!(".photovault/thumbs/{id}.jpg"),
            ],
        )
        .expect("insert photo");
    }

    fn candidate(cover_photo_id: i64) -> StackCandidate {
        StackCandidate {
            kind: "burst".into(),
            source_group_id: 42,
            source_group_hash: None,
            photo_ids: vec![1, 2, 3],
            cover_photo_id,
            confidence: 0.8,
            member_scores: vec![
                (1, 10.0, "resolution".into()),
                (2, 30.0, "sharpness".into()),
                (3, 20.0, "faces".into()),
            ],
        }
    }

    #[test]
    fn sync_stacks_persists_members_and_cover() {
        let conn = setup();
        for id in 1..=3 {
            insert_photo(&conn, id);
        }

        let repo = PhotoStackRepo::new(&conn);
        repo.sync_stacks(&[candidate(2)]).expect("sync stacks");

        let stack = repo.get_stack_for_photo(1).expect("load stack").unwrap();
        assert_eq!(stack.kind, "burst");
        assert_eq!(stack.cover_photo_id, 2);
        assert_eq!(stack.member_count, 3);

        let members = repo.get_members(stack.id).expect("load members");
        assert_eq!(members.len(), 3);
        assert_eq!(members[0].photo_id, 2);
        assert!(members[0].is_cover);
    }

    #[test]
    fn refresh_preserves_manual_cover_when_member_still_exists() {
        let conn = setup();
        for id in 1..=3 {
            insert_photo(&conn, id);
        }

        let repo = PhotoStackRepo::new(&conn);
        repo.sync_stacks(&[candidate(2)]).expect("initial sync");
        let stack_id = repo.get_stack_for_photo(1).unwrap().unwrap().id;
        repo.set_cover(stack_id, 1).expect("manual cover");

        repo.sync_stacks(&[candidate(2)]).expect("refresh");

        let stack = repo.get_stack(stack_id).expect("load stack").unwrap();
        assert_eq!(stack.cover_photo_id, 1);
        let members = repo.get_members(stack_id).expect("load members");
        assert_eq!(members[0].photo_id, 1);
        assert!(members[0].is_cover);
    }

    #[test]
    fn removing_cover_promotes_best_remaining_member_and_deletes_singleton() {
        let conn = setup();
        for id in 1..=3 {
            insert_photo(&conn, id);
        }

        let repo = PhotoStackRepo::new(&conn);
        repo.sync_stacks(&[candidate(2)]).expect("sync stacks");
        let stack_id = repo.get_stack_for_photo(1).unwrap().unwrap().id;

        repo.remove_member(stack_id, 2).expect("remove cover");
        let stack = repo.get_stack(stack_id).expect("load stack").unwrap();
        assert_eq!(stack.cover_photo_id, 3);

        repo.remove_member(stack_id, 3)
            .expect("remove second member");
        assert!(repo.get_stack(stack_id).expect("load stack").is_none());
    }
}
