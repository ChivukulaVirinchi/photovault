//! Semantic search model install, indexing, and photo similarity.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use smriti::db::connection::{db_path_for, open_secondary};
use smriti::db::photo_repo::PhotoRepo;
use smriti::services::semantic::SemanticSearchService;

use super::library::spawn_semantic_warmup;
use crate::dto::{JobIdDto, PhotoSummaryDto, SemanticStatusDto};
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
pub async fn semantic_warm_runtime(state: State<'_, AppState>) -> CommandResult<()> {
    let (drive_root, semantic_index, semantic_runner) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            lib.drive_root.clone(),
            lib.semantic_index.clone(),
            lib.semantic_runner.clone(),
        )
    };
    spawn_semantic_warmup(drive_root, semantic_index, semantic_runner);
    Ok(())
}

#[tauri::command]
pub async fn semantic_install_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    if state
        .jobs
        .lock()
        .await
        .has_any_of_kind(JobKind::SemanticAssets)
    {
        return Err(CommandError::Conflict {
            reason: "semantic model installation is already in progress".into(),
        });
    }

    let job = jobs::start_job(&state, JobKind::SemanticAssets).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let result = SemanticSearchService::install_model_assets(
            Some(cancel.as_ref()),
            |stage, processed, total| {
                emit(
                    &app_clone,
                    EV_SEMANTIC_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: stage.into(),
                        processed,
                        total,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("Downloading semantic search model ({stage})")),
                    },
                );
            },
        )
        .await;

        match result {
            Ok(()) => emit(
                &app_clone,
                EV_SEMANTIC_COMPLETE,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "install-complete".into(),
                    processed: 1,
                    total: Some(1),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some("Semantic search model installed".into()),
                },
            ),
            Err(err) if err.to_lowercase().contains("cancelled") => emit(
                &app_clone,
                EV_SEMANTIC_COMPLETE,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "install-cancelled".into(),
                    processed: 0,
                    total: Some(1),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some("Semantic model install cancelled".into()),
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
                    message: Some(format!("Semantic model install failed: {err}")),
                },
            ),
        }

        let st: tauri::State<AppState> = app_clone.state();
        jobs::finish_job(&st, &job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}

#[tauri::command]
pub async fn semantic_start_indexing(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
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
            reason: "Install ONNX Runtime from Settings -> Assets -> Download assets first.".into(),
        });
    }
    let job = jobs::start_job(&state, JobKind::SemanticIndex).await?;
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

#[derive(Debug, Deserialize)]
pub struct SemanticSimilarArgs {
    pub photo_id: i64,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn semantic_similar_photos(
    state: State<'_, AppState>,
    args: SemanticSimilarArgs,
) -> CommandResult<Vec<PhotoSummaryDto>> {
    let (drive_root, db_path, semantic_index) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        (
            lib.drive_root.clone(),
            db_path_for(&lib.drive_root),
            lib.semantic_index.clone(),
        )
    };
    let limit = args.limit.unwrap_or(24).clamp(1, 100) as usize;
    let photo_id = args.photo_id;
    let (ids, photos) = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let svc = SemanticSearchService::new(&drive_root);
        let candidates = {
            let mut cache = semantic_index
                .lock()
                .map_err(|_| CommandError::internal("semantic index cache poisoned"))?;
            svc.similar_to_photo_cached(&conn, &mut cache, photo_id, limit)
                .map_err(|reason| CommandError::MlUnavailable { reason })?
        };
        let ids: Vec<i64> = candidates.iter().map(|c| c.photo_id).collect();
        let photos = PhotoRepo::new(&conn).get_by_ids(&ids)?;
        Ok::<_, CommandError>((ids, photos))
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("semantic similarity worker failed: {e}"),
    })??;
    let mut by_id: HashMap<i64, PhotoSummaryDto> =
        photos.into_iter().map(|p| (p.id, p.into())).collect();
    Ok(ids.into_iter().filter_map(|id| by_id.remove(&id)).collect())
}
