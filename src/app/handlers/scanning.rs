//! Handlers for drive/folder selection and scanning.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use iced::Task;

use crate::db::{create_schema, migrations, AlbumSuggestionRepo, Database, PhotoRepo};
use crate::services::{DriveDetector, DriveInfo, ScanProgress};

use super::super::messages::{Message, ScanResult};
use super::super::state::{PhotoVault, ScanState, View};

/// Map a View to a stable config string (only top-level views are persisted).
fn view_to_config_string(v: &View) -> Option<&'static str> {
    Some(match v {
        View::Timeline => "timeline",
        View::Map => "map",
        View::Memories => "memories",
        View::Albums => "albums",
        View::Insights => "insights",
        View::Search => "search",
        View::People => "people",
        View::Documents => "documents",
        View::Duplicates => "duplicates",
        View::Bursts => "bursts",
        View::Trash => "trash",
        View::Settings => "settings",
        // Detail / transient views: don't persist.
        _ => return None,
    })
}

pub(crate) fn config_string_to_view(s: &str) -> Option<View> {
    Some(match s {
        "timeline" => View::Timeline,
        "map" => View::Map,
        "memories" => View::Memories,
        "albums" => View::Albums,
        "insights" => View::Insights,
        "search" => View::Search,
        "people" => View::People,
        "documents" => View::Documents,
        "duplicates" => View::Duplicates,
        "bursts" => View::Bursts,
        "trash" => View::Trash,
        "settings" => View::Settings,
        _ => return None,
    })
}

pub(crate) fn navigate_to(app: &mut PhotoVault, view: View) -> Task<Message> {
    tracing::info!("NavigateTo: {:?}", view);
    app.sidebar_highlight_index = Some(match view {
        View::Timeline => 0,
        View::Map => 1,
        View::Memories => 2,
        View::Albums | View::AlbumDetail => 3,
        View::Insights => 4,
        View::Search | View::Cull => 5,
        View::People | View::ClusterDetail => 6,
        View::FaceReview => 7,
        View::Duplicates | View::DuplicateDetail => 8,
        View::Bursts | View::BurstDetail => 9,
        View::Trash => 10,
        View::Documents => 11,
        View::Settings => 12,
        _ => app.sidebar_highlight_index.unwrap_or(0),
    });
    if view == app.current_view {
        return Task::none();
    }
    if view != View::Timeline {
        app.begin_thumbnail_generation_epoch();
    }
    // Persist the new view for next launch (only top-level views).
    if let Some(key) = view_to_config_string(&view) {
        let want = Some(key.to_string());
        if app.config.last_view != want {
            app.config.last_view = want;
            if let Err(e) = app.config.save() {
                tracing::debug!("Failed to persist last_view: {}", e);
            }
        }
    }
    // If navigating to Timeline, always reload photos from DB
    // (photos may have new thumbnails, or user may have re-scanned)
    let task = if view == View::Timeline {
        let restore = iced::widget::scrollable::scroll_to(
            iced::widget::scrollable::Id::new("timeline"),
            app.timeline_scroll_offset,
        );
        Task::batch(vec![app.load_photos(), restore])
    } else if view == View::People {
        app.load_face_clusters()
    } else if view == View::Map {
        app.current_view = view;
        if app.map_pins_cache.is_empty() {
            return super::map::load_pins(app);
        }
        return Task::none();
    } else if view == View::Documents {
        app.load_documents()
    } else if view == View::Duplicates {
        // Trigger duplicate detection when navigating to Duplicates view
        app.current_view = view;
        return super::handle(app, Message::RunDuplicateDetection);
    } else if view == View::Bursts {
        // Trigger burst detection when navigating to Bursts view
        app.current_view = view;
        return super::handle(app, Message::RunBurstDetection);
    } else if view == View::Trash {
        app.current_view = view;
        return super::handle(app, Message::LoadTrash);
    } else if view == View::FaceReview {
        return super::handle(app, Message::EnterFaceReview);
    } else if view == View::Memories {
        // Cards already in app.memories from startup/tick; just switch view.
        app.current_view = view;
        return Task::none();
    } else if view == View::Albums {
        app.current_view = view;
        return app.load_albums();
    } else if view == View::Insights {
        app.current_view = view;
        if let Some(cached) = app.insights_cache.get(&app.insights_selected_year).cloned() {
            app.insights_data = Some(cached);
            app.insights_loading = false;
            return Task::none();
        }
        let geocode_task = if app.geocoding_progress.is_none() {
            super::handle(app, Message::RunGeocoding)
        } else {
            Task::none()
        };
        return Task::batch([super::insights::load_insights(app), geocode_task]);
    } else if view == View::Search {
        app.current_view = view;
        if app.recent_searches.is_empty() {
            return super::search_cull::load_recent_searches(app);
        }
        return Task::none();
    } else {
        Task::none()
    };
    app.current_view = view;
    task
}

pub(crate) fn select_drive(app: &mut PhotoVault, path: PathBuf) -> Task<Message> {
    tracing::info!("Selected drive: {:?}", path);
    app.begin_thumbnail_generation_epoch();
    app.invalidate_insights_cache();

    match Database::open_for_drive(&path) {
        Ok(db) => {
            // Create schema if needed
            if db.needs_schema().unwrap_or(true) {
                if let Err(e) = create_schema(&db.conn) {
                    tracing::error!("Failed to create schema: {}", e);
                    return super::handle(
                        app,
                        Message::ToastShow(crate::components::toast::Toast::error(
                            "Couldn't initialize database",
                            format!("{}", e),
                        )),
                    );
                }
            }

            // Backup database before migrations
            if let Err(e) = Database::backup(&path, 3) {
                tracing::debug!("DB backup skipped: {}", e);
            }

            if let Err(e) = migrations::run_migrations(&db.conn) {
                tracing::error!("Failed to run migrations: {}", e);
                return super::handle(
                    app,
                    Message::ToastShow(crate::components::toast::Toast::error(
                        "Couldn't migrate database",
                        format!("{}", e),
                    )),
                );
            }

            // Quick integrity check on open
            match db.check_integrity() {
                Ok(true) => {}
                Ok(false) => tracing::warn!("Database integrity check failed for {:?}", path),
                Err(e) => tracing::debug!("Could not run integrity check: {}", e),
            }

            // Get photo count
            let repo = PhotoRepo::new(&db.conn);
            app.photo_count = repo.count().unwrap_or(0);

            app.selected_drive = Some(path);
            if let Some(ref p) = app.selected_drive {
                app.config.remember_drive(p.clone());
                if let Err(e) = app.config.save() {
                    tracing::warn!("Failed to save remembered drive: {}", e);
                }
            }
            app.database = Some(db);

            // Increment seen_count for pending suggestions on drive select
            if let Some(ref db) = app.database {
                let sug_repo = AlbumSuggestionRepo::new(&db.conn);
                let _ = sug_repo.increment_seen_counts();
            }

            super::map::init_tile_cache(app);
            let pin_task = super::map::load_pins(app);

            // Kick off background geocoding once a drive is selected.
            if app.geocoding_progress.is_none() {
                let _ = super::handle(app, Message::RunGeocoding);
            }

            // Kick off the initial Memories generation for today's date.
            let memories_task = if app.memories_enabled {
                let today = chrono::Local::now().date_naive();
                super::memories::regenerate(app, today)
            } else {
                Task::none()
            };

            // Load existing suggestions
            let suggestions_task = app.load_suggestions();
            let detect_suggestions_task = super::handle(app, Message::RunSuggestionDetection);

            // If library is empty, start scanning
            let next = if app.photo_count == 0 {
                super::handle(app, Message::StartScan)
            } else {
                // Existing library: auto-sync incremental changes first.
                super::handle(app, Message::CheckForChanges)
            };

            // Restore last-viewed view (if persisted & not Timeline which is default).
            let restore = if let Some(saved) = app
                .config
                .last_view
                .as_deref()
                .and_then(config_string_to_view)
            {
                if saved != View::Timeline && saved != app.current_view {
                    super::handle(app, Message::NavigateTo(saved))
                } else {
                    Task::none()
                }
            } else {
                Task::none()
            };

            Task::batch(vec![
                pin_task,
                memories_task,
                suggestions_task,
                detect_suggestions_task,
                next,
                restore,
            ])
        }
        Err(e) => {
            tracing::error!("Failed to open database: {}", e);
            super::handle(
                app,
                Message::ToastShow(crate::components::toast::Toast::error(
                    "Couldn't open library",
                    format!("{}", e),
                )),
            )
        }
    }
}

pub(crate) fn browse_for_folder(_app: &mut PhotoVault) -> Task<Message> {
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

pub(crate) fn folder_selected(app: &mut PhotoVault, path: Option<PathBuf>) -> Task<Message> {
    if let Some(path) = path {
        return super::handle(app, Message::SelectDrive(path));
    }
    Task::none()
}

pub(crate) fn back_to_welcome(app: &mut PhotoVault) -> Task<Message> {
    app.current_view = View::Welcome;
    app.selected_drive = None;
    app.database = None;
    app.photos.clear();
    app.albums.clear();
    app.face_clusters.clear();
    app.documents.clear();
    app.memories.clear();
    app.album_suggestions.clear();
    app.selected_photo_index = None;
    app.previous_view = None;
    app.selected_cluster_id = None;
    app.selected_album_id = None;
    app.selected_duplicate_group = None;
    app.selected_burst_group = None;
    app.selected_memory_id = None;
    app.memory_photos.clear();
    app.insights_scope_photos.clear();
    app.shortcuts_overlay_open = false;
    app.pending_confirmation = None;
    Task::perform(async { DriveDetector::detect() }, Message::DrivesDetected)
}

pub(crate) fn drives_detected(app: &mut PhotoVault, drives: Vec<DriveInfo>) -> Task<Message> {
    let merged = app.merge_detected_and_remembered_drives(drives);
    tracing::info!("Detected {} drives (including remembered)", merged.len());
    app.drives = merged;
    Task::none()
}

pub(crate) fn start_scan(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive_path) = &app.selected_drive else {
        return Task::none();
    };

    // Take the database -- scanner will own it
    let Some(database) = app.database.take() else {
        tracing::error!("No database available for scanning");
        return Task::none();
    };

    tracing::info!("Starting scan of {:?}", drive_path);
    // Non-blocking: stay on current view (or Scanning if first scan)
    if app.photo_count == 0 {
        app.current_view = View::Scanning;
    }

    let drive_path = drive_path.clone();
    let drive_path_for_recovery = drive_path.clone();

    // Start the scanner
    let (progress_rx, cancel_flag, join_handle) =
        crate::services::scanner::start_scan(drive_path, database, app.config.scan_hidden_folders);

    // Store scan state
    app.scan_state = Some(ScanState {
        progress: ScanProgress::default(),
        progress_receiver: progress_rx,
        cancel_flag,
    });

    // Spawn a task to await the join handle and return the result
    Task::perform(
        async move {
            match join_handle.await {
                Ok(result) => {
                    let count = PhotoRepo::new(&result.database.conn).count().unwrap_or(0);
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
                    // Return a zero-count result instead of panicking
                    // Re-open DB for recovery
                    let db = Database::open_for_drive(&drive_path_for_recovery)
                        .expect("Failed to re-open database after scanner panic");
                    (
                        db,
                        ScanResult {
                            photo_count: 0,
                            final_progress: ScanProgress::default(),
                        },
                    )
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

pub(crate) fn poll_scan_channels(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut state) = app.scan_state {
        // Drain all available progress updates
        while let Ok(progress) = state.progress_receiver.try_recv() {
            state.progress = progress;
        }
    }

    if let Some(ref mut rx) = app.face_progress_receiver {
        while let Ok(progress) = rx.try_recv() {
            app.face_processing_progress = Some(progress);
        }
    }

    if let Some(ref mut rx) = app.ocr_progress_receiver {
        while let Ok(progress) = rx.try_recv() {
            app.ocr_progress = Some(progress);
        }
    }
    Task::none()
}

pub(crate) fn cancel_scan(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref state) = app.scan_state {
        state.cancel_flag.store(true, Ordering::Relaxed);
        tracing::info!("Scan cancellation requested");
    }
    // Don't clear scan_state yet -- wait for ScanFinished
    Task::none()
}

pub(crate) fn scan_finished(app: &mut PhotoVault, result: ScanResult) -> Task<Message> {
    tracing::info!("Scan finished: {} photos indexed", result.photo_count);
    app.photo_count = result.photo_count;
    app.invalidate_insights_cache();

    // Update the final progress in scan state so UI shows completion
    if let Some(ref mut state) = app.scan_state {
        state.progress = result.final_progress;
    }

    // Re-open the database (scanner consumed it, we need a fresh connection)
    if let Some(ref drive_path) = app.selected_drive {
        match Database::open_for_drive(drive_path) {
            Ok(db) => {
                // Run maintenance after bulk scan
                let _ = db.run_maintenance();
                app.database = Some(db);
            }
            Err(e) => {
                tracing::error!("Failed to re-open database: {}", e);
            }
        }
    }

    // If still on Scanning view (first scan), auto-advance
    if app.current_view == View::Scanning {
        Task::none()
    } else {
        // Scan was running in background — clear state and reload if on Timeline
        app.scan_state = None;
        let mut tasks = Vec::new();

        if app.current_view == View::Timeline {
            tasks.push(app.load_photos());
        }

        if app.run_face_processing_after_scan {
            app.run_face_processing_after_scan = false;
            tasks.push(super::handle(app, Message::ProcessFaces));
        }

        // Trigger suggestion detection after scan completes
        tasks.push(super::handle(app, Message::RunSuggestionDetection));

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
}

pub(crate) fn scan_complete(app: &mut PhotoVault) -> Task<Message> {
    // User clicked "Continue" after scan completed
    app.scan_state = None;
    app.current_view = View::Timeline;
    // Load photos for the timeline
    app.load_photos()
}
