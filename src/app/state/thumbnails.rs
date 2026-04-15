use std::sync::Arc;

use iced::Task;
use tokio::task::JoinSet;

use crate::services::ThumbnailService;

use super::super::messages::Message;
use super::PhotoVault;

impl PhotoVault {
    pub(crate) fn begin_thumbnail_generation_epoch(&mut self) {
        self.thumbnail_generation_epoch = self.thumbnail_generation_epoch.wrapping_add(1);
        self.thumbnail_generation_active = false;
        self.thumbnail_queue.clear();
        self.thumbnail_scan_cursor = 0;
    }

    pub(crate) fn seed_thumbnail_queue_for_timeline(&mut self) {
        let columns = Self::timeline_columns_for_width(self.window_width);
        let priority_count =
            columns * (Self::THUMBNAIL_VISIBLE_ROWS + Self::THUMBNAIL_PREFETCH_ROWS);
        let initial_end = priority_count.min(self.photos.len());

        for photo in self.photos.iter().take(initial_end) {
            if photo.thumbnail_path.is_none() {
                self.thumbnail_queue.push((
                    photo.id,
                    photo.file_path.clone(),
                    photo.file_hash.clone(),
                    photo.orientation,
                ));
            }
        }

        self.thumbnail_scan_cursor = initial_end;
    }

    /// Re-prioritize thumbnail generation for the currently visible region
    /// after a scroll event. Inserts missing thumbnails near the scroll
    /// position at the front of the queue so they generate first.
    pub(crate) fn reprioritize_thumbnails_for_scroll(&mut self, scroll_y: f32) {
        let columns = Self::timeline_columns_for_width(self.window_width);
        // Each row is ~168px tall (thumbnail 160 + 8 gap). Day headers add
        // ~48px. Rough estimate: 168px per photo row.
        let row_height = 168.0_f32;
        let start_row = (scroll_y / row_height).floor().max(0.0) as usize;
        let visible_rows = Self::THUMBNAIL_VISIBLE_ROWS + Self::THUMBNAIL_PREFETCH_ROWS;
        let start_idx = start_row * columns;
        let end_idx = ((start_row + visible_rows) * columns).min(self.photos.len());

        if start_idx >= self.photos.len() {
            return;
        }

        // Collect photo ids that need thumbnails in the visible region
        let mut urgent: Vec<(i64, String, String, i32)> = Vec::new();
        for photo in &self.photos[start_idx..end_idx] {
            if photo.thumbnail_path.is_none() {
                // Only add if not already in queue
                let already_queued = self.thumbnail_queue.iter().any(|(id, _, _, _)| *id == photo.id);
                if !already_queued {
                    urgent.push((
                        photo.id,
                        photo.file_path.clone(),
                        photo.file_hash.clone(),
                        photo.orientation,
                    ));
                }
            }
        }

        if !urgent.is_empty() {
            // Prepend urgent items to front of queue
            urgent.append(&mut self.thumbnail_queue);
            self.thumbnail_queue = urgent;
            // Advance scan cursor past this region to avoid re-scanning
            if end_idx > self.thumbnail_scan_cursor {
                self.thumbnail_scan_cursor = end_idx;
            }
        }
    }

    pub(crate) fn schedule_thumbnail_chunk(&mut self) {
        if self.thumbnail_queue.len() >= Self::THUMBNAIL_QUEUE_TARGET {
            return;
        }

        let end = (self.thumbnail_scan_cursor + Self::THUMBNAIL_SCAN_CHUNK).min(self.photos.len());

        for photo in &self.photos[self.thumbnail_scan_cursor..end] {
            if self.thumbnail_queue.len() >= Self::THUMBNAIL_QUEUE_TARGET {
                break;
            }
            if photo.thumbnail_path.is_none() {
                self.thumbnail_queue.push((
                    photo.id,
                    photo.file_path.clone(),
                    photo.file_hash.clone(),
                    photo.orientation,
                ));
            }
        }

        self.thumbnail_scan_cursor = end;
    }

    /// Start background thumbnail generation for the next batch from the queue.
    ///
    /// Drains up to THUMBNAIL_BATCH_SIZE items from `self.thumbnail_queue` and
    /// spawns them concurrently via JoinSet. When the batch finishes, the
    /// `ThumbnailBatchReady` handler will call this again if the queue is
    /// not empty, creating a natural batch chain until all thumbnails are done.
    pub(crate) fn start_thumbnail_generation(&mut self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        // Create thumbnail service if needed
        if self.thumbnail_service.is_none() {
            match ThumbnailService::new(drive_path, 2.0) {
                Ok(service) => {
                    // Load existing thumbnails from disk
                    if let Err(e) = service.load_existing_thumbnails() {
                        tracing::warn!("Failed to load existing thumbnails: {}", e);
                    }
                    self.thumbnail_service = Some(service);
                }
                Err(e) => {
                    tracing::error!("Failed to create thumbnail service: {}", e);
                    return Task::none();
                }
            }
        }

        // If the queue is empty, nothing to do
        if self.thumbnail_queue.is_empty() {
            self.thumbnail_generation_active = false;
            return Task::none();
        }

        self.thumbnail_generation_active = true;

        // Drain the next batch from the front of the queue
        let batch_end = self.thumbnail_queue.len().min(Self::THUMBNAIL_BATCH_SIZE);
        let batch: Vec<(i64, String, String, i32)> =
            self.thumbnail_queue.drain(..batch_end).collect();
        let remaining = self.thumbnail_queue.len();

        tracing::info!(
            "Starting thumbnail batch: {} photos ({} remaining in queue)",
            batch.len(),
            remaining
        );

        let drive_path = drive_path.clone();
        let thumb_size = self.configured_thumbnail_size();
        let epoch = self.thumbnail_generation_epoch;

        // Clone the shared service into an Arc so all spawn_blocking calls reuse it.
        let service = Arc::new(
            self.thumbnail_service
                .take()
                .expect("thumbnail_service was just set above"),
        );
        let service_for_restore = Arc::clone(&service);

        // Spawn background thumbnail generation for this batch only
        Task::perform(
            async move {
                let mut join_set = JoinSet::new();

                for (photo_id, file_path, file_hash, orientation) in batch {
                    let full_path = drive_path.join(&file_path);
                    let svc = Arc::clone(&service);

                    join_set.spawn_blocking(move || {
                        if !full_path.exists() {
                            return None;
                        }

                        match svc.generate_thumbnail(
                            &full_path,
                            &file_hash,
                            orientation,
                            thumb_size,
                        ) {
                            Ok(path) => Some((photo_id, path)),
                            Err(e) => {
                                tracing::debug!(
                                    "Thumbnail generation failed for {}: {}",
                                    file_path,
                                    e
                                );
                                None
                            }
                        }
                    });
                }

                // Collect results as they complete
                let mut results = Vec::new();
                while let Some(res) = join_set.join_next().await {
                    if let Ok(Some((id, path))) = res {
                        results.push((id, path));
                    }
                }

                // Return the Arc so we can restore the service
                (results, service_for_restore)
            },
            move |(results, _service_arc)| Message::ThumbnailBatchReady(epoch, results),
        )
    }
}
