//! People (face clusters) — read-only commands for M1.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::face_repo::{FaceDetail, FaceRepo};

use crate::dto::{
    ClusterSuggestionDto, ClusteringDiagnosticsDto, FaceDetailDto, JobIdDto, Page,
    PendingFaceCountDto, PersonDto, ReviewFaceCountDto, ReviewItemDto,
};
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

/// Count of photos with `faces_processed = FALSE`. Drives the
/// "Resume face detection" banner on the People page.
#[tauri::command]
pub async fn people_pending_face_count(
    state: State<'_, AppState>,
) -> CommandResult<PendingFaceCountDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let pending_photos = FaceRepo::new(&db.conn).count_pending_face_processing()?;
    Ok(PendingFaceCountDto { pending_photos })
}

/// Return face detection diagnostics: cluster count, unclustered count,
/// and the quality-filter rejection breakdown from the last processing run.
#[tauri::command]
pub async fn people_clustering_diagnostics(
    state: State<'_, AppState>,
) -> CommandResult<ClusteringDiagnosticsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);

    let clusters = repo.get_all_clusters().unwrap_or_default();
    let total_faces: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM faces WHERE user_confirmed >= 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let _unclustered: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM faces WHERE cluster_id IS NULL AND user_confirmed >= 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let photos_with_faces: i64 = db
        .conn
        .query_row("SELECT COUNT(DISTINCT photo_id) FROM faces", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    // Pull the last-run rejection counts from face_processing_stats.
    // Missing row → all zeroes (no run has completed yet on this DB).
    let (rejected_small, rejected_lowconf, rejected_blurry, rejected_yaw): (i64, i64, i64, i64) =
        db.conn
            .query_row(
                "SELECT rejected_small, rejected_lowconf, rejected_blurry, rejected_yaw \
                 FROM face_processing_stats WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap_or((0, 0, 0, 0));

    Ok(ClusteringDiagnosticsDto {
        faces_detected: total_faces as usize,
        clusters_created: clusters.len(),
        photos_processed: photos_with_faces as usize,
        rejected_small: rejected_small.max(0) as usize,
        rejected_lowconf: rejected_lowconf.max(0) as usize,
        rejected_blurry: rejected_blurry.max(0) as usize,
        rejected_yaw: rejected_yaw.max(0) as usize,
    })
}

// ---------- face-level commands (Phase B) ----------

#[derive(Debug, Deserialize)]
pub struct FaceListArgs {
    pub person_id: i64,
    pub status: String,
    pub cursor: Option<i64>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn people_face_list(
    state: State<'_, AppState>,
    args: FaceListArgs,
) -> CommandResult<Page<FaceDetailDto>> {
    use smriti::db::face_repo::FaceStatus;

    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);

    let status = match args.status.as_str() {
        "confirmed" => FaceStatus::Confirmed,
        "unconfirmed" => FaceStatus::Unconfirmed,
        _ => FaceStatus::All,
    };

    let limit = args.limit.unwrap_or(50).min(200) as usize;
    let faces = repo.get_faces_by_cluster(args.person_id, status, args.cursor, limit)?;

    // Populate cluster names.
    let mut cluster_name: Option<String> = None;
    if let Ok(clusters) = repo.get_all_clusters() {
        cluster_name = clusters
            .iter()
            .find(|c| c.id == args.person_id)
            .and_then(|c| c.name.clone());
    }

    let has_more = faces.len() == limit;
    let next_cursor = faces.last().map(|f| f.face_id);
    let total = Some(faces.len() as u64);

    let items: Vec<FaceDetailDto> = faces
        .into_iter()
        .map(|mut f| {
            f.cluster_id = Some(args.person_id);
            let mut dto: FaceDetailDto = f.into();
            dto.cluster_name = cluster_name.clone();
            dto
        })
        .collect();

    Ok(Page {
        items,
        next_cursor: next_cursor.map(|c| c.to_string()),
        has_more,
        total,
    })
}

#[derive(Debug, Deserialize)]
pub struct UnclusteredFaceListArgs {
    pub cursor: Option<i64>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn people_unclustered_faces(
    state: State<'_, AppState>,
    args: UnclusteredFaceListArgs,
) -> CommandResult<Page<FaceDetailDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);
    let limit = args.limit.unwrap_or(24).min(100) as usize;
    let faces = repo.get_unclustered_faces(args.cursor, limit)?;
    let has_more = faces.len() == limit;
    let next_cursor = faces.last().map(|f| f.face_id);
    let items: Vec<FaceDetailDto> = faces.into_iter().map(FaceDetailDto::from).collect();
    let total = Some(items.len() as u64);

    Ok(Page {
        items,
        next_cursor: next_cursor.map(|c| c.to_string()),
        has_more,
        total,
    })
}

#[derive(Debug, Deserialize)]
pub struct FaceActionArgs {
    pub face_id: i64,
}

#[tauri::command]
pub async fn people_face_confirm(
    state: State<'_, AppState>,
    args: FaceActionArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    FaceRepo::new(&db.conn).confirm_face(args.face_id)?;
    Ok(())
}

#[tauri::command]
pub async fn people_face_reject(
    state: State<'_, AppState>,
    args: FaceActionArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;

    let cluster_id: i64 = db.conn.query_row(
        "SELECT cluster_id FROM faces WHERE id = ?1",
        rusqlite::params![args.face_id],
        |row| row.get(0),
    )?;
    FaceRepo::new(&db.conn).reject_face_to_unknown(args.face_id, cluster_id)?;
    Ok(())
}

#[tauri::command]
pub async fn people_face_hide(
    state: State<'_, AppState>,
    args: FaceActionArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    FaceRepo::new(&db.conn).hide_face(args.face_id)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct FaceReassignArgs {
    pub face_id: i64,
    pub target_cluster_id: i64,
}

#[tauri::command]
pub async fn people_face_reassign(
    state: State<'_, AppState>,
    args: FaceReassignArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;

    let old_cluster_id: i64 = db.conn.query_row(
        "SELECT cluster_id FROM faces WHERE id = ?1",
        rusqlite::params![args.face_id],
        |row| row.get(0),
    )?;
    FaceRepo::new(&db.conn).reassign_face(args.face_id, args.target_cluster_id, old_cluster_id)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct FaceSuggestArgs {
    pub face_id: i64,
    pub top_k: Option<u32>,
}

#[tauri::command]
pub async fn people_face_suggest_clusters(
    state: State<'_, AppState>,
    args: FaceSuggestArgs,
) -> CommandResult<Vec<ClusterSuggestionDto>> {
    use smriti::ml::{retrieve_candidates, FaceEmbedding};

    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);

    // Load the face's embedding.
    let embedding_bytes: Vec<u8> = db.conn.query_row(
        "SELECT embedding FROM faces WHERE id = ?1",
        rusqlite::params![args.face_id],
        |row| row.get(0),
    )?;
    let query_emb =
        FaceEmbedding::from_bytes(&embedding_bytes).ok_or_else(|| CommandError::Internal {
            message: "Corrupted face embedding".into(),
        })?;

    // Load gallery embeddings grouped by cluster.
    let galleries = repo.get_gallery_embeddings()?;
    let mut grouped: std::collections::HashMap<i64, Vec<(i64, FaceEmbedding)>> =
        std::collections::HashMap::new();
    for g in galleries {
        grouped
            .entry(g.cluster_id)
            .or_default()
            .push((g.face_id, g.embedding));
    }
    let grouped_vec: Vec<(i64, Vec<(i64, FaceEmbedding)>)> = grouped.into_iter().collect();

    // Exclude clusters this face has negatives against.
    let negatives = repo.get_negatives_for_face(args.face_id)?;
    let exclude: std::collections::HashSet<i64> = negatives.into_iter().collect();

    let top_k = args.top_k.unwrap_or(3) as usize;
    let hits = retrieve_candidates(&query_emb, &grouped_vec, top_k, 0.3, &exclude);

    // Enrich with cluster names and face counts.
    let clusters = repo.get_all_clusters().unwrap_or_default();
    let suggestions: Vec<ClusterSuggestionDto> = hits
        .into_iter()
        .take(top_k)
        .map(|hit| {
            let cluster = clusters.iter().find(|c| c.id == hit.cluster_id);
            ClusterSuggestionDto {
                cluster_id: hit.cluster_id,
                name: cluster
                    .and_then(|c| c.name.clone())
                    .unwrap_or_else(|| format!("Person {}", hit.cluster_id)),
                score: hit.score,
                face_count: cluster.map(|c| c.photo_count).unwrap_or(0),
                representative_face_id: cluster.and_then(|c| c.representative_face_id),
            }
        })
        .collect();

    Ok(suggestions)
}

#[derive(Debug, Deserialize)]
pub struct KSimilarArgs {
    pub cluster_id: i64,
    pub k: Option<u32>,
}

#[tauri::command]
pub async fn people_k_similar_to_cluster(
    state: State<'_, AppState>,
    args: KSimilarArgs,
) -> CommandResult<Vec<FaceDetailDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = FaceRepo::new(&db.conn);

    let k = args.k.unwrap_or(20).min(100) as usize;
    let scored = repo.k_similar_to_cluster(args.cluster_id, k)?;

    // Build FaceDetail for each scored face.
    let mut cluster_name: Option<String> = None;
    if let Ok(clusters) = repo.get_all_clusters() {
        cluster_name = clusters
            .iter()
            .find(|c| c.id == args.cluster_id)
            .and_then(|c| c.name.clone());
    }

    let items: Vec<FaceDetailDto> = scored
        .into_iter()
        .map(|(face_id, _score)| {
            let mut dto: FaceDetailDto = FaceDetail {
                face_id,
                photo_id: 0,
                cluster_id: None,
                confidence: 0.0,
                user_confirmed: 0,
            }
            .into();
            dto.thumbnail_path = Some(format!(".photovault/faces/{}.jpg", face_id));
            dto.cluster_name = cluster_name.clone();
            dto
        })
        .collect();

    Ok(items)
}

#[tauri::command]
pub async fn people_review_face_count(
    state: State<'_, AppState>,
) -> CommandResult<ReviewFaceCountDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let (unconfirmed_total, clusters_with_unconfirmed) =
        FaceRepo::new(&db.conn).count_unconfirmed_global()?;
    Ok(ReviewFaceCountDto {
        unconfirmed_total,
        clusters_with_unconfirmed,
    })
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
    /// Quality-filter rejection counts (only meaningful on the "complete" event).
    #[serde(default)]
    pub rejected_small: u64,
    #[serde(default)]
    pub rejected_lowconf: u64,
    #[serde(default)]
    pub rejected_blurry: u64,
    #[serde(default)]
    pub rejected_yaw: u64,
    /// Where embeddings are being computed for this run. "bridge" =
    /// configured cloud GPU, "local" = on-device ONNX session. Surfaced
    /// to the UI so the user can confirm cloud bridge is actually
    /// carrying load. Reflects intent (bridge enabled + URL set), not
    /// per-batch runtime health — use Test Connection in Settings for
    /// live status.
    #[serde(default)]
    pub embedder_route: String,
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

    let cfg = smriti::config::AppConfig::load();
    let model_dir = smriti::bootstrap::model_dir();
    let detector_conf = cfg.face_detection_confidence;
    let clustering_threshold = cfg.face_clustering_threshold;
    let resolver_weights = smriti::ml::ResolverWeights {
        cooccurrence: cfg.weight_cooccurrence,
        temporal: cfg.weight_temporal,
        ..Default::default()
    };
    // Snapshot the intended embedder route for the EV_FACES_COMPLETE
    // payload — the engine reports it per progress tick too, but the
    // completion event is emitted on this side and doesn't get the
    // last in-flight value.
    let route_str: String = if cfg.face_gpu_bridge_enabled && cfg.face_gpu_bridge_url.is_some() {
        "bridge".to_string()
    } else {
        "local".to_string()
    };

    tokio::spawn(async move {
        let (tx, rx) =
            async_channel::bounded::<smriti::services::face_processor::FaceProcessingProgress>(64);

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
                    rejected_small: 0,
                    rejected_lowconf: 0,
                    rejected_blurry: 0,
                    rejected_yaw: 0,
                    embedder_route: p.embedder_route.as_str().to_string(),
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
            smriti::services::face_processor::FaceProcessingResult,
            String,
        > = tokio::task::spawn_blocking(move || {
            smriti::services::face_processor::FaceProcessor::process_photos(
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
        let (rej_small, rej_lowconf, rej_blurry, rej_yaw) = match &pipeline_result {
            Ok(r) => (
                r.rejected_small as u64,
                r.rejected_lowconf as u64,
                r.rejected_blurry as u64,
                r.rejected_yaw as u64,
            ),
            Err(_) => (0, 0, 0, 0),
        };

        // Persist the rejection counts so people_clustering_diagnostics
        // can answer "why did face_count drop?" after an app restart.
        // Only on success — failure keeps the previous run's stats.
        if pipeline_result.is_ok() {
            let st: tauri::State<AppState> = app_clone.state();
            let lib_guard = st.library.read().await;
            if let Some(lib) = lib_guard.as_ref() {
                let db = lib.db.lock().await;
                let _ = db.conn.execute(
                    "INSERT INTO face_processing_stats \
                       (id, rejected_small, rejected_lowconf, rejected_blurry, rejected_yaw, completed_at) \
                     VALUES (1, ?1, ?2, ?3, ?4, strftime('%s','now')) \
                     ON CONFLICT(id) DO UPDATE SET \
                       rejected_small=excluded.rejected_small, \
                       rejected_lowconf=excluded.rejected_lowconf, \
                       rejected_blurry=excluded.rejected_blurry, \
                       rejected_yaw=excluded.rejected_yaw, \
                       completed_at=excluded.completed_at",
                    rusqlite::params![
                        rej_small as i64,
                        rej_lowconf as i64,
                        rej_blurry as i64,
                        rej_yaw as i64,
                    ],
                );
            }
        }
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
                rejected_small: rej_small,
                rejected_lowconf: rej_lowconf,
                rejected_blurry: rej_blurry,
                rejected_yaw: rej_yaw,
                embedder_route: route_str.clone(),
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
