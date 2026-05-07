//! Geocoding — single-point resolution + library backfill.

use serde::{Deserialize, Serialize};
use tauri::State;

use photovault::services::geocoding::GeocodingService;

use crate::dto::LocationDto;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct GeocodingResolveOneArgs {
    pub lat: f64,
    pub lng: f64,
}

#[tauri::command]
pub async fn geocoding_resolve_one(
    args: GeocodingResolveOneArgs,
) -> CommandResult<Option<LocationDto>> {
    let path = photovault::db::geonames::geonames_db_path();
    if !path.exists() {
        return Ok(None);
    }
    let svc = GeocodingService::new(&path).map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?;
    Ok(svc.reverse_geocode(args.lat, args.lng).map(Into::into))
}

#[derive(Debug, Serialize)]
pub struct GeocodingBackfillResult {
    pub considered: u64,
    pub updated: u64,
    pub cleared: u64,
    pub geonames_db_present: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct GeocodingBackfillArgs {
    /// When true, re-resolve EVERY GPS-tagged photo (including those
    /// already attributed). Necessary to repair stale data after the
    /// geocoder rules change — without this we never overwrite the
    /// pre-fix wrong attributions.
    #[serde(default)]
    pub force_refresh: bool,
}

/// Re-geocode photos. With `force_refresh = false` (default) only
/// fills in NULL `location_city` rows. With `force_refresh = true`
/// re-resolves every GPS-tagged photo, overwriting prior values —
/// use this after the geocoder data or rules change to flush stale
/// attributions.
#[tauri::command]
pub async fn geocoding_backfill(
    state: State<'_, AppState>,
    args: GeocodingBackfillArgs,
) -> CommandResult<GeocodingBackfillResult> {
    let path = photovault::db::geonames::geonames_db_path();
    if !path.exists() {
        return Ok(GeocodingBackfillResult {
            considered: 0,
            updated: 0,
            cleared: 0,
            geonames_db_present: false,
        });
    }
    let svc = GeocodingService::new(&path).map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?;

    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;

    let sql = if args.force_refresh {
        r#"
        SELECT id, gps_latitude, gps_longitude
        FROM photos
        WHERE is_trashed = FALSE
          AND gps_latitude  IS NOT NULL
          AND gps_longitude IS NOT NULL
        "#
    } else {
        r#"
        SELECT id, gps_latitude, gps_longitude
        FROM photos
        WHERE is_trashed = FALSE
          AND gps_latitude  IS NOT NULL
          AND gps_longitude IS NOT NULL
          AND location_city IS NULL
        "#
    };

    let rows: Vec<(i64, f64, f64)> = {
        let mut stmt = db.conn.prepare(sql)?;
        let r: rusqlite::Result<Vec<(i64, f64, f64)>> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?
            .collect();
        r?
    };

    let considered = rows.len() as u64;
    let mut updated = 0u64;
    let mut cleared = 0u64;

    let tx = db.conn.unchecked_transaction()?;
    {
        // Two statements: one to set fresh values, one to clear stale
        // entries when the new geocoder rejects a previously-attributed
        // photo (e.g., the 50 km cutoff dropped a far-away match).
        let mut update_stmt = tx
            .prepare("UPDATE photos SET location_city = ?1, location_country = ?2 WHERE id = ?3")?;
        let mut clear_stmt = tx.prepare(
            "UPDATE photos SET location_city = NULL, location_country = NULL WHERE id = ?1",
        )?;
        for (id, lat, lng) in rows {
            match svc.reverse_geocode(lat, lng) {
                Some(r) => {
                    update_stmt.execute(rusqlite::params![r.city, r.country, id])?;
                    updated += 1;
                }
                None if args.force_refresh => {
                    // No match under the new rules — clear any stale value
                    // so the user isn't shown wrong place names.
                    clear_stmt.execute(rusqlite::params![id])?;
                    cleared += 1;
                }
                None => {}
            }
        }
    }
    tx.commit()?;

    Ok(GeocodingBackfillResult {
        considered,
        updated,
        cleared,
        geonames_db_present: true,
    })
}
