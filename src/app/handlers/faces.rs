//! Face detection, clustering, merge, and cluster-detail handlers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use iced::Task;

use crate::db::{Database, FaceClusterRecord, FaceRepo, PhotoRepo};
use crate::services::{FaceProcessingProgress, FaceProcessingResult, FaceProcessor};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn process_faces(app: &mut PhotoVault) -> Task<Message> {
    if app.face_processing_active {
        tracing::info!("ProcessFaces: already active, ignoring");
        return Task::none();
    }

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    tracing::info!("ProcessFaces: starting face processing pipeline");
    app.face_processing_active = true;
    app.face_processing_progress = Some(FaceProcessingProgress::default());
    app.face_processing_error = None;
    // Reset streaming-flush bookkeeping so chunks from this run drive
    // a fresh wave of People-view refreshes.
    app.face_last_chunks_flushed = 0;
    app.face_last_chunk_refresh_at = None;

    // Only process photos that haven't been analyzed yet.
    // (Previously this reset ALL flags, causing re-detection of every photo
    // and new clusters each time — destroying user-assigned names.)

    let drive_path = drive_path.clone();
    let detector_confidence = app.config.face_detection_confidence;
    let clustering_threshold = app.config.face_clustering_threshold;
    let resolver_weights = crate::ml::ResolverWeights {
        embedding: 1.0,
        cooccurrence: app.config.weight_cooccurrence,
        temporal: app.config.weight_temporal,
    };
    let model_dir = crate::bootstrap::model_dir();

    let detector_path = crate::bootstrap::detector_model_path();
    let embedder_path = crate::bootstrap::embedder_model_path();
    if !detector_path.exists() || !embedder_path.exists() {
        app.face_processing_active = false;
        app.face_processing_progress = None;
        app.face_processing_error = Some(format!(
            "Face models missing. Expected {} and {}",
            detector_path.display(),
            embedder_path.display()
        ));
        return Task::none();
    }

    let (progress_tx, progress_rx) = async_channel::bounded(32);
    app.face_progress_receiver = Some(progress_rx);

    let cancel_flag = Arc::new(AtomicBool::new(false));
    app.face_cancel_flag = Some(Arc::clone(&cancel_flag));

    // Spawn blocking face processing task
    let process_task = Task::perform(
        async move {
            let handle = tokio::task::spawn_blocking(move || {
                FaceProcessor::process_photos(
                    &drive_path,
                    &model_dir,
                    detector_confidence,
                    clustering_threshold,
                    resolver_weights,
                    Some(progress_tx),
                    Some(cancel_flag),
                )
            });

            match handle.await {
                Ok(result) => result,
                Err(e) => Err(format!("Face processing thread panicked: {}", e)),
            }
        },
        Message::FaceProcessingComplete,
    );

    process_task
}

pub(crate) fn cancel_face_processing(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref flag) = app.face_cancel_flag {
        flag.store(true, Ordering::Relaxed);
        tracing::info!("Face processing cancellation requested");
    }
    Task::none()
}

pub(crate) fn face_processing_complete(
    app: &mut PhotoVault,
    result: Result<FaceProcessingResult, String>,
) -> Task<Message> {
    app.face_processing_active = false;
    app.face_progress_receiver = None;
    app.face_cancel_flag = None;
    // Keep `face_processing_progress` populated through the toast +
    // cluster reload so the progress bar doesn't blank out one frame
    // before the success toast appears. It's cleared in
    // `face_clusters_loaded` once the People view has fresh data.

    let toast_task = match &result {
        Ok(r) => {
            app.face_processing_error = None;
            tracing::info!(
                "Face processing complete: {} photos, {} faces, {} clusters",
                r.photos_processed,
                r.faces_detected,
                r.clusters_created
            );

            // Refresh the review queue badge now that new items may be queued.
            if let Some(ref drive_path) = app.selected_drive {
                if let Ok(db) = Database::open_for_drive(drive_path) {
                    if let Ok(n) = FaceRepo::new(&db.conn).review_queue_size() {
                        app.face_review_pending = n;
                    }
                }
            }

            super::handle(
                app,
                Message::ToastShow(crate::components::toast::Toast::success(format!(
                    "Found {} faces across {} photos",
                    r.faces_detected, r.photos_processed
                ))),
            )
        }
        Err(e) => {
            app.face_processing_error = Some(e.clone());
            tracing::error!("Face processing failed: {}", e);
            super::handle(
                app,
                Message::ToastShow(crate::components::toast::Toast::error(
                    "Face processing failed",
                    e.clone(),
                )),
            )
        }
    };

    let invalidate = super::handle(app, Message::InvalidateInsights);
    // Reload clusters
    Task::batch([app.load_face_clusters(), toast_task, invalidate])
}

pub(crate) fn face_clusters_loaded(
    app: &mut PhotoVault,
    clusters: Vec<FaceClusterRecord>,
) -> Task<Message> {
    tracing::info!(
        "FaceClustersLoaded: received {} clusters (previously had {})",
        clusters.len(),
        app.face_clusters.len()
    );
    app.face_clusters_loading = false;
    // Clusters are now fresh — drop the lingering processing progress so
    // the People view stops showing the status bar. (Set by
    // face_processing_complete, deferred until this point on purpose.)
    if !app.face_processing_active {
        app.face_processing_progress = None;
    }
    for cluster in &clusters {
        if cluster.photo_count > 0
            && !app.cluster_photos.is_empty()
            && app.selected_cluster_id == Some(cluster.id)
        {
            tracing::debug!(
                "Cluster {} has {} photos, currently loaded detail {}",
                cluster.id,
                cluster.photo_count,
                app.cluster_photos.len()
            );
        }
    }
    app.face_clusters = clusters;
    Task::none()
}

pub(crate) fn select_cluster(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    app.selected_cluster_id = Some(cluster_id);
    if app.current_view != View::ClusterDetail {
        app.previous_view = Some(app.current_view.clone());
    }
    app.current_view = View::ClusterDetail;

    // Load photos for this cluster directly from DB to avoid in-memory
    // list truncation issues (timeline cache can be limited).
    if let Some(ref db) = app.database {
        let face_repo = FaceRepo::new(&db.conn);
        match face_repo.get_photos_for_cluster(cluster_id) {
            Ok(photo_ids) => {
                let photo_repo = PhotoRepo::new(&db.conn);
                match photo_repo.get_by_ids(&photo_ids) {
                    Ok(mut photos) => {
                        if let Some(ref drive_path) = app.selected_drive {
                            for photo in &mut photos {
                                if let Some(ref rel_path) = photo.thumbnail_path {
                                    let abs_path = drive_path.join(rel_path);
                                    photo.thumbnail_path =
                                        Some(abs_path.to_string_lossy().to_string());
                                }
                            }
                        }
                        app.cluster_photos = photos;
                    }
                    Err(e) => {
                        tracing::error!("Failed to load cluster photo records: {}", e);
                        app.cluster_photos = Vec::new();
                    }
                }
                tracing::info!(
                    "Loaded {} photos for cluster {}",
                    app.cluster_photos.len(),
                    cluster_id
                );
            }
            Err(e) => {
                tracing::error!("Failed to load cluster photos: {}", e);
                app.cluster_photos = Vec::new();
            }
        }
    }
    Task::none()
}

pub(crate) fn back_to_people(app: &mut PhotoVault) -> Task<Message> {
    app.current_view = View::People;
    app.selected_cluster_id = None;
    app.cluster_photos.clear();
    Task::none()
}

pub(crate) fn start_edit_cluster_name(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    // Set up editing state with current name
    let current_name = app
        .face_clusters
        .iter()
        .find(|c| c.id == cluster_id)
        .and_then(|c| c.name.clone())
        .unwrap_or_default();

    app.editing_cluster_id = Some(cluster_id);
    app.edit_cluster_name = current_name;
    let input_id = iced::widget::text_input::Id::new(format!("cluster-edit-{}", cluster_id));
    Task::batch([
        iced::widget::text_input::focus(input_id.clone()),
        iced::widget::text_input::move_cursor_to_end(input_id),
    ])
}

pub(crate) fn edit_cluster_name(
    app: &mut PhotoVault,
    _cluster_id: i64,
    name: String,
) -> Task<Message> {
    app.edit_cluster_name = name;
    Task::none()
}

pub(crate) fn save_cluster_name(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    let name = app.edit_cluster_name.clone();
    app.editing_cluster_id = None;

    // Update in-memory
    if let Some(cluster) = app.face_clusters.iter_mut().find(|c| c.id == cluster_id) {
        if name.is_empty() {
            cluster.name = None;
        } else {
            cluster.name = Some(name.clone());
        }
    }

    // Find other clusters with the same name for auto-merge
    let same_name_ids: Vec<i64> = if !name.is_empty() {
        app.face_clusters
            .iter()
            .filter(|c| c.id != cluster_id && c.name.as_deref() == Some(&name))
            .map(|c| c.id)
            .collect()
    } else {
        Vec::new()
    };

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    let drive_path = drive_path.clone();
    let save_task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let face_repo = FaceRepo::new(&db.conn);
                let _ = face_repo.name_cluster(cluster_id, &name);

                // Auto-merge: if other clusters share this name, merge them in
                for source_id in same_name_ids {
                    tracing::info!(
                        "Auto-merging cluster {} into {} (same name: {})",
                        source_id,
                        cluster_id,
                        name
                    );
                    let _ = face_repo.merge_clusters(source_id, cluster_id);
                }
            }
        },
        |_| Message::NoOp,
    );

    // Reload clusters after save+merge
    let reload_task = app.load_face_clusters();
    Task::batch([save_task, reload_task])
}

pub(crate) fn toggle_merge_mode(app: &mut PhotoVault) -> Task<Message> {
    app.merge_mode_active = !app.merge_mode_active;
    if !app.merge_mode_active {
        app.merge_selected_clusters.clear();
    }
    Task::none()
}

pub(crate) fn toggle_merge_select(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    if let Some(pos) = app
        .merge_selected_clusters
        .iter()
        .position(|&id| id == cluster_id)
    {
        app.merge_selected_clusters.remove(pos);
    } else {
        app.merge_selected_clusters.push(cluster_id);
    }
    Task::none()
}

pub(crate) fn merge_selected_clusters(app: &mut PhotoVault) -> Task<Message> {
    if app.merge_selected_clusters.len() < 2 {
        return Task::none();
    }

    // Prefer a named cluster as the merge target so the name survives.
    let named_target = app
        .merge_selected_clusters
        .iter()
        .find(|&&id| {
            app.face_clusters
                .iter()
                .any(|c| c.id == id && c.name.is_some())
        })
        .copied();
    let target_id = named_target.unwrap_or(app.merge_selected_clusters[0]);
    let source_ids: Vec<i64> = app
        .merge_selected_clusters
        .iter()
        .copied()
        .filter(|&id| id != target_id)
        .collect();

    app.merge_mode_active = false;
    app.merge_selected_clusters.clear();

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    let drive_path = drive_path.clone();
    let merge_task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let face_repo = FaceRepo::new(&db.conn);
                for source_id in source_ids {
                    let _ = face_repo.merge_clusters(source_id, target_id);
                }
            }
        },
        |_| Message::NoOp,
    );

    let reload_task = app.load_face_clusters();
    Task::batch([merge_task, reload_task])
}

pub(crate) fn rebuild_face_clusters(app: &mut PhotoVault) -> Task<Message> {
    if app.face_processing_active {
        tracing::info!("RebuildFaceClusters ignored: face processing already active");
        return Task::none();
    }

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    let drive_path = drive_path.clone();
    app.face_processing_active = true;
    app.face_processing_progress = Some(FaceProcessingProgress::default());
    app.face_processing_error = None;

    Task::perform(
        async move {
            let handle = tokio::task::spawn_blocking(move || {
                let db = Database::open_for_drive(&drive_path)
                    .map_err(|e| format!("Failed to open database: {}", e))?;

                let tx = db
                    .conn
                    .unchecked_transaction()
                    .map_err(|e| format!("Failed to start reset transaction: {}", e))?;

                tx.execute("DELETE FROM photo_inferred_identities", [])
                    .map_err(|e| format!("Failed to clear inferred identities: {}", e))?;
                tx.execute("DELETE FROM faces", [])
                    .map_err(|e| format!("Failed to clear faces: {}", e))?;
                tx.execute("DELETE FROM face_clusters", [])
                    .map_err(|e| format!("Failed to clear face clusters: {}", e))?;

                let reset = tx
                    .execute(
                        "UPDATE photos SET faces_processed = FALSE WHERE is_trashed = FALSE",
                        [],
                    )
                    .map_err(|e| format!("Failed to reset faces_processed flags: {}", e))?;

                tx.commit()
                    .map_err(|e| format!("Failed to commit face reset: {}", e))?;

                let faces_dir = FaceProcessor::faces_dir(&drive_path);
                if let Ok(entries) = std::fs::read_dir(&faces_dir) {
                    for entry in entries.flatten() {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }

                Ok(reset)
            });

            match handle.await {
                Ok(result) => result,
                Err(e) => Err(format!("Face reset thread panicked: {}", e)),
            }
        },
        Message::FaceDataResetComplete,
    )
}

pub(crate) fn face_data_reset_complete(
    app: &mut PhotoVault,
    result: Result<usize, String>,
) -> Task<Message> {
    match result {
        Ok(reset) => {
            tracing::info!(
                "Face data reset complete: {} photos marked for full re-processing",
                reset
            );
            app.face_processing_active = false;
            app.face_processing_progress = None;
            app.face_progress_receiver = None;
            app.face_cancel_flag = None;
            let invalidate = super::handle(app, Message::InvalidateInsights);
            Task::batch([super::handle(app, Message::ProcessFaces), invalidate])
        }
        Err(e) => {
            app.face_processing_active = false;
            app.face_processing_progress = None;
            app.face_progress_receiver = None;
            app.face_cancel_flag = None;
            app.face_processing_error = Some(e.clone());
            tracing::error!("Face data reset failed: {}", e);
            Task::none()
        }
    }
}
