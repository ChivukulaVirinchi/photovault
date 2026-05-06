//! People (face clusters) — read-only commands for M1.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use photovault::db::face_repo::FaceRepo;

use crate::dto::{JobIdDto, PersonDto, ReviewItemDto};
use crate::events::{EV_FACES_COMPLETE, EV_FACES_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[derive(Debug, Default, Deserialize)]
pub struct PeopleListArgs {
    #[serde(default)]
    pub named_only: bool,
    pub min_photos: Option<i64>,
}

#[tauri::command]
pub async fn people_list(
    state: State<'_, AppState>,
    args: PeopleListArgs,
) -> CommandResult<Vec<PersonDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let mut clusters = repo.get_all_clusters()?;
    if args.named_only {
        clusters.retain(|c| c.name.is_some() && !c.name.as_deref().unwrap_or("").is_empty());
    }
    if let Some(min) = args.min_photos {
        clusters.retain(|c| c.photo_count >= min);
    }
    repo.populate_face_thumbnails(&mut clusters, &lib.drive_root)?;
    Ok(clusters.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct PeopleGetArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn people_get(
    state: State<'_, AppState>,
    args: PeopleGetArgs,
) -> CommandResult<PersonDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let mut clusters = repo.get_all_clusters()?;
    repo.populate_face_thumbnails(&mut clusters, &lib.drive_root)?;
    let c = clusters
        .into_iter()
        .find(|c| c.id == args.id)
        .ok_or_else(|| CommandError::not_found("person", args.id))?;
    Ok(c.into())
}

#[derive(Debug, Default, Deserialize)]
pub struct PeopleReviewQueueArgs {
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn people_review_queue(
    state: State<'_, AppState>,
    args: PeopleReviewQueueArgs,
) -> CommandResult<Vec<ReviewItemDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let limit = args.limit.unwrap_or(20).min(200) as usize;
    let items = repo.get_review_queue_items(limit)?;
    Ok(items.into_iter().map(Into::into).collect())
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct PeopleRenameArgs {
    pub id: i64,
    pub name: Option<String>,
}

#[tauri::command]
pub async fn people_rename(
    state: State<'_, AppState>,
    args: PeopleRenameArgs,
) -> CommandResult<PersonDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let name = args.name.unwrap_or_default();
    repo.name_cluster(args.id, &name)?;
    let mut clusters = repo.get_all_clusters()?;
    repo.populate_face_thumbnails(&mut clusters, &lib.drive_root)?;
    let cluster = clusters
        .into_iter()
        .find(|c| c.id == args.id)
        .ok_or_else(|| CommandError::not_found("person", args.id))?;
    Ok(cluster.into())
}

#[derive(Debug, Deserialize)]
pub struct PeopleMergeArgs {
    pub source_id: i64,
    pub target_id: i64,
}

#[tauri::command]
pub async fn people_merge(
    state: State<'_, AppState>,
    args: PeopleMergeArgs,
) -> CommandResult<PersonDto> {
    if args.source_id == args.target_id {
        return Err(CommandError::Conflict {
            reason: "source and target are the same cluster".into(),
        });
    }
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    repo.merge_clusters(args.source_id, args.target_id)?;
    let mut clusters = repo.get_all_clusters()?;
    repo.populate_face_thumbnails(&mut clusters, &lib.drive_root)?;
    let cluster = clusters
        .into_iter()
        .find(|c| c.id == args.target_id)
        .ok_or_else(|| CommandError::not_found("person", args.target_id))?;
    Ok(cluster.into())
}

#[derive(Debug, Deserialize)]
pub struct PeopleDeleteArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn people_delete(
    state: State<'_, AppState>,
    args: PeopleDeleteArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    FaceRepo::new(&db.conn).delete_cluster(args.id)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct PeopleReviewArgs {
    pub queue_id: i64,
}

#[tauri::command]
pub async fn people_review_same(
    state: State<'_, AppState>,
    args: PeopleReviewArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    FaceRepo::new(&db.conn).resolve_review_same(args.queue_id)?;
    Ok(())
}

#[tauri::command]
pub async fn people_review_different(
    state: State<'_, AppState>,
    args: PeopleReviewArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    FaceRepo::new(&db.conn).resolve_review_different(args.queue_id)?;
    Ok(())
}

#[tauri::command]
pub async fn people_review_skip(
    state: State<'_, AppState>,
    args: PeopleReviewArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    FaceRepo::new(&db.conn).resolve_review_skip(args.queue_id)?;
    Ok(())
}

// ---------- jobs ----------

#[derive(Debug, Serialize, Clone)]
pub struct FacesProgressDto {
    pub job_id: String,
    pub stage: String,
    pub photos_processed: u64,
    pub total_photos: Option<u64>,
    pub faces_detected: u64,
    pub elapsed_ms: u64,
}

/// Start the face-processing pipeline (detect + embed + cluster).
///
/// `face_processor::process_photos` opens its own connection to the same
/// SQLite file — SQLite's WAL mode handles the concurrent reader.
#[tauri::command]
pub async fn people_start_processing(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };

    let job = jobs::start_job(&state, JobKind::FaceProcessing).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();
    let app_clone = app.clone();
    let started = job.started_at;
    let job_id_clone = job_id.clone();

    let cfg = photovault::config::AppConfig::load();
    let model_dir = photovault::bootstrap::model_dir();
    let detector_conf = cfg.face_detection_confidence;
    let clustering_threshold = cfg.face_clustering_threshold;
    let resolver_weights = photovault::ml::ResolverWeights {
        cooccurrence: cfg.weight_cooccurrence,
        temporal: cfg.weight_temporal,
        ..Default::default()
    };

    tokio::spawn(async move {
        let (tx, rx) = async_channel::bounded::<
            photovault::services::face_processor::FaceProcessingProgress,
        >(64);

        // Forwarder: consumes the service's progress channel and emits Tauri events.
        let job_id_evt = job_id_clone.clone();
        let app_evt = app_clone.clone();
        let forwarder = tokio::spawn(async move {
            while let Ok(p) = rx.recv().await {
                let dto = FacesProgressDto {
                    job_id: job_id_evt.clone(),
                    stage: format!("{:?}", p.stage),
                    photos_processed: p.chunks_flushed as u64,
                    total_photos: None,
                    faces_detected: p.faces_found as u64,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                };
                emit(&app_evt, EV_FACES_PROGRESS, dto);
            }
        });

        // Run the pipeline in a blocking thread.
        let cancel_flag: Option<Arc<std::sync::atomic::AtomicBool>> = Some(cancel.clone());
        let _ = tokio::task::spawn_blocking(move || {
            let _ = photovault::services::face_processor::FaceProcessor::process_photos(
                &drive_root,
                &model_dir,
                detector_conf,
                clustering_threshold,
                resolver_weights,
                Some(tx),
                cancel_flag,
            );
        })
        .await;
        let _ = forwarder.await;

        emit(
            &app_clone,
            EV_FACES_COMPLETE,
            FacesProgressDto {
                job_id: job_id_clone.clone(),
                stage: "complete".into(),
                photos_processed: 0,
                total_photos: None,
                faces_detected: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        );

        let st: tauri::State<AppState> = app_clone.state();
        jobs::finish_job(&st, &job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}

#[derive(Debug, Deserialize)]
pub struct CancelJobArgs {
    pub job_id: String,
}

#[tauri::command]
pub async fn people_cancel_processing(
    state: State<'_, AppState>,
    args: CancelJobArgs,
) -> CommandResult<()> {
    state.jobs.lock().await.cancel(&args.job_id);
    let _ = Ordering::Relaxed;
    Ok(())
}
