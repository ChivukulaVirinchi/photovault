//! Undo for a decision about one face: "yes, that's them", "no", "move to
//! someone else", "not a face", "not sure".
//!
//! Before the decision, everything it can change about the face is copied:
//! its person and decision, its "not this person" marks, its places in
//! people's galleries, its review-queue rows and whether it was skipped.
//! Undo puts exactly that back, so it needs no per-action inverse and
//! never erases an older decision it didn't make.

use std::collections::BTreeSet;

use rusqlite::{params, OptionalExtension, Result as SqliteResult};

use super::FaceRepo;

const SKIPPED_KEY: &str = "people.review_skipped";

/// One face's state before a decision.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceSnapshot {
    pub face_id: i64,
    cluster_id: Option<i64>,
    user_confirmed: i32,
    negatives: Vec<i64>,
    gallery: Vec<GalleryRow>,
    review: Vec<ReviewRow>,
    skipped: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct GalleryRow {
    cluster_id: i64,
    embedding: Vec<u8>,
    pose_label: Option<String>,
    quality_score: Option<f64>,
    source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct ReviewRow {
    candidate_cluster_id: i64,
    score: f64,
    ambiguity: Option<f64>,
    resolved_at: Option<String>,
    resolved_as: Option<String>,
}

/// A face's person and decision: what an undo checks has not changed since.
pub type FaceState = (Option<i64>, i32);

impl<'a> FaceRepo<'a> {
    pub fn face_state(&self, face_id: i64) -> SqliteResult<Option<FaceState>> {
        self.conn
            .query_row(
                "SELECT cluster_id, user_confirmed FROM faces WHERE id = ?1",
                params![face_id],
                |row| Ok((row.get(0)?, row.get::<_, Option<i32>>(1)?.unwrap_or(0))),
            )
            .optional()
    }

    pub fn snapshot_face(&self, face_id: i64) -> SqliteResult<FaceSnapshot> {
        let (cluster_id, user_confirmed) = self
            .face_state(face_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let negatives = {
            let mut stmt = self
                .conn
                .prepare("SELECT not_cluster_id FROM face_negatives WHERE face_id = ?1")?;
            let rows = stmt.query_map(params![face_id], |row| row.get(0))?;
            rows.collect::<SqliteResult<_>>()?
        };
        let gallery = {
            let mut stmt = self.conn.prepare(
                "SELECT cluster_id, embedding, pose_label, quality_score, source
                 FROM person_gallery_embeddings WHERE face_id = ?1",
            )?;
            let rows = stmt.query_map(params![face_id], |row| {
                Ok(GalleryRow {
                    cluster_id: row.get(0)?,
                    embedding: row.get(1)?,
                    pose_label: row.get(2)?,
                    quality_score: row.get(3)?,
                    source: row.get(4)?,
                })
            })?;
            rows.collect::<SqliteResult<_>>()?
        };
        let review = {
            let mut stmt = self.conn.prepare(
                "SELECT candidate_cluster_id, score, ambiguity, resolved_at, resolved_as
                 FROM face_review_queue WHERE face_id = ?1",
            )?;
            let rows = stmt.query_map(params![face_id], |row| {
                Ok(ReviewRow {
                    candidate_cluster_id: row.get(0)?,
                    score: row.get(1)?,
                    ambiguity: row.get(2)?,
                    resolved_at: row.get(3)?,
                    resolved_as: row.get(4)?,
                })
            })?;
            rows.collect::<SqliteResult<_>>()?
        };
        let skipped = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM json_each(COALESCE(
                 (SELECT value FROM library_meta WHERE key = ?1), '[]')) WHERE value = ?2)",
            params![SKIPPED_KEY, face_id],
            |row| row.get(0),
        )?;
        Ok(FaceSnapshot {
            face_id,
            cluster_id,
            user_confirmed,
            negatives,
            gallery,
            review,
            skipped,
        })
    }

    /// Put a face back as it was in `snapshot`. The caller checks first that
    /// nothing else has changed the face since the decision.
    pub fn restore_face(&self, snapshot: &FaceSnapshot) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let face = snapshot.face_id;
        let exists = |cluster: i64| -> SqliteResult<bool> {
            tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM face_clusters WHERE id = ?1)",
                params![cluster],
                |row| row.get(0),
            )
        };

        // Every person the face belongs to, or is in the gallery of, now or
        // before: their counts and galleries are rebuilt at the end.
        let mut touched: BTreeSet<i64> = snapshot.gallery.iter().map(|g| g.cluster_id).collect();
        touched.extend(snapshot.cluster_id);
        let now: Option<i64> = tx.query_row(
            "SELECT cluster_id FROM faces WHERE id = ?1",
            params![face],
            |row| row.get(0),
        )?;
        touched.extend(now);
        {
            let mut stmt =
                tx.prepare("SELECT cluster_id FROM person_gallery_embeddings WHERE face_id = ?1")?;
            let rows = stmt.query_map(params![face], |row| row.get::<_, i64>(0))?;
            for row in rows {
                touched.insert(row?);
            }
        }

        let cluster = match snapshot.cluster_id {
            Some(c) if exists(c)? => Some(c),
            _ => None,
        };
        tx.execute(
            "UPDATE faces SET cluster_id = ?1, user_confirmed = ?2 WHERE id = ?3",
            params![cluster, snapshot.user_confirmed, face],
        )?;

        tx.execute(
            "DELETE FROM face_negatives WHERE face_id = ?1",
            params![face],
        )?;
        for &not_cluster in &snapshot.negatives {
            if exists(not_cluster)? {
                tx.execute(
                    "INSERT OR IGNORE INTO face_negatives (face_id, not_cluster_id) VALUES (?1, ?2)",
                    params![face, not_cluster],
                )?;
            }
        }

        tx.execute(
            "DELETE FROM person_gallery_embeddings WHERE face_id = ?1",
            params![face],
        )?;
        for g in &snapshot.gallery {
            if exists(g.cluster_id)? {
                tx.execute(
                    "INSERT INTO person_gallery_embeddings
                        (cluster_id, face_id, embedding, pose_label, quality_score, source)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        g.cluster_id,
                        face,
                        g.embedding,
                        g.pose_label,
                        g.quality_score,
                        g.source
                    ],
                )?;
            }
        }

        tx.execute(
            "DELETE FROM face_review_queue WHERE face_id = ?1",
            params![face],
        )?;
        for r in &snapshot.review {
            if exists(r.candidate_cluster_id)? {
                tx.execute(
                    "INSERT INTO face_review_queue
                        (face_id, candidate_cluster_id, score, ambiguity, resolved_at, resolved_as)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        face,
                        r.candidate_cluster_id,
                        r.score,
                        r.ambiguity,
                        r.resolved_at,
                        r.resolved_as
                    ],
                )?;
            }
        }

        if !snapshot.skipped {
            tx.execute(
                "UPDATE library_meta
                 SET value = (SELECT json_group_array(value) FROM json_each(library_meta.value)
                              WHERE value != ?2)
                 WHERE key = ?1",
                params![SKIPPED_KEY, face],
            )?;
        }

        for cluster in touched {
            if exists(cluster)? {
                Self::refresh_cluster_stats_tx(&tx, cluster)?;
                Self::refresh_gallery_tx(&tx, cluster)?;
            }
        }
        tx.commit()
    }
}
