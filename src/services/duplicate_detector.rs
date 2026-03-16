//! Exact duplicate detection using SHA256 hash matching

use rusqlite::Connection;

/// Result of duplicate detection
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Unique group identifier (hash value)
    pub hash: String,

    /// Photo IDs in this group
    pub photo_ids: Vec<i64>,

    /// Suggested photo ID to keep
    pub suggested_keep_id: Option<i64>,
}

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
            });
        }

        Ok(groups)
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

    /// Get count of duplicate groups
    pub fn count_duplicate_groups(conn: &Connection) -> rusqlite::Result<usize> {
        let count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM (
                SELECT file_hash
                FROM photos
                WHERE is_trashed = FALSE
                GROUP BY file_hash
                HAVING COUNT(*) > 1
            )
            "#,
            [],
            |row| row.get(0),
        )?;

        Ok(count as usize)
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
