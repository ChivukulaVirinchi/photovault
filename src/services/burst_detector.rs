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
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap_or_else(|_| panic!("Failed to query photos"))
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

}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

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
