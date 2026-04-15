//! Duplicate detection handlers.

use iced::Task;

use crate::db::{Database, DuplicateGroupRecord, DuplicateRepo};
use crate::services::DuplicateDetector;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn run_duplicate_detection(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    app.duplicate_detection_running = true;

    let drive_path = drive_path.clone();

    Task::perform(
        async move {
            let handle = tokio::task::spawn_blocking(move || {
                let db = Database::open_for_drive(&drive_path)
                    .map_err(|e| format!("Failed to open database: {}", e))?;

                // Run duplicate detection
                let dup_groups = DuplicateDetector::find_duplicates(&db.conn)
                    .map_err(|e| format!("Duplicate detection failed: {}", e))?;

                // Sync to database
                let repo = DuplicateRepo::new(&db.conn);
                let sync_data: Vec<(String, Vec<i64>, Option<i64>)> = dup_groups
                    .iter()
                    .map(|g| (g.hash.clone(), g.photo_ids.clone(), g.suggested_keep_id))
                    .collect();
                repo.sync_duplicate_groups(&sync_data)
                    .map_err(|e| format!("Failed to sync duplicate groups: {}", e))?;

                // Load groups and wasted space
                let groups = repo
                    .get_all_groups()
                    .map_err(|e| format!("Failed to load groups: {}", e))?;
                let wasted = DuplicateDetector::calculate_wasted_space(&db.conn).unwrap_or(0);

                // Build overview summaries per group
                let mut overview = Vec::new();
                for g in &groups {
                    let members = repo.get_group_members(g.id).unwrap_or_default();
                    if members.is_empty() {
                        overview.push((g.id, 0, None));
                        continue;
                    }

                    let mut total = 0u64;
                    let mut max_size = 0u64;
                    let mut preview_photo_id = None;

                    for m in &members {
                        let s = m.file_size.unwrap_or(0).max(0) as u64;
                        total += s;
                        if s > max_size {
                            max_size = s;
                        }
                        if m.is_suggested_keep {
                            preview_photo_id = Some(m.photo_id);
                        }
                    }

                    if preview_photo_id.is_none() {
                        preview_photo_id = members.first().map(|m| m.photo_id);
                    }

                    let recoverable = total.saturating_sub(max_size);
                    overview.push((g.id, recoverable, preview_photo_id));
                }

                Ok::<
                    (
                        Vec<crate::db::DuplicateGroupRecord>,
                        u64,
                        Vec<(i64, u64, Option<i64>)>,
                    ),
                    String,
                >((groups, wasted, overview))
            });

            match handle.await {
                Ok(Ok((groups, wasted, overview))) => (groups, wasted, overview),
                Ok(Err(e)) => {
                    tracing::error!("Duplicate detection failed: {}", e);
                    (Vec::new(), 0, Vec::new())
                }
                Err(e) => {
                    tracing::error!("Duplicate detection thread panicked: {}", e);
                    (Vec::new(), 0, Vec::new())
                }
            }
        },
        |(groups, wasted, overview)| Message::DuplicateDetectionComplete(groups, wasted, overview),
    )
}

pub(crate) fn duplicate_detection_complete(
    app: &mut PhotoVault,
    groups: Vec<DuplicateGroupRecord>,
    wasted: u64,
    overview: Vec<(i64, u64, Option<i64>)>,
) -> Task<Message> {
    tracing::info!(
        "Duplicate detection complete: {} groups, {} bytes wasted",
        groups.len(),
        wasted
    );
    app.duplicate_groups = groups;
    app.duplicate_wasted_space = wasted;
    app.duplicate_detection_running = false;
    app.duplicate_overview = overview;
    Task::none()
}

pub(crate) fn open_duplicate_group(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    // Find the group record
    let group = app
        .duplicate_groups
        .iter()
        .find(|g| g.id == group_id)
        .cloned();

    if let Some(group) = group {
        app.selected_duplicate_group = Some(group);

        // Load members from DB
        if let Some(ref drive_path) = app.selected_drive {
            if let Ok(db) = Database::open_for_drive(drive_path) {
                let repo = DuplicateRepo::new(&db.conn);
                app.selected_duplicate_members =
                    repo.get_group_members(group_id).unwrap_or_default();
            }
        }

        app.current_view = View::DuplicateDetail;
    }
    Task::none()
}

pub(crate) fn close_duplicate_detail(app: &mut PhotoVault) -> Task<Message> {
    app.selected_duplicate_group = None;
    app.selected_duplicate_members.clear();
    app.current_view = View::Duplicates;
    Task::none()
}

pub(crate) fn set_keep_duplicate(
    app: &mut PhotoVault,
    group_id: i64,
    photo_id: i64,
) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = DuplicateRepo::new(&db.conn);
            let _ = repo.set_keep_photo(group_id, photo_id);

            // Reload members
            app.selected_duplicate_members = repo.get_group_members(group_id).unwrap_or_default();
        }
    }
    Task::none()
}

pub(crate) fn keep_suggested_duplicate(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let task = Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let repo = DuplicateRepo::new(&db.conn);
                    // Trash non-suggested photos
                    if let Ok(photo_ids) = repo.get_photos_to_trash(group_id) {
                        for pid in &photo_ids {
                            let _ = db.conn.execute(
                                "UPDATE photos SET is_trashed = TRUE WHERE id = ?1",
                                rusqlite::params![pid],
                            );
                        }
                    }
                    // Remove the group
                    let _ = repo.delete_group(group_id);
                }
            },
            |_| Message::RunDuplicateDetection,
        );
        return task;
    }
    Task::none()
}

pub(crate) fn trash_non_suggested_duplicates(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    // Same as KeepSuggested — soft-delete non-keep photos and remove group
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let task = Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let repo = DuplicateRepo::new(&db.conn);
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
            |_| Message::RunDuplicateDetection,
        );
        // After trashing, go back to duplicates list
        app.selected_duplicate_group = None;
        app.selected_duplicate_members.clear();
        app.current_view = View::Duplicates;
        return task;
    }
    Task::none()
}

pub(crate) fn dismiss_duplicate_group(app: &mut PhotoVault, group_id: i64) -> Task<Message> {
    // Just remove the group from DB without trashing any photos
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let task = Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let repo = DuplicateRepo::new(&db.conn);
                    let _ = repo.delete_group(group_id);
                }
            },
            |_| Message::RunDuplicateDetection,
        );
        return task;
    }
    Task::none()
}
