//! Semantic search model install, indexing, and photo similarity.

use std::sync::atomic::Ordering;
use std::time::Instant;

use tauri::{AppHandle, Manager, State};

use smriti::db::connection::{db_path_for, open_secondary};
use smriti::services::semantic::SemanticSearchService;

use crate::dto::{JobIdDto, SemanticStatusDto};
use crate::events::{JobProgress, EV_SEMANTIC_COMPLETE, EV_SEMANTIC_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn semantic_status(state: State<'_, AppState>) -> CommandResult<SemanticStatusDto> {
    let (drive_root, db_path) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), db_path_for(&lib.drive_root))
    };
    let status = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        Ok::<_, CommandError>(SemanticSearchService::new(&drive_root).status(&conn)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("semantic status worker failed: {e}"),
    })??;
    Ok(status.into())
}

#[tauri::command]
pub async fn semantic_start_indexing(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    start_semantic_indexing_job(app, &state).await
}

async fn start_semantic_indexing_job(app: AppHandle, state: &AppState) -> CommandResult<JobIdDto> {
    let _lifecycle = state.library_lifecycle.lock().await;
    if state
        .jobs
        .lock()
        .await
        .has_any_of_kind(JobKind::SemanticIndex)
    {
        return Err(CommandError::Conflict {
            reason: "semantic indexing is already in progress".into(),
        });
    }

    let (drive_root, db_path) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (lib.drive_root.clone(), db_path_for(&lib.drive_root))
    };
    let status_drive = drive_root.clone();
    let status_db_path = db_path.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&status_db_path)?;
        Ok::<_, CommandError>(SemanticSearchService::new(&status_drive).status(&conn)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("semantic status worker failed: {e}"),
    })??;
    if !status.assets_installed {
        return Err(CommandError::MlUnavailable {
            reason: "Install the visual search model from Settings -> Assets first.".into(),
        });
    }
    if !status.onnx_runtime_installed {
        return Err(CommandError::MlUnavailable {
            reason: "Enable visual search in Search or Settings to install the runtime and model."
                .into(),
        });
    }
    let job = jobs::start_job(state, JobKind::SemanticIndex).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let worker_app = app_clone.clone();
        let worker_job_id = job_id_clone.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = open_secondary(&db_path).map_err(|e| e.to_string())?;
            let svc = SemanticSearchService::new(&drive_root);
            let mut runner = SemanticSearchService::image_runner()?;
            let mut processed = 0u64;
            let total = svc.index_stats(&conn).map_err(|e| e.to_string())?.pending;
            let started_inner = Instant::now();

            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok::<String, String>("Semantic indexing cancelled".into());
                }
                let outcome = svc.index_next_batch(&mut conn, &mut runner, 16, &cancel)?;
                if outcome.done {
                    return Ok("Semantic indexing complete".into());
                }
                processed += outcome.processed;
                emit(
                    &worker_app,
                    EV_SEMANTIC_PROGRESS,
                    JobProgress {
                        job_id: worker_job_id.clone(),
                        stage: "index".into(),
                        processed,
                        total: Some(total),
                        elapsed_ms: started_inner.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(if outcome.failed == 0 {
                            "Indexing visual meaning".into()
                        } else {
                            format!("Indexing visual meaning ({} failed)", outcome.failed)
                        }),
                    },
                );
            }
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);

        match result {
            Ok(msg) => emit(
                &app_clone,
                EV_SEMANTIC_COMPLETE,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "index-complete".into(),
                    processed: 1,
                    total: Some(1),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(msg),
                },
            ),
            Err(err) => emit(
                &app_clone,
                EV_SEMANTIC_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "error".into(),
                    processed: 0,
                    total: Some(1),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(format!("Semantic indexing failed: {err}")),
                },
            ),
        }

        let st: tauri::State<AppState> = app_clone.state();
        jobs::finish_job(&st, &job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}

/// Start indexing pending photos when the user has installed the local model.
/// This is intentionally best-effort: opening a library must never fail because
/// an optional background index could not be started.
pub(crate) async fn maybe_start_semantic_indexing(app: AppHandle) {
    if !SemanticSearchService::model_assets_installed() || !smriti::bootstrap::onnx_runtime_exists()
    {
        return;
    }
    let state: tauri::State<AppState> = app.state();
    if state
        .jobs
        .lock()
        .await
        .has_any_of_kind(JobKind::SemanticIndex)
    {
        return;
    }
    let pending = {
        let library = state.library.read().await;
        let Some(library) = library.as_ref() else {
            return;
        };
        let drive_root = library.drive_root.clone();
        let db_path = db_path_for(&drive_root);
        tauri::async_runtime::spawn_blocking(move || {
            let conn = open_secondary(&db_path).ok()?;
            SemanticSearchService::new(&drive_root)
                .index_stats(&conn)
                .ok()
                .map(|stats| stats.pending)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
    };
    if pending == 0 {
        return;
    }
    if let Err(err) = start_semantic_indexing_job(app.clone(), &state).await {
        tracing::debug!("automatic semantic indexing skipped: {err}");
    }
}
