//! Duplicate detection: byte-identical (SHA-256) and near-identical
//! (perceptual DCT hash). The two passes run in sequence and emit
//! groups with `duplicate_type = 'exact' | 'perceptual'`.

use std::path::Path;

use image_hasher::{HashAlg, HasherConfig};
use rusqlite::Connection;

/// Result of duplicate detection
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Unique group identifier (SHA hash for exact, "phash:<hex>" for perceptual).
    pub hash: String,

    /// Photo IDs in this group
    pub photo_ids: Vec<i64>,

    /// Suggested photo ID to keep
    pub suggested_keep_id: Option<i64>,

    /// 'exact' | 'perceptual'.
    pub duplicate_type: &'static str,
}

/// Hamming-distance threshold (out of 64 bits) below which two photos
/// are considered perceptually similar. 14 bits ≈ 78% bit agreement,
/// catches re-edits, re-encodes, AND exposure-bracketed sequences
/// (±1-2 EV typical) without false-positives between visually distinct
/// photos that happen to share rough composition. Bumped from 10 after
/// users reported burst/exposure-variant pairs being missed.
const PHASH_HAMMING_THRESHOLD: u32 = 14;

/// Duplicate detection service
pub struct DuplicateDetector;

impl DuplicateDetector {
    /// Find all exact duplicate groups in the database
    ///
    /// Returns groups where 2+ photos share the same SHA256 hash.
    pub fn find_duplicates(conn: &Connection) -> rusqlite::Result<Vec<DuplicateGroup>> {
        // Query for duplicate hashes
        let mut stmt = conn.prepare(
            r#"
            SELECT file_hash, COUNT(*) as count
            FROM photos
            WHERE is_trashed = FALSE
            GROUP BY file_hash
            HAVING count > 1
            ORDER BY count DESC
            "#,
        )?;

        let hashes: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut groups = Vec::new();

        for hash in hashes {
            // Get all photos with this hash
            let mut photo_stmt = conn.prepare(
                r#"
                SELECT id, file_path, date_taken, file_size
                FROM photos
                WHERE file_hash = ?1 AND is_trashed = FALSE
                ORDER BY date_taken ASC, file_path ASC
                "#,
            )?;

            let photos: Vec<(i64, String, Option<String>, i64)> = photo_stmt
                .query_map([&hash], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

            if photos.len() < 2 {
                continue;
            }

            let photo_ids: Vec<i64> = photos.iter().map(|(id, _, _, _)| *id).collect();

            // Determine which photo to suggest keeping
            let suggested_keep_id = Self::suggest_keep(&photos);

            groups.push(DuplicateGroup {
                hash,
                photo_ids,
                suggested_keep_id,
                duplicate_type: "exact",
            });
        }

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
        // Backfill phash for any non-trashed photo that doesn't yet
        // have one. Cheap when the cached Small thumbnail is on disk;
        // skips the photo silently otherwise (next prewarm warms it).
        Self::backfill_phashes(conn, drive_root)?;

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
            .filter_map(|r| r.ok())
            .filter(|row| !exclude_ids.contains(&row.0))
            .collect();

        if rows.len() < 2 {
            return Ok(Vec::new());
        }

        // Naive O(n²) pairwise. ~1 ns per popcount means 100k photos
        // = 100M ops = under a second in release. For libraries
        // larger than that, the bucket-by-prefix optimisation in
        // CLAUDE.md is the next step; we don't need it for v1.0.
        let mut parent: Vec<usize> = (0..rows.len()).collect();
        fn find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for i in 0..rows.len() {
            for j in (i + 1)..rows.len() {
                let a = rows[i].1 as u64;
                let b = rows[j].1 as u64;
                let dist = (a ^ b).count_ones();
                if dist <= PHASH_HAMMING_THRESHOLD {
                    let ra = find(&mut parent, i);
                    let rb = find(&mut parent, j);
                    if ra != rb {
                        parent[ra] = rb;
                    }
                }
            }
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
            // Use the first member's phash as the group key (any
            // member would work — they're all within Hamming 10).
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

    /// Compute and persist phash for any photo where it's NULL, using
    /// the cached Small thumbnail when available. Photos whose thumb
    /// hasn't been generated yet are skipped silently — the next
    /// prewarm pass will populate them.
    ///
    /// The thumbnail layout is
    /// `<drive>/.photovault/thumbnails/small/v2/<2hash>/<hash>.jpg`,
    /// matching what `ThumbnailService::thumbnail_path` produces. An
    /// earlier version of this code looked under `.photovault/thumbs/`
    /// (the long-removed layout), which silently caused EVERY photo to
    /// be skipped — and therefore the perceptual pass to find nothing.
    fn backfill_phashes(conn: &Connection, drive_root: &Path) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(
            "SELECT id, file_hash FROM photos WHERE is_trashed = FALSE AND phash IS NULL",
        )?;
        let pending: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        if pending.is_empty() {
            return Ok(());
        }

        let hasher = HasherConfig::new()
            .hash_alg(HashAlg::DoubleGradient)
            .hash_size(8, 8)
            .to_hasher();

        let small_root = drive_root
            .join(".photovault")
            .join("thumbnails")
            .join("small")
            .join("v2");

        let tx = conn.unchecked_transaction()?;
        let mut update = tx.prepare("UPDATE photos SET phash = ?2 WHERE id = ?1")?;
        let mut written = 0usize;
        for (id, file_hash) in &pending {
            let subdir = &file_hash[..2.min(file_hash.len())];
            let thumb_path = small_root.join(subdir).join(format!("{}.jpg", file_hash));
            if !thumb_path.exists() {
                continue;
            }
            let img = match crate::services::image_io::open_image(&thumb_path) {
                Ok(i) => i,
                Err(e) => {
                    tracing::trace!("phash skip {}: {}", thumb_path.display(), e);
                    continue;
                }
            };
            let h = hasher.hash_image(&img);
            // image_hasher returns variable-length bytes; truncate or
            // pad to exactly 8 bytes (= 64 bits) and reinterpret as i64.
            let bytes = h.as_bytes();
            let mut buf = [0u8; 8];
            let n = bytes.len().min(8);
            buf[..n].copy_from_slice(&bytes[..n]);
            let phash = i64::from_le_bytes(buf);
            update.execute(rusqlite::params![id, phash])?;
            written += 1;
        }
        drop(update);
        tx.commit()?;
        if written > 0 {
            tracing::info!("Backfilled phash for {} photos", written);
        }
        Ok(())
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

    /// Get total wasted space from duplicates (in bytes)
    pub fn calculate_wasted_space(conn: &Connection) -> rusqlite::Result<u64> {
        // For each duplicate group, sum all file sizes except the largest
        let mut stmt = conn.prepare(
            r#"
            SELECT file_hash, SUM(file_size) as total_size, MAX(file_size) as max_size, COUNT(*) as count
            FROM photos
            WHERE is_trashed = FALSE
            GROUP BY file_hash
            HAVING count > 1
            "#,
        )?;

        let wasted: i64 = stmt
            .query_map([], |row| {
                let total: i64 = row.get(1)?;
                let max: i64 = row.get(2)?;
                // Wasted = total - one copy
                Ok(total - max)
            })?
            .filter_map(|r| r.ok())
            .sum();

        Ok(wasted as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
