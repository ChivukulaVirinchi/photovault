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

#[tauri::command]
pub async fn geocoding_resolve_one(
    args: GeocodingResolveOneArgs,
) -> CommandResult<Option<LocationDto>> {
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

    tokio::task::spawn_blocking(move || {
        let result = backfill_inner(
            &app_clone,
            &job_id_clone,
            &drive_root,
            force_refresh,
            started,
            &cancel,
        );
        let dto = match result {
            Ok(dto) => dto,
            Err(e) => {
                tracing::error!("geocoding backfill failed: {}", e);
                GeocodingCompleteDto {
                    job_id: job_id_clone.clone(),
                    considered: 0,
                    updated: 0,
                    cleared: 0,
                    geonames_db_present: false,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    message: format!("Geocoding failed: {}", e),
                }
            }
        };
        emit(&app_clone, EV_GEOCODING_COMPLETE, dto);

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let app_for_finish = app_clone.clone();
            let job_id = job_id_clone.clone();
            handle.spawn(async move {
                let st: tauri::State<AppState> = app_for_finish.state();
                jobs::finish_job(&st, &job_id).await;
            });
        }
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
                    update_stmt
                        .execute(rusqlite::params![r.city, r.country, id])
                        .map_err(|e| e.to_string())?;
                    updated += 1;
                }
                None if force_refresh => {
                    clear_stmt
                        .execute(rusqlite::params![id])
                        .map_err(|e| e.to_string())?;
                    cleared += 1;
                }
                None => {}
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
    }
    tx.commit().map_err(|e| e.to_string())?;

    let message = if cancelled {
        format!(
            "Cancelled — {} resolved, {} cleared so far.",
            updated, cleared
        )
    } else {
        build_message(considered, updated, cleared, true)
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
