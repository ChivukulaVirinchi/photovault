//! Settings, reindexing, geocoding, and rotated-data handlers.

use iced::Task;

use crate::config::{AppTheme, DateFormat};
use crate::db::Database;
use crate::services::{ApplyResult, GeocodingService, IndexChanges, Reindexer};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn set_theme(app: &mut PhotoVault, theme: AppTheme) -> Task<Message> {
    app.config.theme = theme;
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn set_thumbnail_size(app: &mut PhotoVault, size: u32) -> Task<Message> {
    app.config.thumbnail_size = size;
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    if app.current_view == View::Timeline {
        app.begin_thumbnail_generation_epoch();
        app.seed_thumbnail_queue_for_timeline();
        app.schedule_thumbnail_chunk();
        if !app.thumbnail_queue.is_empty() {
            return app.start_thumbnail_generation();
        }
    }
    Task::none()
}

pub(crate) fn set_scan_hidden_folders(app: &mut PhotoVault, enabled: bool) -> Task<Message> {
    app.config.scan_hidden_folders = enabled;
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn set_face_confidence(app: &mut PhotoVault, v: f32) -> Task<Message> {
    app.config.face_detection_confidence = v.clamp(0.0, 1.0);
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn set_clustering_threshold(app: &mut PhotoVault, v: f32) -> Task<Message> {
    app.config.face_clustering_threshold = v.clamp(0.0, 1.0);
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn set_burst_window(app: &mut PhotoVault, seconds: i64) -> Task<Message> {
    app.config.burst_time_window_seconds = seconds.max(1);
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn set_trash_auto_delete(app: &mut PhotoVault, days: u32) -> Task<Message> {
    app.config.trash_auto_delete_days = days;
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn set_date_format(app: &mut PhotoVault, format: DateFormat) -> Task<Message> {
    app.config.date_format = format;
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn rescan_library(app: &mut PhotoVault) -> Task<Message> {
    super::handle(app, Message::StartScan)
}

pub(crate) fn check_for_changes(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    let scan_hidden_folders = app.config.scan_hidden_folders;

    Task::perform(
        async move {
            match Database::open_for_drive(&drive_path) {
                Ok(db) => {
                    let reindexer = Reindexer::new_with_options(scan_hidden_folders);
                    reindexer
                        .detect_changes(&db.conn, &drive_path)
                        .unwrap_or_default()
                }
                Err(e) => {
                    tracing::error!("CheckForChanges DB open failed: {}", e);
                    IndexChanges::default()
                }
            }
        },
        Message::ChangesDetected,
    )
}

pub(crate) fn changes_detected(app: &mut PhotoVault, changes: IndexChanges) -> Task<Message> {
    app.pending_index_changes = Some(changes.clone());
    if changes.is_empty() {
        tracing::info!("No index changes detected");
        app.current_view = View::Timeline;
        return app.load_photos();
    }
    super::handle(app, Message::ApplyChanges)
}

pub(crate) fn apply_changes(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let Some(changes) = app.pending_index_changes.clone() else {
        return Task::none();
    };

    let drive_path = drive_path.clone();
    let scan_hidden_folders = app.config.scan_hidden_folders;
    Task::perform(
        async move {
            match Database::open_for_drive(&drive_path) {
                Ok(db) => {
                    let reindexer = Reindexer::new_with_options(scan_hidden_folders);
                    reindexer
                        .apply_changes(&db.conn, &changes)
                        .unwrap_or_default()
                }
                Err(e) => {
                    tracing::error!("ApplyChanges DB open failed: {}", e);
                    ApplyResult::default()
                }
            }
        },
        Message::ChangesApplied,
    )
}

pub(crate) fn changes_applied(app: &mut PhotoVault, result: ApplyResult) -> Task<Message> {
    tracing::info!("Applied index changes: {:?}", result);
    app.pending_index_changes = None;
    app.current_view = View::Timeline;

    let mut tasks = vec![app.load_photos()];
    if result.new_files > 0 {
        app.run_face_processing_after_scan = true;
        tasks.push(super::handle(app, Message::StartScan));
    } else if result.updates_applied > 0 {
        tasks.push(super::handle(app, Message::ProcessFaces));
    }
    Task::batch(tasks)
}

pub(crate) fn run_geocoding(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    if app.geocoding_progress.is_some() {
        return Task::none();
    }
    let drive_path = drive_path.clone();
    app.geocoding_progress = Some((0, 0));

    Task::perform(
        async move {
            use crate::db::geonames::{geonames_db_exists, geonames_db_path};

            if !geonames_db_exists() {
                tracing::warn!("GeoNames DB missing at {}", geonames_db_path().display());
                return (0usize, 0usize);
            }

            let geocoder = match GeocodingService::new(geonames_db_path()) {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!("Failed to open geonames DB: {}", e);
                    return (0, 0);
                }
            };

            let db = match Database::open_for_drive(&drive_path) {
                Ok(db) => db,
                Err(e) => {
                    tracing::error!("Failed to open drive DB for geocoding: {}", e);
                    return (0, 0);
                }
            };

            let mut stmt = match db.conn.prepare(
                "SELECT id, gps_latitude, gps_longitude FROM photos WHERE gps_latitude IS NOT NULL AND gps_longitude IS NOT NULL AND (location_city IS NULL OR location_country IS NULL)",
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to query photos for geocoding: {}", e);
                    return (0, 0);
                }
            };

            let rows: Vec<(i64, f64, f64)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                    ))
                })
                .map(|iter| iter.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            let total = rows.len();
            let mut processed = 0usize;

            if total == 0 {
                return (0, 0);
            }

            if let Ok(tx) = db.conn.unchecked_transaction() {
                for (id, lat, lon) in rows {
                    if let Some(result) = geocoder.reverse_geocode(lat, lon) {
                        let _ = tx.execute(
                            "UPDATE photos SET location_city = ?1, location_country = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
                            rusqlite::params![result.city, result.country, id],
                        );
                    }
                    processed += 1;
                }
                let _ = tx.commit();
                return (processed, total);
            }

            for (id, lat, lon) in rows {
                if let Some(result) = geocoder.reverse_geocode(lat, lon) {
                    let _ = db.conn.execute(
                        "UPDATE photos SET location_city = ?1, location_country = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
                        rusqlite::params![result.city, result.country, id],
                    );
                }
                processed += 1;
            }

            (processed, total)
        },
        |(processed, total)| Message::GeocodingProgress { processed, total },
    )
}

pub(crate) fn geocoding_progress(
    app: &mut PhotoVault,
    processed: usize,
    total: usize,
) -> Task<Message> {
    app.geocoding_progress = Some((processed, total));
    if total == 0 {
        app.geocoding_progress = None;
        return Task::none();
    }
    if processed >= total {
        return super::handle(app, Message::GeocodingComplete);
    }
    Task::none()
}

pub(crate) fn geocoding_complete(app: &mut PhotoVault) -> Task<Message> {
    app.geocoding_progress = None;
    app.load_photos()
}

pub(crate) fn regenerate_rotated_data(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    if app.rotated_data_regen_active {
        return Task::none();
    }

    let drive_path = drive_path.clone();
    app.begin_thumbnail_generation_epoch();
    app.rotated_data_regen_active = true;

    Task::perform(
        async move {
            let mut cleared_thumbnails = 0usize;
            let mut reset_faces = 0usize;

            if let Ok(db) = Database::open_for_drive(&drive_path) {
                if let Ok(mut stmt) = db.conn.prepare(
                    "SELECT id, thumbnail_path FROM photos WHERE is_trashed = FALSE AND COALESCE(orientation, 1) != 1",
                ) {
                    let rows = stmt
                        .query_map([], |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, Option<String>>(1)?,
                            ))
                        })
                        .map(|iter| iter.filter_map(|r| r.ok()).collect::<Vec<_>>())
                        .unwrap_or_default();

                    for (_photo_id, rel_thumb) in &rows {
                        if let Some(rel_thumb) = rel_thumb {
                            let abs_thumb = drive_path.join(rel_thumb);
                            if std::fs::remove_file(&abs_thumb).is_ok() {
                                cleared_thumbnails += 1;
                            }
                        }
                    }
                }

                let _ = db.conn.execute(
                    "UPDATE photos SET thumbnail_path = NULL WHERE is_trashed = FALSE AND COALESCE(orientation, 1) != 1",
                    [],
                );

                let _ = db.conn.execute(
                    "UPDATE photos SET ocr_processed = FALSE WHERE is_trashed = FALSE AND COALESCE(orientation, 1) != 1",
                    [],
                );

                let affected_face_ids = db
                    .conn
                    .prepare(
                        "SELECT f.id FROM faces f JOIN photos p ON p.id = f.photo_id WHERE p.is_trashed = FALSE AND COALESCE(p.orientation, 1) != 1",
                    )
                    .and_then(|mut stmt| {
                        stmt.query_map([], |row| row.get::<_, i64>(0))
                            .map(|iter| iter.filter_map(|r| r.ok()).collect::<Vec<_>>())
                    })
                    .unwrap_or_default();

                if let Ok(tx) = db.conn.unchecked_transaction() {
                    let _ = tx.execute(
                        "DELETE FROM photo_inferred_identities WHERE photo_id IN (SELECT id FROM photos WHERE is_trashed = FALSE AND COALESCE(orientation, 1) != 1)",
                        [],
                    );
                    let _ = tx.execute(
                        "DELETE FROM faces WHERE photo_id IN (SELECT id FROM photos WHERE is_trashed = FALSE AND COALESCE(orientation, 1) != 1)",
                        [],
                    );
                    if let Ok(changed) = tx.execute(
                        "UPDATE photos SET faces_processed = FALSE WHERE is_trashed = FALSE AND COALESCE(orientation, 1) != 1",
                        [],
                    ) {
                        reset_faces = changed;
                    }
                    let _ = tx.commit();
                }

                let faces_dir = drive_path.join(".photovault").join("faces");
                for face_id in affected_face_ids {
                    let crop_path = faces_dir.join(format!("{}.jpg", face_id));
                    if crop_path.exists() {
                        let _ = std::fs::remove_file(crop_path);
                    }
                }
            }

            (cleared_thumbnails, reset_faces)
        },
        |(cleared_thumbnails, reset_faces)| Message::RotatedDataRegenerated {
            cleared_thumbnails,
            reset_faces,
        },
    )
}

pub(crate) fn rotated_data_regenerated(
    app: &mut PhotoVault,
    cleared_thumbnails: usize,
    reset_faces: usize,
) -> Task<Message> {
    app.rotated_data_regen_active = false;
    tracing::info!(
        "Regenerated rotated-data state: removed {} thumbnail files, reset {} photos for face processing",
        cleared_thumbnails,
        reset_faces
    );

    let mut tasks = vec![app.load_photos(), super::handle(app, Message::ProcessFaces)];
    if app.current_view == View::Documents {
        tasks.push(super::handle(app, Message::RunDocumentAnalysis));
    }
    Task::batch(tasks)
}

pub(crate) fn toggle_sidebar(app: &mut PhotoVault) -> Task<Message> {
    app.config.sidebar_collapsed = !app.config.sidebar_collapsed;
    let _ = app.config.save();
    Task::none()
}

pub(crate) fn regenerate_thumbnails(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    app.begin_thumbnail_generation_epoch();

    Task::perform(
        async move {
            let mut cleared = 0usize;

            let thumb_root = drive_path.join(".photovault").join("thumbnails");
            if thumb_root.exists() {
                if let Ok(entries) = std::fs::read_dir(&thumb_root) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            if let Ok(walker) = std::fs::read_dir(&path) {
                                for sub in walker.flatten() {
                                    let sub_path = sub.path();
                                    if sub_path.is_dir() {
                                        if let Ok(files) = std::fs::read_dir(&sub_path) {
                                            for f in files.flatten() {
                                                if std::fs::remove_file(f.path()).is_ok() {
                                                    cleared += 1;
                                                }
                                            }
                                        }
                                    } else if std::fs::remove_file(&sub_path).is_ok() {
                                        cleared += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let _ = db.conn.execute(
                    "UPDATE photos SET thumbnail_path = NULL WHERE is_trashed = FALSE",
                    [],
                );
            }

            Message::ThumbnailsRegenerated { cleared }
        },
        |m| m,
    )
}

pub(crate) fn thumbnails_regenerated(app: &mut PhotoVault, cleared: usize) -> Task<Message> {
    tracing::info!("Regenerated thumbnails: cleared {} old files", cleared);
    let mut tasks = vec![app.load_photos()];
    if app.current_view == View::Timeline {
        app.begin_thumbnail_generation_epoch();
        app.seed_thumbnail_queue_for_timeline();
        app.schedule_thumbnail_chunk();
        if !app.thumbnail_queue.is_empty() {
            tasks.push(app.start_thumbnail_generation());
        }
    }
    Task::batch(tasks)
}

pub(crate) fn set_home_city(app: &mut PhotoVault, city: String) -> Task<Message> {
    let city = city.trim().to_string();
    app.config.home_city_override = if city.is_empty() { None } else { Some(city) };
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Task::none()
}

pub(crate) fn no_op(_app: &mut PhotoVault) -> Task<Message> {
    Task::none()
}
