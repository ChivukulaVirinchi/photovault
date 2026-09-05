//! Geocoding — single-point resolution + library backfill.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::services::geocoding::GeocodingService;

use crate::dto::{JobIdDto, LocationDto};
use crate::events::{JobProgress, EV_GEOCODING_COMPLETE, EV_GEOCODING_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct GeocodingResolveOneArgs {
    pub lat: f64,
    pub lng: f64,
}

fn validate_lat_lng(lat: f64, lng: f64) -> CommandResult<()> {
    if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
        return Err(CommandError::Validation {
            field: "lat".into(),
            reason: "latitude must be between -90 and 90".into(),
        });
    }
    if !lng.is_finite() || !(-180.0..=180.0).contains(&lng) {
        return Err(CommandError::Validation {
            field: "lng".into(),
            reason: "longitude must be between -180 and 180".into(),
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn geocoding_resolve_one(
    args: GeocodingResolveOneArgs,
) -> CommandResult<Option<LocationDto>> {
    validate_lat_lng(args.lat, args.lng)?;
    let path = smriti::db::geonames::geonames_db_path();
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

#[derive(Debug, Serialize, Clone)]
pub struct GeocodingCompleteDto {
    pub job_id: String,
    pub considered: u64,
    pub updated: u64,
    pub cleared: u64,
    pub geonames_db_present: bool,
    pub elapsed_ms: u64,
    /// Pre-formatted summary the UI surfaces as a toast — see comment
    /// on `AlbumSuggestionsCompleteDto::message`.
    pub message: String,
}

fn build_message(considered: u64, updated: u64, cleared: u64, db_present: bool) -> String {
    if !db_present {
        return "GeoNames database not found. Run scripts/setup_assets.sh.".to_string();
    }
    if considered == 0 {
        return "No GPS-tagged photos to backfill.".to_string();
    }
    if cleared > 0 {
        return format!(
            "Resolved {}, cleared {} stale match{}.",
            updated,
            cleared,
            if cleared == 1 { "" } else { "es" }
        );
    }
    format!("Resolved {} of {} GPS-tagged photos.", updated, considered)
}

#[cfg(test)]
mod tests {
    use super::validate_lat_lng;
    use crate::CommandError;

    #[test]
    fn validate_lat_lng_rejects_non_finite_and_out_of_range() {
        assert!(validate_lat_lng(17.4, 78.5).is_ok());
        assert!(matches!(
            validate_lat_lng(f64::NAN, 78.5),
            Err(CommandError::Validation { .. })
        ));
        assert!(matches!(
            validate_lat_lng(91.0, 78.5),
            Err(CommandError::Validation { .. })
        ));
        assert!(matches!(
            validate_lat_lng(17.4, 181.0),
            Err(CommandError::Validation { .. })
        ));
    }
}

/// Re-geocode photos. With `force_refresh = false` (default) only
/// fills in NULL `location_city` rows. With `force_refresh = true`
/// re-resolves every GPS-tagged photo, overwriting prior values —
/// use this after the geocoder data or rules change to flush stale
/// attributions.
///
/// Runs in the background. The handler returns a job_id; final counts
/// arrive via the `geocoding:complete` event so the UI can navigate
/// freely while a backfill on a 50k-photo library is grinding through.
#[tauri::command]
pub async fn geocoding_backfill(
    app: AppHandle,
    state: State<'_, AppState>,
    args: GeocodingBackfillArgs,
) -> CommandResult<JobIdDto> {
    let _lifecycle = state.library_lifecycle.lock().await;
    if state.jobs.lock().await.has_any_of_kind(JobKind::Geocoding) {
        return Err(CommandError::Conflict {
            reason: "geocoding is already in progress".into(),
        });
    }

    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };

    let job = jobs::start_job(&state, JobKind::Geocoding).await?;
    let job_id = job.id.clone();
    let started = job.started_at;
    let cancel = job.cancel.clone();
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();
    let force_refresh = args.force_refresh;
    let finish_handle = tokio::runtime::Handle::current();
    let app_for_finish = app.clone();

    tokio::task::spawn_blocking(move || {
        let result = backfill_inner(
            &app_clone,
            &job_id_clone,
            &drive_root,
            force_refresh,
            started,
            &cancel,
        );
        match result {
            Ok(dto) => emit(&app_clone, EV_GEOCODING_COMPLETE, dto),
            Err(e) => {
                tracing::error!("geocoding backfill failed: {}", e);
                emit(
                    &app_clone,
                    EV_GEOCODING_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "error".into(),
                        processed: 0,
                        total: Some(1),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("Geocoding failed: {}", e)),
                    },
                );
            }
        };

        finish_handle.spawn(async move {
            let st: tauri::State<AppState> = app_for_finish.state();
            jobs::finish_job(&st, &job_id_clone).await;
        });
    });

    Ok(JobIdDto { job_id })
}

fn backfill_inner(
    app: &AppHandle,
    job_id: &str,
    drive_root: &std::path::Path,
    force_refresh: bool,
    started: std::time::Instant,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<GeocodingCompleteDto, String> {
    use std::sync::atomic::Ordering;
    let path = smriti::db::geonames::geonames_db_path();
    if !path.exists() {
        return Ok(GeocodingCompleteDto {
            job_id: job_id.to_string(),
            considered: 0,
            updated: 0,
            cleared: 0,
            geonames_db_present: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            message: build_message(0, 0, 0, false),
        });
    }
    // Schema check: older builds of geonames.db are missing the
    // `feature_code` column, which the v2 reverse_geocode logic needs
    // to rank city candidates. Rebuild from cities1000.txt in place
    // when the schema is stale and the source file is still on disk
    // (true for anyone who ran setup_assets at any point). Takes
    // ~10-30s on a typical machine; far better than failing silently
    // with a SQL error or asking the user to re-run setup_assets.
    if !smriti::db::geonames::geonames_schema_is_current() {
        tracing::info!(
            "geonames.db schema is stale (no feature_code column) — rebuilding in place"
        );
        emit(
            app,
            EV_GEOCODING_PROGRESS,
            JobProgress {
                job_id: job_id.to_string(),
                stage: "rebuild-geonames".into(),
                processed: 0,
                total: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some("Upgrading GeoNames database…".into()),
            },
        );
        let source_root = path
            .parent()
            .and_then(std::path::Path::parent)
            .filter(|root| {
                root.join("data").join("cities1000.txt").is_file()
                    && root.join("data").join("country_codes.txt").is_file()
            })
            .map(std::path::Path::to_path_buf)
            .or_else(|| {
                let root = smriti::bootstrap::project_root();
                (root.join("data").join("cities1000.txt").is_file()
                    && root.join("data").join("country_codes.txt").is_file())
                .then_some(root)
            })
            .ok_or_else(|| {
                "GeoNames database is out of date and its source data is unavailable. Use Set up assets on the Welcome screen or Download assets in Settings to replace it."
                    .to_string()
            })?;
        if let Err(e) = smriti::db::geonames::build_geonames_db(&source_root) {
            tracing::error!("geonames rebuild failed: {}", e);
            return Err(format!(
                "GeoNames database is out of date and rebuild failed: {}. Download assets again to refresh it.",
                e
            ));
        }
        if !smriti::db::geonames::geonames_db_is_current(&path) {
            return Err(
                "GeoNames rebuild completed at an unexpected location. Download assets again to repair it."
                    .to_string(),
            );
        }
        tracing::info!("geonames.db rebuilt successfully");
    }
    let svc = GeocodingService::new(&path).map_err(|e| e.to_string())?;
    let db_path = smriti::db::db_path_for(drive_root);
    let conn = smriti::db::open_secondary(&db_path).map_err(|e| e.to_string())?;

    let sql = if force_refresh {
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
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let r: rusqlite::Result<Vec<(i64, f64, f64)>> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect();
        r.map_err(|e| e.to_string())?
    };

    let considered = rows.len() as u64;
    let mut updated = 0u64;
    let mut cleared = 0u64;

    tracing::info!(
        "geocoding backfill: {} candidate photos (force_refresh={})",
        considered,
        force_refresh
    );

    emit(
        app,
        EV_GEOCODING_PROGRESS,
        JobProgress {
            job_id: job_id.to_string(),
            stage: "geocoding".into(),
            processed: 0,
            total: Some(considered),
            elapsed_ms: started.elapsed().as_millis() as u64,
            eta_ms: None,
            message: Some(format!("0 / {} photos", considered)),
        },
    );

    // Tally matched cities so we can include the top hits in the
    // completion toast. Helps the user verify the geocoder is producing
    // sensible results (and that the writes actually committed) without
    // having to crack open SQLite themselves.
    let mut city_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    let mut cancelled = false;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    {
        let mut update_stmt = tx
            .prepare("UPDATE photos SET location_city = ?1, location_country = ?2 WHERE id = ?3")
            .map_err(|e| e.to_string())?;
        let mut clear_stmt = tx
            .prepare(
                "UPDATE photos SET location_city = NULL, location_country = NULL WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut processed = 0u64;
        // Throttle progress events to one every 250 photos so we don't
        // flood the IPC channel on a 50k-photo library.
        let tick = considered.div_ceil(40).max(250);
        // Log a few sample resolutions so it's possible to confirm from
        // the dev console whether the geocoder is actually returning
        // hits or returning None for everything (e.g. coords stored as
        // null/zero, geonames.db missing rows for that region, etc.).
        let mut samples_logged = 0u32;
        const MAX_SAMPLES: u32 = 5;
        let mut updates_executed = 0u64;
        for (id, lat, lng) in rows {
            // Honour the cancel flag at every iteration. The current
            // batch's already-resolved rows still get committed by the
            // tx below — we just stop doing more work.
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            match svc.reverse_geocode(lat, lng) {
                Some(r) => {
                    if samples_logged < MAX_SAMPLES {
                        tracing::info!(
                            "geocode sample: photo_id={} ({:.4}, {:.4}) -> {} / {}",
                            id,
                            lat,
                            lng,
                            r.city,
                            r.country
                        );
                        samples_logged += 1;
                    }
                    let rows_changed = update_stmt
                        .execute(rusqlite::params![r.city, r.country, id])
                        .map_err(|e| e.to_string())?;
                    updates_executed += rows_changed as u64;
                    *city_counts.entry(r.city.clone()).or_insert(0) += 1;
                    updated += 1;
                }
                None if force_refresh => {
                    clear_stmt
                        .execute(rusqlite::params![id])
                        .map_err(|e| e.to_string())?;
                    cleared += 1;
                }
                None => {
                    if samples_logged < MAX_SAMPLES {
                        tracing::info!(
                            "geocode sample: photo_id={} ({:.4}, {:.4}) -> NO MATCH",
                            id,
                            lat,
                            lng
                        );
                        samples_logged += 1;
                    }
                }
            }
            processed += 1;
            if processed.is_multiple_of(tick) || processed == considered {
                emit(
                    app,
                    EV_GEOCODING_PROGRESS,
                    JobProgress {
                        job_id: job_id.to_string(),
                        stage: "geocoding".into(),
                        processed,
                        total: Some(considered),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("{updated} matched · {cleared} cleared")),
                    },
                );
            }
        }
        tracing::info!(
            "geocoding pre-commit: matched={} updates_actually_changed_rows={} cleared={}",
            updated,
            updates_executed,
            cleared
        );
    }
    tx.commit().map_err(|e| {
        tracing::error!("geocoding transaction commit failed: {}", e);
        e.to_string()
    })?;
    tracing::info!("geocoding transaction committed");

    // Build a top-cities summary for the toast. Sort by count desc,
    // take top 3.
    let top: Vec<(String, u64)> = {
        let mut v: Vec<(String, u64)> = city_counts.into_iter().collect();
        v.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        v.into_iter().take(3).collect()
    };

    let message = if cancelled {
        format!(
            "Cancelled — {} resolved, {} cleared so far.",
            updated, cleared
        )
    } else {
        let base = build_message(considered, updated, cleared, true);
        if !top.is_empty() {
            let suffix = top
                .iter()
                .map(|(c, n)| format!("{} ({})", c, n))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} Top: {}.", base, suffix)
        } else {
            base
        }
    };
    Ok(GeocodingCompleteDto {
        job_id: job_id.to_string(),
        considered,
        updated,
        cleared,
        geonames_db_present: true,
        elapsed_ms: started.elapsed().as_millis() as u64,
        message,
    })
}
