//! Timeline date-grouping (pure logic, no UI deps).
//!
//! Lives under `models` rather than `views` so it can be exercised
//! from integration tests that don't depend on iced / the view layer.

use chrono::{DateTime, Datelike, Utc};

use super::Photo;

/// Zero-copy view of a contiguous slice of `photos: &[Photo]` taken
/// on the same date. Built once per photos-load (not every render),
/// stored in app state, and consumed by the timeline view to avoid
/// re-grouping the entire library on every scroll event.
#[derive(Debug, Clone)]
pub struct DateGroupRange {
    pub day_key: String,
    pub display_date: String,
    /// Half-open `[start, end)` range into the parent `photos` slice.
    pub start: usize,
    pub end: usize,
}

/// Pre-compute timeline date groups from a date-descending photo
/// slice. Called by the `photos_loaded` handler once per load,
/// NOT per render frame.
///
/// Photos must already be sorted by `date_taken DESC` (the loader
/// guarantees this via SQL `ORDER BY date_taken DESC`). Photos
/// without a `date_taken` cluster under a single "Unknown Date"
/// group at the end.
pub fn compute_groups(photos: &[Photo]) -> Vec<DateGroupRange> {
    let mut out: Vec<DateGroupRange> = Vec::new();
    let mut i = 0;
    while i < photos.len() {
        let head_date = photos[i].date_taken.map(|d| d.date_naive());

        // Extend the group while subsequent photos share the same
        // calendar date. Compare `NaiveDate` values directly — much
        // cheaper than formatting both sides to strings, which would
        // run `format!("%Y-%m-%d")` once per photo (~50K times for a
        // 50K-photo library).
        let mut j = i + 1;
        while j < photos.len() {
            let next_date = photos[j].date_taken.map(|d| d.date_naive());
            if next_date != head_date {
                break;
            }
            j += 1;
        }

        // Format only once per group.
        let (day_key, display_date) = match photos[i].date_taken {
            Some(d) => (d.format("%Y-%m-%d").to_string(), format_display_date(&d)),
            None => ("0000-00-00".to_string(), "Unknown Date".to_string()),
        };

        out.push(DateGroupRange {
            day_key,
            display_date,
            start: i,
            end: j,
        });
        i = j;
    }
    out
}

/// Human-friendly display date relative to today.
pub fn format_display_date(date: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let today = now.date_naive();
    let photo_date = date.date_naive();

    if photo_date == today {
        "Today".to_string()
    } else if photo_date == today.pred_opt().unwrap_or(today) {
        "Yesterday".to_string()
    } else if photo_date.year() == today.year() {
        date.format("%B %d").to_string() // "March 15"
    } else {
        date.format("%B %d, %Y").to_string() // "March 15, 2019"
    }
}
