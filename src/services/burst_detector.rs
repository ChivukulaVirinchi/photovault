//! Burst photo detection - groups near-duplicate photos taken in short sessions

use chrono::{DateTime, Duration, Utc};
use image::{imageops::FilterType, DynamicImage};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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

#[derive(Debug, Clone)]
pub struct BurstProgress {
    pub processed: u64,
    pub total: u64,
    pub groups_found: u64,
    pub message: String,
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
        // Loosened from "3+ photos in 3s, same folder, 80% similar" to
        // "2+ photos in 10s anywhere, 65% similar". The strict defaults
        // matched almost nothing on real-world libraries (one phone
        // over years, photos organized by month not by burst, varied
        // subjects). Users who want tighter detection can tune the
        // window in Settings; require_same_folder is no longer the
        // default because modern phones group by month, not burst.
        Self {
            max_gap_seconds: 10,
            min_photos: 2,
            max_burst_span_seconds: 60,
            similarity_threshold: 0.65,
            require_same_folder: false,
        }
    }
}

#[derive(Debug, Clone)]
struct BurstPhotoCandidate {
    id: i64,
    date: DateTime<Utc>,
    file_path: String,
    file_hash: String,
    thumbnail_path: Option<String>,
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

    /// Find all burst groups in the database.
    ///
    /// `thumb_root` should point at the Small thumbnail directory
    /// (`<drive>/.photovault/thumbnails/small/`). When supplied, the
    /// signature pass loads the cached 260px thumbnail by file_hash
    /// instead of re-decoding the original — turning ~1 minute per
    /// 1000 photos into seconds. Falls back to the original photo
    /// when the thumb file isn't on disk yet.
    pub fn find_bursts(
        &self,
        conn: &Connection,
        drive_root: Option<&Path>,
        thumb_root: Option<&Path>,
    ) -> rusqlite::Result<Vec<BurstGroup>> {
        self.find_bursts_with_progress(conn, drive_root, thumb_root, None, |_| {})
    }

    pub fn find_bursts_with_progress(
        &self,
        conn: &Connection,
        drive_root: Option<&Path>,
        thumb_root: Option<&Path>,
        cancel: Option<&AtomicBool>,
        mut progress: impl FnMut(BurstProgress),
    ) -> rusqlite::Result<Vec<BurstGroup>> {
        self.find_bursts_streaming(conn, drive_root, thumb_root, cancel, &mut progress, |_| {})
    }

    pub fn find_bursts_streaming(
        &self,
        conn: &Connection,
        drive_root: Option<&Path>,
        thumb_root: Option<&Path>,
        cancel: Option<&AtomicBool>,
        mut progress: impl FnMut(BurstProgress),
        mut on_group: impl FnMut(&BurstGroup),
    ) -> rusqlite::Result<Vec<BurstGroup>> {
        // Get all photos ordered by date_taken. Signatures are built lazily
        // after cheap timestamp/folder checks, so old libraries do not decode
        // every image just to reject photos taken minutes or days apart.
        let mut stmt = conn.prepare(
            r#"
            SELECT id, date_taken, file_path, file_hash, thumbnail_path
            FROM photos
            WHERE date_taken IS NOT NULL AND is_trashed = FALSE
            ORDER BY date_taken ASC
            "#,
        )?;

        let photos: Vec<(i64, String, String, String, Option<String>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        if photos.is_empty() {
            return Ok(Vec::new());
        }

        let mut photos: Vec<BurstPhotoCandidate> = photos
            .into_iter()
            .filter_map(|(id, date_str, file_path, file_hash, thumbnail_path)| {
                Some(BurstPhotoCandidate {
                    id,
                    date: Self::parse_datetime(&date_str)?,
                    file_path,
                    file_hash,
                    thumbnail_path,
                    signature: None,
                })
            })
            .collect();
        photos.sort_by_key(|p| (p.date, p.id));

        if photos.is_empty() {
            return Ok(Vec::new());
        }
        let total = photos.len() as u64;
        progress(BurstProgress {
            processed: 0,
            total,
            groups_found: 0,
            message: format!("checking {} dated photos", total),
        });

        let mut groups = Vec::new();
        let mut current_group: Vec<BurstPhotoCandidate> = Vec::new();
        let tick = total.div_ceil(40).max(250);

        for (idx, candidate) in photos.into_iter().enumerate() {
            if cancel
                .map(|flag| flag.load(Ordering::Relaxed))
                .unwrap_or(false)
            {
                break;
            }

            if current_group.is_empty() {
                current_group.push(candidate);
            } else {
                let mut candidate = candidate;
                let should_join = self.should_join_group(
                    &mut current_group,
                    &mut candidate,
                    drive_root,
                    thumb_root,
                );

                if should_join {
                    current_group.push(candidate);
                } else {
                    // Finalize current group if candidate doesn't belong
                    if current_group.len() >= self.config.min_photos {
                        if let Some(group) = self.finalize_candidate_group(&current_group) {
                            on_group(&group);
                            groups.push(group);
                        }
                    }
                    current_group = vec![candidate];
                }
            }

            let processed = (idx + 1) as u64;
            if processed.is_multiple_of(tick) || processed == total {
                progress(BurstProgress {
                    processed,
                    total,
                    groups_found: groups.len() as u64,
                    message: format!("{} burst groups so far", groups.len()),
                });
            }
        }

        // Don't forget the last group
        if !cancel
            .map(|flag| flag.load(Ordering::Relaxed))
            .unwrap_or(false)
            && current_group.len() >= self.config.min_photos
        {
            if let Some(group) = self.finalize_candidate_group(&current_group) {
                on_group(&group);
                groups.push(group);
            }
        }

        Ok(groups)
    }

    fn finalize_candidate_group(&self, photos: &[BurstPhotoCandidate]) -> Option<BurstGroup> {
        let first = photos.first()?;
        let last = photos.last()?;
        let photo_ids: Vec<i64> = photos.iter().map(|p| p.id).collect();

        Some(BurstGroup {
            photo_ids,
            start_time: first.date,
            end_time: last.date,
        })
    }

    fn should_join_group(
        &self,
        group: &mut [BurstPhotoCandidate],
        candidate: &mut BurstPhotoCandidate,
        drive_root: Option<&Path>,
        thumb_root: Option<&Path>,
    ) -> bool {
        let last = match group.last() {
            Some(p) => p,
            None => return true,
        };

        let gap = candidate.date.signed_duration_since(last.date);
        if gap > Duration::seconds(self.config.max_gap_seconds) {
            return false;
        }

        let start = match group.first() {
            Some(p) => p.date,
            None => return true,
        };
        let span = candidate.date.signed_duration_since(start);
        if span > Duration::seconds(self.config.max_burst_span_seconds) {
            return false;
        }

        if self.config.require_same_folder
            && Self::folder_key(&last.file_path) != Self::folder_key(&candidate.file_path)
        {
            return false;
        }

        let Some(last) = group.last_mut() else {
            return true;
        };
        Self::ensure_signature(last, drive_root, thumb_root);
        Self::ensure_signature(candidate, drive_root, thumb_root);

        if let (Some(a), Some(b)) = (&last.signature, &candidate.signature) {
            let sim = Self::cosine_similarity(a, b);
            if sim < self.config.similarity_threshold {
                return false;
            }
        }

        true
    }

    fn ensure_signature(
        photo: &mut BurstPhotoCandidate,
        drive_root: Option<&Path>,
        thumb_root: Option<&Path>,
    ) {
        if photo.signature.is_some() {
            return;
        }
        photo.signature = Self::signature_source_path(drive_root, thumb_root, photo)
            .and_then(|p| Self::build_signature(&p));
    }

    fn signature_source_path(
        drive_root: Option<&Path>,
        thumb_root: Option<&Path>,
        photo: &BurstPhotoCandidate,
    ) -> Option<PathBuf> {
        let mut candidates = Vec::with_capacity(5);
        if let (Some(root), Some(path)) = (drive_root, &photo.thumbnail_path) {
            if let Ok(path) = crate::services::path_util::safe_join_relative(root, path) {
                candidates.push(path);
            }
        }

        if let Some(root) = thumb_root {
            let subdir = &photo.file_hash[..2.min(photo.file_hash.len())];
            candidates.push(root.join(subdir).join(format!("{}.jpg", photo.file_hash)));
        }

        if let Some(root) = drive_root {
            let subdir = &photo.file_hash[..2.min(photo.file_hash.len())];
            for size in ["medium", "small", "large"] {
                candidates.push(
                    root.join(".photovault")
                        .join("thumbnails")
                        .join(size)
                        .join("v2")
                        .join(subdir)
                        .join(format!("{}.jpg", photo.file_hash)),
                );
            }
            if let Ok(path) = crate::services::path_util::safe_join_relative(root, &photo.file_path)
            {
                candidates.push(path);
            }
        }

        candidates.into_iter().find(|p| p.exists())
    }

    fn folder_key(file_path: &str) -> String {
        Path::new(file_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn build_signature(path: &Path) -> Option<Vec<f32>> {
        // Route HEIC/HEIF through libheif when feature on; otherwise
        // identical to image::open.
        let img = crate::services::image_io::open_image(path).ok()?;
        Some(Self::signature_from_image(&img))
    }

    fn signature_from_image(img: &DynamicImage) -> Vec<f32> {
        let gray = img
            .grayscale()
            .resize_exact(48, 48, FilterType::Triangle)
            .to_luma8();
        let mut sig = Vec::with_capacity(48 * 48);
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
    use rusqlite::params;

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

    #[test]
    fn finalize_candidate_group_ignores_empty_input() {
        let detector = BurstDetector::new(BurstConfig::default());
        assert!(detector.finalize_candidate_group(&[]).is_none());
    }

    #[test]
    fn bursts_sort_by_parsed_utc_time_not_raw_timestamp_text() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                date_taken TEXT,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                thumbnail_path TEXT,
                is_trashed BOOLEAN NOT NULL DEFAULT 0
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (id, date_taken, file_path, file_hash, is_trashed)
             VALUES (1, '2024-01-01T00:00:00-09:00', 'a.jpg', 'aa111', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (id, date_taken, file_path, file_hash, is_trashed)
             VALUES (2, '2024-01-01T00:00:20+09:00', 'b.jpg', 'bb222', 0)",
            [],
        )
        .unwrap();

        let detector = BurstDetector::new(BurstConfig::default());
        let groups = detector.find_bursts(&conn, None, None).unwrap();

        assert!(groups.is_empty());
    }

    #[test]
    fn bursts_use_db_thumbnail_path_when_small_thumb_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                date_taken TEXT,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                thumbnail_path TEXT,
                is_trashed BOOLEAN NOT NULL DEFAULT 0
            );
            "#,
        )
        .unwrap();

        let thumb_dir = temp.path().join(".photovault/thumbnails/medium/v2/aa");
        std::fs::create_dir_all(&thumb_dir).unwrap();
        for (id, hash, value) in [(1, "aa111", 190u8), (2, "aa222", 192u8)] {
            let thumb = thumb_dir.join(format!("{hash}.jpg"));
            DynamicImage::ImageLuma8(ImageBuffer::from_fn(32, 32, |_x, _y| Luma([value])))
                .save(&thumb)
                .unwrap();
            conn.execute(
                "INSERT INTO photos (id, date_taken, file_path, file_hash, thumbnail_path, is_trashed)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                params![
                    id,
                    format!("2025-01-01 10:00:0{id}"),
                    format!("missing/original-{id}.jpg"),
                    hash,
                    format!(".photovault/thumbnails/medium/v2/aa/{hash}.jpg")
                ],
            )
            .unwrap();
        }

        let detector = BurstDetector::new(BurstConfig::default());
        let groups = detector
            .find_bursts(
                &conn,
                Some(temp.path()),
                Some(&temp.path().join("missing-small")),
            )
            .unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].photo_ids, vec![1, 2]);
    }
}
