//! EXIF metadata extraction service
//!
//! Extracts date, GPS, camera info from image files.
//! Falls back to filename parsing and file mtime when EXIF unavailable.

use std::path::Path;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use exif::{In, Reader as ExifReader, Tag, Value};
use lazy_static::lazy_static;
use regex::Regex;

/// Extracted metadata from an image
#[derive(Debug, Clone, Default)]
pub struct ImageMetadata {
    // Date information
    pub date_taken: Option<DateTime<Utc>>,
    pub date_taken_source: Option<String>, // "exif", "filename", "mtime"
    /// True when EXIF date tags exist, even if parsing fails.
    pub had_exif_date_field: bool,

    // GPS information
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,

    // Camera information
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,

    // Shooting parameters
    pub iso: Option<i32>,
    pub aperture: Option<String>,      // e.g. "f/2.8"
    pub shutter_speed: Option<String>, // e.g. "1/125"
    pub focal_length: Option<String>,  // e.g. "50mm"
    pub lens_model: Option<String>,    // e.g. "iPhone 15 Pro back camera"
    pub flash: Option<String>,         // e.g. "Off" or "Fired"
    pub gps_altitude: Option<f64>,     // meters

    // Image dimensions
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub orientation: Option<u16>,
}

/// EXIF extractor service
pub struct ExifExtractor;

impl ExifExtractor {
    /// Extract metadata from an image file
    pub fn extract<P: AsRef<Path>>(path: P) -> ImageMetadata {
        let path = path.as_ref();
        let mut metadata = ImageMetadata::default();

        // Try EXIF extraction
        if let Some(exif_data) = Self::extract_exif(path) {
            metadata = exif_data;
        }

        // If no date from EXIF and EXIF date tags are absent, try filename.
        if metadata.date_taken.is_none() && !metadata.had_exif_date_field {
            if let Some(date) = Self::parse_date_from_filename(path) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("filename".to_string());
            }
        }

        // If still no date and EXIF date tags are absent, use file mtime.
        if metadata.date_taken.is_none() && !metadata.had_exif_date_field {
            if let Some(date) = Self::get_file_mtime(path) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("mtime".to_string());
            }
        }

        // Get image dimensions if not from EXIF
        if metadata.width.is_none() || metadata.height.is_none() {
            if let Some((w, h)) = Self::get_image_dimensions(path) {
                metadata.width = Some(w);
                metadata.height = Some(h);
            }
        }

        metadata
    }

    /// Extract EXIF data from a file
    fn extract_exif<P: AsRef<Path>>(path: P) -> Option<ImageMetadata> {
        let file = std::fs::File::open(path.as_ref()).ok()?;
        let mut bufreader = std::io::BufReader::new(&file);
        let exif = ExifReader::new().read_from_container(&mut bufreader).ok()?;

        let mut metadata = ImageMetadata::default();

        // Date taken
        if let Some(field) = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY) {
            metadata.had_exif_date_field = true;
            let date_str: String = field.display_value().to_string();
            if let Some(date) = Self::parse_exif_date(&date_str) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("exif".to_string());
            }
        } else if let Some(field) = exif.get_field(Tag::DateTime, In::PRIMARY) {
            metadata.had_exif_date_field = true;
            let date_str: String = field.display_value().to_string();
            if let Some(date) = Self::parse_exif_date(&date_str) {
                metadata.date_taken = Some(date);
                metadata.date_taken_source = Some("exif".to_string());
            }
        }

        // GPS coordinates
        metadata.gps_latitude =
            Self::extract_gps_coord(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef);
        metadata.gps_longitude =
            Self::extract_gps_coord(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef);

        // Camera make
        if let Some(field) = exif.get_field(Tag::Make, In::PRIMARY) {
            let val: String = field.display_value().to_string();
            metadata.camera_make = Some(val.trim_matches('"').to_string());
        }

        // Camera model
        if let Some(field) = exif.get_field(Tag::Model, In::PRIMARY) {
            let val: String = field.display_value().to_string();
            metadata.camera_model = Some(val.trim_matches('"').to_string());
        }

        // Image dimensions
        if let Some(field) = exif.get_field(Tag::PixelXDimension, In::PRIMARY) {
            if let Value::Long(ref vec) = field.value {
                if let Some(&w) = vec.first() {
                    metadata.width = Some(w);
                }
            }
        }
        if let Some(field) = exif.get_field(Tag::PixelYDimension, In::PRIMARY) {
            if let Value::Long(ref vec) = field.value {
                if let Some(&h) = vec.first() {
                    metadata.height = Some(h);
                }
            }
        }

        // Orientation
        if let Some(field) = exif.get_field(Tag::Orientation, In::PRIMARY) {
            if let Value::Short(ref vec) = field.value {
                if let Some(&o) = vec.first() {
                    metadata.orientation = Some(o);
                }
            }
        }

        // ISO
        if let Some(field) = exif.get_field(Tag::PhotographicSensitivity, In::PRIMARY) {
            if let Value::Short(ref vec) = field.value {
                if let Some(&iso) = vec.first() {
                    metadata.iso = Some(iso as i32);
                }
            } else if let Value::Long(ref vec) = field.value {
                if let Some(&iso) = vec.first() {
                    metadata.iso = Some(iso as i32);
                }
            }
        }

        // Aperture (FNumber)
        if let Some(field) = exif.get_field(Tag::FNumber, In::PRIMARY) {
            if let Value::Rational(ref vec) = field.value {
                if let Some(r) = vec.first() {
                    let f = r.to_f64();
                    if f > 0.0 {
                        metadata.aperture = Some(format!("f/{:.1}", f));
                    }
                }
            }
        }

        // Shutter speed (ExposureTime)
        if let Some(field) = exif.get_field(Tag::ExposureTime, In::PRIMARY) {
            if let Value::Rational(ref vec) = field.value {
                if let Some(r) = vec.first() {
                    if r.num > 0 && r.denom > 0 {
                        if r.num >= r.denom {
                            metadata.shutter_speed = Some(format!("{}s", r.to_f64()));
                        } else {
                            metadata.shutter_speed = Some(format!("{}/{}", r.num, r.denom));
                        }
                    }
                }
            }
        }

        // Focal length
        if let Some(field) = exif.get_field(Tag::FocalLength, In::PRIMARY) {
            if let Value::Rational(ref vec) = field.value {
                if let Some(r) = vec.first() {
                    let fl = r.to_f64();
                    if fl > 0.0 {
                        metadata.focal_length = Some(format!("{:.0}mm", fl));
                    }
                }
            }
        }

        // Lens model
        if let Some(field) = exif.get_field(Tag::LensModel, In::PRIMARY) {
            let val: String = field.display_value().to_string();
            let clean = val.trim_matches('"').to_string();
            if !clean.is_empty() {
                metadata.lens_model = Some(clean);
            }
        }

        // Flash
        if let Some(field) = exif.get_field(Tag::Flash, In::PRIMARY) {
            if let Value::Short(ref vec) = field.value {
                if let Some(&flash_val) = vec.first() {
                    metadata.flash = Some(if flash_val & 1 == 1 {
                        "Fired".to_string()
                    } else {
                        "Off".to_string()
                    });
                }
            }
        }

        // GPS Altitude
        if let Some(field) = exif.get_field(Tag::GPSAltitude, In::PRIMARY) {
            if let Value::Rational(ref vec) = field.value {
                if let Some(r) = vec.first() {
                    let mut alt = r.to_f64();
                    // Check altitude ref (0 = above sea level, 1 = below)
                    if let Some(ref_field) = exif.get_field(Tag::GPSAltitudeRef, In::PRIMARY) {
                        if let Value::Byte(ref bytes) = ref_field.value {
                            if bytes.first() == Some(&1) {
                                alt = -alt;
                            }
                        }
                    }
                    metadata.gps_altitude = Some(alt);
                }
            }
        }

        Some(metadata)
    }

    /// Extract GPS coordinate from EXIF
    fn extract_gps_coord(exif: &exif::Exif, coord_tag: Tag, ref_tag: Tag) -> Option<f64> {
        let field = exif.get_field(coord_tag, In::PRIMARY)?;
        let ref_field = exif.get_field(ref_tag, In::PRIMARY)?;

        // Parse coordinate value (degrees, minutes, seconds)
        let coord = match &field.value {
            Value::Rational(rationals) if rationals.len() >= 3 => {
                let degrees = rationals[0].to_f64();
                let minutes = rationals[1].to_f64();
                let seconds = rationals[2].to_f64();
                degrees + minutes / 60.0 + seconds / 3600.0
            }
            _ => return None,
        };

        // Apply reference (N/S or E/W)
        let ref_str: String = ref_field.display_value().to_string();
        let multiplier = if ref_str.contains('S') || ref_str.contains('W') {
            -1.0
        } else {
            1.0
        };

        Some(coord * multiplier)
    }

    /// Parse EXIF date format: "2019:03:15 14:30:22"
    fn parse_exif_date(date_str: &str) -> Option<DateTime<Utc>> {
        let mut clean = date_str.trim().trim_matches('"').replace('\0', "");
        clean = clean.trim().to_string();

        // Common EXIF forms seen in the wild.
        let formats = [
            "%Y:%m:%d %H:%M:%S",
            "%Y:%m:%d %H:%M:%S%.f",
            "%Y:%m:%d %H:%M:%S%:z",
            "%Y:%m:%d %H:%M:%S%.f%:z",
        ];

        for fmt in formats {
            if fmt.contains("%:z") {
                if let Ok(dt) = chrono::DateTime::parse_from_str(&clean, fmt) {
                    return Some(dt.with_timezone(&Utc));
                }
            } else if let Ok(parsed) = NaiveDateTime::parse_from_str(&clean, fmt) {
                return Some(Utc.from_utc_datetime(&parsed));
            }
        }

        // Some cameras use '-' instead of ':' in the date segment.
        if clean.len() >= 10 {
            let mut normalized = clean.clone();
            let bytes = normalized.as_bytes().to_vec();
            if bytes.get(4) == Some(&b'-') && bytes.get(7) == Some(&b'-') {
                normalized.replace_range(4..5, ":");
                normalized.replace_range(7..8, ":");
                for fmt in ["%Y:%m:%d %H:%M:%S", "%Y:%m:%d %H:%M:%S%.f"] {
                    if let Ok(parsed) = NaiveDateTime::parse_from_str(&normalized, fmt) {
                        return Some(Utc.from_utc_datetime(&parsed));
                    }
                }
            }
        }

        None
    }

    /// Parse date from filename patterns
    pub fn parse_date_from_filename<P: AsRef<Path>>(path: P) -> Option<DateTime<Utc>> {
        lazy_static! {
            // Common filename date patterns
            static ref PATTERNS: Vec<(Regex, &'static str)> = vec![
                // IMG_20190315_143022.jpg
                (Regex::new(r"IMG_(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // 20190315_143022.jpg
                (Regex::new(r"(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // 2019-03-15 14.30.22.jpg
                (Regex::new(r"(\d{4}-\d{2}-\d{2}) (\d{2}\.\d{2}\.\d{2})").unwrap(), "%Y-%m-%d%H.%M.%S"),
                // Screenshot_20190315-143022.png
                (Regex::new(r"Screenshot_(\d{8})-(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // VID_20190315_143022.mp4
                (Regex::new(r"VID_(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // PXL_20190315_143022.jpg (Pixel phones)
                (Regex::new(r"PXL_(\d{8})_(\d{6})").unwrap(), "%Y%m%d%H%M%S"),
                // Just date: 20190315.jpg
                (Regex::new(r"(\d{8})\.").unwrap(), "%Y%m%d"),
            ];
        }

        let filename = path.as_ref().file_name()?.to_str()?;

        for (regex, format) in PATTERNS.iter() {
            if let Some(caps) = regex.captures(filename) {
                // Combine captured groups
                let date_str: String = caps
                    .iter()
                    .skip(1)
                    .filter_map(|m| m.map(|m| m.as_str()))
                    .collect();

                // Try to parse as datetime
                if let Ok(parsed) = NaiveDateTime::parse_from_str(&date_str, format) {
                    return Some(Utc.from_utc_datetime(&parsed));
                }

                // Try date only format
                if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&date_str, "%Y%m%d") {
                    let datetime = parsed.and_hms_opt(0, 0, 0)?;
                    return Some(Utc.from_utc_datetime(&datetime));
                }
            }
        }

        None
    }

    /// Get file modification time
    fn get_file_mtime<P: AsRef<Path>>(path: P) -> Option<DateTime<Utc>> {
        let metadata = std::fs::metadata(path.as_ref()).ok()?;
        let mtime = metadata.modified().ok()?;
        let duration = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;

        DateTime::from_timestamp(duration.as_secs() as i64, 0)
    }

    /// Get image dimensions by reading the header only
    fn get_image_dimensions<P: AsRef<Path>>(path: P) -> Option<(u32, u32)> {
        let reader = image::ImageReader::open(path.as_ref()).ok()?;
        let (w, h) = reader.into_dimensions().ok()?;
        Some((w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_filename_date_parsing() {
        let path = std::path::Path::new("IMG_20190315_143022.jpg");
        let date = ExifExtractor::parse_date_from_filename(path);
        assert!(date.is_some());

        let dt = date.unwrap();
        assert_eq!(dt.year(), 2019);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn test_filename_date_parsing_plain() {
        let path = std::path::Path::new("20190315_143022.jpg");
        let date = ExifExtractor::parse_date_from_filename(path);
        assert!(date.is_some());

        let dt = date.unwrap();
        assert_eq!(dt.year(), 2019);
    }

    #[test]
    fn test_filename_date_parsing_pixel() {
        let path = std::path::Path::new("PXL_20210801_120000.jpg");
        let date = ExifExtractor::parse_date_from_filename(path);
        assert!(date.is_some());

        let dt = date.unwrap();
        assert_eq!(dt.year(), 2021);
        assert_eq!(dt.month(), 8);
    }

    #[test]
    fn test_exif_date_parsing() {
        let date = ExifExtractor::parse_exif_date("2019:03:15 14:30:22");
        assert!(date.is_some());

        let dt = date.unwrap();
        assert_eq!(dt.year(), 2019);
    }

    #[test]
    fn test_no_date_from_random_filename() {
        let path = std::path::Path::new("vacation_photo.jpg");
        let date = ExifExtractor::parse_date_from_filename(path);
        assert!(date.is_none());
    }
}
