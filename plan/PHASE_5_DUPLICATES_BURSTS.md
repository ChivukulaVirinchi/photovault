# Phase 5: Duplicate & Burst Detection

## Overview

This phase implements intelligent photo organization: finding exact duplicate files, grouping burst photos (rapid shooting), and suggesting the best photo from each burst to keep. These features help users reclaim storage by identifying redundant photos.

**Estimated Time:** 3-4 days  
**Difficulty:** Intermediate  
**Prerequisites:** Phase 3 complete (thumbnails + timeline), Phase 4 optional (face count for scoring)

---

## UI Design Guidelines

> **IMPORTANT:** When implementing any UI components in this phase, you MUST read and follow the design principles in `SKILL.md`. This file contains critical guidelines for:
> - Typography and spacing standards
> - Color usage and contrast requirements
> - Animation and interaction patterns
> - Component design principles
> - Accessibility requirements
>
> **Before writing ANY UI code, read SKILL.md thoroughly.** The goal is a delightful, polished user experience - not just functional code.

---

## Goals

- [ ] Implement exact duplicate detection (SHA256 hash matching)
- [ ] Build duplicate groups management
- [ ] Create burst detection algorithm (photos within 3 seconds)
- [ ] Implement best-pick scoring (sharpness, blur detection)
- [ ] Build Duplicates review UI
- [ ] Build Bursts review UI with best-pick highlighting
- [ ] Add "keep suggested" batch actions

---

## New Files

```
src/
├── services/
│   ├── duplicate_detector.rs   # Duplicate detection logic
│   └── burst_detector.rs       # Burst grouping + best-pick
├── db/
│   ├── duplicate_repo.rs       # Duplicate database operations
│   └── burst_repo.rs           # Burst database operations
├── scoring/
│   ├── mod.rs                  # Scoring module
│   ├── sharpness.rs            # Laplacian variance sharpness
│   └── blur.rs                 # Blur detection
└── views/
    ├── duplicates.rs           # Duplicates review view
    └── bursts.rs               # Bursts review view
```

---

## Step 1: Add Dependencies

Update `Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...

# Image processing for quality scoring
imageproc = "0.24"
```

---

## Step 2: Duplicate Detection Service

### File: `src/services/duplicate_detector.rs`

```rust
//! Exact duplicate detection using SHA256 hash matching

use std::collections::HashMap;

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
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                    ))
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
    /// 2. Prefer oldest by date_taken
    /// 3. Prefer shortest path (better organized)
    fn suggest_keep(photos: &[(i64, String, Option<String>, i64)]) -> Option<i64> {
        if photos.is_empty() {
            return None;
        }

        let bad_folder_patterns = ["backup", "copy", "old", "duplicate", "temp", "tmp"];

        // Score each photo (lower is better)
        let mut scored: Vec<(i64, i32)> = photos
            .iter()
            .map(|(id, path, _date, _size)| {
                let path_lower = path.to_lowercase();
                let mut score = 0i32;

                // Penalize bad folder names
                for pattern in &bad_folder_patterns {
                    if path_lower.contains(pattern) {
                        score += 100;
                    }
                }

                // Prefer shorter paths
                score += path.len() as i32;

                (*id, score)
            })
            .collect();

        // Sort by score, then by original order (oldest first from query)
        scored.sort_by_key(|(_, score)| *score);

        scored.first().map(|(id, _)| *id)
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
            (1, "/Photos/2019/March/Trip/image.jpg".to_string(), None, 1000),
            (2, "/Photos/image.jpg".to_string(), None, 1000),
        ];

        let suggested = DuplicateDetector::suggest_keep(&photos);
        
        // Should prefer ID 2 (shorter path)
        assert_eq!(suggested, Some(2));
    }
}
```

---

## Step 3: Duplicate Repository

### File: `src/db/duplicate_repo.rs`

```rust
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
        self.conn.execute("DELETE FROM duplicate_group_members", [])?;
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
    pub fn get_group_members(&self, group_id: i64) -> SqliteResult<Vec<DuplicateGroupMemberRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                dgm.id,
                dgm.group_id,
                dgm.photo_id,
                dgm.is_suggested_keep,
                p.file_path,
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
                file_size: row.get(5)?,
                date_taken: row.get(6)?,
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
```

Update `src/db/mod.rs`:

```rust
pub mod duplicate_repo;
pub use duplicate_repo::{DuplicateRepo, DuplicateGroupRecord, DuplicateGroupMemberRecord};
```

---

## Step 4: Burst Detection Service

### File: `src/services/burst_detector.rs`

```rust
//! Burst photo detection - groups photos taken within seconds of each other

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

/// A burst group of photos
#[derive(Debug, Clone)]
pub struct BurstGroup {
    /// Photo IDs in this burst (ordered by time)
    pub photo_ids: Vec<i64>,
    
    /// Start timestamp
    pub start_time: DateTime<Utc>,
    
    /// End timestamp
    pub end_time: DateTime<Utc>,
    
    /// Suggested best photo ID
    pub suggested_best_id: Option<i64>,
}

/// Burst detection configuration
#[derive(Debug, Clone)]
pub struct BurstConfig {
    /// Maximum time gap between photos in a burst (seconds)
    pub max_gap_seconds: i64,
    
    /// Minimum photos to form a burst
    pub min_photos: usize,
}

impl Default for BurstConfig {
    fn default() -> Self {
        Self {
            max_gap_seconds: 3,
            min_photos: 3,
        }
    }
}

/// Burst detection service
pub struct BurstDetector {
    config: BurstConfig,
}

impl BurstDetector {
    pub fn new(config: BurstConfig) -> Self {
        Self { config }
    }

    /// Find all burst groups in the database
    pub fn find_bursts(&self, conn: &Connection) -> rusqlite::Result<Vec<BurstGroup>> {
        // Get all photos ordered by date_taken
        let mut stmt = conn.prepare(
            r#"
            SELECT id, date_taken
            FROM photos
            WHERE date_taken IS NOT NULL AND is_trashed = FALSE
            ORDER BY date_taken ASC
            "#,
        )?;

        let photos: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        if photos.is_empty() {
            return Ok(Vec::new());
        }

        let mut groups = Vec::new();
        let mut current_group: Vec<(i64, DateTime<Utc>)> = Vec::new();

        for (id, date_str) in photos {
            let date = match Self::parse_datetime(&date_str) {
                Some(d) => d,
                None => continue,
            };

            if current_group.is_empty() {
                current_group.push((id, date));
            } else {
                let last_date = current_group.last().unwrap().1;
                let gap = date.signed_duration_since(last_date);

                if gap <= Duration::seconds(self.config.max_gap_seconds) {
                    current_group.push((id, date));
                } else {
                    // Gap too large, finalize current group
                    if current_group.len() >= self.config.min_photos {
                        groups.push(self.finalize_group(&current_group));
                    }
                    current_group = vec![(id, date)];
                }
            }
        }

        // Don't forget the last group
        if current_group.len() >= self.config.min_photos {
            groups.push(self.finalize_group(&current_group));
        }

        Ok(groups)
    }

    /// Create a BurstGroup from collected photos
    fn finalize_group(&self, photos: &[(i64, DateTime<Utc>)]) -> BurstGroup {
        let photo_ids: Vec<i64> = photos.iter().map(|(id, _)| *id).collect();
        let start_time = photos.first().unwrap().1;
        let end_time = photos.last().unwrap().1;

        BurstGroup {
            photo_ids,
            start_time,
            end_time,
            suggested_best_id: None, // Will be set by best-pick scoring
        }
    }

    /// Parse datetime string to DateTime<Utc>
    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        // Try common formats
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Some(dt.with_timezone(&Utc));
        }

        // Try SQLite datetime format: "YYYY-MM-DD HH:MM:SS"
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
        }

        None
    }

    /// Get burst statistics
    pub fn get_stats(&self, conn: &Connection) -> rusqlite::Result<BurstStats> {
        let groups = self.find_bursts(conn)?;
        
        let total_groups = groups.len();
        let total_photos: usize = groups.iter().map(|g| g.photo_ids.len()).sum();
        let saveable_photos = total_photos - total_groups; // Keep 1 per group

        Ok(BurstStats {
            total_groups,
            total_photos,
            saveable_photos,
        })
    }
}

/// Burst detection statistics
#[derive(Debug, Clone)]
pub struct BurstStats {
    pub total_groups: usize,
    pub total_photos: usize,
    pub saveable_photos: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_datetime() {
        let dt = BurstDetector::parse_datetime("2019-03-15 14:30:22");
        assert!(dt.is_some());
        
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2019);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
    }
}
```

---

## Step 5: Image Quality Scoring

### File: `src/scoring/mod.rs`

```rust
//! Image quality scoring for best-pick selection

pub mod sharpness;
pub mod blur;

pub use sharpness::SharpnessScorer;
pub use blur::BlurDetector;

use image::DynamicImage;

/// Combined quality score for a photo
#[derive(Debug, Clone, Default)]
pub struct QualityScore {
    /// Sharpness score (0-1, higher is sharper)
    pub sharpness: f32,
    
    /// Blur score (0-1, higher means less blur)
    pub blur: f32,
    
    /// Face count (normalized)
    pub face_count: f32,
    
    /// Average face detection confidence
    pub face_confidence: f32,
    
    /// Combined weighted score
    pub combined: f32,
}

impl QualityScore {
    /// Calculate combined score from components
    pub fn calculate_combined(&mut self) {
        // Weights for each component
        const SHARPNESS_WEIGHT: f32 = 0.4;
        const BLUR_WEIGHT: f32 = 0.3;
        const FACE_CONFIDENCE_WEIGHT: f32 = 0.2;
        const FACE_COUNT_WEIGHT: f32 = 0.1;

        self.combined = (self.sharpness * SHARPNESS_WEIGHT)
            + (self.blur * BLUR_WEIGHT)
            + (self.face_confidence * FACE_CONFIDENCE_WEIGHT)
            + (self.face_count * FACE_COUNT_WEIGHT);
    }
}

/// Score a single image for quality
pub fn score_image(image: &DynamicImage) -> QualityScore {
    let sharpness = SharpnessScorer::score(image);
    let blur = BlurDetector::score(image);

    let mut score = QualityScore {
        sharpness,
        blur,
        face_count: 0.0,      // Will be set from database
        face_confidence: 0.0,  // Will be set from database
        combined: 0.0,
    };

    score.calculate_combined();
    score
}
```

### File: `src/scoring/sharpness.rs`

```rust
//! Sharpness scoring using Laplacian variance
//!
//! The Laplacian operator highlights edges. A sharp image has
//! strong edges, resulting in high variance of the Laplacian.

use image::{DynamicImage, GrayImage, Luma};

/// Sharpness scoring using Laplacian variance
pub struct SharpnessScorer;

impl SharpnessScorer {
    /// Calculate sharpness score for an image
    /// 
    /// Returns a value between 0 and 1, where higher is sharper.
    pub fn score(image: &DynamicImage) -> f32 {
        let gray = image.to_luma8();
        let variance = Self::laplacian_variance(&gray);
        
        // Normalize to 0-1 range
        // Empirically, variance > 500 is very sharp, < 100 is blurry
        Self::normalize(variance, 50.0, 500.0)
    }

    /// Calculate Laplacian variance of a grayscale image
    fn laplacian_variance(image: &GrayImage) -> f64 {
        let (width, height) = image.dimensions();
        
        if width < 3 || height < 3 {
            return 0.0;
        }

        // Laplacian kernel: [0, 1, 0]
        //                   [1,-4, 1]
        //                   [0, 1, 0]
        let mut sum = 0.0f64;
        let mut sum_sq = 0.0f64;
        let mut count = 0u64;

        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let center = image.get_pixel(x, y).0[0] as f64;
                let top = image.get_pixel(x, y - 1).0[0] as f64;
                let bottom = image.get_pixel(x, y + 1).0[0] as f64;
                let left = image.get_pixel(x - 1, y).0[0] as f64;
                let right = image.get_pixel(x + 1, y).0[0] as f64;

                let laplacian = top + bottom + left + right - 4.0 * center;
                
                sum += laplacian;
                sum_sq += laplacian * laplacian;
                count += 1;
            }
        }

        if count == 0 {
            return 0.0;
        }

        let mean = sum / count as f64;
        let variance = (sum_sq / count as f64) - (mean * mean);

        variance.max(0.0)
    }

    /// Normalize a value to 0-1 range
    fn normalize(value: f64, min: f64, max: f64) -> f32 {
        let clamped = value.clamp(min, max);
        ((clamped - min) / (max - min)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    #[test]
    fn test_uniform_image_is_not_sharp() {
        // Uniform gray image should have low sharpness
        let img: GrayImage = ImageBuffer::from_fn(100, 100, |_, _| Luma([128u8]));
        let variance = SharpnessScorer::laplacian_variance(&img);
        
        assert!(variance < 1.0, "Uniform image should have near-zero variance");
    }

    #[test]
    fn test_edge_image_is_sharp() {
        // Image with sharp vertical edge should have high sharpness
        let img: GrayImage = ImageBuffer::from_fn(100, 100, |x, _| {
            if x < 50 { Luma([0u8]) } else { Luma([255u8]) }
        });
        let variance = SharpnessScorer::laplacian_variance(&img);
        
        assert!(variance > 100.0, "Edge image should have high variance");
    }
}
```

### File: `src/scoring/blur.rs`

```rust
//! Blur detection using frequency domain analysis
//!
//! Blurry images have less high-frequency content.
//! We use a simplified approach based on edge density.

use image::{DynamicImage, GrayImage};

/// Blur detection scorer
pub struct BlurDetector;

impl BlurDetector {
    /// Calculate blur score for an image
    /// 
    /// Returns a value between 0 and 1, where higher means LESS blur (sharper).
    pub fn score(image: &DynamicImage) -> f32 {
        let gray = image.to_luma8();
        let edge_density = Self::calculate_edge_density(&gray);
        
        // Normalize to 0-1 range
        Self::normalize(edge_density, 0.01, 0.15)
    }

    /// Calculate edge density using Sobel operator
    fn calculate_edge_density(image: &GrayImage) -> f64 {
        let (width, height) = image.dimensions();
        
        if width < 3 || height < 3 {
            return 0.0;
        }

        let mut edge_count = 0u64;
        let mut total_pixels = 0u64;
        let edge_threshold = 30.0; // Threshold for considering a pixel as edge

        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                // Sobel X kernel
                let gx = Self::pixel(image, x + 1, y - 1) as f64
                    + 2.0 * Self::pixel(image, x + 1, y) as f64
                    + Self::pixel(image, x + 1, y + 1) as f64
                    - Self::pixel(image, x - 1, y - 1) as f64
                    - 2.0 * Self::pixel(image, x - 1, y) as f64
                    - Self::pixel(image, x - 1, y + 1) as f64;

                // Sobel Y kernel
                let gy = Self::pixel(image, x - 1, y + 1) as f64
                    + 2.0 * Self::pixel(image, x, y + 1) as f64
                    + Self::pixel(image, x + 1, y + 1) as f64
                    - Self::pixel(image, x - 1, y - 1) as f64
                    - 2.0 * Self::pixel(image, x, y - 1) as f64
                    - Self::pixel(image, x + 1, y - 1) as f64;

                let magnitude = (gx * gx + gy * gy).sqrt();

                if magnitude > edge_threshold {
                    edge_count += 1;
                }
                total_pixels += 1;
            }
        }

        if total_pixels == 0 {
            return 0.0;
        }

        edge_count as f64 / total_pixels as f64
    }

    /// Get pixel value as u8
    fn pixel(image: &GrayImage, x: u32, y: u32) -> u8 {
        image.get_pixel(x, y).0[0]
    }

    /// Normalize a value to 0-1 range
    fn normalize(value: f64, min: f64, max: f64) -> f32 {
        let clamped = value.clamp(min, max);
        ((clamped - min) / (max - min)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    #[test]
    fn test_uniform_image_is_blurry() {
        let img: GrayImage = ImageBuffer::from_fn(100, 100, |_, _| Luma([128u8]));
        let edge_density = BlurDetector::calculate_edge_density(&img);
        
        assert!(edge_density < 0.01, "Uniform image should have very low edge density");
    }
}
```

---

## Step 6: Burst Repository

### File: `src/db/burst_repo.rs`

```rust
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
    pub id: i64,
    pub group_id: i64,
    pub photo_id: i64,
    pub sharpness_score: Option<f32>,
    pub blur_score: Option<f32>,
    pub face_count: Option<i32>,
    pub is_suggested_best: bool,
    
    // Joined from photos
    pub file_path: Option<String>,
    pub date_taken: Option<String>,
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

    /// Update quality scores for a burst member
    pub fn update_member_scores(
        &self,
        group_id: i64,
        photo_id: i64,
        sharpness: f32,
        blur: f32,
        face_count: i32,
    ) -> SqliteResult<()> {
        self.conn.execute(
            r#"
            UPDATE burst_group_members
            SET sharpness_score = ?1, blur_score = ?2, face_count = ?3
            WHERE group_id = ?4 AND photo_id = ?5
            "#,
            params![sharpness, blur, face_count, group_id, photo_id],
        )?;
        Ok(())
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

    /// Calculate and set best picks for all groups
    pub fn calculate_all_best_picks(&self) -> SqliteResult<()> {
        // For each group, find the photo with highest combined score
        let mut stmt = self.conn.prepare(
            r#"
            SELECT group_id, photo_id
            FROM burst_group_members
            WHERE (group_id, COALESCE(sharpness_score, 0) * 0.4 + COALESCE(blur_score, 0) * 0.3 + COALESCE(face_count, 0) * 0.1) IN (
                SELECT group_id, MAX(COALESCE(sharpness_score, 0) * 0.4 + COALESCE(blur_score, 0) * 0.3 + COALESCE(face_count, 0) * 0.1)
                FROM burst_group_members
                GROUP BY group_id
            )
            "#,
        )?;

        let best_picks: Vec<(i64, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        for (group_id, photo_id) in best_picks {
            self.set_suggested_best(group_id, photo_id)?;
        }

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
                bgm.id,
                bgm.group_id,
                bgm.photo_id,
                bgm.sharpness_score,
                bgm.blur_score,
                bgm.face_count,
                bgm.is_suggested_best,
                p.file_path,
                p.date_taken
            FROM burst_group_members bgm
            JOIN photos p ON bgm.photo_id = p.id
            WHERE bgm.group_id = ?1
            ORDER BY bgm.is_suggested_best DESC, p.date_taken ASC
            "#,
        )?;

        let rows = stmt.query_map(params![group_id], |row| {
            Ok(BurstGroupMemberRecord {
                id: row.get(0)?,
                group_id: row.get(1)?,
                photo_id: row.get(2)?,
                sharpness_score: row.get(3)?,
                blur_score: row.get(4)?,
                face_count: row.get(5)?,
                is_suggested_best: row.get(6)?,
                file_path: row.get(7)?,
                date_taken: row.get(8)?,
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
        self.conn.execute(
            "DELETE FROM burst_groups WHERE id = ?1",
            params![group_id],
        )?;
        Ok(())
    }
}
```

Update `src/db/mod.rs`:

```rust
pub mod burst_repo;
pub use burst_repo::{BurstRepo, BurstGroupRecord, BurstGroupMemberRecord};
```

---

## Step 7: Duplicates View

### File: `src/views/duplicates.rs`

```rust
//! Duplicates review view

use iced::widget::{button, column, container, row, scrollable, text, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::{DuplicateGroupRecord, DuplicateGroupMemberRecord};
use crate::theme::colors::{Accent, Backgrounds, Border, Text};
use crate::utils::format_bytes;

/// Duplicates view state
pub struct DuplicatesView;

impl DuplicatesView {
    /// Render the duplicates overview
    pub fn view(
        groups: &[DuplicateGroupRecord],
        wasted_space: u64,
    ) -> Element<'static, Message> {
        if groups.is_empty() {
            return Self::empty_view();
        }

        let title = text("Duplicates")
            .size(28)
            .color(Text::PRIMARY);

        let subtitle = text(format!(
            "{} duplicate groups found - {} wasted",
            groups.len(),
            format_bytes(wasted_space)
        ))
        .size(14)
        .color(Text::SECONDARY);

        // Group list
        let group_list: Vec<Element<'static, Message>> = groups
            .iter()
            .map(|g| Self::group_row(g))
            .collect();

        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(24),
            scrollable(
                Column::with_children(group_list)
                    .spacing(12)
            )
            .height(Length::Fill),
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Empty state when no duplicates
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Duplicates")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("No duplicates found!")
                .size(16)
                .color(Text::SECONDARY),
            Space::with_height(8),
            text("Your photo library has no exact duplicate files.")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single duplicate group row
    fn group_row(group: &DuplicateGroupRecord) -> Element<'static, Message> {
        let group_id = group.id;
        
        let header = row![
            text(format!("Group #{}", group.id))
                .size(14)
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            text(format!("{} identical files", group.member_count))
                .size(12)
                .color(Text::SECONDARY),
        ]
        .align_y(Alignment::Center);

        let actions = row![
            button(
                text("Keep Suggested")
                    .size(12)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Accent::PRIMARY.into()),
                    _ => Some(Accent::MUTED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::KeepSuggestedDuplicate(group_id)),
            
            Space::with_width(8),
            
            button(
                text("Review")
                    .size(12)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => Some(Backgrounds::ELEVATED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: Border::SUBTLE,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::OpenDuplicateGroup(group_id)),
            
            Space::with_width(8),
            
            button(
                text("Dismiss")
                    .size(12)
                    .color(Text::TERTIARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::DismissDuplicateGroup(group_id)),
        ];

        let content = column![
            header,
            Space::with_height(8),
            actions,
        ]
        .spacing(4);

        container(content)
            .padding(16)
            .width(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Render detailed view of a duplicate group
    pub fn group_detail_view(
        group: &DuplicateGroupRecord,
        members: &[DuplicateGroupMemberRecord],
    ) -> Element<'static, Message> {
        let group_id = group.id;
        
        let header = row![
            button(
                text("<")
                    .size(16)
                    .color(Text::PRIMARY)
            )
            .padding(8)
            .style(|_theme, _status| button::Style::default())
            .on_press(Message::CloseDuplicateDetail),
            
            Space::with_width(16),
            
            text(format!("Duplicate Group #{}", group.id))
                .size(20)
                .color(Text::PRIMARY),
            
            Space::with_width(Length::Fill),
            
            text(format!("{} files", members.len()))
                .size(14)
                .color(Text::SECONDARY),
        ]
        .align_y(Alignment::Center);

        // Member list
        let member_list: Vec<Element<'static, Message>> = members
            .iter()
            .map(|m| Self::member_row(group_id, m))
            .collect();

        let content = column![
            header,
            Space::with_height(24),
            scrollable(
                Column::with_children(member_list)
                    .spacing(8)
            )
            .height(Length::Fill),
            Space::with_height(16),
            row![
                button(
                    text("Trash Non-Suggested")
                        .size(14)
                        .color(Text::PRIMARY)
                )
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::PRIMARY.into()),
                        _ => Some(Accent::MUTED.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::TrashNonSuggestedDuplicates(group_id)),
                
                Space::with_width(Length::Fill),
            ],
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single member in the detail view
    fn member_row(group_id: i64, member: &DuplicateGroupMemberRecord) -> Element<'static, Message> {
        let photo_id = member.photo_id;
        let is_keep = member.is_suggested_keep;
        
        let path = member.file_path.as_deref().unwrap_or("Unknown path");
        let size = member.file_size.map(format_bytes).unwrap_or_default();
        let date = member.date_taken.as_deref().unwrap_or("Unknown date");

        let keep_indicator = if is_keep {
            text("KEEP")
                .size(10)
                .color(Accent::PRIMARY)
        } else {
            text("")
                .size(10)
        };

        let content = row![
            // Thumbnail placeholder
            container(
                text("IMG")
                    .size(12)
                    .color(Text::TERTIARY)
            )
            .width(60)
            .height(60)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            
            Space::with_width(16),
            
            column![
                text(path)
                    .size(13)
                    .color(Text::PRIMARY),
                Space::with_height(4),
                row![
                    text(size)
                        .size(12)
                        .color(Text::SECONDARY),
                    Space::with_width(16),
                    text(date)
                        .size(12)
                        .color(Text::TERTIARY),
                ],
            ]
            .width(Length::Fill),
            
            keep_indicator,
            
            Space::with_width(16),
            
            button(
                text(if is_keep { "Keeping" } else { "Keep This" })
                    .size(12)
                    .color(if is_keep { Accent::PRIMARY } else { Text::PRIMARY })
            )
            .padding(Padding::from([6, 12]))
            .style(move |_theme, status| {
                let background = if is_keep {
                    Some(Accent::MUTED.into())
                } else {
                    match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    }
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: if is_keep { Accent::PRIMARY } else { Border::SUBTLE },
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetKeepDuplicate(group_id, photo_id)),
        ]
        .align_y(Alignment::Center);

        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(if is_keep {
                    Backgrounds::SELECTED
                } else {
                    Backgrounds::ELEVATED
                }.into()),
                border: iced::Border {
                    color: if is_keep { Accent::PRIMARY } else { Border::SUBTLE },
                    width: if is_keep { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}

// Helper function (add to utils module)
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
```

---

## Step 8: Bursts View

### File: `src/views/bursts.rs`

```rust
//! Bursts review view

use iced::widget::{button, column, container, row, scrollable, text, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::{BurstGroupRecord, BurstGroupMemberRecord};
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Bursts view
pub struct BurstsView;

impl BurstsView {
    /// Render the bursts overview
    pub fn view(
        groups: &[BurstGroupRecord],
        total_saveable: usize,
    ) -> Element<'static, Message> {
        if groups.is_empty() {
            return Self::empty_view();
        }

        let title = text("Burst Photos")
            .size(28)
            .color(Text::PRIMARY);

        let subtitle = text(format!(
            "{} bursts found - {} photos could be removed",
            groups.len(),
            total_saveable
        ))
        .size(14)
        .color(Text::SECONDARY);

        // Group list
        let group_list: Vec<Element<'static, Message>> = groups
            .iter()
            .map(|g| Self::group_card(g))
            .collect();

        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(24),
            scrollable(
                Column::with_children(group_list)
                    .spacing(12)
            )
            .height(Length::Fill),
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Empty state
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Burst Photos")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("No burst photos found!")
                .size(16)
                .color(Text::SECONDARY),
            Space::with_height(8),
            text("Bursts are photos taken within 3 seconds of each other.")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a burst group card
    fn group_card(group: &BurstGroupRecord) -> Element<'static, Message> {
        let group_id = group.id;
        
        // Parse and format time range
        let time_range = format!("{} - {}", 
            &group.start_time[..19.min(group.start_time.len())],
            &group.end_time[11..19.min(group.end_time.len())]
        );

        let header = row![
            text(format!("{} photos", group.photo_count))
                .size(16)
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            text(time_range)
                .size(12)
                .color(Text::TERTIARY),
        ]
        .align_y(Alignment::Center);

        let actions = row![
            button(
                text("Keep Best")
                    .size(12)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Accent::PRIMARY.into()),
                    _ => Some(Accent::MUTED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::KeepBestFromBurst(group_id)),
            
            Space::with_width(8),
            
            button(
                text("Review All")
                    .size(12)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => Some(Backgrounds::ELEVATED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: Border::SUBTLE,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::OpenBurstGroup(group_id)),
            
            Space::with_width(8),
            
            button(
                text("Keep All")
                    .size(12)
                    .color(Text::TERTIARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::DismissBurstGroup(group_id)),
        ];

        let content = column![
            header,
            Space::with_height(12),
            // Thumbnail strip placeholder
            container(
                row![
                    text("Thumbnails will appear here")
                        .size(11)
                        .color(Text::TERTIARY)
                ]
            )
            .width(Length::Fill)
            .height(60)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            Space::with_height(12),
            actions,
        ];

        container(content)
            .padding(16)
            .width(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Render detailed burst review
    pub fn group_detail_view(
        group: &BurstGroupRecord,
        members: &[BurstGroupMemberRecord],
    ) -> Element<'static, Message> {
        let group_id = group.id;
        
        let header = row![
            button(
                text("<")
                    .size(16)
                    .color(Text::PRIMARY)
            )
            .padding(8)
            .style(|_theme, _status| button::Style::default())
            .on_press(Message::CloseBurstDetail),
            
            Space::with_width(16),
            
            text(format!("Burst - {} photos", members.len()))
                .size(20)
                .color(Text::PRIMARY),
            
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        // Member grid with quality scores
        let member_cards: Vec<Element<'static, Message>> = members
            .iter()
            .map(|m| Self::member_card(group_id, m))
            .collect();

        // Arrange in rows of 4
        let mut rows: Vec<Element<'static, Message>> = Vec::new();
        for chunk in member_cards.chunks(4) {
            let row_elements: Vec<Element<'static, Message>> = chunk.to_vec();
            rows.push(
                Row::with_children(row_elements)
                    .spacing(12)
                    .into()
            );
        }

        let content = column![
            header,
            Space::with_height(24),
            scrollable(
                Column::with_children(rows)
                    .spacing(12)
            )
            .height(Length::Fill),
            Space::with_height(16),
            row![
                button(
                    text("Keep Only Selected")
                        .size(14)
                        .color(Text::PRIMARY)
                )
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::PRIMARY.into()),
                        _ => Some(Accent::MUTED.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::TrashNonBestFromBurst(group_id)),
                
                Space::with_width(Length::Fill),
            ],
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single burst member card
    fn member_card(group_id: i64, member: &BurstGroupMemberRecord) -> Element<'static, Message> {
        let photo_id = member.photo_id;
        let is_best = member.is_suggested_best;
        
        let sharpness = member.sharpness_score.unwrap_or(0.0);
        let blur = member.blur_score.unwrap_or(0.0);
        
        // Quality bar
        let quality = (sharpness * 0.5 + blur * 0.5) * 100.0;
        
        let quality_indicator = container(
            Space::new(Length::Fixed(quality as f32), Length::Fixed(3.0))
        )
        .width(Length::Fixed(100.0))
        .style(move |_theme| container::Style {
            background: Some(Backgrounds::PRIMARY.into()),
            ..Default::default()
        });

        let best_badge = if is_best {
            container(
                text("BEST")
                    .size(9)
                    .color(Backgrounds::PRIMARY)
            )
            .padding(Padding::from([2, 6]))
            .style(|_theme| container::Style {
                background: Some(Accent::PRIMARY.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
        } else {
            container(Space::new(Length::Shrink, Length::Shrink))
        };

        let content = column![
            // Image placeholder
            container(
                column![
                    best_badge,
                    Space::with_height(Length::Fill),
                ]
                .width(Length::Fill)
                .padding(4)
            )
            .width(Length::Fixed(140.0))
            .height(Length::Fixed(100.0))
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            
            Space::with_height(8),
            
            // Quality bar
            column![
                text(format!("Quality: {:.0}%", quality))
                    .size(10)
                    .color(Text::TERTIARY),
                Space::with_height(2),
                container(quality_indicator)
                    .style(|_theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
                        ..Default::default()
                    }),
            ],
            
            Space::with_height(8),
            
            // Select button
            button(
                text(if is_best { "Selected" } else { "Select" })
                    .size(11)
                    .color(if is_best { Accent::PRIMARY } else { Text::PRIMARY })
            )
            .padding(Padding::from([4, 8]))
            .width(Length::Fill)
            .style(move |_theme, status| {
                let background = if is_best {
                    Some(Accent::MUTED.into())
                } else {
                    match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    }
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: if is_best { Accent::PRIMARY } else { Border::SUBTLE },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetBestFromBurst(group_id, photo_id)),
        ]
        .width(Length::Fixed(140.0));

        container(content)
            .padding(8)
            .style(move |_theme| container::Style {
                background: Some(if is_best {
                    Backgrounds::SELECTED
                } else {
                    Backgrounds::ELEVATED
                }.into()),
                border: iced::Border {
                    color: if is_best { Accent::PRIMARY } else { Border::SUBTLE },
                    width: if is_best { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
```

---

## Step 9: Add Messages to App

Add these messages to `src/app.rs`:

```rust
/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing messages ...

    // Duplicate detection
    RunDuplicateDetection,
    DuplicateDetectionComplete(Vec<DuplicateGroup>),
    OpenDuplicateGroup(i64),
    CloseDuplicateDetail,
    SetKeepDuplicate(i64, i64),  // (group_id, photo_id)
    KeepSuggestedDuplicate(i64),
    TrashNonSuggestedDuplicates(i64),
    DismissDuplicateGroup(i64),

    // Burst detection
    RunBurstDetection,
    BurstDetectionComplete(Vec<BurstGroup>),
    OpenBurstGroup(i64),
    CloseBurstDetail,
    SetBestFromBurst(i64, i64),  // (group_id, photo_id)
    KeepBestFromBurst(i64),
    TrashNonBestFromBurst(i64),
    DismissBurstGroup(i64),
}
```

---

## UI Design: Duplicates View

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  Duplicates                                        │
│             │  234 duplicate groups found - 4.2 GB wasted        │
│  Timeline   │─────────────────────────────────────────────────── │
│  People     │                                                    │
│  Duplicates●│  ┌─────────────────────────────────────────────┐  │
│  Bursts     │  │ Group #1 - 3 identical files                │  │
│             │  │                                              │  │
│  ─────────  │  │  [Keep Suggested]  [Review]  [Dismiss]       │  │
│             │  └─────────────────────────────────────────────┘  │
│  Settings   │                                                    │
│             │  ┌─────────────────────────────────────────────┐  │
│             │  │ Group #2 - 2 identical files                │  │
│             │  │                                              │  │
│             │  │  [Keep Suggested]  [Review]  [Dismiss]       │  │
│             │  └─────────────────────────────────────────────┘  │
│             │                                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## UI Design: Bursts View

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  Burst Photos                                      │
│             │  45 bursts found - 156 photos could be removed     │
│  Timeline   │─────────────────────────────────────────────────── │
│  People     │                                                    │
│  Duplicates │  ┌─────────────────────────────────────────────┐  │
│  Bursts   ● │  │ 5 photos                    Mar 15 14:30-32 │  │
│             │  │                                              │  │
│  ─────────  │  │  ┌───┐┌───┐┌───┐┌───┐┌───┐                  │  │
│             │  │  │ ★ ││   ││   ││   ││   │  (★ = best)      │  │
│  Settings   │  │  └───┘└───┘└───┘└───┘└───┘                  │  │
│             │  │                                              │  │
│             │  │  [Keep Best]  [Review All]  [Keep All]       │  │
│             │  └─────────────────────────────────────────────┘  │
│             │                                                    │
│             │  ┌─────────────────────────────────────────────┐  │
│             │  │ 8 photos                    Mar 15 15:22-25 │  │
│             │  │  ...                                        │  │
│             │  └─────────────────────────────────────────────┘  │
│             │                                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Verification Checklist

- [ ] Duplicate detection finds all files with matching SHA256
- [ ] Suggested keep logic prefers good paths over backup folders
- [ ] Wasted space calculation is accurate
- [ ] Burst detection groups photos within 3-second window
- [ ] Minimum 3 photos required to form a burst
- [ ] Sharpness scoring uses Laplacian variance correctly
- [ ] Blur detection identifies blurry images
- [ ] Best-pick suggestion combines scores appropriately
- [ ] Duplicates view shows all groups with actions
- [ ] Group detail allows changing which file to keep
- [ ] Bursts view shows quality scores for each photo
- [ ] "Keep Best" action trashes all non-best photos
- [ ] Trash integration works (soft delete, not permanent)

---

## Performance Notes

For 100k photos:
- Duplicate detection: ~2 seconds (single SQL query)
- Burst detection: ~5 seconds (sorted scan)
- Quality scoring: ~500ms per photo (on-demand, not batch)

Scoring is done on-demand when viewing burst detail, not during initial detection.

---

## Next Phase Preview

**Phase 6: Search & Quick Cull** will add:
- Search by date, location, person
- Natural language date parsing
- Quick cull workflow with keyboard controls
- Trash staging with soft delete

---

## Expected Results & Behavior

> **IMPORTANT:** Do not proceed to Phase 6 until a human has verified ALL of the following behaviors. Each item must be manually tested and confirmed.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **Duplicates View** | List of duplicate groups with file count and wasted space |
| **Duplicate Group Cards** | Each group shows thumbnail, file count, and total size |
| **Suggested Keep** | One file per group highlighted as "suggested keep" |
| **Wasted Space Summary** | Header shows total duplicate groups and space that can be freed |
| **Bursts View** | List of burst groups with photo count and time range |
| **Burst Thumbnails** | Strip of thumbnails showing all burst photos side-by-side |
| **Best Pick Badge** | Star/badge on the best quality photo in each burst |
| **Quality Scores** | Sharpness/quality percentage visible on burst photos |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **Click "Keep Suggested" on duplicate** | Non-suggested duplicates moved to trash |
| **Click "Review" on duplicate group** | Opens detail view showing all duplicate files side by side |
| **Change which duplicate to keep** | Click different file, "suggested" highlight moves to it |
| **Click "Keep Best" on burst** | Non-best burst photos moved to trash |
| **Click "Review All" on burst** | Opens detail view with all burst photos and quality scores |
| **Change best pick in burst** | Click different photo, "BEST" badge moves to it |
| **Click "Keep All" on burst** | Dismisses burst group, no photos trashed |
| **Click "Dismiss" on duplicate** | Removes group from view, no action taken |
| **Run duplicate detection** | Scanning indicator shown, groups appear when complete |
| **Run burst detection** | Scanning indicator shown, groups appear when complete |

### Technical Verification

```bash
# Check duplicate groups found
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM duplicate_groups;"

# Verify duplicates share same hash
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT dg.id, COUNT(dgm.photo_id), p.sha256_hash FROM duplicate_groups dg JOIN duplicate_group_members dgm ON dg.id = dgm.group_id JOIN photos p ON dgm.photo_id = p.id GROUP BY dg.id LIMIT 5;"

# Check burst groups
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM burst_groups;"

# Verify burst members are within 3 seconds
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT bg.id, COUNT(bgm.photo_id), bg.start_time, bg.end_time FROM burst_groups bg JOIN burst_group_members bgm ON bg.id = bgm.group_id GROUP BY bg.id LIMIT 5;"

# Check quality scores populated
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT photo_id, sharpness_score, blur_score, is_suggested_best FROM burst_group_members WHERE sharpness_score IS NOT NULL LIMIT 10;"

# Verify wasted space calculation
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT SUM(p.file_size) FROM duplicate_group_members dgm JOIN photos p ON dgm.photo_id = p.id WHERE dgm.is_suggested_keep = 0;"
```

**Expected:** Duplicate groups contain files with matching SHA256 hashes. Burst groups contain photos within 3-second windows with at least 3 members. Quality scores populated for burst members.

### Performance Verification

| Metric | Expected |
|--------|----------|
| **Duplicate detection** | < 2 seconds for 100k photos (SQL query) |
| **Burst detection** | < 5 seconds for 100k photos (sorted scan) |
| **Quality scoring** | < 500ms per photo (on-demand) |
| **Duplicates view load** | < 1 second to display groups |
| **Bursts view load** | < 1 second to display groups |

### Sign-off Checklist

Before proceeding to Phase 6, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **Duplicate detection works:** Groups created for files with identical SHA256 hashes
- [ ] **Suggested keep logic:** Prefers files in non-backup paths, larger files
- [ ] **Wasted space accurate:** Correctly sums size of non-suggested duplicates
- [ ] **Burst detection works:** Photos within 3-second window grouped together
- [ ] **Minimum burst size:** Groups require at least 3 photos
- [ ] **Sharpness scoring:** Laplacian variance calculated correctly
- [ ] **Blur detection:** Blurry images get lower quality scores
- [ ] **Best pick selection:** Highest quality photo marked as suggested best
- [ ] **Trash integration:** "Keep" actions move non-kept photos to trash (soft delete)
- [ ] **Duplicates UI complete:** Groups displayed with actions (keep, review, dismiss)
- [ ] **Bursts UI complete:** Groups displayed with quality scores and best pick badges
- [ ] **No console errors:** Clean detection and scoring operation
- [ ] **SKILL.md followed:** Duplicates and Bursts views match design guidelines

**Signature:** ___________________ **Date:** _______________

---

## Proceed to Phase 6

Only after ALL items above are verified, proceed to:

📁 `plan/PHASE_6_SEARCH_CULL.md`
