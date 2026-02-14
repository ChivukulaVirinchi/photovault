//! Photo data model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a photo in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: i64,

    // File info
    pub file_path: String,
    pub file_name: String,
    pub file_hash: String,
    pub file_size: i64,

    // EXIF data
    pub date_taken: Option<DateTime<Utc>>,
    pub date_taken_source: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,

    // Processing state
    pub thumbnail_path: Option<String>,
    pub faces_processed: bool,

    // Soft delete
    pub is_trashed: bool,
    pub trashed_at: Option<DateTime<Utc>>,

    // Timestamps
    pub indexed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Photo {
    /// Get the display date for this photo
    pub fn display_date(&self) -> Option<DateTime<Utc>> {
        self.date_taken
    }

    /// Check if this photo has GPS coordinates
    pub fn has_location(&self) -> bool {
        self.gps_latitude.is_some() && self.gps_longitude.is_some()
    }

    /// Get a human-readable location string
    pub fn location_string(&self) -> Option<String> {
        match (&self.location_city, &self.location_country) {
            (Some(city), Some(country)) => Some(format!("{}, {}", city, country)),
            (Some(city), None) => Some(city.clone()),
            (None, Some(country)) => Some(country.clone()),
            (None, None) => None,
        }
    }
}
