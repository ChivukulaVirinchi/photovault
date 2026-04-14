//! Burst detection handlers.

use iced::Task;

use crate::db::{BurstGroupRecord, BurstRepo, Database};
use crate::services::{BurstConfig, BurstDetector};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn run_burst_detection(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    app.burst_detection_running = true;

    let drive_path = drive_path.clone();
    let burst_window = app.config.burst_time_window_seconds.max(1);

    Task::perform(
        async move {
            let handle = tokio::task::spawn_blocking(move || {
                let db = Database::open_for_drive(&drive_path)
                    .map_err(|e| format!("Failed to open database: {}", e))?;

                // Run burst detection
                let detector = BurstDetector::new(BurstConfig {
                    max_gap_seconds: burst_window,
                    min_photos: 3,
                    max_burst_span_seconds: (burst_window * 5).max(15),
                    similarity_threshold: 0.90,
                    require_same_folder: true,
                });
                let burst_groups = detector.find_bursts(&db.conn, Some(&drive_path))
                    .map_err(|e| format!("Burst detection failed: {}", e))?;

                // Sync to database
                let repo = BurstRepo::new(&db.conn);
                let sync_data: Vec<(String, String, Vec<i64>)> = burst_groups
                    .iter()
                    .map(|g| {
                        (
                            g.start_time.to_rfc3339(),
                            g.end_time.to_rfc3339(),
                            g.photo_ids.clone(),
                        )
                    })
                    .collect();
                repo.sync_burst_groups(&sync_data)
                    .map_err(|e| format!("Failed to sync burst groups: {}", e))?;

                // Set a default best pick quickly (first/earliest member).
                let groups_from_db = repo.get_all_groups()
                    .map_err(|e| format!("Failed to load groups: {}", e))?;

                for group in &groups_from_db {
                    let members = repo.get_group_members(group.id).unwrap_or_default();
                    if let Some(first) = members.first() {
                        let _ = repo.set_suggested_best(group.id, first.photo_id);
                    }
                }

                // Reload groups
                let final_groups = repo.get_all_groups()
                    .map_err(|e| format!("Failed to reload groups: {}", e))?;

                // Calculate saveable count
                let total_photos: usize = final_groups.iter().map(|g| g.photo_count as usize).sum();
                let saveable = if total_photos > final_groups.len() {
                    total_photos - final_groups.len()
                } else {
                    0
                };

                // Build overview preview strips (up to 5 photo ids per group)
                let mut previews: Vec<(i64, Vec<i64>)> = Vec::new();
                for g in &final_groups {
                    let members = repo.get_group_members(g.id).unwrap_or_default();
                    let ids: Vec<i64> = members.into_iter().take(5).map(|m| m.photo_id).collect();
                    previews.push((g.id, ids));
                }

                Ok::<(Vec<crate::db::BurstGroupRecord>, usize, Vec<(i64, Vec<i64>)>), String>((
                    final_groups,
                    saveable,
                    previews,
                ))
            });

            match handle.await {
                Ok(Ok((groups, saveable, previews))) => (groups, saveable, previews),
                Ok(Err(e)) => {
                    tracing::error!("Burst detection failed: {}", e);
                    (Vec::new(), 0, Vec::new())
                }
                Err(e) => {
                    tracing::error!("Burst detection thread panicked: {}", e);
                    (Vec::new(), 0, Vec::new())
                }
            }
        },
        |(groups, saveable, previews)| {
            Message::BurstDetectionComplete(groups, saveable, previews)
        },
    )
}

pub(crate) fn burst_detection_complete(
    app: &mut PhotoVault,
    groups: Vec<BurstGroupRecord>,
    saveable: usize,
    previews: Vec<(i64, Vec<i64>)>,
) -> Task<Message> {
    tracing::info!(
        "Burst detection complete: {} groups, {} saveable photos",
        groups.len(),
        saveable
    );
    app.burst_groups = groups;
    app.burst_saveable_count = saveable;
    app.burst_detection_running = false;
    app.burst_overview_previews = previews;
    Task::none()
}

pub(crate) fn open_burst_group(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    let group = app
        .burst_groups
        .iter()
        .find(|g| g.id == group_id)
        .cloned();

    if let Some(group) = group {
        app.selected_burst_group = Some(group);

        // Load members from DB
        if let Some(ref drive_path) = app.selected_drive {
            if let Ok(db) = Database::open_for_drive(drive_path) {
                let repo = BurstRepo::new(&db.conn);
                app.selected_burst_members =
                    repo.get_group_members(group_id).unwrap_or_default();
            }
        }

        app.current_view = View::BurstDetail;
    }
    Task::none()
}

pub(crate) fn close_burst_detail(app: &mut PhotoVault) -> Task<Message> {
    app.selected_burst_group = None;
    app.selected_burst_members.clear();
    app.current_view = View::Bursts;
    Task::none()
}

pub(crate) fn set_best_from_burst(
    app: &mut PhotoVault,
    group_id: i64,
    photo_id: i64,
) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = BurstRepo::new(&db.conn);
            let _ = repo.set_suggested_best(group_id, photo_id);

            // Reload members
            app.selected_burst_members =
                repo.get_group_members(group_id).unwrap_or_default();
        }
    }
    Task::none()
}

pub(crate) fn keep_best_from_burst(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let task = Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let repo = BurstRepo::new(&db.conn);
                    if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                        for pid in &photo_ids {
                            let _ = db.conn.execute(
                                "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                rusqlite::params![pid],
                            );
                        }
                    }
                    let _ = repo.delete_group(group_id);
                }
            },
            |_| Message::RunBurstDetection,
        );
        return task;
    }
    Task::none()
}

pub(crate) fn trash_non_best_from_burst(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let task = Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let repo = BurstRepo::new(&db.conn);
                    if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                        for pid in &photo_ids {
                            let _ = db.conn.execute(
                                "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                rusqlite::params![pid],
                            );
                        }
                    }
                    let _ = repo.delete_group(group_id);
                }
            },
            |_| Message::RunBurstDetection,
        );
        app.selected_burst_group = None;
        app.selected_burst_members.clear();
        app.current_view = View::Bursts;
        return task;
    }
    Task::none()
}

pub(crate) fn dismiss_burst_group(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let task = Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let repo = BurstRepo::new(&db.conn);
                    let _ = repo.delete_group(group_id);
                }
            },
            |_| Message::RunBurstDetection,
        );
        return task;
    }
    Task::none()
}
