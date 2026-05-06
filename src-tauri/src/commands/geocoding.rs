//! Geocoding (read-only — single point resolution. Bulk run is M2).

use serde::Deserialize;

use photovault::services::geocoding::GeocodingService;

use crate::dto::LocationDto;
use crate::CommandResult;

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
    let svc = GeocodingService::new(&path).map_err(|e| crate::CommandError::Internal {
        message: e.to_string(),
    })?;
    Ok(svc.reverse_geocode(args.lat, args.lng).map(Into::into))
}
