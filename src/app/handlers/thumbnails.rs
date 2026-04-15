//! Thumbnail generation/persistence handlers.

use std::path::PathBuf;

use iced::Task;

use crate::db::Database;
use crate::services::ThumbnailService;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View, THUMBNAIL_DB_FLUSH_BATCH};

pub(crate) fn thumbnail_batch_ready(
    app: &mut PhotoVault,
    epoch: u64,
    results: Vec<(i64, PathBuf)>,
) -> Task<Message> {
    if epoch != app.thumbnail_generation_epoch {
        tracing::debug!("Ignoring stale thumbnail batch for epoch {}", epoch);
        return Task::none();
    }
    tracing::info!(
        "Thumbnail batch ready: {} thumbnails generated",
        results.len()
    );

    // Restore the thumbnail service from the Arc (it was taken in start_thumbnail_generation)
    // If the Arc still has other refs, just recreate from drive_path
    if app.thumbnail_service.is_none() {
        if let Some(ref drive_path) = app.selected_drive {
            if let Ok(service) = ThumbnailService::new(drive_path, 2.0) {
                app.thumbnail_service = Some(service);
            }
        }
    }

    // Update in-memory photo data and DB
    if let Some(ref drive_path) = app.selected_drive {
        // Update in-memory list (keep absolute paths for UI display)
        for (photo_id, path) in &results {
            if let Some(photo) = app.photos.iter_mut().find(|p| p.id == *photo_id) {
                photo.thumbnail_path = Some(path.to_string_lossy().to_string());
            }
        }

        // Batch update DB (store relative paths for portability)
        // When done, send ThumbnailBatchSaved to trigger the next batch
        let drive_path = drive_path.clone();
        let results_for_db = results;
        return Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    if !results_for_db.is_empty() {
                        let mut pending = Vec::with_capacity(results_for_db.len());
                        for (photo_id, path) in &results_for_db {
                            let rel_path = path
                                .strip_prefix(&drive_path)
                                .unwrap_or(path)
                                .to_string_lossy()
                                .to_string();
                            pending.push((*photo_id, rel_path));
                        }

                        let mut idx = 0;
                        while idx < pending.len() {
                            let end = (idx + THUMBNAIL_DB_FLUSH_BATCH).min(pending.len());
                            if let Ok(tx) = db.conn.unchecked_transaction() {
                                for (photo_id, rel_path) in &pending[idx..end] {
                                    let _ = tx.execute(
                                        "UPDATE photos SET thumbnail_path = ?1 WHERE id = ?2",
                                        rusqlite::params![rel_path, photo_id],
                                    );
                                }
                                let _ = tx.commit();
                            }
                            idx = end;
                        }
                    }
                }
            },
            move |_| Message::ThumbnailBatchSaved(epoch),
        );
    }
    Task::none()
}

pub(crate) fn thumbnail_batch_saved(app: &mut PhotoVault, epoch: u64) -> Task<Message> {
    if epoch != app.thumbnail_generation_epoch {
        tracing::debug!(
            "Ignoring stale thumbnail saved callback for epoch {}",
            epoch
        );
        return Task::none();
    }
    app.thumbnail_generation_active = false;
    app.schedule_thumbnail_chunk();
    // Previous batch DB write completed; start the next batch
    if !app.thumbnail_queue.is_empty() {
        tracing::info!(
            "Thumbnail batch saved, starting next batch ({} remaining)",
            app.thumbnail_queue.len()
        );
        app.start_thumbnail_generation()
    } else if app.thumbnail_scan_cursor < app.photos.len() {
        Task::perform(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            },
            move |_| Message::ContinueThumbnailScheduling(epoch),
        )
    } else {
        tracing::info!("All thumbnails generated successfully");
        app.thumbnail_generation_active = false;
        Task::none()
    }
}

pub(crate) fn continue_thumbnail_scheduling(app: &mut PhotoVault, epoch: u64) -> Task<Message> {
    if epoch != app.thumbnail_generation_epoch {
        tracing::debug!(
            "Ignoring stale thumbnail scheduling tick for epoch {}",
            epoch
        );
        return Task::none();
    }
    if app.current_view != View::Timeline {
        app.thumbnail_generation_active = false;
        return Task::none();
    }
    app.schedule_thumbnail_chunk();
    if !app.thumbnail_queue.is_empty() {
        app.start_thumbnail_generation()
    } else if app.thumbnail_scan_cursor < app.photos.len() {
        Task::perform(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            },
            move |_| Message::ContinueThumbnailScheduling(epoch),
        )
    } else {
        app.thumbnail_generation_active = false;
        Task::none()
    }
}
