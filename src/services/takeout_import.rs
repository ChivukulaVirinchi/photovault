//! Google Photos Takeout ZIP importer.
//!
//! The importer treats every selected ZIP as one logical export, so sidecars
//! and repeated album copies can be reconciled across split Takeout parts.
//! Media is streamed through a temporary file, hashed, and atomically moved
//! into the library. The database ledger makes reruns idempotent and retains
//! Google-only metadata for future refreshes.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::read::ZipArchive;

use crate::db::{AlbumRepo, TakeoutImportRepo, TakeoutLedgerItem};
use crate::services::scanner::{calculate_hash, media_type_for_path};

const IMPORT_DIR: &str = "Imported from Google Photos";
const STAGING_DIR: &str = "takeout-import";
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 2_000_000;
const MIN_MEDIA_BYTES: u64 = 10 * 1024;
const SPACE_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TakeoutMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub taken_at_unix: Option<i64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub favorited: bool,
}

impl TakeoutMetadata {
    fn merge_from(&mut self, other: &Self) {
        if self.title.is_none() {
            self.title.clone_from(&other.title);
        }
        if self.description.is_none() {
            self.description.clone_from(&other.description);
        }
        if self.taken_at_unix.is_none() {
            self.taken_at_unix = other.taken_at_unix;
        }
        if self.latitude.is_none() {
            self.latitude = other.latitude;
        }
        if self.longitude.is_none() {
            self.longitude = other.longitude;
        }
        if self.altitude.is_none() {
            self.altitude = other.altitude;
        }
        self.favorited |= other.favorited;
    }

    pub fn taken_at_rfc3339(&self) -> Option<String> {
        self.taken_at_unix
            .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
            .map(|dt| dt.to_rfc3339())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TakeoutProgress {
    pub stage: &'static str,
    pub processed: u64,
    pub total: u64,
    pub message: String,
    pub elapsed_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TakeoutImportReport {
    pub archives: u64,
    pub media_found: u64,
    pub imported: u64,
    pub reused_existing: u64,
    pub duplicates_collapsed: u64,
    pub unsupported_or_small: u64,
    pub unmatched_sidecars: u64,
    pub albums_restored: u64,
    pub metadata_restored: u64,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
struct MediaEntry {
    archive_index: usize,
    entry_index: usize,
    logical_path: String,
    size: u64,
    sidecar_key: String,
    folder_key: String,
    file_name: String,
}

#[derive(Debug, Default)]
struct ImportPlan {
    archives: Vec<PathBuf>,
    media: Vec<MediaEntry>,
    sidecars: HashMap<String, TakeoutMetadata>,
    album_dirs: HashMap<String, String>,
    json_count: u64,
    ignored_count: u64,
    estimated_unique_bytes: u64,
}

#[derive(Debug)]
struct ImportedAggregate {
    relative_path: String,
    metadata: TakeoutMetadata,
    albums: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct RawTime {
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawGeo {
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawSidecar {
    title: Option<String>,
    description: Option<String>,
    photo_taken_time: Option<RawTime>,
    creation_time: Option<RawTime>,
    geo_data_exif: Option<RawGeo>,
    geo_data: Option<RawGeo>,
    favorited: Option<bool>,
}

pub fn import_google_takeout(
    archive_paths: &[PathBuf],
    library_root: &Path,
    conn: &Connection,
    cancel: &AtomicBool,
    mut progress: impl FnMut(TakeoutProgress),
) -> Result<TakeoutImportReport, String> {
    let started = Instant::now();
    validate_library_root(library_root)?;
    let mut report = TakeoutImportReport {
        archives: archive_paths.len() as u64,
        ..Default::default()
    };
    progress(TakeoutProgress {
        stage: "inspect",
        processed: 0,
        total: archive_paths.len() as u64,
        message: "Checking Takeout archives".into(),
        elapsed_seconds: 0.0,
    });
    let plan = inspect_archives(archive_paths, cancel, |processed, total, message| {
        progress(TakeoutProgress {
            stage: "inspect",
            processed,
            total,
            message,
            elapsed_seconds: started.elapsed().as_secs_f64(),
        });
    })?;
    report.media_found = plan.media.len() as u64;
    report.unsupported_or_small = plan.ignored_count;
    if cancel.load(Ordering::Relaxed) {
        report.cancelled = true;
        return Ok(report);
    }
    if plan.media.is_empty() {
        return Err("No supported Google Photos media was found in the selected ZIP files".into());
    }
    check_available_space(library_root, plan.estimated_unique_bytes)?;

    let candidate_sizes: HashSet<i64> = plan
        .media
        .iter()
        .filter_map(|m| i64::try_from(m.size).ok())
        .collect();
    let existing_hashes = hash_existing_candidates(conn, library_root, &candidate_sizes, cancel);
    let import_root = library_root.join(IMPORT_DIR);
    fs::create_dir_all(&import_root)
        .map_err(|e| format!("Could not create {}: {e}", import_root.display()))?;
    let staging = crate::db::connection::library_metadata_dir(library_root).join(STAGING_DIR);
    prepare_staging(&staging)?;

    let mut aggregates: HashMap<String, ImportedAggregate> = HashMap::new();
    let mut used_sidecars: HashSet<String> = HashSet::new();
    let mut current_archive_index = usize::MAX;
    let mut current_archive: Option<ZipArchive<BufReader<File>>> = None;
    let repo = TakeoutImportRepo::new(conn);

    for (ordinal, media) in plan.media.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        if current_archive_index != media.archive_index {
            let file = File::open(&plan.archives[media.archive_index]).map_err(|e| {
                format!(
                    "Could not reopen {}: {e}",
                    plan.archives[media.archive_index].display()
                )
            })?;
            current_archive = Some(
                ZipArchive::new(BufReader::new(file))
                    .map_err(|e| format!("Invalid ZIP during import: {e}"))?,
            );
            current_archive_index = media.archive_index;
        }

        let temp_path = staging.join(format!("{}.partial", ordinal));
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }
        let archive = current_archive.as_mut().expect("archive opened above");
        let mut entry = match archive.by_index(media.entry_index) {
            Ok(entry) => entry,
            Err(error) => {
                report
                    .errors
                    .push(format!("Could not read {}: {error}", media.logical_path));
                continue;
            }
        };
        let content_hash = match stream_entry_to_temp(&mut entry, &temp_path) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                report
                    .errors
                    .push(format!("Could not import {}: {error}", media.logical_path));
                continue;
            }
        };

        let metadata = plan
            .sidecars
            .get(&media.sidecar_key)
            .cloned()
            .unwrap_or_default();
        if plan.sidecars.contains_key(&media.sidecar_key) {
            used_sidecars.insert(media.sidecar_key.clone());
        }
        let mut albums = BTreeSet::new();
        if let Some(album) = plan.album_dirs.get(&media.folder_key) {
            albums.insert(album.clone());
        }

        if let Some(existing) = aggregates.get_mut(&content_hash) {
            existing.metadata.merge_from(&metadata);
            existing.albums.extend(albums);
            report.duplicates_collapsed += 1;
            let _ = fs::remove_file(&temp_path);
        } else {
            let prior_takeout = repo
                .path_for_hash(&content_hash)
                .map_err(|e| e.to_string())?;
            let existing_path = prior_takeout
                .filter(|path| library_root.join(path).is_file())
                .or_else(|| existing_hashes.get(&content_hash).cloned());
            let relative_path = if let Some(path) = existing_path {
                report.reused_existing += 1;
                let _ = fs::remove_file(&temp_path);
                path
            } else {
                let destination = destination_for(&import_root, media, &metadata, &content_hash)?;
                if destination.is_file()
                    && calculate_hash(&destination).ok().as_deref() == Some(&content_hash)
                {
                    let _ = fs::remove_file(&temp_path);
                    report.reused_existing += 1;
                } else {
                    atomic_move(&temp_path, &destination)?;
                    report.imported += 1;
                }
                crate::services::path_util::relative_path_for_storage(
                    destination
                        .strip_prefix(library_root)
                        .map_err(|_| "Import destination escaped the library root".to_string())?,
                )
            };
            aggregates.insert(
                content_hash,
                ImportedAggregate {
                    relative_path,
                    metadata,
                    albums,
                },
            );
        }

        progress(TakeoutProgress {
            stage: "extract",
            processed: (ordinal + 1) as u64,
            total: plan.media.len() as u64,
            message: media.file_name.clone(),
            elapsed_seconds: started.elapsed().as_secs_f64(),
        });
    }

    let ledger_items: Vec<TakeoutLedgerItem> = aggregates
        .iter()
        .map(|(hash, aggregate)| TakeoutLedgerItem {
            content_hash: hash.clone(),
            file_path: aggregate.relative_path.clone(),
            metadata_json: (aggregate.metadata != TakeoutMetadata::default())
                .then(|| serde_json::to_string(&aggregate.metadata).ok())
                .flatten(),
            albums: aggregate.albums.clone(),
        })
        .collect();
    repo.upsert_items(&ledger_items)
        .map_err(|e| format!("Could not save Takeout import state: {e}"))?;
    report.unmatched_sidecars = plan
        .sidecars
        .keys()
        .filter(|key| !used_sidecars.contains(*key))
        .count() as u64;
    let _ = fs::remove_dir_all(&staging);
    Ok(report)
}

pub fn apply_takeout_metadata_and_albums(conn: &Connection) -> Result<(u64, u64), String> {
    let geocoder =
        crate::services::geocoding::GeocodingService::new(crate::db::geonames::geonames_db_path())
            .ok();
    let mut stmt = conn
        .prepare(
            r#"
            SELECT i.content_hash, i.metadata_json, p.id
              FROM google_takeout_items i
              JOIN photos p ON p.file_path = i.file_path
             WHERE p.is_trashed = FALSE
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, Option<String>, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    let mut parsed_rows = Vec::with_capacity(rows.len());
    let mut album_photos: HashMap<String, Vec<i64>> = HashMap::new();
    for (hash, metadata_json, photo_id) in rows {
        let metadata = metadata_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<TakeoutMetadata>(json).ok());
        parsed_rows.push((photo_id, metadata));
        let mut album_stmt = conn
            .prepare("SELECT album_name FROM google_takeout_albums WHERE content_hash = ?1")
            .map_err(|e| e.to_string())?;
        let albums: Vec<String> = album_stmt
            .query_map(params![hash], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        for album in albums {
            album_photos.entry(album).or_default().push(photo_id);
        }
    }

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut metadata_restored = 0u64;
    for (photo_id, metadata) in parsed_rows {
        if let Some(metadata) = metadata {
            let taken = metadata.taken_at_rfc3339();
            let (city, country) = match (metadata.latitude, metadata.longitude, geocoder.as_ref()) {
                (Some(lat), Some(lon), Some(g)) => g
                    .reverse_geocode(lat, lon)
                    .map(|place| (Some(place.city), Some(place.country)))
                    .unwrap_or((None, None)),
                _ => (None, None),
            };
            tx.execute(
                    r#"
                    UPDATE photos SET
                        date_taken = COALESCE(?1, date_taken),
                        date_taken_source = CASE WHEN ?1 IS NOT NULL THEN 'google_takeout' ELSE date_taken_source END,
                        gps_latitude = COALESCE(?2, gps_latitude),
                        gps_longitude = COALESCE(?3, gps_longitude),
                        gps_altitude = COALESCE(?4, gps_altitude),
                        location_city = COALESCE(?5, location_city),
                        location_country = COALESCE(?6, location_country),
                        is_favorite = CASE WHEN ?7 THEN TRUE ELSE is_favorite END,
                        updated_at = CURRENT_TIMESTAMP
                    WHERE id = ?8
                    "#,
                    params![
                        taken,
                        metadata.latitude,
                        metadata.longitude,
                        metadata.altitude,
                        city,
                        country,
                        metadata.favorited,
                        photo_id
                    ],
                )
                .map_err(|e| e.to_string())?;
            metadata_restored += 1;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    let mut album_ids = HashSet::new();
    for (album_name, photo_ids) in album_photos {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM albums WHERE name = ?1 ORDER BY id LIMIT 1",
                params![album_name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let album_id = match existing {
            Some(id) => id,
            None => AlbumRepo::new(conn)
                .create(&album_name)
                .map_err(|e| e.to_string())?,
        };
        AlbumRepo::new(conn)
            .add_photos(album_id, &photo_ids)
            .map_err(|e| e.to_string())?;
        album_ids.insert(album_id);
    }
    Ok((metadata_restored, album_ids.len() as u64))
}

fn inspect_archives(
    archive_paths: &[PathBuf],
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64, String),
) -> Result<ImportPlan, String> {
    if archive_paths.is_empty() {
        return Err("Select at least one Google Takeout ZIP file".into());
    }
    let mut plan = ImportPlan {
        archives: archive_paths.to_vec(),
        ..Default::default()
    };
    let mut unique_size_crc = HashSet::new();
    for (archive_index, path) in archive_paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some("zip")
        {
            return Err(format!("{} is not a ZIP file", path.display()));
        }
        let file =
            File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
        let mut archive = ZipArchive::new(BufReader::new(file))
            .map_err(|e| format!("Invalid ZIP {}: {e}", path.display()))?;
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            return Err(format!(
                "{} contains too many entries ({})",
                path.display(),
                archive.len()
            ));
        }
        for entry_index in 0..archive.len() {
            let mut entry = archive.by_index(entry_index).map_err(|e| {
                format!(
                    "Could not inspect {} entry {entry_index}: {e}",
                    path.display()
                )
            })?;
            if entry.is_dir() {
                continue;
            }
            let enclosed = entry
                .enclosed_name()
                .ok_or_else(|| format!("Unsafe path in {}: {}", path.display(), entry.name()))?;
            let logical_path = normalize_zip_path(&enclosed);
            let Some(relative_google_path) = google_photos_relative(&logical_path) else {
                continue;
            };
            let lower = relative_google_path.to_ascii_lowercase();
            if lower.ends_with(".json") {
                if entry.size() > MAX_JSON_BYTES {
                    plan.ignored_count += 1;
                    continue;
                }
                let mut json = String::new();
                entry
                    .read_to_string(&mut json)
                    .map_err(|e| format!("Could not read sidecar {logical_path}: {e}"))?;
                if file_name_of(&relative_google_path).eq_ignore_ascii_case("metadata.json") {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
                        if let Some(title) = value.get("title").and_then(|v| v.as_str()) {
                            let folder = folder_of(&relative_google_path);
                            if !is_year_bucket(&folder) && !title.trim().is_empty() {
                                plan.album_dirs.insert(folder, title.trim().to_string());
                            }
                        }
                    }
                } else {
                    match parse_sidecar(&relative_google_path, &json) {
                        Some((key, metadata)) => {
                            plan.sidecars
                                .entry(key)
                                .and_modify(|m| m.merge_from(&metadata))
                                .or_insert(metadata);
                            plan.json_count += 1;
                        }
                        None => plan.ignored_count += 1,
                    }
                }
                continue;
            }
            if media_type_for_path(Path::new(&relative_google_path)).is_none()
                || entry.size() < MIN_MEDIA_BYTES
            {
                plan.ignored_count += 1;
                continue;
            }
            let file_name = file_name_of(&relative_google_path).to_string();
            let folder_key = folder_of(&relative_google_path);
            let sidecar_key = sidecar_key(&folder_key, &file_name);
            let size_crc = (entry.size(), entry.crc32());
            if unique_size_crc.insert(size_crc) {
                plan.estimated_unique_bytes =
                    plan.estimated_unique_bytes.saturating_add(entry.size());
            }
            plan.media.push(MediaEntry {
                archive_index,
                entry_index,
                logical_path,
                size: entry.size(),
                sidecar_key,
                folder_key,
                file_name,
            });
        }
        progress(
            (archive_index + 1) as u64,
            archive_paths.len() as u64,
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Takeout archive")
                .to_string(),
        );
    }

    Ok(plan)
}

fn parse_sidecar(relative_path: &str, json: &str) -> Option<(String, TakeoutMetadata)> {
    let raw: RawSidecar = serde_json::from_str(json).ok()?;
    let folder = folder_of(relative_path);
    let fallback = sidecar_media_name(file_name_of(relative_path));
    let title = raw
        .title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&fallback)
        .to_string();
    if title.is_empty() {
        return None;
    }
    let taken_at_unix = raw
        .photo_taken_time
        .or(raw.creation_time)
        .and_then(|t| t.timestamp)
        .and_then(|s| s.parse::<i64>().ok());
    let geo = valid_geo(raw.geo_data_exif).or_else(|| valid_geo(raw.geo_data));
    let metadata = TakeoutMetadata {
        title: Some(title.clone()),
        description: raw.description.filter(|s| !s.is_empty()),
        taken_at_unix,
        latitude: geo.as_ref().and_then(|g| g.latitude),
        longitude: geo.as_ref().and_then(|g| g.longitude),
        altitude: geo.and_then(|g| g.altitude),
        favorited: raw.favorited.unwrap_or(false),
    };
    Some((sidecar_key(&folder, &title), metadata))
}

fn valid_geo(geo: Option<RawGeo>) -> Option<RawGeo> {
    let geo = geo?;
    let lat = geo.latitude?;
    let lon = geo.longitude?;
    if !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
        || (lat == 0.0 && lon == 0.0)
    {
        return None;
    }
    Some(geo)
}

fn hash_existing_candidates(
    conn: &Connection,
    library_root: &Path,
    sizes: &HashSet<i64>,
    cancel: &AtomicBool,
) -> HashMap<String, String> {
    let repo = TakeoutImportRepo::new(conn);
    let mut result = HashMap::new();
    let candidates = repo.candidate_existing_files(sizes).unwrap_or_default();
    for (relative, _) in candidates {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(path) = crate::services::path_util::safe_join_relative(library_root, &relative)
        else {
            continue;
        };
        if let Ok(hash) = calculate_hash(path) {
            result.entry(hash).or_insert(relative);
        }
    }
    result
}

fn stream_entry_to_temp<R: Read>(reader: &mut R, temp_path: &Path) -> Result<String, String> {
    let file = File::create(temp_path)
        .map_err(|e| format!("Could not create temporary import file: {e}"))?;
    let mut writer = BufWriter::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| format!("Could not decompress Takeout media: {e}"))?;
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|e| format!("Could not write imported media: {e}"))?;
        hasher.update(&buffer[..count]);
    }
    writer
        .flush()
        .map_err(|e| format!("Could not finish imported media: {e}"))?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn destination_for(
    import_root: &Path,
    media: &MediaEntry,
    metadata: &TakeoutMetadata,
    hash: &str,
) -> Result<PathBuf, String> {
    let year = metadata
        .taken_at_unix
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .map(|d: DateTime<Utc>| d.format("%Y").to_string())
        .unwrap_or_else(|| "Undated".to_string());
    let dir = import_root.join(year);
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let safe_name = sanitize_file_name(&media.file_name);
    let preferred = dir.join(&safe_name);
    if !preferred.exists() {
        return Ok(preferred);
    }
    if calculate_hash(&preferred).ok().as_deref() == Some(hash) {
        return Ok(preferred);
    }
    let source = Path::new(&safe_name);
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("photo");
    let ext = source.extension().and_then(|s| s.to_str());
    let mut candidate = match ext {
        Some(ext) => dir.join(format!("{stem}_{}.{}", &hash[..10], ext)),
        None => dir.join(format!("{stem}_{}", &hash[..10])),
    };
    let mut suffix = 2u32;
    while candidate.exists() && calculate_hash(&candidate).ok().as_deref() != Some(hash) {
        candidate = match ext {
            Some(ext) => dir.join(format!("{stem}_{}_{}.{}", &hash[..10], suffix, ext)),
            None => dir.join(format!("{stem}_{}_{}", &hash[..10], suffix)),
        };
        suffix += 1;
    }
    Ok(candidate)
}

fn atomic_move(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    fs::rename(source, destination).map_err(|e| {
        format!(
            "Could not place imported file at {}: {e}",
            destination.display()
        )
    })
}

fn prepare_staging(staging: &Path) -> Result<(), String> {
    if staging.exists() {
        fs::remove_dir_all(staging)
            .map_err(|e| format!("Could not clear stale Takeout staging data: {e}"))?;
    }
    fs::create_dir_all(staging)
        .map_err(|e| format!("Could not create Takeout staging directory: {e}"))
}

fn check_available_space(root: &Path, required: u64) -> Result<(), String> {
    let available = fs2::available_space(root)
        .map_err(|e| format!("Could not determine available library space: {e}"))?;
    let needed = required.saturating_add(SPACE_MARGIN_BYTES);
    if available < needed {
        return Err(format!(
            "Not enough free space. The import needs about {}, but only {} is available",
            human_bytes(needed),
            human_bytes(available)
        ));
    }
    Ok(())
}

fn validate_library_root(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("Library folder does not exist: {}", root.display()));
    }
    let metadata = crate::db::connection::library_metadata_dir(root);
    if !metadata.is_dir() {
        return Err("Open the destination as a Smriti library before importing".into());
    }
    Ok(())
}

fn google_photos_relative(path: &str) -> Option<String> {
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let idx = components
        .iter()
        .position(|component| component.eq_ignore_ascii_case("Google Photos"))?;
    let rest = components.get(idx + 1..)?.join("/");
    (!rest.is_empty()).then_some(rest)
}

fn normalize_zip_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn folder_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(folder, _)| folder.to_string())
        .unwrap_or_default()
}

fn file_name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn last_folder_component(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn sidecar_key(folder: &str, title: &str) -> String {
    format!("{}\0{}", folder.to_lowercase(), title.to_lowercase())
}

fn sidecar_media_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".supplemental-metadata.json") {
        name[..name.len() - ".supplemental-metadata.json".len()].to_string()
    } else if lower.ends_with(".json") {
        name[..name.len() - 5].to_string()
    } else {
        name.to_string()
    }
}

fn is_year_bucket(folder: &str) -> bool {
    let name = last_folder_component(folder).trim();
    let lower = name.to_ascii_lowercase();
    if let Some(year) = lower.strip_prefix("photos from ") {
        return year.len() == 4 && year.chars().all(|c| c.is_ascii_digit());
    }
    false
}

fn sanitize_file_name(name: &str) -> String {
    let mut safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    safe = safe.trim_matches([' ', '.']).to_string();
    if safe.is_empty() {
        safe = "photo".into();
    }
    let stem = Path::new(&safe)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        safe.insert(0, '_');
    }
    truncate_file_name(&safe, 180)
}

fn truncate_file_name(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_string();
    }
    let path = Path::new(name);
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .filter(|ext| ext.len() <= 20)
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("photo");
    let budget = max_bytes.saturating_sub(extension.len());
    let mut shortened = String::new();
    for ch in stem.chars() {
        if shortened.len() + ch.len_utf8() > budget {
            break;
        }
        shortened.push(ch);
    }
    if shortened.is_empty() {
        shortened.push_str("photo");
    }
    shortened.push_str(&extension);
    shortened
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn jpeg_bytes(seed: u8) -> Vec<u8> {
        let mut bytes = vec![seed; MIN_MEDIA_BYTES as usize + 1];
        bytes[0..3].copy_from_slice(&[0xff, 0xd8, 0xff]);
        bytes
    }

    #[test]
    fn parses_google_sidecar_with_exif_geo_priority() {
        let json = r#"{
          "title":"IMG_0001.JPG",
          "description":"At the beach",
          "photoTakenTime":{"timestamp":"1704067200"},
          "geoData":{"latitude":1.0,"longitude":2.0,"altitude":3.0},
          "geoDataExif":{"latitude":10.0,"longitude":20.0,"altitude":30.0},
          "favorited":true
        }"#;
        let (_, meta) = parse_sidecar("Photos from 2024/IMG_0001.JPG.json", json).unwrap();
        assert_eq!(meta.taken_at_unix, Some(1_704_067_200));
        assert_eq!(meta.latitude, Some(10.0));
        assert_eq!(meta.longitude, Some(20.0));
        assert!(meta.favorited);
    }

    #[test]
    fn multi_part_import_collapses_album_copy_and_restores_metadata() {
        let root = tempdir().unwrap();
        let db = crate::db::Database::open_for_drive(root.path()).unwrap();
        crate::db::create_schema(&db.conn).unwrap();
        let first = root.path().join("takeout-001.zip");
        let second = root.path().join("takeout-002.zip");
        let photo = jpeg_bytes(7);
        let sidecar = br#"{
          "title":"IMG_0001.JPG",
          "photoTakenTime":{"timestamp":"1704067200"},
          "geoDataExif":{"latitude":10.0,"longitude":20.0,"altitude":30.0},
          "favorited":true
        }"#;
        write_zip(
            &first,
            &[
                (
                    "Takeout/Google Photos/Photos from 2024/IMG_0001.JPG",
                    &photo,
                ),
                (
                    "Takeout/Google Photos/Photos from 2024/IMG_0001.JPG.json",
                    sidecar,
                ),
            ],
        );
        write_zip(
            &second,
            &[
                (
                    "Takeout/Google Photos/Beach/metadata.json",
                    br#"{"title":"Beach"}"#,
                ),
                ("Takeout/Google Photos/Beach/IMG_0001.JPG", &photo),
            ],
        );

        let report = import_google_takeout(
            &[first.clone(), second.clone()],
            root.path(),
            &db.conn,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.duplicates_collapsed, 1);

        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let db_arc = std::sync::Arc::new(tokio::sync::Mutex::new(db));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let scan_report = runtime.block_on(async {
            let (rx, task) = crate::services::scanner::start_scan(
                root.path().to_path_buf(),
                db_arc.clone(),
                cancel,
                false,
            );
            let drain = tokio::spawn(async move { while rx.recv().await.is_ok() {} });
            let report = task.await.unwrap();
            drain.await.unwrap();
            report
        });
        assert!(
            scan_report.errors.is_empty(),
            "scan errors: {:?}",
            scan_report.errors
        );
        let guard = runtime.block_on(db_arc.lock());
        let photo_count: i64 = guard
            .conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .unwrap();
        let ledger: Vec<(String, Option<String>)> = guard
            .conn
            .prepare("SELECT file_path, metadata_json FROM google_takeout_items")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(photo_count, 1, "ledger={ledger:?}");
        let (metadata, albums) = apply_takeout_metadata_and_albums(&guard.conn).unwrap();
        assert_eq!(metadata, 1);
        assert_eq!(albums, 1);
        let restored: (String, f64, i64) = guard
            .conn
            .query_row(
                "SELECT date_taken_source, gps_latitude, is_favorite FROM photos",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(restored.0, "google_takeout");
        assert_eq!(restored.1, 10.0);
        assert_eq!(restored.2, 1);
        let album_count: i64 = guard
            .conn
            .query_row("SELECT COUNT(*) FROM album_photos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(album_count, 1);

        let repeated = import_google_takeout(
            &[first, second],
            root.path(),
            &guard.conn,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(repeated.imported, 0);
        assert_eq!(repeated.reused_existing, 1);
        assert_eq!(repeated.duplicates_collapsed, 1);
    }

    #[test]
    fn sidecar_can_live_in_a_different_takeout_part() {
        let root = tempdir().unwrap();
        let first = root.path().join("takeout-001.zip");
        let second = root.path().join("takeout-002.zip");
        write_zip(
            &first,
            &[(
                "Takeout/Google Photos/Photos from 2020/a.jpg",
                &jpeg_bytes(2),
            )],
        );
        write_zip(
            &second,
            &[(
                "Takeout/Google Photos/Photos from 2020/a.jpg.json",
                br#"{"title":"a.jpg","photoTakenTime":{"timestamp":"1577836800"}}"#,
            )],
        );
        let plan =
            inspect_archives(&[first, second], &AtomicBool::new(false), |_, _, _| {}).unwrap();
        assert_eq!(plan.media.len(), 1);
        assert_eq!(
            plan.sidecars
                .get(&plan.media[0].sidecar_key)
                .and_then(|metadata| metadata.taken_at_unix),
            Some(1_577_836_800)
        );
    }

    #[test]
    fn cancellation_before_inspection_is_resumable() {
        let root = tempdir().unwrap();
        let db = crate::db::Database::open_for_drive(root.path()).unwrap();
        crate::db::create_schema(&db.conn).unwrap();
        let archive = root.path().join("takeout.zip");
        write_zip(
            &archive,
            &[(
                "Takeout/Google Photos/Photos from 2020/a.jpg",
                &jpeg_bytes(3),
            )],
        );
        let cancel = AtomicBool::new(true);
        let report =
            import_google_takeout(&[archive], root.path(), &db.conn, &cancel, |_| {}).unwrap();
        assert!(report.cancelled);
        assert_eq!(report.imported, 0);
    }

    #[test]
    fn rejects_zip_slip_paths() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.zip");
        write_zip(
            &path,
            &[("../Google Photos/Photos from 2024/a.jpg", &jpeg_bytes(1))],
        );
        let err = inspect_archives(&[path], &AtomicBool::new(false), |_, _, _| {}).unwrap_err();
        assert!(err.contains("Unsafe path"));
    }

    #[test]
    fn sanitizes_windows_names_and_collisions() {
        assert_eq!(sanitize_file_name("CON.jpg"), "_CON.jpg");
        assert_eq!(sanitize_file_name("bad:name?.jpg"), "bad_name_.jpg");
        assert_eq!(sanitize_file_name("  .  "), "photo");
        let long = format!("{}.jpeg", "🙂".repeat(100));
        let shortened = sanitize_file_name(&long);
        assert!(shortened.len() <= 180);
        assert!(shortened.ends_with(".jpeg"));
    }
}
