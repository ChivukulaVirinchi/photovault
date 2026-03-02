//! Main application state and logic

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_channel::Receiver;
use iced::widget::{container, row};
use iced::{Element, Length, Subscription, Task};

use crate::components::{ScanProgressView, Sidebar};
use crate::db::{create_schema, Database, PhotoRepo};
use crate::services::{DriveDetector, DriveInfo, ScanProgress};
use crate::theme::colors::Backgrounds;
use crate::views::{PeopleView, SearchView, SettingsView, TimelineView, WelcomeView};

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Scanning,
    Timeline,
    People,
    Search,
    Settings,
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

    /// Subscription for polling scan progress
    pub fn subscription(&self) -> Subscription<Message> {
        if self.scan_state.is_some() {
            iced::time::every(std::time::Duration::from_millis(50))
                .map(|_| Message::PollScanChannels)
        } else {
            Subscription::none()
        }
    }

    /// Handle messages
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NavigateTo(view) => {
                self.current_view = view;
                Task::none()
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
                    |(database, scan_result)| {
                        // We need to return the database AND the result.
                        // Since Message must be Clone+Debug, we return just the result
                        // and handle database restoration separately.
                        // This is a design limitation -- we'll use a different approach.
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
                // Show initial scanning state
                return ScanProgressView::view(&ScanProgress::default());
            }
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
            View::Timeline => TimelineView::view(),
            View::People => PeopleView::view(),
            View::Search => SearchView::view(),
            View::Settings => SettingsView::view(),
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
