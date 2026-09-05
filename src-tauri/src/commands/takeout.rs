//! Google Photos Takeout migration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::dto::JobIdDto;
use crate::events::{EV_TAKEOUT_COMPLETE, EV_TAKEOUT_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[derive(Debug, Deserialize)]
pub struct TakeoutStartImportArgs {
    pub archive_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TakeoutEvent {
    pub job_id: String,
    pub stage: String,
    pub processed: u64,
    pub total: Option<u64>,
    pub elapsed_ms: u64,
    pub message: Option<String>,
    pub archives: u64,
    pub media_found: u64,
    pub imported: u64,
    pub reused_existing: u64,
    pub duplicates_collapsed: u64,
    pub unsupported_or_small: u64,
    pub unmatched_sidecars: u64,
    pub albums_restored: u64,
    pub metadata_restored: u64,
    pub error_count: usize,
    pub cancelled: bool,
}

impl TakeoutEvent {
    fn from_report(
        job_id: &str,
        stage: &str,
        message: Option<String>,
        report: &smriti::services::takeout_import::TakeoutImportReport,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            job_id: job_id.to_string(),
            stage: stage.to_string(),
            processed: report.media_found,
            total: Some(report.media_found),
            elapsed_ms,
            message,
            archives: report.archives,
            media_found: report.media_found,
            imported: report.imported,
            reused_existing: report.reused_existing,
            duplicates_collapsed: report.duplicates_collapsed,
            unsupported_or_small: report.unsupported_or_small,
            unmatched_sidecars: report.unmatched_sidecars,
            albums_restored: report.albums_restored,
            metadata_restored: report.metadata_restored,
            error_count: report.errors.len(),
            cancelled: report.cancelled,
        }
    }
}

#[tauri::command]
pub async fn takeout_start_import(
    app: AppHandle,
    state: State<'_, AppState>,
    args: TakeoutStartImportArgs,
) -> CommandResult<JobIdDto> {
    let _lifecycle = state.library_lifecycle.lock().await;
    if args.archive_paths.is_empty() {
        return Err(CommandError::Validation {
            field: "archive_paths".into(),
            reason: "select at least one Google Takeout ZIP file".into(),
        });
    }
    let archive_paths: Vec<PathBuf> = args
        .archive_paths
        .iter()
        .map(|raw| PathBuf::from(raw.trim()))
        .collect();
    for path in &archive_paths {
        if !path.is_file() {
            return Err(CommandError::Validation {
                field: "archive_paths".into(),
                reason: format!("archive does not exist: {}", path.display()),
            });
        }
        if path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some("zip")
        {
            return Err(CommandError::Validation {
                field: "archive_paths".into(),
                reason: format!(
                    "only ZIP Takeout archives are supported: {}",
                    path.display()
                ),
            });
        }
    }

    let (drive_root, db, thumbnails) = {
        let guard = state.library.read().await;
        let lib = guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            lib.drive_root.clone(),
            lib.db.clone(),
            lib.thumbnails.clone(),
        )
    };
    {
        let jobs = state.jobs.lock().await;
        if jobs.has_any_of_kind(JobKind::Scan)
            || jobs.has_any_of_kind(JobKind::MetadataExtraction)
            || jobs.has_any_of_kind(JobKind::GoogleTakeoutImport)
        {
            return Err(CommandError::Conflict {
                reason: "finish the current scan, metadata pass, or Takeout import first".into(),
            });
        }
    }

    let job = jobs::start_job(&state, JobKind::GoogleTakeoutImport).await?;
    let job_id = job.id.clone();
    let return_id = job_id.clone();
    let cancel = job.cancel.clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let db_path = smriti::db::db_path_for(&drive_root);
        let drive_for_import = drive_root.clone();
        let progress_app = app_clone.clone();
        let progress_id = job_id.clone();
        let cancel_for_import = cancel.clone();
        let import_result = tokio::task::spawn_blocking(move || {
            let conn = smriti::db::open_secondary(&db_path)
                .map_err(|e| format!("Could not open the library database: {e}"))?;
            smriti::services::takeout_import::import_google_takeout(
                &archive_paths,
                &drive_for_import,
                &conn,
                &cancel_for_import,
                |progress| {
                    emit(
                        &progress_app,
                        EV_TAKEOUT_PROGRESS,
                        TakeoutEvent {
                            job_id: progress_id.clone(),
                            stage: progress.stage.to_string(),
                            processed: progress.processed,
                            total: Some(progress.total),
                            elapsed_ms: (progress.elapsed_seconds * 1000.0) as u64,
                            message: Some(progress.message),
                            ..Default::default()
                        },
                    );
                },
            )
        })
        .await;

        let mut report = match import_result {
            Ok(Ok(report)) => report,
            Ok(Err(message)) => {
                emit(
                    &app_clone,
                    EV_TAKEOUT_COMPLETE,
                    TakeoutEvent {
                        job_id: job_id.clone(),
                        stage: "error".into(),
                        message: Some(message),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        error_count: 1,
                        ..Default::default()
                    },
                );
                finish(&app_clone, &job_id).await;
                return;
            }
            Err(error) => {
                emit(
                    &app_clone,
                    EV_TAKEOUT_COMPLETE,
                    TakeoutEvent {
                        job_id: job_id.clone(),
                        stage: "error".into(),
                        message: Some(format!("Takeout import worker failed: {error}")),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        error_count: 1,
                        ..Default::default()
                    },
                );
                finish(&app_clone, &job_id).await;
                return;
            }
        };

        if report.cancelled || cancel.load(std::sync::atomic::Ordering::Relaxed) {
            report.cancelled = true;
            emit(
                &app_clone,
                EV_TAKEOUT_COMPLETE,
                TakeoutEvent::from_report(
                    &job_id,
                    "cancelled",
                    Some("Import cancelled. Run it again to resume.".into()),
                    &report,
                    started.elapsed().as_millis() as u64,
                ),
            );
            finish(&app_clone, &job_id).await;
            return;
        }

        emit(
            &app_clone,
            EV_TAKEOUT_PROGRESS,
            TakeoutEvent {
                job_id: job_id.clone(),
                stage: "index".into(),
                total: Some(report.media_found),
                message: Some("Adding imported files to the library".into()),
                elapsed_ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            },
        );
        let (scan_rx, scan_task) = smriti::services::scanner::start_scan(
            drive_root.clone(),
            db.clone(),
            cancel.clone(),
            false,
        );
        while let Ok(scan) = scan_rx.recv().await {
            emit(
                &app_clone,
                EV_TAKEOUT_PROGRESS,
                TakeoutEvent {
                    job_id: job_id.clone(),
                    stage: "index".into(),
                    processed: scan.files_processed,
                    total: Some(scan.files_found),
                    message: Some(scan.current_file),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    ..Default::default()
                },
            );
        }
        let _ = scan_task.await;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            report.cancelled = true;
            emit(
                &app_clone,
                EV_TAKEOUT_COMPLETE,
                TakeoutEvent::from_report(
                    &job_id,
                    "cancelled",
                    Some("Import cancelled. Run it again to resume.".into()),
                    &report,
                    started.elapsed().as_millis() as u64,
                ),
            );
            finish(&app_clone, &job_id).await;
            return;
        }

        let (metadata_rx, metadata_task) = smriti::services::metadata_processor::start_metadata_job(
            drive_root.clone(),
            db.clone(),
            cancel.clone(),
        );
        while let Ok(metadata) = metadata_rx.recv().await {
            emit(
                &app_clone,
                EV_TAKEOUT_PROGRESS,
                TakeoutEvent {
                    job_id: job_id.clone(),
                    stage: "metadata".into(),
                    processed: metadata.done,
                    total: Some(metadata.total),
                    message: Some("Reading photo metadata".into()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    ..Default::default()
                },
            );
        }
        let _ = metadata_task.await;

        let db_path = smriti::db::db_path_for(&drive_root);
        match tokio::task::spawn_blocking(move || {
            let conn = smriti::db::open_secondary(&db_path)
                .map_err(|e| format!("Could not reopen library database: {e}"))?;
            smriti::services::takeout_import::apply_takeout_metadata_and_albums(&conn)
        })
        .await
        {
            Ok(Ok((metadata, albums))) => {
                report.metadata_restored = metadata;
                report.albums_restored = albums;
            }
            Ok(Err(message)) => report.errors.push(message),
            Err(error) => report
                .errors
                .push(format!("Metadata restoration worker failed: {error}")),
        }

        let stage = if report.errors.is_empty() {
            "complete"
        } else {
            "complete_with_warnings"
        };
        let message = format!(
            "Imported {} new · reused {} · collapsed {} duplicate{} · restored {} metadata records and {} album{} · skipped {} · unmatched metadata {} · warnings {}",
            report.imported,
            report.reused_existing,
            report.duplicates_collapsed,
            if report.duplicates_collapsed == 1 { "" } else { "s" },
            report.metadata_restored,
            report.albums_restored,
            if report.albums_restored == 1 { "" } else { "s" },
            report.unsupported_or_small,
            report.unmatched_sidecars,
            report.errors.len(),
        );
        emit(
            &app_clone,
            EV_TAKEOUT_COMPLETE,
            TakeoutEvent::from_report(
                &job_id,
                stage,
                Some(message),
                &report,
                started.elapsed().as_millis() as u64,
            ),
        );
        finish(&app_clone, &job_id).await;

        if !report.cancelled {
            super::library::run_post_scan_pipeline(app_clone, drive_root, db, thumbnails).await;
        }
    });

    Ok(JobIdDto { job_id: return_id })
}

async fn finish(app: &AppHandle, job_id: &str) {
    let state: tauri::State<AppState> = app.state();
    jobs::finish_job(&state, job_id).await;
}
