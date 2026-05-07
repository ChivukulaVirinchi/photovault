//! Map: pins (with server-side zoom-level clustering) and tile cache stats.

use std::collections::HashMap;

use serde::Deserialize;
use tauri::State;

use photovault::db::PhotoRepo;

use crate::dto::{MapPinDto, PhotoSummaryDto, TileCacheStatsDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct BoundsDto {
    pub north: f64,
    pub south: f64,
    pub east: f64,
    pub west: f64,
}

#[derive(Debug, Deserialize)]
pub struct MapPinsArgs {
    pub bounds: BoundsDto,
    /// MapLibre integer zoom (0 = whole world, ~22 = building level).
    pub zoom: u8,
    /// Optional soft cap on raw pin count before we switch to clustering.
    pub max_pins: Option<u32>,
}

/// Snap a (lat, lng) to a grid cell whose size depends on zoom.
///
/// Higher zoom → smaller cells → less aggregation (more pins resolve).
/// At zoom 22 the cell is ~9e-5° (a few metres) — effectively no clustering.
fn cell_size_deg(zoom: u8) -> f64 {
    // 360 / 2^(zoom + 2): 4×4 cells per tile.
    360.0 / (1u64 << (zoom as u32 + 2)) as f64
}

#[tauri::command]
pub async fn map_pins(
    state: State<'_, AppState>,
    args: MapPinsArgs,
) -> CommandResult<Vec<MapPinDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoRepo::new(&db.conn);

    // Hard upper bound for SQL; clustering operates on this snapshot.
    let cap = 50_000i64;
    let raw = repo.list_in_bounds(
        args.bounds.north,
        args.bounds.south,
        args.bounds.east,
        args.bounds.west,
        cap,
    )?;

    // Always cluster at low zoom (≤7 = country / continent level), even
    // for small libraries. Otherwise a few hundred geotagged photos
    // render as individual thumb pins splattered across a continent —
    // visually noisy and useless. At higher zooms we fall back to
    // "single pins until pin_count exceeds max_pins".
    let max_pins = args.max_pins.unwrap_or(1000) as usize;
    let force_cluster = args.zoom <= 7;
    if !force_cluster && raw.len() <= max_pins {
        return Ok(raw
            .into_iter()
            .filter_map(|p| {
                let lat = p.gps_latitude?;
                let lng = p.gps_longitude?;
                Some(MapPinDto {
                    photo_id: p.id,
                    lat,
                    lng,
                    thumbnail_path: p.thumbnail_path,
                    count: 1,
                    photo_ids: Vec::new(),
                })
            })
            .collect());
    }

    // Cluster server-side: snap to grid, count members, remember
    // member ids for the filmstrip drawer. The pin's lat/lng is set
    // to the **cell center** so adjacent clusters never visually
    // collide along the representative photo's exact coords.
    let cell = cell_size_deg(args.zoom);
    type Key = (i64, i64);
    struct CellAcc {
        thumb: Option<String>,
        rep_id: i64,
        photo_ids: Vec<i64>,
    }
    let mut cells: HashMap<Key, CellAcc> = HashMap::new();

    for p in raw.iter() {
        let (Some(lat), Some(lng)) = (p.gps_latitude, p.gps_longitude) else {
            continue;
        };
        let key: Key = ((lat / cell).floor() as i64, (lng / cell).floor() as i64);
        let entry = cells.entry(key).or_insert_with(|| CellAcc {
            thumb: p.thumbnail_path.clone(),
            rep_id: p.id,
            photo_ids: Vec::new(),
        });
        entry.photo_ids.push(p.id);
        // Keep the newest member as representative (raw is ordered
        // date_taken DESC, so first-seen is newest).
    }

    Ok(cells
        .into_iter()
        .map(|((kx, ky), acc)| {
            // Cell center: (kx + 0.5) * cell_size_deg.
            let cell_lat = (kx as f64 + 0.5) * cell;
            let cell_lng = (ky as f64 + 0.5) * cell;
            let count = acc.photo_ids.len() as u32;
            // Leave photo_ids empty for single-photo "clusters" — the
            // frontend renders those as regular thumb pins, no
            // filmstrip needed.
            let photo_ids = if count > 1 { acc.photo_ids } else { Vec::new() };
            MapPinDto {
                photo_id: acc.rep_id,
                lat: cell_lat,
                lng: cell_lng,
                thumbnail_path: acc.thumb,
                count,
                photo_ids,
            }
        })
        .collect())
}

/// Return every geotagged photo (lat/lng + thumb) in one shot.
///
/// Lets the frontend cluster client-side via supercluster.js, which
/// updates instantly on every zoom — no IPC roundtrip per zoom, no
/// snap-back glitch where markers sit at the previous zoom's positions
/// while a new query is in flight. Capped at 100k pins (anything
/// bigger needs a more sophisticated scheme; a 100k-pin library is
/// already ~8 MB on the wire which is fine but not infinite).
#[tauri::command]
pub async fn map_pins_all(state: State<'_, AppState>) -> CommandResult<Vec<MapPinDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;

    // Direct SQL — bypassing list_in_bounds since we want every
    // GPS-tagged photo regardless of viewport.
    let mut stmt = db.conn.prepare(
        r#"
        SELECT id, gps_latitude, gps_longitude, thumbnail_path
          FROM photos
         WHERE is_trashed = FALSE
           AND gps_latitude  IS NOT NULL
           AND gps_longitude IS NOT NULL
         ORDER BY date_taken DESC
         LIMIT 100000
        "#,
    )?;
    let pins: Vec<MapPinDto> = stmt
        .query_map([], |row| {
            Ok(MapPinDto {
                photo_id: row.get(0)?,
                lat: row.get::<_, f64>(1)?,
                lng: row.get::<_, f64>(2)?,
                thumbnail_path: row.get(3)?,
                count: 1,
                photo_ids: Vec::new(),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(pins)
}

#[derive(Debug, Deserialize)]
pub struct MapClusterFilmstripArgs {
    pub photo_ids: Vec<i64>,
}

#[tauri::command]
pub async fn map_cluster_filmstrip(
    state: State<'_, AppState>,
    args: MapClusterFilmstripArgs,
) -> CommandResult<Vec<PhotoSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = PhotoRepo::new(&db.conn);
    let ids: Vec<i64> = args.photo_ids.into_iter().take(500).collect();
    let photos = repo.get_by_ids(&ids)?;
    Ok(photos.iter().map(PhotoSummaryDto::from).collect())
}

#[tauri::command]
pub async fn map_tile_cache_stats(state: State<'_, AppState>) -> CommandResult<TileCacheStatsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let dir = photovault::db::tile_cache_dir(&lib.drive_root);
    let mut size_bytes: u64 = 0;
    let mut file_count: u64 = 0;
    if dir.exists() {
        for entry in walk(&dir) {
            if let Ok(meta) = entry.metadata() {
                size_bytes += meta.len();
                file_count += 1;
            }
        }
    }
    let cfg = photovault::config::AppConfig::load();
    let limit_bytes = (cfg.map_cache_limit_mb as u64) * 1024 * 1024;
    Ok(TileCacheStatsDto {
        size_bytes,
        file_count,
        limit_bytes,
    })
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct MapTileCacheSetLimitArgs {
    pub limit_mb: u32,
}

#[tauri::command]
pub async fn map_tile_cache_set_limit(args: MapTileCacheSetLimitArgs) -> CommandResult<()> {
    let mut cfg = photovault::config::AppConfig::load();
    cfg.map_cache_limit_mb = args.limit_mb.clamp(50, 10_000);
    cfg.save().map_err(|e| CommandError::Io {
        message: e.to_string(),
    })?;
    Ok(())
}

#[derive(Debug, serde::Serialize)]
pub struct MapTileCacheClearedDto {
    pub freed_bytes: u64,
}

#[tauri::command]
pub async fn map_tile_cache_clear(
    state: State<'_, AppState>,
) -> CommandResult<MapTileCacheClearedDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let dir = photovault::db::tile_cache_dir(&lib.drive_root);
    let mut freed: u64 = 0;
    if dir.exists() {
        for entry in walk(&dir) {
            if let Ok(meta) = entry.metadata() {
                freed += meta.len();
            }
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(MapTileCacheClearedDto { freed_bytes: freed })
}

fn walk(dir: &std::path::Path) -> impl Iterator<Item = std::fs::DirEntry> + '_ {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
}
