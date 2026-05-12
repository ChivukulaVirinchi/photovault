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
use crate::services::exif_extractor::ExifExtractor;
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
        let extracted: Vec<(i64, crate::services::exif_extractor::ImageMetadata)> = chunk
            .par_iter()
            .map(|(id, rel_path)| {
                let abs = root.join(rel_path);
                let meta = ExifExtractor::extract(&abs);
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
                let (city, country): (Option<String>, Option<String>) =
                    match (meta.gps_latitude, meta.gps_longitude, &geocoder) {
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
                        metadata_extracted = TRUE
                     WHERE id = ?",
                    params![
                        meta.date_taken
                            .map(|d: chrono::DateTime<chrono::Utc>| d.to_rfc3339()),
                        meta.date_taken_source,
                        meta.gps_latitude,
                        meta.gps_longitude,
                        city,
                        country,
                        meta.camera_make,
                        meta.camera_model,
                        meta.iso,
                        meta.aperture,
                        meta.shutter_speed,
                        meta.focal_length,
                        meta.lens_model,
                        meta.flash,
                        meta.gps_altitude,
                        meta.width.map(|v| v as i64),
                        meta.height.map(|v| v as i64),
                        meta.orientation.unwrap_or(1) as i64,
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
