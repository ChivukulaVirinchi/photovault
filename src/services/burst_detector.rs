//! Burst photo detection - groups near-duplicate photos taken in short sessions

use chrono::{DateTime, Duration, Utc};
use image::{imageops::FilterType, DynamicImage};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// A burst group of photos
#[derive(Debug, Clone)]
pub struct BurstGroup {
    /// Photo IDs in this burst (ordered by time)
    pub photo_ids: Vec<i64>,

    /// Start timestamp
    pub start_time: DateTime<Utc>,

    /// End timestamp
    pub end_time: DateTime<Utc>,
}

/// Burst detection configuration
#[derive(Debug, Clone)]
pub struct BurstConfig {
    /// Maximum time gap between photos in a burst (seconds)
    pub max_gap_seconds: i64,

    /// Minimum photos to form a burst
    pub min_photos: usize,

    /// Maximum total span for a burst sequence (seconds)
    pub max_burst_span_seconds: i64,

    /// Require visual similarity between consecutive shots
    pub similarity_threshold: f32,

    /// Require photos from the same folder for a burst
    pub require_same_folder: bool,
}

impl Default for BurstConfig {
    fn default() -> Self {
        Self {
            max_gap_seconds: 3,
            min_photos: 3,
            max_burst_span_seconds: 30,
            similarity_threshold: 0.90,
            require_same_folder: true,
        }
    }
}

#[derive(Debug, Clone)]
struct BurstPhotoCandidate {
    id: i64,
    date: DateTime<Utc>,
    file_path: String,
    signature: Option<Vec<f32>>,
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
    pub fn find_bursts(
        &self,
        conn: &Connection,
        drive_root: Option<&Path>,
    ) -> rusqlite::Result<Vec<BurstGroup>> {
        // Get all photos ordered by date_taken
        let mut stmt = conn.prepare(
            r#"
            SELECT id, date_taken, file_path
            FROM photos
            WHERE date_taken IS NOT NULL AND is_trashed = FALSE
            ORDER BY date_taken ASC
            "#,
        )?;

        let photos: Vec<(i64, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        if photos.is_empty() {
            return Ok(Vec::new());
        }

        let mut groups = Vec::new();
        let mut current_group: Vec<BurstPhotoCandidate> = Vec::new();

        for (id, date_str, file_path) in photos {
            let date = match Self::parse_datetime(&date_str) {
                Some(d) => d,
                None => continue,
            };

            let signature = drive_root.and_then(|root| {
                let abs = root.join(&file_path);
                Self::build_signature(&abs)
            });

            let candidate = BurstPhotoCandidate {
                id,
                date,
                file_path,
                signature,
            };

            if current_group.is_empty() {
                current_group.push(candidate);
            } else {
                let should_join = self.should_join_group(&current_group, &candidate);

                if should_join {
                    current_group.push(candidate);
                } else {
                    // Finalize current group if candidate doesn't belong
                    if current_group.len() >= self.config.min_photos {
                        groups.push(self.finalize_candidate_group(&current_group));
                    }
                    current_group = vec![candidate];
                }
            }
        }

        // Don't forget the last group
        if current_group.len() >= self.config.min_photos {
            groups.push(self.finalize_candidate_group(&current_group));
        }

        Ok(groups)
    }

    fn finalize_candidate_group(&self, photos: &[BurstPhotoCandidate]) -> BurstGroup {
        let photo_ids: Vec<i64> = photos.iter().map(|p| p.id).collect();
        let start_time = photos
            .first()
            .expect("finalize_candidate_group called with empty slice")
            .date;
        let end_time = photos
            .last()
            .expect("finalize_candidate_group called with empty slice")
            .date;

        BurstGroup {
            photo_ids,
            start_time,
            end_time,
        }
    }

    fn should_join_group(
        &self,
        group: &[BurstPhotoCandidate],
        candidate: &BurstPhotoCandidate,
    ) -> bool {
        let last = match group.last() {
            Some(p) => p,
            None => return true,
        };

        let gap = candidate.date.signed_duration_since(last.date);
        if gap > Duration::seconds(self.config.max_gap_seconds) {
            return false;
        }

        let start = group.first().expect("group has at least one item").date;
        let span = candidate.date.signed_duration_since(start);
        if span > Duration::seconds(self.config.max_burst_span_seconds) {
            return false;
        }

        if self.config.require_same_folder
            && Self::folder_key(&last.file_path) != Self::folder_key(&candidate.file_path)
        {
            return false;
        }

        if let (Some(a), Some(b)) = (&last.signature, &candidate.signature) {
            let sim = Self::cosine_similarity(a, b);
            if sim < self.config.similarity_threshold {
                return false;
            }
        }

        true
    }

    fn folder_key(file_path: &str) -> String {
        Path::new(file_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn build_signature(path: &PathBuf) -> Option<Vec<f32>> {
        let img = image::open(path).ok()?;
        Some(Self::signature_from_image(&img))
    }

    fn signature_from_image(img: &DynamicImage) -> Vec<f32> {
        let gray = img
            .grayscale()
            .resize_exact(32, 32, FilterType::Triangle)
            .to_luma8();
        let mut sig = Vec::with_capacity(32 * 32);
        for px in gray.pixels() {
            sig.push(f32::from(px.0[0]) / 255.0);
        }
        sig
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let mut dot = 0.0f32;
        let mut a_norm = 0.0f32;
        let mut b_norm = 0.0f32;
        for (x, y) in a.iter().zip(b.iter()) {
            dot += x * y;
            a_norm += x * x;
            b_norm += y * y;
        }

        if a_norm <= f32::EPSILON || b_norm <= f32::EPSILON {
            return 0.0;
        }

        dot / (a_norm.sqrt() * b_norm.sqrt())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use image::{DynamicImage, ImageBuffer, Luma};

    #[test]
    fn test_parse_datetime() {
        let dt = BurstDetector::parse_datetime("2019-03-15 14:30:22");
        assert!(dt.is_some());

        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2019);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn test_similarity_identical_images_is_high() {
        let img = DynamicImage::ImageLuma8(ImageBuffer::from_fn(32, 32, |_x, _y| Luma([200u8])));
        let a = BurstDetector::signature_from_image(&img);
        let b = BurstDetector::signature_from_image(&img);
        let sim = BurstDetector::cosine_similarity(&a, &b);
        assert!(sim > 0.999);
    }
}
