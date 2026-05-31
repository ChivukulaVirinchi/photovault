//! Background EXIF + geocoding pass.
//!
//! Reads photos with `metadata_extracted = 0`, runs `ExifExtractor::extract`
//! on each (header read only, ~10 KB per file), reverse-geocodes any GPS
//! coordinates, and updates the row in place. Idempotent and resumable.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use rusqlite::params;

use crate::db::Database;
use crate::models::MediaType;
use crate::services::exif_extractor::ExifExtractor;
use crate::services::scanner::media_type_for_path;
use crate::services::GeocodingService;

const METADATA_CHUNK_SIZE: usize = 50;

#[derive(Debug, Clone)]
pub struct MetadataProgress {
    pub total: u64,
    pub done: u64,
    pub is_complete: bool,
    pub elapsed_seconds: f64,
}

pub fn start_metadata_job(
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
) -> (Receiver<MetadataProgress>, tokio::task::JoinHandle<()>) {
    let (progress_tx, progress_rx) = bounded::<MetadataProgress>(32);
    let handle = tokio::spawn(async move {
        run_metadata_job(drive_root, db, cancel, progress_tx).await;
    });
    (progress_rx, handle)
}

async fn run_metadata_job(
    drive_root: PathBuf,
    db: Arc<tokio::sync::Mutex<Database>>,
    cancel: Arc<AtomicBool>,
    progress_tx: Sender<MetadataProgress>,
) {
    let start = Instant::now();

    let total: u64 = {
        let guard = db.lock().await;
        guard.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE metadata_extracted = FALSE AND is_trashed = FALSE",
            [],
            |row| row.get(0),
        ).unwrap_or(0)
    };
    let total = total as u64;
    let mut done = 0u64;

    let geonames_path = crate::db::geonames::geonames_db_path();
    let geocoder = if geonames_path.exists() {
        GeocodingService::new(&geonames_path).ok()
    } else {
        None
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let chunk: Vec<(i64, String)> = {
            let guard = db.lock().await;
            let mut stmt = match guard.conn.prepare(
                "SELECT id, file_path FROM photos \
                 WHERE metadata_extracted = FALSE AND is_trashed = FALSE \
                 LIMIT ?",
            ) {
                Ok(s) => s,
                Err(_) => break,
            };
            stmt.query_map([METADATA_CHUNK_SIZE as i64], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
        };

        if chunk.is_empty() {
            break;
        }

        let root = drive_root.clone();
        let extracted: Vec<(i64, ExtractedMetadata)> = chunk
            .par_iter()
            .map(|(id, rel_path)| {
                let abs = root.join(rel_path);
                let meta = match media_type_for_path(&abs).unwrap_or_default() {
                    MediaType::Video => ExtractedMetadata::Video(VideoMetadata::from_path(&abs)),
                    MediaType::Photo => {
                        ExtractedMetadata::Photo(Box::new(ExifExtractor::extract(&abs)))
                    }
                };
                (*id, meta)
            })
            .collect();

        let mut errors_this_chunk = 0usize;
        {
            let mut guard = db.lock().await;
            let tx = match guard.conn.transaction() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("metadata tx start: {e}");
                    continue;
                }
            };

            for (id, meta) in &extracted {
                let gps_latitude = meta.gps_latitude();
                let gps_longitude = meta.gps_longitude();
                let (city, country): (Option<String>, Option<String>) =
                    match (gps_latitude, gps_longitude, &geocoder) {
                        (Some(lat), Some(lon), Some(g)) => g
                            .reverse_geocode(lat, lon)
                            .map(|r| (Some(r.city), Some(r.country)))
                            .unwrap_or((None, None)),
                        _ => (None, None),
                    };

                let res = tx.execute(
                    "UPDATE photos SET
                        date_taken = ?,
                        date_taken_source = ?,
                        gps_latitude = ?,
                        gps_longitude = ?,
                        location_city = ?,
                        location_country = ?,
                        camera_make = ?,
                        camera_model = ?,
                        iso = ?,
                        aperture = ?,
                        shutter_speed = ?,
                        focal_length = ?,
                        lens_model = ?,
                        flash = ?,
                        gps_altitude = ?,
                        width = ?,
                        height = ?,
                        orientation = ?,
                        media_type = ?,
                        duration_ms = ?,
                        video_codec = ?,
                        audio_codec = ?,
                        frame_rate = ?,
                        bitrate = ?,
                        has_audio = ?,
                        metadata_extracted = TRUE
                     WHERE id = ?",
                    params![
                        meta.date_taken()
                            .map(|d: chrono::DateTime<chrono::Utc>| d.to_rfc3339()),
                        meta.date_taken_source(),
                        gps_latitude,
                        gps_longitude,
                        city,
                        country,
                        meta.camera_make(),
                        meta.camera_model(),
                        meta.iso(),
                        meta.aperture(),
                        meta.shutter_speed(),
                        meta.focal_length(),
                        meta.lens_model(),
                        meta.flash(),
                        meta.gps_altitude(),
                        meta.width().map(|v| v as i64),
                        meta.height().map(|v| v as i64),
                        meta.orientation() as i64,
                        meta.media_type().as_str(),
                        meta.duration_ms(),
                        meta.video_codec(),
                        meta.audio_codec(),
                        meta.frame_rate(),
                        meta.bitrate(),
                        meta.has_audio(),
                        id,
                    ],
                );
                if res.is_err() {
                    errors_this_chunk += 1;
                }
            }
            if let Err(e) = tx.commit() {
                tracing::error!("metadata tx commit: {e}");
                continue;
            }
        }

        done += (chunk.len() - errors_this_chunk) as u64;
        let _ = progress_tx.try_send(MetadataProgress {
            total,
            done,
            is_complete: false,
            elapsed_seconds: start.elapsed().as_secs_f64(),
        });
    }

    let _ = progress_tx
        .send(MetadataProgress {
            total,
            done,
            is_complete: true,
            elapsed_seconds: start.elapsed().as_secs_f64(),
        })
        .await;
}

enum ExtractedMetadata {
    Photo(Box<crate::services::exif_extractor::ImageMetadata>),
    Video(VideoMetadata),
}

#[derive(Debug, Clone, Default)]
struct VideoMetadata {
    date_taken: Option<chrono::DateTime<chrono::Utc>>,
    date_taken_source: Option<String>,
}

impl VideoMetadata {
    fn from_path(path: &std::path::Path) -> Self {
        if let Some(date) = ExifExtractor::parse_date_from_filename(path) {
            return Self {
                date_taken: Some(date),
                date_taken_source: Some("filename".into()),
            };
        }
        Self {
            date_taken: file_mtime(path),
            date_taken_source: Some("mtime".into()),
        }
    }
}

fn file_mtime(path: &std::path::Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?;
    let duration = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
}

impl ExtractedMetadata {
    fn media_type(&self) -> MediaType {
        match self {
            Self::Photo(_) => MediaType::Photo,
            Self::Video(_) => MediaType::Video,
        }
    }

    fn date_taken(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        match self {
            Self::Photo(m) => m.date_taken,
            Self::Video(m) => m.date_taken,
        }
    }

    fn date_taken_source(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.date_taken_source.clone(),
            Self::Video(m) => m.date_taken_source.clone(),
        }
    }

    fn gps_latitude(&self) -> Option<f64> {
        match self {
            Self::Photo(m) => m.gps_latitude,
            Self::Video(_) => None,
        }
    }

    fn gps_longitude(&self) -> Option<f64> {
        match self {
            Self::Photo(m) => m.gps_longitude,
            Self::Video(_) => None,
        }
    }

    fn camera_make(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.camera_make.clone(),
            Self::Video(_) => None,
        }
    }

    fn camera_model(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.camera_model.clone(),
            Self::Video(_) => None,
        }
    }

    fn iso(&self) -> Option<i32> {
        match self {
            Self::Photo(m) => m.iso,
            Self::Video(_) => None,
        }
    }

    fn aperture(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.aperture.clone(),
            Self::Video(_) => None,
        }
    }

    fn shutter_speed(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.shutter_speed.clone(),
            Self::Video(_) => None,
        }
    }

    fn focal_length(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.focal_length.clone(),
            Self::Video(_) => None,
        }
    }

    fn lens_model(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.lens_model.clone(),
            Self::Video(_) => None,
        }
    }

    fn flash(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.flash.clone(),
            Self::Video(_) => None,
        }
    }

    fn gps_altitude(&self) -> Option<f64> {
        match self {
            Self::Photo(m) => m.gps_altitude,
            Self::Video(_) => None,
        }
    }

    fn width(&self) -> Option<u32> {
        match self {
            Self::Photo(m) => m.width,
            Self::Video(_) => None,
        }
    }

    fn height(&self) -> Option<u32> {
        match self {
            Self::Photo(m) => m.height,
            Self::Video(_) => None,
        }
    }

    fn orientation(&self) -> u16 {
        match self {
            Self::Photo(m) => m.orientation.unwrap_or(1),
            Self::Video(_) => 1,
        }
    }

    fn duration_ms(&self) -> Option<i64> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
        }
    }

    fn video_codec(&self) -> Option<String> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
        }
    }

    fn audio_codec(&self) -> Option<String> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
        }
    }

    fn frame_rate(&self) -> Option<f32> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
        }
    }

    fn bitrate(&self) -> Option<i64> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
        }
    }

    fn has_audio(&self) -> bool {
        match self {
            Self::Photo(_) => false,
            Self::Video(_) => false,
        }
    }
}
