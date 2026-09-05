//! Duplicate detection: byte-identical (SHA-256) and near-identical
//! (perceptual DCT hash). The two passes run in sequence and emit
//! groups with `duplicate_type = 'exact' | 'perceptual'`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use image_hasher::{HashAlg, HasherConfig};
use rayon::prelude::*;
use rusqlite::Connection;

use crate::services::path_util::safe_join_relative;

/// Result of duplicate detection
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Unique group identifier (SHA hash for exact, `phash:<hex>` for perceptual).
    pub hash: String,

    /// Photo IDs in this group
    pub photo_ids: Vec<i64>,

    /// Suggested photo ID to keep
    pub suggested_keep_id: Option<i64>,

    /// 'exact' | 'perceptual'.
    pub duplicate_type: &'static str,
}

#[derive(Debug, Clone)]
pub struct DuplicateProgress {
    pub stage: &'static str,
    pub processed: u64,
    pub total: Option<u64>,
    pub message: String,
}

type ExactCandidate = (i64, String, Option<String>, i64);

/// Hamming-distance threshold (out of 64 bits) below which two photos
/// are considered the same image. 4 bits ≈ 94% bit agreement — the
/// floor for "really actually the same shot, different file":
/// re-encoded JPEGs, stripped-EXIF copies, scale variants, watermark-
/// added copies. The previous 6-bit threshold (91%) still flagged too
/// many compositionally-similar but visually distinct photos as
/// "duplicates" (same wall + same lighting → near-identical pHash but
/// different subjects). Tighter is the right error: missing a dup is
/// fine, false-flagging real photos as dups erodes trust in the
/// listing. Burst-style near-dupes belong to the burst detector.
const PHASH_HAMMING_THRESHOLD: u32 = 4;

/// Duplicate detection service
pub struct DuplicateDetector;

struct PendingHashPhoto {
    id: i64,
    file_hash: String,
    file_path: String,
    orientation: i32,
    thumbnail_path: Option<String>,
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

impl DuplicateDetector {
    /// Find all exact duplicate groups in the database
    ///
    /// Returns groups where 2+ photos share the same SHA256 hash.
    pub fn find_duplicates(
        conn: &Connection,
        drive_root: &Path,
    ) -> rusqlite::Result<Vec<DuplicateGroup>> {
        Self::find_duplicates_cancellable(conn, drive_root, None)
    }

    pub fn find_duplicates_cancellable(
        conn: &Connection,
        drive_root: &Path,
        cancel: Option<&AtomicBool>,
    ) -> rusqlite::Result<Vec<DuplicateGroup>> {
        // Fast scanner hashes include file metadata, so byte-identical
        // copies with different mtimes may not share photos.file_hash.
        // Use file_size only to narrow candidates, then compute the
        // true full-file SHA-256 for exact duplicate grouping.
        let mut stmt = conn.prepare(
            r#"
            SELECT file_size, COUNT(*) as count
            FROM photos
            WHERE is_trashed = FALSE
            GROUP BY file_size
            HAVING count > 1
            ORDER BY count DESC
            "#,
        )?;

        let sizes: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut groups = Vec::new();

        for size in sizes {
            if is_cancelled(cancel) {
                return Ok(Vec::new());
            }
            let mut photo_stmt = conn.prepare(
                r#"
                SELECT id, file_path, date_taken, file_size
                FROM photos
                WHERE file_size = ?1 AND is_trashed = FALSE
                ORDER BY date_taken ASC, file_path ASC
                "#,
            )?;

            let photos: Vec<ExactCandidate> = photo_stmt
                .query_map([size], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            if photos.len() < 2 {
                continue;
            }

            let mut by_full_hash: std::collections::HashMap<String, Vec<ExactCandidate>> =
                std::collections::HashMap::new();
            for photo in photos {
                if is_cancelled(cancel) {
                    return Ok(Vec::new());
                }
                let Ok(path) = safe_join_relative(drive_root, &photo.1) else {
                    continue;
                };
                let Ok(full_hash) =
                    crate::services::scanner::calculate_hash_cancellable(&path, cancel)
                else {
                    continue;
                };
                by_full_hash.entry(full_hash).or_default().push(photo);
            }

            for (hash, photos) in by_full_hash {
                if photos.len() < 2 {
                    continue;
                }
                let photo_ids: Vec<i64> = photos.iter().map(|(id, _, _, _)| *id).collect();
                let suggested_keep_id = Self::suggest_keep(&photos);

                groups.push(DuplicateGroup {
                    hash,
                    photo_ids,
                    suggested_keep_id,
                    duplicate_type: "exact",
                });
            }
        }

        if is_cancelled(cancel) {
            return Ok(Vec::new());
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.photo_ids.len()));
        Ok(groups)
    }

    /// Find perceptually-similar duplicate groups (re-edits, re-saves,
    /// quality-adjusted exports). Computes phash on demand for any
    /// photo where it's NULL and persists into `photos.phash`, then
    /// groups by Hamming distance ≤ 10 across the 64-bit hashes.
    ///
    /// Excludes photos already grouped as exact duplicates so the UI
    /// doesn't show the same pair twice.
    pub fn find_perceptual_duplicates(
        conn: &Connection,
        drive_root: &Path,
        exclude_ids: &std::collections::HashSet<i64>,
    ) -> rusqlite::Result<Vec<DuplicateGroup>> {
        Self::find_perceptual_duplicates_with_progress(conn, drive_root, exclude_ids, None, |_| {})
    }

    pub fn find_perceptual_duplicates_with_progress(
        conn: &Connection,
        drive_root: &Path,
        exclude_ids: &std::collections::HashSet<i64>,
        cancel: Option<&AtomicBool>,
        mut progress: impl FnMut(DuplicateProgress),
    ) -> rusqlite::Result<Vec<DuplicateGroup>> {
        // Backfill phash for any non-trashed photo that doesn't yet
        // have one. Prefer the DB thumbnail path, then known thumbnail
        // tiers, then the original file so duplicate detection is not
        // coupled to a particular cache size.
        Self::backfill_phashes(conn, drive_root, cancel, &mut progress)?;
        if is_cancelled(cancel) {
            return Ok(Vec::new());
        }

        // Pull (id, phash, file_path, date_taken, file_size) for
        // photos with non-null phash that aren't already in an exact
        // group. file_path / size feed `suggest_keep`.
        let mut stmt = conn.prepare(
            r#"
            SELECT id, phash, file_path, date_taken, file_size
            FROM photos
            WHERE is_trashed = FALSE AND phash IS NOT NULL
            "#,
        )?;
        let rows: Vec<(i64, i64, String, Option<String>, i64)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|row| !exclude_ids.contains(&row.0))
            .collect();

        if rows.len() < 2 {
            return Ok(Vec::new());
        }
        progress(DuplicateProgress {
            stage: "perceptual-index",
            processed: 0,
            total: Some(rows.len() as u64),
            message: format!("indexing {} visual fingerprints", rows.len()),
        });

        // Candidate generation uses five disjoint pHash bands. With a
        // threshold of four bit differences across the whole 64-bit
        // hash, the pigeonhole principle guarantees that any valid
        // match has at least one identical band. That avoids the old
        // O(n^2) full comparison while preserving recall for the
        // configured threshold.
        fn find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        let n = rows.len();
        let phashes: Vec<u64> = rows.iter().map(|r| r.1 as u64).collect();
        let mut parent: Vec<usize> = (0..n).collect();
        let mut representatives = std::collections::HashMap::new();
        const BANDS: [(u32, u32); 5] = [(0, 13), (13, 13), (26, 13), (39, 13), (52, 12)];
        let mut buckets: std::collections::HashMap<(usize, u64), Vec<usize>> =
            std::collections::HashMap::with_capacity(n * BANDS.len());
        for (idx, hash) in phashes.iter().copied().enumerate() {
            if is_cancelled(cancel) {
                return Ok(Vec::new());
            }
            if let Some(&representative) = representatives.get(&hash) {
                parent[idx] = representative;
                continue;
            }
            representatives.insert(hash, idx);
            for (band_idx, (shift, width)) in BANDS.iter().copied().enumerate() {
                let mask = (1u64 << width) - 1;
                buckets
                    .entry((band_idx, (hash >> shift) & mask))
                    .or_default()
                    .push(idx);
            }
        }

        progress(DuplicateProgress {
            stage: "perceptual-compare",
            processed: 0,
            total: Some(buckets.len() as u64),
            message: format!("checking {} candidate buckets", buckets.len()),
        });

        let mut matches = 0usize;
        let total_buckets = buckets.len() as u64;
        let tick = total_buckets.div_ceil(40).max(1_000);
        for (bucket_idx, ((band_idx, _), members)) in buckets.into_iter().enumerate() {
            if is_cancelled(cancel) {
                return Ok(Vec::new());
            }
            if members.len() > 1 {
                for i in 0..members.len() {
                    if is_cancelled(cancel) {
                        return Ok(Vec::new());
                    }
                    for j in (i + 1)..members.len() {
                        if j % 256 == 0 && is_cancelled(cancel) {
                            return Ok(Vec::new());
                        }
                        let a_idx = members[i].min(members[j]);
                        let b_idx = members[i].max(members[j]);
                        // Compare a pair only in its first shared band, without
                        // retaining a potentially quadratic set of seen pairs.
                        if BANDS[..band_idx].iter().any(|&(shift, width)| {
                            ((phashes[a_idx] ^ phashes[b_idx]) >> shift) & ((1u64 << width) - 1)
                                == 0
                        }) {
                            continue;
                        }
                        let dist = (phashes[a_idx] ^ phashes[b_idx]).count_ones();
                        if dist <= PHASH_HAMMING_THRESHOLD {
                            let ra = find(&mut parent, a_idx);
                            let rb = find(&mut parent, b_idx);
                            if ra != rb {
                                parent[ra] = rb;
                            }
                            matches += 1;
                        }
                    }
                }
            }
            let processed = (bucket_idx + 1) as u64;
            if processed.is_multiple_of(tick) || processed == total_buckets {
                progress(DuplicateProgress {
                    stage: "perceptual-compare",
                    processed,
                    total: Some(total_buckets),
                    message: format!("{} visual matches", matches),
                });
            }
        }
        if is_cancelled(cancel) {
            return Ok(Vec::new());
        }

        let mut groups_map: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..rows.len() {
            let r = find(&mut parent, i);
            groups_map.entry(r).or_default().push(i);
        }

        let mut groups = Vec::new();
        for (_root, members) in groups_map {
            if members.len() < 2 {
                continue;
            }
            let photo_quad: Vec<(i64, String, Option<String>, i64)> = members
                .iter()
                .map(|&m| (rows[m].0, rows[m].2.clone(), rows[m].3.clone(), rows[m].4))
                .collect();
            let suggested_keep_id = Self::suggest_keep(&photo_quad);
            // Components are transitively connected; their endpoints need not
            // be within the pairwise threshold. Use the first member as a key.
            let phash_key = format!("phash:{:016x}", rows[members[0]].1 as u64);
            groups.push(DuplicateGroup {
                hash: phash_key,
                photo_ids: photo_quad.into_iter().map(|(id, _, _, _)| id).collect(),
                suggested_keep_id,
                duplicate_type: "perceptual",
            });
        }
        Ok(groups)
    }

    fn backfill_phashes(
        conn: &Connection,
        drive_root: &Path,
        cancel: Option<&AtomicBool>,
        progress: &mut impl FnMut(DuplicateProgress),
    ) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(
            "SELECT id, file_hash, file_path, orientation, thumbnail_path
             FROM photos
             WHERE is_trashed = FALSE AND media_type = 'photo' AND phash IS NULL",
        )?;
        let pending: Vec<PendingHashPhoto> = stmt
            .query_map([], |r| {
                Ok(PendingHashPhoto {
                    id: r.get(0)?,
                    file_hash: r.get(1)?,
                    file_path: r.get(2)?,
                    orientation: r.get::<_, Option<i32>>(3)?.unwrap_or(1),
                    thumbnail_path: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if pending.is_empty() {
            return Ok(());
        }
        let total = pending.len() as u64;
        progress(DuplicateProgress {
            stage: "perceptual-hash",
            processed: 0,
            total: Some(total),
            message: format!("building visual fingerprints for {} photos", total),
        });

        let processed = AtomicU64::new(0);

        let computed: Vec<(i64, i64)> = pending
            .par_iter()
            .filter_map(|photo| {
                if is_cancelled(cancel) {
                    return None;
                }
                let (source, apply_orientation) = Self::phash_source_path(drive_root, photo)?;
                let result = match Self::compute_phash(
                    &source,
                    apply_orientation.then_some(photo.orientation),
                ) {
                    Ok(phash) => Some((photo.id, phash)),
                    Err(e) => {
                        tracing::trace!("phash skip {}: {}", source.display(), e);
                        None
                    }
                };
                processed.fetch_add(1, Ordering::Relaxed);
                result
            })
            .collect();
        let done = processed.load(Ordering::Relaxed);
        progress(DuplicateProgress {
            stage: "perceptual-hash",
            processed: done,
            total: Some(total),
            message: format!("{} visual fingerprints ready", computed.len()),
        });
        if is_cancelled(cancel) {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        {
            let mut update = tx.prepare("UPDATE photos SET phash = ?2 WHERE id = ?1")?;
            for (id, phash) in &computed {
                update.execute(rusqlite::params![id, phash])?;
            }
        }
        tx.commit()?;
        if !computed.is_empty() {
            tracing::info!("Backfilled phash for {} photos", computed.len());
        }
        Ok(())
    }

    fn phash_source_path(drive_root: &Path, photo: &PendingHashPhoto) -> Option<(PathBuf, bool)> {
        let mut candidates = Vec::with_capacity(5);
        if let Some(path) = &photo.thumbnail_path {
            if let Ok(path) = crate::services::path_util::safe_join_relative(drive_root, path) {
                candidates.push((path, false));
            }
        }

        let subdir = &photo.file_hash[..2.min(photo.file_hash.len())];
        for size in ["small", "medium", "large"] {
            candidates.push((
                drive_root
                    .join(".photovault")
                    .join("thumbnails")
                    .join(size)
                    .join("v2")
                    .join(subdir)
                    .join(format!("{}.jpg", photo.file_hash)),
                false,
            ));
        }

        if let Ok(path) =
            crate::services::path_util::safe_join_relative(drive_root, &photo.file_path)
        {
            candidates.push((path, true));
        }
        candidates.into_iter().find(|(p, _)| p.exists())
    }

    fn compute_phash(path: &Path, orientation: Option<i32>) -> Result<i64, String> {
        let img = crate::services::image_io::open_image(path)?;
        let img = match orientation {
            Some(o) => crate::services::image_utils::apply_exif_orientation(img, o),
            None => img,
        };
        let hasher = HasherConfig::new()
            .hash_alg(HashAlg::DoubleGradient)
            .hash_size(8, 8)
            .to_hasher();
        let h = hasher.hash_image(&img);
        let bytes = h.as_bytes();
        let mut buf = [0u8; 8];
        let n = bytes.len().min(8);
        buf[..n].copy_from_slice(&bytes[..n]);
        Ok(i64::from_le_bytes(buf))
    }

    /// Suggest which photo to keep from a duplicate group
    ///
    /// Priority:
    /// 1. Prefer paths NOT containing "backup", "copy", "old", "duplicate"
    /// 2. Prefer larger file size
    /// 3. Prefer shortest path (better organized)
    /// 4. Prefer oldest by date_taken (stable tie-break via query order)
    fn suggest_keep(photos: &[(i64, String, Option<String>, i64)]) -> Option<i64> {
        if photos.is_empty() {
            return None;
        }

        let bad_folder_patterns = ["backup", "copy", "old", "duplicate", "temp", "tmp"];

        // Score each photo (lower bad-pattern score is better, larger size is better)
        let mut scored: Vec<(i64, i32, i64, usize)> = photos
            .iter()
            .map(|(id, path, _date, size)| {
                let path_lower = path.to_lowercase();
                let mut bad_score = 0i32;

                // Penalize bad folder names
                for pattern in &bad_folder_patterns {
                    if path_lower.contains(pattern) {
                        bad_score += 100;
                    }
                }

                (*id, bad_score, *size, path.len())
            })
            .collect();

        // Stable sort preserves original order (oldest first from query) when keys are equal.
        scored.sort_by(|a, b| {
            a.1.cmp(&b.1) // fewer bad-pattern penalties first
                .then_with(|| b.2.cmp(&a.2)) // larger file first
                .then_with(|| a.3.cmp(&b.3)) // shorter path first
        });

        scored.first().map(|(id, _, _, _)| *id)
    }

    /// Wasted bytes across the duplicate listing.
    ///
    /// Counts every detected duplicate group — both byte-exact dupes
    /// (same file_hash) AND perceptual ones (pHash match). The earlier
    /// version of this function only summed exact matches, which on a
    /// perceptual-only library reported "0 MB potentially wasted" even
    /// when the listing had dozens of groups. Now: walk the
    /// duplicate_groups table directly and sum (total_size - largest)
    /// per group, since the user's win is keeping one copy and
    /// trashing the rest.
    pub fn calculate_wasted_space(conn: &Connection) -> rusqlite::Result<u64> {
        let wasted: i64 = conn.query_row(
            r#"
            SELECT COALESCE(SUM(total_size - max_size), 0)
              FROM (
                SELECT SUM(p.file_size) AS total_size,
                       MAX(p.file_size) AS max_size
                  FROM duplicate_groups g
                  JOIN duplicate_group_members m ON m.group_id = g.id
                  JOIN photos p ON p.id = m.photo_id
                 WHERE p.is_trashed = FALSE
              GROUP BY g.id
                HAVING COUNT(*) > 1
              )
            "#,
            [],
            |row| row.get(0),
        )?;

        Ok(wasted.max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::create_schema;
    use image::{Rgb, RgbImage};
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn test_suggest_keep_prefers_good_paths() {
        let photos = vec![
            (1, "/Photos/backup/image.jpg".to_string(), None, 1000),
            (2, "/Photos/2019/image.jpg".to_string(), None, 1000),
            (3, "/Photos/old/copy/image.jpg".to_string(), None, 1000),
        ];

        let suggested = DuplicateDetector::suggest_keep(&photos);

        // Should prefer ID 2 (no bad patterns, shorter path)
        assert_eq!(suggested, Some(2));
    }

    #[test]
    fn test_suggest_keep_prefers_shorter_path() {
        let photos = vec![
            (
                1,
                "/Photos/2019/March/Trip/image.jpg".to_string(),
                None,
                1000,
            ),
            (2, "/Photos/image.jpg".to_string(), None, 1000),
        ];

        let suggested = DuplicateDetector::suggest_keep(&photos);

        // Should prefer ID 2 (shorter path)
        assert_eq!(suggested, Some(2));
    }

    #[test]
    fn exact_duplicates_use_full_file_hash_not_scanner_fast_hash() {
        let temp = tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        std::fs::create_dir_all(temp.path().join("photos")).unwrap();
        std::fs::write(temp.path().join("photos/a.jpg"), b"identical bytes").unwrap();
        std::fs::write(temp.path().join("photos/a-copy.jpg"), b"identical bytes").unwrap();

        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, is_trashed)
             VALUES
             (1, 'photos/a.jpg', 'a.jpg', 'fast-hash-a', 15, FALSE),
             (2, 'photos/a-copy.jpg', 'a-copy.jpg', 'fast-hash-b', 15, FALSE)",
            [],
        )
        .unwrap();

        let groups = DuplicateDetector::find_duplicates(&conn, temp.path()).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].duplicate_type, "exact");
        let mut ids = groups[0].photo_ids.clone();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn perceptual_duplicates_use_medium_thumbnail_when_small_is_missing() {
        let temp = tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        insert_photo_with_thumb(
            &conn,
            temp.path(),
            1,
            "photos/a.jpg",
            "aa11111111111111111111111111111111111111111111111111111111111111",
        );
        insert_photo_with_thumb(
            &conn,
            temp.path(),
            2,
            "exports/a-copy.jpg",
            "bb22222222222222222222222222222222222222222222222222222222222222",
        );

        let groups =
            DuplicateDetector::find_perceptual_duplicates(&conn, temp.path(), &Default::default())
                .unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].duplicate_type, "perceptual");
        assert_eq!(groups[0].photo_ids.len(), 2);

        let phash_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM photos WHERE phash IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(phash_count, 2);
    }

    #[test]
    fn perceptual_duplicates_fall_back_to_original_file() {
        let temp = tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        write_test_image(&temp.path().join("photos/a.jpg"));
        write_test_image(&temp.path().join("exports/a-copy.jpg"));
        conn.execute(
            "INSERT INTO photos (id, file_path, file_name, file_hash, file_size, media_type, is_trashed)
             VALUES
             (1, 'photos/a.jpg', 'a.jpg', 'hash-a', 100, 'photo', 0),
             (2, 'exports/a-copy.jpg', 'a-copy.jpg', 'hash-b', 100, 'photo', 0)",
            [],
        )
        .unwrap();

        let groups =
            DuplicateDetector::find_perceptual_duplicates(&conn, temp.path(), &Default::default())
                .unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].photo_ids.len(), 2);
    }

    fn insert_photo_with_thumb(
        conn: &Connection,
        drive_root: &Path,
        id: i64,
        file_path: &str,
        file_hash: &str,
    ) {
        let subdir = &file_hash[..2];
        let rel_thumb = format!(
            ".photovault/thumbnails/medium/v2/{}/{}.jpg",
            subdir, file_hash
        );
        write_test_image(&drive_root.join(&rel_thumb));
        conn.execute(
            "INSERT INTO photos
             (id, file_path, file_name, file_hash, file_size, media_type, thumbnail_path, is_trashed)
             VALUES (?1, ?2, ?3, ?4, 100, 'photo', ?5, 0)",
            rusqlite::params![id, file_path, file_path, file_hash, rel_thumb],
        )
        .unwrap();
    }

    fn write_test_image(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut img = RgbImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let color = if x < 32 {
                    Rgb([220, 40, 40])
                } else if y < 32 {
                    Rgb([40, 180, 80])
                } else {
                    Rgb([40, 80, 220])
                };
                img.put_pixel(x, y, color);
            }
        }
        img.save(path).unwrap();
    }
}
