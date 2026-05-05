use std::path::PathBuf;

use iced::Task;

use crate::db::{
    AlbumRepo, AlbumSuggestionRepo, Database, DocumentRepo, FaceRepo, PhotoRepo, TrashRepo,
};
use crate::services::image_utils::apply_exif_orientation;
use crate::services::map_math;
use crate::services::{FaceProcessor, TrashService, TrashStats};

use super::super::messages::Message;
use super::PhotoVault;

/// wgpu maximum buffer is 268 435 456 bytes → 8192×8192 RGBA.
/// Only images exceeding this hard GPU limit get downscaled.
const WGPU_MAX_DIM: u32 = 8192;

impl PhotoVault {
    /// Load photos from database
    pub(crate) fn load_photos(&mut self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();
        self.photos_loading = true;

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = PhotoRepo::new(&db.conn);

                        // Load up to 250K photos. At ~500 bytes per
                        // Photo record that's ~125 MB of in-memory
                        // metadata, still well within desktop budgets
                        // but 5× the previous 50K ceiling. Libraries
                        // beyond 250K will need the deferred
                        // PhotoLibrary cursor work tracked on the
                        // Phase 2 backlog.
                        const LOAD_LIMIT: i64 = 250_000;
                        let mut photos = match repo.get_all_by_date(LOAD_LIMIT, 0) {
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
                                photo.thumbnail_path = Some(abs_path.to_string_lossy().to_string());
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
    pub(crate) fn load_face_clusters(&mut self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };

        let drive_path = drive_path.clone();
        self.face_clusters_loading = true;

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
                        if let Err(e) =
                            face_repo.populate_face_thumbnails(&mut clusters, &drive_path)
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

    /// Load all albums from the database.
    pub(crate) fn load_albums(&mut self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };
        let drive_path = drive_path.clone();
        self.albums_loading = true;

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = AlbumRepo::new(&db.conn);
                        let mut albums = repo.get_all().unwrap_or_default();

                        // Resolve cover photo thumbnail paths
                        let photo_repo = PhotoRepo::new(&db.conn);
                        for album in &mut albums {
                            if let Some(cover_id) = album.cover_photo_id {
                                if let Ok(Some(photo)) = photo_repo.get_by_id(cover_id) {
                                    album.cover_thumbnail_path = photo
                                        .thumbnail_path
                                        .map(|p| drive_path.join(p).to_string_lossy().to_string());
                                }
                            }
                        }

                        albums
                    }
                    Err(e) => {
                        tracing::error!("Failed to load albums: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::AlbumsLoaded,
        )
    }

    /// Load photos for an album detail view.
    pub(crate) fn load_album_photos(&self, album_id: i64) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };
        let drive_path = drive_path.clone();

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let album_repo = AlbumRepo::new(&db.conn);
                        let photo_ids =
                            album_repo.get_album_photo_ids(album_id).unwrap_or_default();

                        if photo_ids.is_empty() {
                            return Vec::new();
                        }

                        let photo_repo = PhotoRepo::new(&db.conn);
                        let mut photos: Vec<crate::models::Photo> = photo_ids
                            .iter()
                            .filter_map(|id| photo_repo.get_by_id(*id).ok())
                            .flatten()
                            .collect();

                        for photo in &mut photos {
                            if let Some(ref rel_path) = photo.thumbnail_path {
                                let abs_path = drive_path.join(rel_path);
                                photo.thumbnail_path = Some(abs_path.to_string_lossy().to_string());
                            }
                        }

                        photos
                    }
                    Err(e) => {
                        tracing::error!("Failed to load album photos: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::AlbumPhotosLoaded,
        )
    }

    pub(crate) fn load_photo_detail_for_index(&mut self, idx: usize) -> Task<Message> {
        let Some(photo) = self.photos.get(idx) else {
            return Task::none();
        };

        self.photo_rotation = 0;

        // Keep this synchronous path lightweight. Image decode happens on
        // background tasks below to avoid UI hitching when opening from
        // Insights day/month drill-ins.
        self.current_display_image = None;

        // Reset mini-map state to this photo's GPS (if present).
        if let (Some(lat), Some(lng)) = (photo.gps_latitude, photo.gps_longitude) {
            self.photo_map_center = Some(map_math::LatLng { lat, lng });
            self.photo_map_zoom = 13;
            self.photo_map_drag_origin = None;
        } else {
            self.photo_map_center = None;
        }

        let photo_id = photo.id;

        self.current_photo_people.clear();
        self.current_photo_face_count = 0;
        self.current_photo_location = None;
        self.current_photo_location_resolved = false;
        self.current_photo_albums.clear();
        if let Some(ref db) = self.database {
            let face_repo = FaceRepo::new(&db.conn);
            if let Ok(people) = face_repo.get_people_for_photo(photo_id) {
                self.current_photo_people = people;
            }
            if let Ok(count) = db.conn.query_row(
                "SELECT COUNT(*) FROM faces WHERE photo_id = ?1",
                rusqlite::params![photo_id],
                |row| row.get::<_, i64>(0),
            ) {
                self.current_photo_face_count = count as usize;
            }
            // Load album membership
            let album_repo = AlbumRepo::new(&db.conn);
            if let Ok(albums) = album_repo.get_albums_for_photo(photo_id) {
                self.current_photo_albums = albums;
            }
        }

        // Resolve display location: prefer stored city/country, fall back to
        // an on-the-fly geocode lookup from the GeoNames DB.
        self.current_photo_location = photo.location_string();
        // If the DB already has a name, mark as resolved so the view
        // doesn't show the "looking up…" copy.
        self.current_photo_location_resolved = self.current_photo_location.is_some();

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
            let photo_id = photo.id;
            let coords = (photo.gps_latitude, photo.gps_longitude);

            let mut tasks: Vec<Task<Message>> = Vec::new();

            // Fast placeholder decode from thumbnail, then full-resolution swap.
            if let Some(ref thumb_path) = photo.thumbnail_path {
                let thumb = PathBuf::from(thumb_path);
                if thumb.exists() {
                    let orientation = photo.orientation;
                    tasks.push(Task::perform(
                        async move {
                            let result = tokio::task::spawn_blocking(move || {
                                let img = image::open(&thumb).ok()?;
                                let img = apply_exif_orientation(img, orientation);
                                let rgba = img.to_rgba8();
                                let (w, h) = (rgba.width(), rgba.height());
                                Some((rgba.into_raw(), w, h))
                            })
                            .await
                            .ok()
                            .flatten();
                            match result {
                                Some((bytes, w, h)) => {
                                    Message::DisplayImageReady(Some(bytes), w, h)
                                }
                                None => Message::DisplayImageReady(None, 0, 0),
                            }
                        },
                        |m| m,
                    ));
                }
            }

            // Async reverse-geocode only when DB location is missing.
            if self.current_photo_location.is_none() {
                if let (Some(lat), Some(lng)) = coords {
                    tasks.push(Task::perform(
                        async move {
                            let resolved = tokio::task::spawn_blocking(move || {
                                let geonames_path = crate::db::geonames::geonames_db_path();
                                if !geonames_path.exists() {
                                    return None;
                                }
                                let geo =
                                    crate::services::GeocodingService::new(&geonames_path).ok()?;
                                let r = geo.reverse_geocode(lat, lng)?;
                                Some(format!("{}, {}", r.city, r.country))
                            })
                            .await
                            .ok()
                            .flatten();
                            Message::PhotoLocationResolved {
                                photo_id,
                                location: resolved,
                            }
                        },
                        |m| m,
                    ));
                }
            }

            // Pre-fetch the mini-map tile. Dispatch MapTileFetched on
            // completion so iced re-renders and the mini-map picks up
            // the newly cached tile.
            if let (Some(lat), Some(lng), Some(cache)) = (
                photo.gps_latitude,
                photo.gps_longitude,
                self.tile_cache.clone(),
            ) {
                let tile = map_math::TileId {
                    z: 13,
                    x: map_math::lng_to_tile_x(lng, 13) as u32,
                    y: map_math::lat_to_tile_y(lat, 13) as u32,
                };
                tasks.push(Task::perform(
                    async move {
                        match cache.get_or_fetch(tile).await {
                            Ok(_) => Message::MapTileFetched(tile),
                            Err(e) => Message::MapTileFetchFailed(tile, e),
                        }
                    },
                    |m| m,
                ));
            }

            tasks.push(Task::perform(
                async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let img = image::open(&path).ok()?;
                        let img = apply_exif_orientation(img, orientation);
                        // Only downscale if the image exceeds the GPU hard
                        // limit (wgpu max buffer = 268 MB ≈ 8192×8192 RGBA).
                        let img = if img.width() > WGPU_MAX_DIM || img.height() > WGPU_MAX_DIM {
                            img.resize(
                                WGPU_MAX_DIM,
                                WGPU_MAX_DIM,
                                image::imageops::FilterType::Lanczos3,
                            )
                        } else {
                            img
                        };
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
            ));

            return Task::batch(tasks);
        }

        Task::none()
    }

    /// Load pending album suggestions from the database.
    pub(crate) fn load_suggestions(&self) -> Task<Message> {
        let Some(ref drive_path) = self.selected_drive else {
            return Task::none();
        };
        let drive_path = drive_path.clone();

        Task::perform(
            async move {
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => {
                        let repo = AlbumSuggestionRepo::new(&db.conn);
                        let mut suggestions = repo.get_pending().unwrap_or_default();

                        // Resolve cover thumbnail paths
                        let photo_repo = PhotoRepo::new(&db.conn);
                        for s in &mut suggestions {
                            if let Some(cover_id) = s.cover_photo_id {
                                if let Ok(Some(photo)) = photo_repo.get_by_id(cover_id) {
                                    s.cover_thumbnail_path = photo
                                        .thumbnail_path
                                        .map(|p| drive_path.join(p).to_string_lossy().to_string());
                                }
                            }
                        }

                        suggestions
                    }
                    Err(e) => {
                        tracing::error!("Failed to load suggestions: {}", e);
                        Vec::new()
                    }
                }
            },
            Message::SuggestionsLoaded,
        )
    }
}
