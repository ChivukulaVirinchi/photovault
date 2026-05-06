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
    pub geonames_db_present: bool,
}

/// Walk every photo with GPS but no resolved location_city, run reverse
/// geocoding, and write the result back. One-shot pass for libraries
/// scanned before geocoding was wired up (or scanned with a missing
/// GeoNames DB).
#[tauri::command]
pub async fn geocoding_backfill(
    state: State<'_, AppState>,
) -> CommandResult<GeocodingBackfillResult> {
    let path = photovault::db::geonames::geonames_db_path();
    if !path.exists() {
        return Ok(GeocodingBackfillResult {
            considered: 0,
            updated: 0,
            geonames_db_present: false,
        });
    }
    let svc = GeocodingService::new(&path).map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?;

    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;

    // Pull every photo with GPS but no city. This is bounded by the
    // count of geotagged-but-not-yet-geocoded photos — typically small
    // unless the library was indexed without GeoNames available.
    let rows: Vec<(i64, f64, f64)> = {
        let mut stmt = db.conn.prepare(
            r#"
            SELECT id, gps_latitude, gps_longitude
            FROM photos
            WHERE is_trashed = FALSE
              AND gps_latitude  IS NOT NULL
              AND gps_longitude IS NOT NULL
              AND location_city IS NULL
            "#,
        )?;
        let r = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        r
    };

    let considered = rows.len() as u64;
    let mut updated = 0u64;

    // Single statement reused — avoid prepare-per-row overhead.
    let tx = db.conn.unchecked_transaction()?;
    {
        let mut stmt = tx
            .prepare("UPDATE photos SET location_city = ?1, location_country = ?2 WHERE id = ?3")?;
        for (id, lat, lng) in rows {
            if let Some(r) = svc.reverse_geocode(lat, lng) {
                stmt.execute(rusqlite::params![r.city, r.country, id])?;
                updated += 1;
            }
        }
    }
    tx.commit()?;

    Ok(GeocodingBackfillResult {
        considered,
        updated,
        geonames_db_present: true,
    })
}
