//! Message handlers for Albums.

use iced::Task;

use crate::db::{AlbumRepo, Database};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn albums_loaded(
    app: &mut PhotoVault,
    albums: Vec<crate::db::AlbumRecord>,
) -> Task<Message> {
    app.albums = albums;
    app.albums_loading = false;
    Task::none()
}

pub(crate) fn create_album(app: &mut PhotoVault, name: String) -> Task<Message> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Task::none();
    }

    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = AlbumRepo::new(&db.conn);
            match repo.create(&name) {
                Ok(album_id) => {
                    tracing::info!("Created album '{}' with id {}", name, album_id);
                    // If picker is open, also add the queued photos
                    if app.album_picker_open {
                        let photo_ids = app.album_picker_target_ids.clone();
                        if !photo_ids.is_empty() {
                            let _ = repo.add_photos(album_id, &photo_ids);
                        }
                        app.album_picker_open = false;
                        app.album_picker_target_ids.clear();
                        app.album_picker_new_name.clear();
                        app.album_picker_creating = false;
                        app.selected_timeline_photo_ids.clear();
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to create album: {}", e);
                }
            }
        }
    }
    // Clear the inline create state
    app.album_picker_creating = false;
    app.album_picker_new_name.clear();
    app.load_albums()
}

pub(crate) fn rename_album(app: &mut PhotoVault, album_id: i64, name: String) -> Task<Message> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Task::none();
    }

    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = AlbumRepo::new(&db.conn);
            if let Err(e) = repo.rename(album_id, &name) {
                tracing::error!("Failed to rename album: {}", e);
            }
        }
    }

    // Update in-memory
    if let Some(album) = app.albums.iter_mut().find(|a| a.id == album_id) {
        album.name = name;
    }
    app.editing_album_id = None;
    app.edit_album_name.clear();
    Task::none()
}

pub(crate) fn delete_album(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = AlbumRepo::new(&db.conn);
            if let Err(e) = repo.delete(album_id) {
                tracing::error!("Failed to delete album: {}", e);
            }
        }
    }

    if app.selected_album_id == Some(album_id) {
        app.selected_album_id = None;
        app.album_photos.clear();
        app.current_view = View::Albums;
    }

    app.load_albums()
}

pub(crate) fn open_album(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    app.selected_album_id = Some(album_id);
    app.current_view = View::AlbumDetail;
    app.load_album_photos(album_id)
}

pub(crate) fn album_photos_loaded(
    app: &mut PhotoVault,
    photos: Vec<crate::models::Photo>,
) -> Task<Message> {
    app.album_photos = photos;
    Task::none()
}

pub(crate) fn add_photos_to_album(
    app: &mut PhotoVault,
    album_id: i64,
    photo_ids: Vec<i64>,
) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = AlbumRepo::new(&db.conn);
            match repo.add_photos(album_id, &photo_ids) {
                Ok(count) => {
                    tracing::info!("Added {} photos to album {}", count, album_id);
                }
                Err(e) => {
                    tracing::error!("Failed to add photos to album: {}", e);
                }
            }
        }
    }

    // Clear picker and selection state
    app.album_picker_open = false;
    app.album_picker_target_ids.clear();
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    app.selected_timeline_photo_ids.clear();

    let mut tasks = vec![app.load_albums()];
    if app.selected_album_id == Some(album_id) {
        tasks.push(app.load_album_photos(album_id));
    }
    Task::batch(tasks)
}

pub(crate) fn remove_photos_from_album(
    app: &mut PhotoVault,
    album_id: i64,
    photo_ids: Vec<i64>,
) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = AlbumRepo::new(&db.conn);
            if let Err(e) = repo.remove_photos(album_id, &photo_ids) {
                tracing::error!("Failed to remove photos from album: {}", e);
            }
        }
    }

    app.selected_timeline_photo_ids.clear();

    let mut tasks = vec![app.load_albums()];
    if app.selected_album_id == Some(album_id) {
        tasks.push(app.load_album_photos(album_id));
    }
    Task::batch(tasks)
}

pub(crate) fn open_album_picker(app: &mut PhotoVault, photo_ids: Vec<i64>) -> Task<Message> {
    app.album_picker_open = true;
    app.album_picker_target_ids = photo_ids;
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    // Ensure album list is fresh
    app.load_albums()
}

pub(crate) fn close_album_picker(app: &mut PhotoVault) -> Task<Message> {
    app.album_picker_open = false;
    app.album_picker_target_ids.clear();
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    Task::none()
}

pub(crate) fn album_picker_name_changed(app: &mut PhotoVault, name: String) -> Task<Message> {
    app.album_picker_new_name = name;
    Task::none()
}

pub(crate) fn album_picker_toggle_create(app: &mut PhotoVault) -> Task<Message> {
    app.album_picker_creating = !app.album_picker_creating;
    app.album_picker_new_name.clear();
    Task::none()
}

pub(crate) fn album_picker_create_and_add(app: &mut PhotoVault) -> Task<Message> {
    let name = app.album_picker_new_name.trim().to_string();
    if name.is_empty() {
        return Task::none();
    }

    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = AlbumRepo::new(&db.conn);
            match repo.create(&name) {
                Ok(album_id) => {
                    let photo_ids = app.album_picker_target_ids.clone();
                    if !photo_ids.is_empty() {
                        let _ = repo.add_photos(album_id, &photo_ids);
                    }
                    tracing::info!(
                        "Created album '{}' and added {} photos",
                        name,
                        photo_ids.len()
                    );
                }
                Err(e) => {
                    tracing::error!("Failed to create album from picker: {}", e);
                }
            }
        }
    }

    app.album_picker_open = false;
    app.album_picker_target_ids.clear();
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    app.selected_timeline_photo_ids.clear();
    app.load_albums()
}

pub(crate) fn album_picker_select(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    let photo_ids = app.album_picker_target_ids.clone();
    add_photos_to_album(app, album_id, photo_ids)
}

pub(crate) fn start_edit_album_name(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    let current_name = app
        .albums
        .iter()
        .find(|a| a.id == album_id)
        .map(|a| a.name.clone())
        .unwrap_or_default();
    app.editing_album_id = Some(album_id);
    app.edit_album_name = current_name;
    Task::none()
}

pub(crate) fn edit_album_name(app: &mut PhotoVault, name: String) -> Task<Message> {
    app.edit_album_name = name;
    Task::none()
}

pub(crate) fn save_album_name(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    let name = app.edit_album_name.clone();
    app.editing_album_id = None;
    rename_album(app, album_id, name)
}

pub(crate) fn save_memory_as_album(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref memory_id) = app.selected_memory_id else {
        return Task::none();
    };
    let card = app.memories.iter().find(|c| &c.id == memory_id);
    let Some(card) = card else {
        return Task::none();
    };

    let name = card.title.clone();
    let photo_ids = card.photo_ids.clone();

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        if let Ok(album_id) = repo.create(&name) {
            let _ = repo.add_photos(album_id, &photo_ids);
            app.selected_album_id = Some(album_id);
            app.current_view = View::AlbumDetail;
            return app.load_album_photos(album_id);
        }
    }
    Task::none()
}

pub(crate) fn back_to_albums(app: &mut PhotoVault) -> Task<Message> {
    app.selected_album_id = None;
    app.album_photos.clear();
    app.current_view = View::Albums;
    app.load_albums()
}

// ---------------------------------------------------------------------------
// Album Suggestion handlers
// ---------------------------------------------------------------------------

pub(crate) fn run_suggestion_detection(app: &mut PhotoVault) -> Task<Message> {
    if app.suggestion_detection_running {
        return Task::none();
    }
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    app.suggestion_detection_running = true;
    let drive_path = drive_path.clone();
    let home_override = app.config.home_city_override.clone();

    Task::perform(
        async move {
            match Database::open_for_drive(&drive_path) {
                Ok(db) => {
                    let override_ref = home_override.as_deref();
                    crate::services::album_suggestions::detect_suggestions(&db.conn, override_ref);
                    // Re-load all pending suggestions to return
                    let repo = crate::db::AlbumSuggestionRepo::new(&db.conn);
                    let mut suggestions = repo.get_pending().unwrap_or_default();

                    // Resolve cover thumbnail paths
                    let photo_repo = crate::db::PhotoRepo::new(&db.conn);
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
                    tracing::error!("Failed to open DB for suggestion detection: {}", e);
                    Vec::new()
                }
            }
        },
        Message::SuggestionsDetected,
    )
}

pub(crate) fn suggestions_detected(
    app: &mut PhotoVault,
    suggestions: Vec<crate::db::AlbumSuggestionRecord>,
) -> Task<Message> {
    app.suggestion_detection_running = false;
    app.album_suggestions = suggestions;
    Task::none()
}

pub(crate) fn suggestions_loaded(
    app: &mut PhotoVault,
    suggestions: Vec<crate::db::AlbumSuggestionRecord>,
) -> Task<Message> {
    app.album_suggestions = suggestions;
    Task::none()
}

pub(crate) fn begin_accept_suggestion(app: &mut PhotoVault, id: i64) -> Task<Message> {
    let name = app
        .album_suggestions
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.title.clone())
        .unwrap_or_default();
    app.accepting_suggestion_id = Some(id);
    app.accepting_suggestion_name = name;
    Task::none()
}

pub(crate) fn accept_suggestion_name_changed(app: &mut PhotoVault, name: String) -> Task<Message> {
    app.accepting_suggestion_name = name;
    Task::none()
}

pub(crate) fn confirm_accept_suggestion(app: &mut PhotoVault, id: i64) -> Task<Message> {
    let name = app.accepting_suggestion_name.trim().to_string();
    if name.is_empty() {
        return Task::none();
    }

    let suggestion = app.album_suggestions.iter().find(|s| s.id == id).cloned();
    let Some(suggestion) = suggestion else {
        return Task::none();
    };

    let photo_ids = suggestion.photo_ids();

    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let album_repo = AlbumRepo::new(&db.conn);
            if let Ok(album_id) = album_repo.create(&name) {
                if !photo_ids.is_empty() {
                    let _ = album_repo.add_photos(album_id, &photo_ids);
                }
                tracing::info!(
                    "Accepted suggestion '{}' -> album {} ({} photos)",
                    suggestion.title,
                    album_id,
                    photo_ids.len(),
                );
            }

            // Mark accepted in DB
            let sug_repo = crate::db::AlbumSuggestionRepo::new(&db.conn);
            let _ = sug_repo.accept(id);
        }
    }

    // Remove from in-memory list
    app.album_suggestions.retain(|s| s.id != id);
    app.accepting_suggestion_id = None;
    app.accepting_suggestion_name.clear();

    app.load_albums()
}

pub(crate) fn cancel_accept_suggestion(app: &mut PhotoVault) -> Task<Message> {
    app.accepting_suggestion_id = None;
    app.accepting_suggestion_name.clear();
    Task::none()
}

pub(crate) fn dismiss_suggestion(app: &mut PhotoVault, id: i64) -> Task<Message> {
    if let Some(ref drive_path) = app.selected_drive {
        if let Ok(db) = Database::open_for_drive(drive_path) {
            let repo = crate::db::AlbumSuggestionRepo::new(&db.conn);
            let _ = repo.dismiss(id);
        }
    }

    app.album_suggestions.retain(|s| s.id != id);
    Task::none()
}
