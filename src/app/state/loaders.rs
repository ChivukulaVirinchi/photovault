use std::path::PathBuf;

use iced::Task;

use crate::db::{Database, DocumentRepo, FaceRepo, PhotoRepo, TrashRepo};
use crate::services::{FaceProcessor, TrashService, TrashStats};
use crate::services::image_utils::apply_exif_orientation;

use super::super::messages::Message;
use super::PhotoVault;

impl PhotoVault {
    /// Load photos from database
    pub(crate) fn load_photos(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = PhotoRepo::new(&db.conn);
                        // Load all photos (up to 50k for now)
                        let mut photos = match repo.get_all_by_date(50000, 0) {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::error!("Failed to load photos: {}", e);
                                Vec::new()
                            }
                        };

                        // Resolve relative thumbnail paths to absolute (DB stores relative for portability)
                        for photo in &mut photos {
                            if let Some(ref rel_path) = photo.thumbnail_path {
                                let abs_path = drive_path.join(rel_path);
                                photo.thumbnail_path =
                                    Some(abs_path.to_string_lossy().to_string());
                            }
                        }

                        photos
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database for loading photos: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::PhotosLoaded,
        )
    }

    /// Load face clusters from the database
    pub(crate) fn load_face_clusters(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();

        Task::perform(
            async move {
                // Regenerate any missing face crop thumbnails in a blocking thread
                // (handles faces detected before crop-saving code was added)
                let drive_for_regen = drive_path.clone();
                let regen_result = tokio::task::spawn_blocking(move || {
                    FaceProcessor::regenerate_missing_crops(&drive_for_regen)
                })
                .await;
                match regen_result {
                    Ok(Ok(count)) => {
                        if count > 0 {
                            tracing::info!("Regenerated {} face crop thumbnails", count);
                        }
                    }
                    Ok(Err(e)) => tracing::warn!("Failed to regenerate face crops: {}", e),
                    Err(e) => tracing::warn!("Face crop regeneration task panicked: {}", e),
                }

                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let face_repo = FaceRepo::new(&db.conn);
                        if let Err(e) = face_repo.normalize_cluster_stats() {
                            tracing::warn!("Failed to normalize cluster stats: {}", e);
                        }
                        let mut clusters = face_repo.get_all_clusters().unwrap_or_default();
                        tracing::info!(
                            "load_face_clusters: got {} clusters from DB",
                            clusters.len()
                        );
                        if let Err(e) = face_repo.populate_face_thumbnails(&mut clusters, &drive_path)
                        {
                            tracing::warn!("Failed to populate face thumbnails: {}", e);
                        }
                        clusters
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database for face clusters: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::FaceClustersLoaded,
        )
    }

    /// Load trash items and stats from DB.
    pub(crate) fn load_trash(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();
        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = TrashRepo::new(&db.conn);
                        let items = repo.get_all().unwrap_or_default();
                        let stats = TrashService::get_stats(&db.conn).unwrap_or_default();
                        (items, stats)
                    }
                    Err(e) => {
                        tracing::error!("Failed to open DB for trash load: {}", e);
                        (Vec::new(), TrashStats::default())
                    }
                }
            },
            |(items, stats)| Message::TrashLoaded(items, stats),
        )
    }

    pub(crate) fn load_documents(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();
        let query = self.documents_query.clone();
        let filter = self.documents_filter;

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = DocumentRepo::new(&db.conn);
                        let mut docs = if !query.trim().is_empty() {
                            repo.search_documents_fts(&query, 50000, 0)
                                .unwrap_or_default()
                        } else if let Some(cat) = filter {
                            repo.get_documents_by_category(cat.as_str(), 50000, 0)
                                .unwrap_or_default()
                        } else {
                            repo.get_non_photo_documents(50000, 0).unwrap_or_default()
                        };

                        for photo in &mut docs {
                            if let Some(ref rel_path) = photo.thumbnail_path {
                                let abs_path = drive_path.join(rel_path);
                                photo.thumbnail_path = Some(abs_path.to_string_lossy().to_string());
                            }
                        }

                        docs
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database for documents: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::DocumentsLoaded,
        )
    }

    pub(crate) fn load_photo_detail_for_index(&mut self, idx: usize) -> Task<Message> {
        let Some(photo) = self.photos.get(idx) else {
            return Task::none();
        };

        self.photo_rotation = 0;
        self.current_display_image = None;

        let photo_id = photo.id;

        self.current_photo_people.clear();
        self.current_photo_face_count = 0;
        if let Some(ref db) = self.database {
            let face_repo = FaceRepo::new(&db.conn);
            if let Ok(names) = face_repo.get_person_names_for_photo(photo_id) {
                self.current_photo_people = names;
            }
            if let Ok(count) = db.conn.query_row(
                "SELECT COUNT(*) FROM faces WHERE photo_id = ?1",
                rusqlite::params![photo_id],
                |row| row.get::<_, i64>(0),
            ) {
                self.current_photo_face_count = count as usize;
            }
        }

        // Always prefer original image for full-quality photo detail viewing.
        // Fall back to thumbnail only when the original is unavailable.
        let image_path = if let Some(ref drive) = self.selected_drive {
            let orig = drive.join(&photo.file_path);
            if orig.exists() {
                Some(orig)
            } else if let Some(ref tp) = photo.thumbnail_path {
                let thumb = PathBuf::from(tp);
                if thumb.exists() {
                    Some(thumb)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(path) = image_path {
            let orientation = photo.orientation;
            return Task::perform(
                async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let img = image::open(&path).ok()?;
                        let img = apply_exif_orientation(img, orientation);
                        let rgba = img.to_rgba8();
                        let (w, h) = (rgba.width(), rgba.height());
                        Some((rgba.into_raw(), w, h))
                    })
                    .await
                    .ok()
                    .flatten();
                    match result {
                        Some((bytes, w, h)) => Message::DisplayImageReady(Some(bytes), w, h),
                        None => Message::DisplayImageReady(None, 0, 0),
                    }
                },
                |msg| msg,
            );
        }

        Task::none()
    }
}
