//! Semantic search model install, indexing, and photo similarity.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use smriti::db::connection::{db_path_for, open_secondary};
use smriti::db::photo_repo::PhotoRepo;
use smriti::services::semantic::SemanticSearchService;

use crate::dto::{JobIdDto, PhotoSummaryDto, SemanticStatusDto};
use crate::events::{JobProgress, EV_SEMANTIC_COMPLETE, EV_SEMANTIC_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn semantic_status(state: State<'_, AppState>) -> CommandResult<SemanticStatusDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    Ok(SemanticSearchService::new(&lib.drive_root)
        .status(&db.conn)?
        .into())
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

        emit(
            &app_clone,
            EV_SEMANTIC_COMPLETE,
            JobProgress {
                job_id: job_id_clone.clone(),
                stage: "install-complete".into(),
                processed: 1,
                total: Some(1),
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some(match result {
                    Ok(()) => "Semantic search model installed".into(),
                    Err(err) if err.to_lowercase().contains("cancelled") => {
                        "Semantic model install cancelled".into()
                    }
                    Err(err) => format!("Semantic model install failed: {err}"),
                }),
            },
        );

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
            let conn = open_secondary(&db_path).map_err(|e| e.to_string())?;
            let svc = SemanticSearchService::new(&drive_root);
            let mut runner = SemanticSearchService::model_runner()?;
            let mut processed = 0u64;
            let mut total = svc.index_stats(&conn).map_err(|e| e.to_string())?.pending;
            if total == 0 {
                total = 0;
            }
            let started_inner = Instant::now();

            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok::<String, String>("Semantic indexing cancelled".into());
                }
                let batch = svc
                    .next_pending_batch(&conn, 16)
                    .map_err(|e| e.to_string())?;
                if batch.is_empty() {
                    return Ok("Semantic indexing complete".into());
                }
                for photo in batch {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok("Semantic indexing cancelled".into());
                    }
                    let outcome = photo
                        .source_path(&drive_root)
                        .and_then(|path| runner.embed_image_path(&path))
                        .and_then(|vector| {
                            svc.mark_indexed(&conn, photo.photo_id, &vector)
                                .map_err(|e| e.to_string())
                        });
                    if let Err(err) = outcome {
                        let _ = SemanticSearchService::mark_failed(&conn, photo.photo_id, &err);
                    }
                    processed += 1;
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
                            message: Some("Indexing visual meaning".into()),
                        },
                    );
                }
            }
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);

        emit(
            &app_clone,
            EV_SEMANTIC_COMPLETE,
            JobProgress {
                job_id: job_id_clone.clone(),
                stage: "index-complete".into(),
                processed: 1,
                total: Some(1),
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some(match result {
                    Ok(msg) => msg,
                    Err(err) => format!("Semantic indexing failed: {err}"),
                }),
            },
        );

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
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let svc = SemanticSearchService::new(&lib.drive_root);
    let mut cache = lib
        .semantic_index
        .lock()
        .map_err(|_| CommandError::internal("semantic index cache poisoned"))?;
    let candidates = svc
        .similar_to_photo_cached(
            &db.conn,
            &mut cache,
            args.photo_id,
            args.limit.unwrap_or(24).min(100) as usize,
        )
        .map_err(|reason| CommandError::MlUnavailable { reason })?;
    let ids: Vec<i64> = candidates.iter().map(|c| c.photo_id).collect();
    let photos = PhotoRepo::new(&db.conn).get_by_ids(&ids)?;
    let mut by_id: HashMap<i64, PhotoSummaryDto> =
        photos.into_iter().map(|p| (p.id, p.into())).collect();
    Ok(ids.into_iter().filter_map(|id| by_id.remove(&id)).collect())
}
