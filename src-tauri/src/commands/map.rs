//! Map: pins (with server-side zoom-level clustering) and tile cache stats.

use std::collections::HashMap;

use serde::Deserialize;
use tauri::State;

use smriti::db::{db_path_for, open_secondary, PhotoRepo};

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

fn validate_bounds(bounds: &BoundsDto) -> CommandResult<()> {
    let values = [
        ("north", bounds.north),
        ("south", bounds.south),
        ("east", bounds.east),
        ("west", bounds.west),
    ];
    for (field, value) in values {
        if !value.is_finite() {
            return Err(CommandError::Validation {
                field: format!("bounds.{field}"),
                reason: "must be finite".into(),
            });
        }
    }
    if !(-90.0..=90.0).contains(&bounds.north)
        || !(-90.0..=90.0).contains(&bounds.south)
        || bounds.south > bounds.north
    {
        return Err(CommandError::Validation {
            field: "bounds".into(),
            reason: "latitude bounds must be within -90..90 and south <= north".into(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct QueryBounds {
    north: f64,
    south: f64,
    east: f64,
    west: f64,
}

fn normalize_lng(lng: f64) -> f64 {
    let normalized = (lng + 180.0).rem_euclid(360.0) - 180.0;
    if normalized == -180.0 && lng > 0.0 {
        180.0
    } else {
        normalized
    }
}

fn query_bounds(bounds: &BoundsDto) -> CommandResult<QueryBounds> {
    validate_bounds(bounds)?;

    if (-180.0..=180.0).contains(&bounds.west)
        && (-180.0..=180.0).contains(&bounds.east)
        && bounds.west > bounds.east
    {
        return Ok(QueryBounds {
            north: bounds.north,
            south: bounds.south,
            east: bounds.east,
            west: bounds.west,
        });
    }

    let span = bounds.east - bounds.west;
    let (west, east) = if span >= 360.0 {
        (-180.0, 180.0)
    } else {
        let west = normalize_lng(bounds.west);
        let east_unwrapped = west + span;
        let east = if east_unwrapped > 180.0 {
            east_unwrapped - 360.0
        } else {
            east_unwrapped
        };
        (west, east)
    };

    Ok(QueryBounds {
        north: bounds.north,
        south: bounds.south,
        east,
        west,
    })
}

/// Snap a (lat, lng) to a grid cell whose size depends on zoom.
///
/// Higher zoom → smaller cells → less aggregation (more pins resolve).
/// At zoom 22 the cell is ~9e-5° (a few metres) — effectively no clustering.
fn cell_size_deg(zoom: u8) -> f64 {
    // 360 / 2^(zoom + 2): 4×4 cells per tile.
    let zoom = zoom.min(22);
    360.0 / (1u64 << (zoom as u32 + 2)) as f64
}

#[tauri::command]
pub async fn map_pins(
    state: State<'_, AppState>,
    args: MapPinsArgs,
) -> CommandResult<Vec<MapPinDto>> {
    let bounds = query_bounds(&args.bounds)?;
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        db_path_for(&lib.drive_root)
    };

    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let repo = PhotoRepo::new(&conn);

        // Hard upper bound for SQL; clustering operates on this snapshot.
        let cap = 50_000i64;
        let raw = repo.list_in_bounds(bounds.north, bounds.south, bounds.east, bounds.west, cap)?;

        // Always cluster at low zoom (≤7 = country / continent level), even
        // for small libraries. Otherwise a few hundred geotagged photos
        // render as individual thumb pins splattered across a continent —
        // visually noisy and useless. At higher zooms we fall back to
        // "single pins until pin_count exceeds max_pins".
        let zoom = args.zoom.min(22);
        let max_pins = args.max_pins.unwrap_or(1000).clamp(100, 5_000) as usize;
        let force_cluster = zoom <= 7;
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
        let cell = cell_size_deg(zoom);
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
            .map(|((kx, ky), mut acc)| {
                // Cell center: (kx + 0.5) * cell_size_deg.
                let cell_lat = (kx as f64 + 0.5) * cell;
                let cell_lng = (ky as f64 + 0.5) * cell;
                let count = acc.photo_ids.len() as u32;
                // Leave photo_ids empty for single-photo "clusters" — the
                // frontend renders those as regular thumb pins, no
                // filmstrip needed.
                let photo_ids = if count > 1 {
                    acc.photo_ids.truncate(500);
                    acc.photo_ids
                } else {
                    Vec::new()
                };
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
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("map pins worker failed: {e}"),
    })?
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
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        db_path_for(&lib.drive_root)
    };

    // Direct SQL — bypassing list_in_bounds since we want every
    // GPS-tagged photo regardless of viewport.
    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let mut stmt = conn.prepare(
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
        let pins = stmt
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
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok::<_, CommandError>(pins)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("map pins worker failed: {e}"),
    })?
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
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        db_path_for(&lib.drive_root)
    };
    let ids: Vec<i64> = args.photo_ids.into_iter().take(500).collect();
    let photos = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        Ok::<_, CommandError>(PhotoRepo::new(&conn).get_by_ids(&ids)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("map filmstrip worker failed: {e}"),
    })??;
    Ok(photos.iter().map(PhotoSummaryDto::from).collect())
}

#[tauri::command]
pub async fn map_tile_cache_stats(state: State<'_, AppState>) -> CommandResult<TileCacheStatsDto> {
    let dir = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        smriti::db::tile_cache_dir(&lib.drive_root)
    };
    let (size_bytes, file_count) = tauri::async_runtime::spawn_blocking(move || {
        let mut size_bytes: u64 = 0;
        let mut file_count: u64 = 0;
        if dir.exists() {
            for path in walk_files(&dir) {
                if let Ok(meta) = std::fs::metadata(path) {
                    size_bytes = size_bytes.saturating_add(meta.len());
                    file_count += 1;
                }
            }
        }
        (size_bytes, file_count)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("tile cache stats worker failed: {e}"),
    })?;
    let cfg = smriti::config::AppConfig::load();
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
    let mut cfg = smriti::config::AppConfig::load();
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
    let dir = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        smriti::db::tile_cache_dir(&lib.drive_root)
    };
    let freed = tauri::async_runtime::spawn_blocking(move || {
        let mut freed: u64 = 0;
        if dir.exists() {
            for path in walk_files(&dir) {
                if let Ok(meta) = std::fs::metadata(&path) {
                    freed = freed.saturating_add(meta.len());
                }
                let _ = std::fs::remove_file(&path);
            }
            remove_empty_dirs(&dir);
        }
        freed
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("tile cache clear worker failed: {e}"),
    })?;
    Ok(MapTileCacheClearedDto { freed_bytes: freed })
}

fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => files.extend(walk_files(&path)),
            Ok(t) if t.is_file() => files.push(path),
            _ => {}
        }
    }
    files
}

fn remove_empty_dirs(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            remove_empty_dirs(&path);
            let _ = std::fs::remove_dir(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_files_finds_nested_tile_cache_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("tile_cache");
        let nested = root.join("12").join("2201");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("1320.png"), b"tile").unwrap();
        std::fs::write(root.join("manifest.txt"), b"meta").unwrap();

        let mut files = walk_files(&root);
        files.sort();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&nested.join("1320.png")));
        assert!(files.contains(&root.join("manifest.txt")));
    }

    #[test]
    fn remove_empty_dirs_keeps_cache_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("tile_cache");
        let nested = root.join("12").join("2201");
        std::fs::create_dir_all(&nested).unwrap();

        remove_empty_dirs(&root);

        assert!(root.exists());
        assert!(!root.join("12").exists());
    }

    #[test]
    fn cell_size_clamps_extreme_zoom() {
        assert_eq!(cell_size_deg(u8::MAX), cell_size_deg(22));
    }

    #[test]
    fn validate_bounds_rejects_invalid_ranges() {
        assert!(query_bounds(&BoundsDto {
            north: 10.0,
            south: -10.0,
            east: 20.0,
            west: -20.0,
        })
        .is_ok());

        assert!(query_bounds(&BoundsDto {
            north: f64::NAN,
            south: -10.0,
            east: 20.0,
            west: -20.0,
        })
        .is_err());

        assert!(query_bounds(&BoundsDto {
            north: -10.0,
            south: 10.0,
            east: 20.0,
            west: -20.0,
        })
        .is_err());
    }

    #[test]
    fn query_bounds_normalizes_unwrapped_longitudes() {
        let wrapped = query_bounds(&BoundsDto {
            north: 10.0,
            south: -10.0,
            east: 190.0,
            west: 170.0,
        })
        .unwrap();
        assert_eq!(wrapped.west, 170.0);
        assert_eq!(wrapped.east, -170.0);

        let shifted = query_bounds(&BoundsDto {
            north: 10.0,
            south: -10.0,
            east: 120.0,
            west: -220.0,
        })
        .unwrap();
        assert_eq!(shifted.west, 140.0);
        assert_eq!(shifted.east, 120.0);

        let full_world = query_bounds(&BoundsDto {
            north: 10.0,
            south: -10.0,
            east: 220.0,
            west: -220.0,
        })
        .unwrap();
        assert_eq!(full_world.west, -180.0);
        assert_eq!(full_world.east, 180.0);
    }
}
