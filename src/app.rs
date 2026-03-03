//! Main application state and logic

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_channel::Receiver;
use iced::keyboard;
use iced::widget::{container, row};
use iced::{event, window, Element, Length, Subscription, Task};

use crate::components::{ScanProgressView, Sidebar};
use crate::db::{create_schema, Database, PhotoRepo};
use crate::models::Photo;
use crate::services::{DriveDetector, DriveInfo, ScanProgress, ThumbnailService, ThumbnailSize};
use tokio::task::JoinSet;
use crate::theme::colors::Backgrounds;
use crate::views::{
    PeopleView, PhotoDetailView, SearchView, SettingsView, TimelineView, WelcomeView,
};

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Scanning,
    Timeline,
    People,
    Search,
    Settings,
    PhotoDetail,
}

/// Active scanning state
pub struct ScanState {
    pub progress: ScanProgress,
    pub progress_receiver: Receiver<ScanProgress>,
    pub cancel_flag: Arc<AtomicBool>,
}

/// Application state
pub struct PhotoVault {
    /// Current active view
    current_view: View,

    /// Detected drives
    drives: Vec<DriveInfo>,

    /// Currently selected drive path
    selected_drive: Option<PathBuf>,

    /// Database connection (if a drive is selected and not scanning)
    database: Option<Database>,

    /// Active scanning state
    scan_state: Option<ScanState>,

    /// Photo count after indexing
    photo_count: i64,

    // --- Phase 3 additions ---
    /// All loaded photos (from DB, ordered by date DESC)
    photos: Vec<Photo>,

    /// Currently selected photo index (for detail view)
    selected_photo_index: Option<usize>,

    /// Thumbnail service (created when a drive is selected)
    thumbnail_service: Option<ThumbnailService>,

    /// Whether we're currently generating thumbnails in the background
    thumbnail_generation_active: bool,

    /// Current window width in pixels (for responsive grid columns)
    window_width: f32,
}

/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Navigate to a different view
    NavigateTo(View),

    /// Select a drive to index
    SelectDrive(PathBuf),

    /// Open folder browser dialog
    BrowseForFolder,

    /// Folder selected from browser
    FolderSelected(Option<PathBuf>),

    /// Refresh drive list
    RefreshDrives,

    /// Drives detected
    DrivesDetected(Vec<DriveInfo>),

    /// Start scanning the selected drive
    StartScan,

    /// Poll scan channels (from subscription tick)
    PollScanChannels,

    /// Scan progress update received
    ScanProgressUpdate(ScanProgress),

    /// Cancel ongoing scan
    CancelScan,

    /// Scan finished -- database returned from scanner thread
    ScanFinished(ScanResult),

    /// Scan complete -- user clicked "Continue"
    ScanComplete,

    // --- Phase 3 additions ---
    /// Photos loaded from database
    PhotosLoaded(Vec<Photo>),

    /// Select a photo to view in detail
    SelectPhoto(i64),

    /// Close photo detail view
    ClosePhotoDetail,

    /// Navigate to previous photo
    PreviousPhoto,

    /// Navigate to next photo
    NextPhoto,

    /// Thumbnail generation completed for a photo
    ThumbnailReady {
        photo_id: i64,
        path: PathBuf,
    },

    /// Batch of thumbnails ready
    ThumbnailBatchReady(Vec<(i64, PathBuf)>),

    /// Keyboard event
    KeyPressed(keyboard::Key),

    /// No-op message (used as callback when we don't need the result)
    NoOp,

    /// Window was resized
    WindowResized(f32, f32),
}

/// Wrapper for scan result to make it Debug + Clone for Message
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub photo_count: i64,
    pub final_progress: ScanProgress,
}

impl PhotoVault {
    /// Create new application instance
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            current_view: View::Welcome,
            drives: Vec::new(),
            selected_drive: None,
            database: None,
            scan_state: None,
            photo_count: 0,
            // Phase 3
            photos: Vec::new(),
            selected_photo_index: None,
            thumbnail_service: None,
            thumbnail_generation_active: false,
            window_width: 1280.0, // sensible default until first resize event
        };

        // Detect drives on startup
        let task = Task::perform(
            async { DriveDetector::detect() },
            Message::DrivesDetected,
        );

        (app, task)
    }

    /// Application title
    pub fn title(&self) -> String {
        match &self.selected_drive {
            Some(path) => format!("PhotoVault - {}", path.display()),
            None => "PhotoVault".to_string(),
        }
    }

    /// Subscription for polling scan progress, keyboard events, and window resize
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        // Scan progress polling
        if self.scan_state.is_some() {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(50))
                    .map(|_| Message::PollScanChannels),
            );
        }

        // Keyboard events (for photo detail navigation) + Window resize events
        subs.push(event::listen_with(|event, _status, _id| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                Some(Message::KeyPressed(key))
            }
            iced::Event::Window(window::Event::Resized(size)) => {
                Some(Message::WindowResized(size.width, size.height))
            }
            _ => None,
        }));

        Subscription::batch(subs)
    }

    /// Load photos from database
    fn load_photos(&self) -> Task<Message> {
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
                        let mut photos = repo.get_all_by_date(50000, 0).unwrap_or_default();

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

    /// Start background thumbnail generation for loaded photos
    ///
    /// Uses a shared Arc<ThumbnailService> and spawns all tasks concurrently
    /// via JoinSet so thumbnails are generated in parallel (up to the
    /// ConcurrencyLimiter's cap of 4).
    fn start_thumbnail_generation(&mut self) -> Task<Message> {
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

        // Collect photos that need thumbnails
        let photos_needing_thumbs: Vec<(i64, String, String)> = self
            .photos
            .iter()
            .filter(|p| p.thumbnail_path.is_none())
            .map(|p| (p.id, p.file_path.clone(), p.file_hash.clone()))
            .collect();

        if photos_needing_thumbs.is_empty() {
            return Task::none();
        }

        self.thumbnail_generation_active = true;
        let drive_path = drive_path.clone();

        // Clone the shared service into an Arc so all spawn_blocking calls reuse it.
        // ThumbnailService is Send+Sync (all fields are PathBuf / Arc<RwLock<..>> / Arc<Mutex<..>>).
        let service = Arc::new(
            self.thumbnail_service
                .take()
                .expect("thumbnail_service was just set above"),
        );
        // We'll put it back after generation completes (via Arc::try_unwrap)
        let service_for_restore = Arc::clone(&service);

        // Spawn background thumbnail generation with concurrent JoinSet
        Task::perform(
            async move {
                let mut join_set = JoinSet::new();

                for (photo_id, file_path, file_hash) in photos_needing_thumbs {
                    let full_path = drive_path.join(&file_path);
                    let svc = Arc::clone(&service);

                    join_set.spawn_blocking(move || {
                        if !full_path.exists() {
                            return None;
                        }

                        match svc.generate_thumbnail(
                            &full_path,
                            &file_hash,
                            ThumbnailSize::Medium,
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
            |(results, _service_arc)| Message::ThumbnailBatchReady(results),
        )
    }

    /// Handle messages
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NavigateTo(view) => {
                // If navigating to Timeline, always reload photos from DB
                // (photos may have new thumbnails, or user may have re-scanned)
                let task = if view == View::Timeline {
                    self.load_photos()
                } else {
                    Task::none()
                };
                self.current_view = view;
                task
            }

            Message::SelectDrive(path) => {
                tracing::info!("Selected drive: {:?}", path);

                match Database::open_for_drive(&path) {
                    Ok(db) => {
                        // Create schema if needed
                        if db.needs_schema().unwrap_or(true) {
                            if let Err(e) = create_schema(&db.conn) {
                                tracing::error!("Failed to create schema: {}", e);
                                return Task::none();
                            }
                        }

                        // Get photo count
                        let repo = PhotoRepo::new(&db.conn);
                        self.photo_count = repo.count().unwrap_or(0);

                        self.selected_drive = Some(path);
                        self.database = Some(db);

                        // If library is empty, start scanning
                        if self.photo_count == 0 {
                            return self.update(Message::StartScan);
                        } else {
                            self.current_view = View::Timeline;
                            // Load photos for timeline
                            return self.load_photos();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database: {}", e);
                    }
                }

                Task::none()
            }

            Message::BrowseForFolder => {
                tracing::info!("Browse for folder requested");
                Task::perform(
                    async {
                        let result = rfd::AsyncFileDialog::new()
                            .set_title("Select a folder containing your photos")
                            .pick_folder()
                            .await;
                        result.map(|handle| handle.path().to_path_buf())
                    },
                    Message::FolderSelected,
                )
            }

            Message::FolderSelected(path) => {
                if let Some(path) = path {
                    return self.update(Message::SelectDrive(path));
                }
                Task::none()
            }

            Message::RefreshDrives => Task::perform(
                async { DriveDetector::detect() },
                Message::DrivesDetected,
            ),

            Message::DrivesDetected(drives) => {
                tracing::info!("Detected {} drives", drives.len());
                self.drives = drives;
                Task::none()
            }

            Message::StartScan => {
                let Some(drive_path) = &self.selected_drive else {
                    return Task::none();
                };

                // Take the database -- scanner will own it
                let Some(database) = self.database.take() else {
                    tracing::error!("No database available for scanning");
                    return Task::none();
                };

                tracing::info!("Starting scan of {:?}", drive_path);
                self.current_view = View::Scanning;

                let drive_path = drive_path.clone();

                // Start the scanner
                let (progress_rx, cancel_flag, join_handle) =
                    crate::services::scanner::start_scan(drive_path, database);

                // Store scan state
                self.scan_state = Some(ScanState {
                    progress: ScanProgress::default(),
                    progress_receiver: progress_rx,
                    cancel_flag,
                });

                // Spawn a task to await the join handle and return the result
                Task::perform(
                    async move {
                        match join_handle.await {
                            Ok(result) => {
                                let count = PhotoRepo::new(&result.database.conn)
                                    .count()
                                    .unwrap_or(0);
                                (
                                    result.database,
                                    ScanResult {
                                        photo_count: count,
                                        final_progress: result.final_progress,
                                    },
                                )
                            }
                            Err(e) => {
                                tracing::error!("Scanner thread panicked: {}", e);
                                panic!("Scanner thread panicked: {}", e);
                            }
                        }
                    },
                    |(_database, scan_result)| {
                        // We need to return the database AND the result.
                        // Since Message must be Clone+Debug, we return just the result
                        // and handle database restoration separately.
                        Message::ScanFinished(scan_result)
                    },
                )
            }

            Message::PollScanChannels => {
                if let Some(ref mut state) = self.scan_state {
                    // Drain all available progress updates
                    while let Ok(progress) = state.progress_receiver.try_recv() {
                        state.progress = progress;
                    }
                }
                Task::none()
            }

            Message::ScanProgressUpdate(progress) => {
                if let Some(ref mut state) = self.scan_state {
                    state.progress = progress;
                }
                Task::none()
            }

            Message::CancelScan => {
                if let Some(ref state) = self.scan_state {
                    state.cancel_flag.store(true, Ordering::Relaxed);
                    tracing::info!("Scan cancellation requested");
                }
                // Don't clear scan_state yet -- wait for ScanFinished
                Task::none()
            }

            Message::ScanFinished(result) => {
                tracing::info!(
                    "Scan finished: {} photos indexed",
                    result.photo_count
                );
                self.photo_count = result.photo_count;

                // Update the final progress in scan state so UI shows completion
                if let Some(ref mut state) = self.scan_state {
                    state.progress = result.final_progress;
                }

                // Re-open the database (scanner consumed it, we need a fresh connection)
                if let Some(ref drive_path) = self.selected_drive {
                    match Database::open_for_drive(drive_path) {
                        Ok(db) => {
                            self.database = Some(db);
                        }
                        Err(e) => {
                            tracing::error!("Failed to re-open database: {}", e);
                        }
                    }
                }

                Task::none()
            }

            Message::ScanComplete => {
                // User clicked "Continue" after scan completed
                self.scan_state = None;
                self.current_view = View::Timeline;
                // Load photos for the timeline
                self.load_photos()
            }

            // --- Phase 3 handlers ---
            Message::PhotosLoaded(photos) => {
                tracing::info!("Loaded {} photos for timeline", photos.len());
                self.photos = photos;
                self.photo_count = self.photos.len() as i64;

                // Start thumbnail generation in background
                self.start_thumbnail_generation()
            }

            Message::SelectPhoto(photo_id) => {
                // Find the photo index
                if let Some(idx) = self.photos.iter().position(|p| p.id == photo_id) {
                    self.selected_photo_index = Some(idx);
                    self.current_view = View::PhotoDetail;
                }
                Task::none()
            }

            Message::ClosePhotoDetail => {
                self.selected_photo_index = None;
                self.current_view = View::Timeline;
                Task::none()
            }

            Message::PreviousPhoto => {
                if let Some(ref mut idx) = self.selected_photo_index {
                    if *idx > 0 {
                        *idx -= 1;
                    }
                }
                Task::none()
            }

            Message::NextPhoto => {
                if let Some(ref mut idx) = self.selected_photo_index {
                    if *idx + 1 < self.photos.len() {
                        *idx += 1;
                    }
                }
                Task::none()
            }

            Message::ThumbnailReady { photo_id, path } => {
                // Update the photo's thumbnail path in our in-memory list (absolute for UI)
                if let Some(photo) = self.photos.iter_mut().find(|p| p.id == photo_id) {
                    photo.thumbnail_path = Some(path.to_string_lossy().to_string());
                }

                // Update DB (fire-and-forget) — store relative path for portability
                if let Some(ref drive_path) = self.selected_drive {
                    let drive_path = drive_path.clone();
                    let rel_path = path
                        .strip_prefix(&drive_path)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    return Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let _ = db.conn.execute(
                                    "UPDATE photos SET thumbnail_path = ?1 WHERE id = ?2",
                                    rusqlite::params![rel_path, photo_id],
                                );
                            }
                        },
                        |_| Message::NoOp,
                    );
                }
                Task::none()
            }

            Message::ThumbnailBatchReady(results) => {
                tracing::info!("Thumbnail batch ready: {} thumbnails", results.len());
                self.thumbnail_generation_active = false;

                // Restore the thumbnail service from the Arc (it was taken in start_thumbnail_generation)
                // If the Arc still has other refs, just recreate from drive_path
                if self.thumbnail_service.is_none() {
                    if let Some(ref drive_path) = self.selected_drive {
                        if let Ok(service) = ThumbnailService::new(drive_path, 2.0) {
                            self.thumbnail_service = Some(service);
                        }
                    }
                }

                // Update in-memory photo data and DB
                if let Some(ref drive_path) = self.selected_drive {
                    // Update in-memory list (keep absolute paths for UI display)
                    for (photo_id, path) in &results {
                        if let Some(photo) = self.photos.iter_mut().find(|p| p.id == *photo_id) {
                            photo.thumbnail_path =
                                Some(path.to_string_lossy().to_string());
                        }
                    }

                    // Batch update DB (store relative paths for portability)
                    let drive_path = drive_path.clone();
                    let results_for_db = results;
                    return Task::perform(
                        async move {
                            if let Ok(db) = Database::open_for_drive(&drive_path) {
                                let tx = db.conn.unchecked_transaction();
                                if let Ok(tx) = tx {
                                    for (photo_id, path) in &results_for_db {
                                        // Store relative path: strip drive_root prefix
                                        let rel_path = path
                                            .strip_prefix(&drive_path)
                                            .unwrap_or(path)
                                            .to_string_lossy()
                                            .to_string();
                                        let _ = tx.execute(
                                            "UPDATE photos SET thumbnail_path = ?1 WHERE id = ?2",
                                            rusqlite::params![rel_path, photo_id],
                                        );
                                    }
                                    let _ = tx.commit();
                                }
                            }
                        },
                        |_| Message::NoOp,
                    );
                }
                Task::none()
            }

            Message::KeyPressed(key) => {
                if self.current_view == View::PhotoDetail {
                    match key {
                        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                            return self.update(Message::PreviousPhoto);
                        }
                        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                            return self.update(Message::NextPhoto);
                        }
                        keyboard::Key::Named(keyboard::key::Named::Escape) => {
                            return self.update(Message::ClosePhotoDetail);
                        }
                        _ => {}
                    }
                }
                Task::none()
            }

            Message::NoOp => Task::none(),

            Message::WindowResized(width, _height) => {
                self.window_width = width;
                Task::none()
            }
        }
    }

    /// Render the application
    pub fn view(&self) -> Element<'_, Message> {
        // Show scanning progress if scanning
        if self.current_view == View::Scanning {
            if let Some(ref state) = self.scan_state {
                return ScanProgressView::view(&state.progress);
            } else {
                return ScanProgressView::view(&ScanProgress::default());
            }
        }

        // Photo detail view (full-screen overlay, no sidebar)
        if self.current_view == View::PhotoDetail {
            if let Some(idx) = self.selected_photo_index {
                if let Some(photo) = self.photos.get(idx) {
                    let has_prev = idx > 0;
                    let has_next = idx + 1 < self.photos.len();
                    if let Some(ref drive_path) = self.selected_drive {
                        return PhotoDetailView::view(photo, has_prev, has_next, drive_path);
                    }
                }
            }
            // Fallback: shouldn't happen, but return to timeline
            return TimelineView::view();
        }

        // If no drive selected, show welcome screen
        if self.selected_drive.is_none() {
            return WelcomeView::view(&self.drives);
        }

        // Main layout: sidebar + content
        let sidebar = Sidebar::view(&self.current_view);

        let content = match self.current_view {
            View::Welcome => WelcomeView::view(&self.drives),
            View::Scanning => unreachable!(), // Handled above
            View::Timeline => {
                if self.photos.is_empty() {
                    TimelineView::view()
                } else {
                    // Calculate responsive column count:
                    // Sidebar is ~200px, padding 32px total, each thumb is 160+8px gap
                    let available_width = (self.window_width - 200.0 - 32.0).max(168.0);
                    let columns = (available_width / 168.0).floor().max(2.0) as usize;
                    TimelineView::view_with_photos(&self.photos, columns)
                }
            }
            View::People => PeopleView::view(),
            View::Search => SearchView::view(),
            View::Settings => SettingsView::view(),
            View::PhotoDetail => unreachable!(), // Handled above
        };

        let layout = row![sidebar, content,];

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }
}
