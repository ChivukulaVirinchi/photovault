//! Background EXIF + geocoding pass.
//!
//! Reads photos with `metadata_extracted = 0`, runs `ExifExtractor::extract`
//! on each (header read only, ~10 KB per file), reverse-geocodes any GPS
//! coordinates, and updates the row in place. Idempotent and resumable.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use regex::Regex;
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
    pub stage: Option<String>,
    pub message: Option<String>,
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
    let mut hard_error: Option<String> = None;
    let mut failed_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

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

        let chunk: Vec<(i64, String, String)> = {
            let guard = db.lock().await;
            match load_metadata_chunk(&guard.conn, &failed_ids, METADATA_CHUNK_SIZE) {
                Ok(rows) => rows,
                Err(e) => {
                    hard_error = Some(format!("metadata query failed: {e}"));
                    Vec::new()
                }
            }
        };

        if hard_error.is_some() || chunk.is_empty() {
            break;
        }

        let root = drive_root.clone();
        let extracted: Vec<(i64, String, ExtractedMetadata)> = chunk
            .par_iter()
            .map(|(id, rel_path, file_hash)| {
                let abs = match crate::services::path_util::safe_join_relative(&root, rel_path) {
                    Ok(abs) => abs,
                    Err(e) => {
                        tracing::debug!("metadata skipped unsafe path for photo_id={id}: {e}");
                        return (*id, file_hash.clone(), ExtractedMetadata::Skipped);
                    }
                };
                let meta = match media_type_for_path(&abs).unwrap_or_default() {
                    MediaType::Video => ExtractedMetadata::Video(VideoMetadata::from_path(&abs)),
                    MediaType::Photo => {
                        ExtractedMetadata::Photo(Box::new(ExifExtractor::extract(&abs)))
                    }
                };
                (*id, file_hash.clone(), meta)
            })
            .collect();

        let mut errors_this_chunk = 0usize;
        {
            let mut guard = db.lock().await;
            let tx = match guard.conn.transaction() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("metadata tx start: {e}");
                    hard_error = Some(format!("metadata database transaction failed: {e}"));
                    break;
                }
            };

            for (id, file_hash, meta) in &extracted {
                if matches!(meta, ExtractedMetadata::Skipped) {
                    errors_this_chunk += 1;
                    failed_ids.insert(*id);
                    continue;
                }
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
                     WHERE id = ? AND file_hash = ?",
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
                        file_hash,
                    ],
                );
                match res {
                    Ok(0) => {
                        errors_this_chunk += 1;
                        failed_ids.insert(*id);
                    }
                    Ok(_) => {}
                    Err(_) => {
                        errors_this_chunk += 1;
                        failed_ids.insert(*id);
                    }
                }
            }
            if let Err(e) = tx.commit() {
                tracing::error!("metadata tx commit: {e}");
                hard_error = Some(format!("metadata database commit failed: {e}"));
                break;
            }
        }

        done += (chunk.len() - errors_this_chunk) as u64;
        let _ = progress_tx.try_send(MetadataProgress {
            total,
            done,
            is_complete: false,
            elapsed_seconds: start.elapsed().as_secs_f64(),
            stage: Some("extract".to_string()),
            message: None,
        });
    }

    let _ = progress_tx
        .send(MetadataProgress {
            total,
            done,
            is_complete: hard_error.is_none(),
            elapsed_seconds: start.elapsed().as_secs_f64(),
            stage: hard_error
                .as_ref()
                .map(|_| "error".to_string())
                .or_else(|| Some("complete".to_string())),
            message: hard_error,
        })
        .await;
}

enum ExtractedMetadata {
    Photo(Box<crate::services::exif_extractor::ImageMetadata>),
    Video(VideoMetadata),
    Skipped,
}

fn load_metadata_chunk(
    conn: &rusqlite::Connection,
    failed_ids: &std::collections::HashSet<i64>,
    limit: usize,
) -> rusqlite::Result<Vec<(i64, String, String)>> {
    let mut sql = String::from(
        "SELECT id, file_path, file_hash FROM photos \
         WHERE metadata_extracted = FALSE AND is_trashed = FALSE",
    );
    let mut params: Vec<i64> = Vec::with_capacity(failed_ids.len() + 1);
    if !failed_ids.is_empty() {
        sql.push_str(" AND id NOT IN (");
        for idx in 0..failed_ids.len() {
            if idx > 0 {
                sql.push_str(", ");
            }
            sql.push('?');
        }
        sql.push(')');
        params.extend(failed_ids.iter().copied());
    }
    sql.push_str(" ORDER BY id ASC LIMIT ?");
    params.push(limit as i64);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    rows.collect()
}

#[derive(Debug, Clone, Default)]
struct VideoMetadata {
    date_taken: Option<chrono::DateTime<chrono::Utc>>,
    date_taken_source: Option<String>,
}

impl VideoMetadata {
    fn from_path(path: &Path) -> Self {
        if let Some(date) = video_capture_date(path) {
            return Self {
                date_taken: Some(date),
                date_taken_source: Some("video_metadata".into()),
            };
        }
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

fn video_capture_date(path: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let bytes = read_video_probe_bytes(path).ok()?;
    embedded_video_date_string(&bytes).or_else(|| quicktime_epoch_date(&bytes))
}

fn read_video_probe_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    const PROBE_CHUNK: usize = 8 * 1024 * 1024;
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len <= (PROBE_CHUNK * 2) as u64 {
        let mut bytes = Vec::with_capacity(len as usize);
        file.read_to_end(&mut bytes)?;
        return Ok(bytes);
    }

    let mut bytes = vec![0u8; PROBE_CHUNK];
    file.read_exact(&mut bytes)?;
    file.seek(SeekFrom::End(-(PROBE_CHUNK as i64)))?;
    let mut tail = vec![0u8; PROBE_CHUNK];
    file.read_exact(&mut tail)?;
    bytes.extend_from_slice(&tail);
    Ok(bytes)
}

fn quicktime_epoch_date(bytes: &[u8]) -> Option<chrono::DateTime<chrono::Utc>> {
    find_quicktime_atom_date(bytes, b"mvhd").or_else(|| find_quicktime_atom_date(bytes, b"mdhd"))
}

fn find_quicktime_atom_date(bytes: &[u8], atom: &[u8; 4]) -> Option<chrono::DateTime<chrono::Utc>> {
    const QT_TO_UNIX_SECONDS: i64 = 2_082_844_800;
    for i in 0..bytes.len().saturating_sub(16) {
        if &bytes[i..i + 4] != atom {
            continue;
        }
        let version = bytes.get(i + 4).copied()?;
        let seconds = match version {
            0 => {
                let raw = u32::from_be_bytes(bytes.get(i + 8..i + 12)?.try_into().ok()?);
                i64::from(raw)
            }
            1 => {
                let raw = u64::from_be_bytes(bytes.get(i + 8..i + 16)?.try_into().ok()?);
                i64::try_from(raw).ok()?
            }
            _ => continue,
        };
        if seconds <= QT_TO_UNIX_SECONDS {
            continue;
        }
        let unix = seconds - QT_TO_UNIX_SECONDS;
        let dt = chrono::DateTime::from_timestamp(unix, 0)?;
        ExifExtractor::plausible_datetime(dt.naive_utc())?;
        return Some(dt);
    }
    None
}

fn embedded_video_date_string(bytes: &[u8]) -> Option<chrono::DateTime<chrono::Utc>> {
    lazy_static::lazy_static! {
        static ref VIDEO_DATE: Regex = Regex::new(
            r"(?x)
            (\d{4}[:-]\d{2}[:-]\d{2}
            [ T]
            \d{2}:\d{2}(?::\d{2}(?:\.\d+)? )?
            (?:Z|[+-]\d{2}:?\d{2})?)
            "
        ).unwrap();
    }
    let text = String::from_utf8_lossy(bytes);
    for caps in VIDEO_DATE.captures_iter(&text) {
        let candidate = caps.get(1)?.as_str();
        if let Some(dt) = ExifExtractor::parse_exif_date(candidate) {
            return Some(dt);
        }
    }
    None
}

fn file_mtime(path: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
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
            Self::Skipped => MediaType::Photo,
        }
    }

    fn date_taken(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        match self {
            Self::Photo(m) => m.date_taken,
            Self::Video(m) => m.date_taken,
            Self::Skipped => None,
        }
    }

    fn date_taken_source(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.date_taken_source.clone(),
            Self::Video(m) => m.date_taken_source.clone(),
            Self::Skipped => None,
        }
    }

    fn gps_latitude(&self) -> Option<f64> {
        match self {
            Self::Photo(m) => m.gps_latitude,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn gps_longitude(&self) -> Option<f64> {
        match self {
            Self::Photo(m) => m.gps_longitude,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn camera_make(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.camera_make.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn camera_model(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.camera_model.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn iso(&self) -> Option<i32> {
        match self {
            Self::Photo(m) => m.iso,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn aperture(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.aperture.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn shutter_speed(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.shutter_speed.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn focal_length(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.focal_length.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn lens_model(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.lens_model.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn flash(&self) -> Option<String> {
        match self {
            Self::Photo(m) => m.flash.clone(),
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn gps_altitude(&self) -> Option<f64> {
        match self {
            Self::Photo(m) => m.gps_altitude,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn width(&self) -> Option<u32> {
        match self {
            Self::Photo(m) => m.width,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn height(&self) -> Option<u32> {
        match self {
            Self::Photo(m) => m.height,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn orientation(&self) -> u16 {
        match self {
            Self::Photo(m) => m.orientation.unwrap_or(1),
            Self::Video(_) => 1,
            Self::Skipped => 1,
        }
    }

    fn duration_ms(&self) -> Option<i64> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn video_codec(&self) -> Option<String> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn audio_codec(&self) -> Option<String> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn frame_rate(&self) -> Option<f32> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn bitrate(&self) -> Option<i64> {
        match self {
            Self::Photo(_) => None,
            Self::Video(_) => None,
            Self::Skipped => None,
        }
    }

    fn has_audio(&self) -> bool {
        match self {
            Self::Photo(_) => false,
            Self::Video(_) => false,
            Self::Skipped => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike, Utc};
    use std::collections::HashSet;

    #[test]
    fn quicktime_epoch_date_reads_mvhd_creation_time() {
        const QT_TO_UNIX_SECONDS: i64 = 2_082_844_800;
        let unix = Utc
            .with_ymd_and_hms(2024, 1, 15, 10, 16, 0)
            .single()
            .unwrap()
            .timestamp();
        let qt_seconds = (unix + QT_TO_UNIX_SECONDS) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&108u32.to_be_bytes());
        bytes.extend_from_slice(b"mvhd");
        bytes.push(0);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&qt_seconds.to_be_bytes());
        bytes.resize(108, 0);

        let dt = quicktime_epoch_date(&bytes).unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 16);
    }

    #[test]
    fn embedded_video_date_string_reads_quicktime_creationdate() {
        let bytes = b"com.apple.quicktime.creationdate\0\x002024-01-15T10:16:00+05:30";
        let dt = embedded_video_date_string(bytes).unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 16);
    }

    #[test]
    fn metadata_chunk_skips_current_run_failures_without_hiding_pending_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                metadata_extracted BOOLEAN NOT NULL DEFAULT FALSE,
                is_trashed BOOLEAN NOT NULL DEFAULT FALSE
            );
            INSERT INTO photos (id, file_path, file_hash, metadata_extracted, is_trashed)
            VALUES
                (1, 'bad.jpg', 'aa111', FALSE, FALSE),
                (2, 'good.jpg', 'bb222', FALSE, FALSE),
                (3, 'done.jpg', 'cc333', TRUE, FALSE),
                (4, 'trashed.jpg', 'dd444', FALSE, TRUE);
            "#,
        )
        .unwrap();
        let mut failed = HashSet::new();
        failed.insert(1);

        let rows = load_metadata_chunk(&conn, &failed, 20).unwrap();

        assert_eq!(rows, vec![(2, "good.jpg".into(), "bb222".into())]);
    }
}
