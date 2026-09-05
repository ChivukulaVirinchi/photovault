//! Map pins with server-side zoom-level clustering.

use std::collections::HashMap;

use serde::Deserialize;
use tauri::State;

use smriti::db::{db_path_for, open_secondary, PhotoRepo};

use crate::dto::{MapPinDto, PhotoSummaryDto};
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
        collect_pins(&conn, bounds, args.zoom, args.max_pins)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("map pins worker failed: {e}"),
    })?
}

fn collect_pins(
    conn: &rusqlite::Connection,
    bounds: QueryBounds,
    zoom: u8,
    max_pins: Option<u32>,
) -> CommandResult<Vec<MapPinDto>> {
    // Stream just the fields we need. Do not truncate before clustering:
    // doing so silently loses older photos and undercounts clusters.
    let mut stmt = conn.prepare(
        "SELECT id, gps_latitude, gps_longitude, thumbnail_path FROM photos
         WHERE is_trashed = FALSE AND gps_latitude BETWEEN ?1 AND ?2
         AND ((?3 <= ?4 AND gps_longitude BETWEEN ?3 AND ?4)
           OR (?3 > ?4 AND (gps_longitude >= ?3 OR gps_longitude <= ?4)))
         ORDER BY date_taken DESC, id DESC",
    )?;
    let mut rows = stmt.query(rusqlite::params![
        bounds.south,
        bounds.north,
        bounds.west,
        bounds.east
    ])?;
    let cell = cell_size_deg(zoom);
    let max_pins = max_pins.unwrap_or(1000).clamp(100, 5000) as usize;
    let mut singles = Vec::new();
    let mut cells: HashMap<(i64, i64), MapPinDto> = HashMap::new();
    let mut clustered = zoom <= 7;
    let mut add = |pin: MapPinDto| {
        let key = (
            (pin.lat / cell).floor() as i64,
            (pin.lng / cell).floor() as i64,
        );
        let id = pin.photo_id;
        let entry = cells.entry(key).or_insert_with(|| MapPinDto {
            lat: ((key.0 as f64 + 0.5) * cell).clamp(-90.0, 90.0),
            lng: ((key.1 as f64 + 0.5) * cell).clamp(-180.0, 180.0),
            count: 0,
            ..pin
        });
        entry.count = entry.count.saturating_add(1);
        if entry.photo_ids.len() < 500 {
            entry.photo_ids.push(id);
        }
    };
    while let Some(row) = rows.next()? {
        let pin = MapPinDto {
            photo_id: row.get(0)?,
            lat: row.get(1)?,
            lng: row.get(2)?,
            thumbnail_path: row.get(3)?,
            count: 1,
            photo_ids: Vec::new(),
        };
        if clustered {
            add(pin);
        } else {
            singles.push(pin);
            if singles.len() > max_pins {
                clustered = true;
                for pin in singles.drain(..) {
                    add(pin);
                }
            }
        }
    }
    if !clustered {
        return Ok(singles);
    }
    Ok(cells
        .into_values()
        .map(|mut pin| {
            if pin.count == 1 {
                pin.photo_ids.clear();
            }
            pin
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_include_rows_beyond_old_cap_and_bound_member_samples() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE photos(id INTEGER PRIMARY KEY, gps_latitude REAL,
             gps_longitude REAL, thumbnail_path TEXT, is_trashed INTEGER, date_taken TEXT);
             WITH RECURSIVE n(id) AS (VALUES(1) UNION ALL SELECT id+1 FROM n WHERE id<50001)
             INSERT INTO photos SELECT id, 10, 179, NULL, 0, '2026' FROM n;
             INSERT INTO photos VALUES(50002, 10, -179, NULL, 0, '2026');
             INSERT INTO photos VALUES(50003, 10, 0, NULL, 0, '2026');
             INSERT INTO photos VALUES(50004, 10, 179, NULL, 1, '2026');",
        )
        .unwrap();
        let pins = collect_pins(
            &conn,
            QueryBounds {
                north: 20.0,
                south: 0.0,
                west: 170.0,
                east: -170.0,
            },
            5,
            None,
        )
        .unwrap();
        assert_eq!(pins.iter().map(|p| p.count).sum::<u32>(), 50002);
        let cluster = pins.iter().find(|p| p.count > 1).unwrap();
        assert_eq!(cluster.count, 50001);
        assert_eq!(cluster.photo_ids.len(), 500);
        assert_eq!(cluster.photo_id, 50001);
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
