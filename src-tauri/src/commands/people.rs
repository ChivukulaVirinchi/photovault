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
    /// Photos the worker has finished processing. Field name matches the
    /// generic wire shape consumed by `jobs.svelte.ts` so the progress
    /// bar lights up without further coalescing.
    pub processed: u64,
    /// Total photos queued for this run. None only on the first
    /// pre-discovery tick, never null afterwards.
    pub total: Option<u64>,
    /// Cumulative faces detected so far. The People page reads this for
    /// its "47 faces found" status line.
    pub faces_found: u64,
    /// Bumped each time the writer thread commits a chunk to disk.
    /// People.svelte refreshes its grid when this increments — that's
    /// how new faces stream in mid-run.
    pub chunks_flushed: u32,
    pub elapsed_ms: u64,
    pub message: Option<String>,
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

        // Forwarder: copies engine progress 1:1 into the Tauri event
        // stream. The engine already publishes real per-photo counts and
        // total — the previous version threw both away in favour of
        // `chunks_flushed`, which is why the UI's progress bar never
        // moved. Track final counts so EV_FACES_COMPLETE carries them.
        let job_id_evt = job_id_clone.clone();
        let app_evt = app_clone.clone();
        let last_seen = Arc::new(std::sync::Mutex::new((0u64, 0u64, 0u32, 0u64)));
        let last_seen_writer = last_seen.clone();
        let forwarder = tokio::spawn(async move {
            while let Ok(p) = rx.recv().await {
                let processed = p.processed as u64;
                let total = p.total as u64;
                let faces = p.faces_found as u64;
                let chunks = p.chunks_flushed;
                {
                    let mut g = last_seen_writer.lock().unwrap();
                    *g = (processed, total, chunks, faces);
                }
                let dto = FacesProgressDto {
                    job_id: job_id_evt.clone(),
                    stage: format!("{:?}", p.stage),
                    processed,
                    total: if total == 0 { None } else { Some(total) },
                    faces_found: faces,
                    chunks_flushed: chunks,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    message: Some(format!(
                        "{} face{}",
                        faces,
                        if faces == 1 { "" } else { "s" }
                    )),
                };
                emit(&app_evt, EV_FACES_PROGRESS, dto);
            }
        });

        // Run the pipeline in a blocking thread. Capture the engine's
        // result so the complete event can tell the user whether work
        // actually happened (vs "all photos already processed", vs an
        // error like "model not found").
        let cancel_flag: Option<Arc<std::sync::atomic::AtomicBool>> = Some(cancel.clone());
        let pipeline_result: Result<
            photovault::services::face_processor::FaceProcessingResult,
            String,
        > = tokio::task::spawn_blocking(move || {
            photovault::services::face_processor::FaceProcessor::process_photos(
                &drive_root,
                &model_dir,
                detector_conf,
                clustering_threshold,
                resolver_weights,
                Some(tx),
                cancel_flag,
            )
        })
        .await
        .unwrap_or_else(|join_err| Err(format!("worker panicked: {}", join_err)));
        let _ = forwarder.await;
        if let Err(ref msg) = pipeline_result {
            tracing::error!("face processing failed: {}", msg);
        }

        let (final_processed, final_total, final_chunks, final_faces) = {
            let g = last_seen.lock().unwrap();
            *g
        };
        // Build a human-readable summary the UI can show as a toast.
        // Error path takes precedence — a model-missing failure should
        // not look like "Found 0 faces". Then "all already processed"
        // (engine returned Ok with photos_processed=0). Then real
        // counts.
        let message = match &pipeline_result {
            Err(e) => format!("Face detection failed: {}", e),
            Ok(r) if r.photos_processed == 0 && final_total == 0 => {
                "All photos already analysed.".to_string()
            }
            Ok(r) => format!(
                "Found {} face{} in {} photo{}.",
                r.faces_detected,
                if r.faces_detected == 1 { "" } else { "s" },
                r.photos_processed,
                if r.photos_processed == 1 { "" } else { "s" }
            ),
        };
        emit(
            &app_clone,
            EV_FACES_COMPLETE,
            FacesProgressDto {
                job_id: job_id_clone.clone(),
                stage: if pipeline_result.is_err() {
                    "error".into()
                } else {
                    "complete".into()
                },
                processed: final_processed,
                total: if final_total == 0 {
                    None
                } else {
                    Some(final_total)
                },
                faces_found: final_faces,
                chunks_flushed: final_chunks,
                elapsed_ms: started.elapsed().as_millis() as u64,
                message: Some(message),
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

#[derive(Debug, Serialize)]
pub struct PeopleResetResult {
    pub photos_reflagged: u64,
    pub clusters_dropped: u64,
    pub faces_dropped: u64,
}

/// Wipe everything face-related and ask for a fresh run. Confirmed
/// from the UI behind a destructive-action prompt — drops every face
/// row, every cluster, and flips photos.faces_processed back to FALSE
/// so the next "Find faces" run reprocesses the whole library.
#[tauri::command]
pub async fn people_reset_all(state: State<'_, AppState>) -> CommandResult<PeopleResetResult> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let tx = db.conn.unchecked_transaction()?;
    let faces_dropped = tx.execute("DELETE FROM faces", [])? as u64;
    let clusters_dropped = tx.execute("DELETE FROM face_clusters", [])? as u64;
    // Auxiliary tables that hang off cluster ids — clear them
    // explicitly so the next run starts from a clean slate.
    let _ = tx.execute("DELETE FROM photo_inferred_identities", []);
    let _ = tx.execute("DELETE FROM person_gallery_embeddings", []);
    let _ = tx.execute("DELETE FROM cluster_cannot_merge", []);
    let _ = tx.execute("DELETE FROM face_review_queue", []);
    let photos_reflagged = tx.execute("UPDATE photos SET faces_processed = FALSE", [])? as u64;
    tx.commit()?;

    // Also nuke the on-disk face crops directory — those files are
    // keyed by face_id and would otherwise leak.
    let faces_dir = lib.drive_root.join(".photovault").join("faces");
    if faces_dir.exists() {
        let _ = std::fs::remove_dir_all(&faces_dir);
        let _ = std::fs::create_dir_all(&faces_dir);
    }
    Ok(PeopleResetResult {
        photos_reflagged,
        clusters_dropped,
        faces_dropped,
    })
}

/// Re-cluster from existing embeddings: keeps the detected faces and
/// their on-disk crops, but drops every cluster + assignment so the
/// next face-detection run only does the cluster phase. Useful when
/// the clustering threshold is changed.
#[tauri::command]
pub async fn people_reset_clusters(state: State<'_, AppState>) -> CommandResult<PeopleResetResult> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let tx = db.conn.unchecked_transaction()?;
    // Detach faces from clusters but keep the face rows themselves.
    tx.execute("UPDATE faces SET cluster_id = NULL, user_confirmed = 0", [])?;
    let clusters_dropped = tx.execute("DELETE FROM face_clusters", [])? as u64;
    let _ = tx.execute("DELETE FROM photo_inferred_identities", []);
    let _ = tx.execute("DELETE FROM person_gallery_embeddings", []);
    let _ = tx.execute("DELETE FROM cluster_cannot_merge", []);
    let _ = tx.execute("DELETE FROM face_review_queue", []);
    tx.commit()?;
    Ok(PeopleResetResult {
        photos_reflagged: 0,
        clusters_dropped,
        faces_dropped: 0,
    })
}
