//! Photo data model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    #[default]
    Photo,
    Video,
}

impl MediaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Video => "video",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "video" => Self::Video,
            _ => Self::Photo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentCategory {
    #[default]
    Photo,
    BusinessCard,
    Document,
    Screenshot,
    Presentation,
    Whiteboard,
    Receipt,
}

impl ContentCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::BusinessCard => "business_card",
            Self::Document => "document",
            Self::Screenshot => "screenshot",
            Self::Presentation => "presentation",
            Self::Whiteboard => "whiteboard",
            Self::Receipt => "receipt",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "document" => Self::Document,
            "business_card" => Self::BusinessCard,
            "screenshot" => Self::Screenshot,
            "presentation" => Self::Presentation,
            "whiteboard" => Self::Whiteboard,
            "receipt" => Self::Receipt,
            _ => Self::Photo,
        }
    }
}

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
    pub iso: Option<i32>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,
    pub lens_model: Option<String>,
    pub flash: Option<String>,
    pub gps_altitude: Option<f64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: i32,

    // Media metadata
    pub media_type: MediaType,
    pub duration_ms: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub frame_rate: Option<f32>,
    pub bitrate: Option<i64>,
    pub has_audio: bool,

    // Processing state
    pub thumbnail_path: Option<String>,
    pub faces_processed: bool,
    pub content_category: ContentCategory,
    pub ocr_text: Option<String>,
    pub ocr_processed: bool,
    pub ocr_confidence: Option<f32>,

    // Soft delete
    pub is_trashed: bool,
    pub trashed_at: Option<DateTime<Utc>>,

    // Timestamps
    pub indexed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Photo {
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
