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
    Task::none()
}

pub(crate) fn create_album(app: &mut PhotoVault, name: String) -> Task<Message> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Task::none();
    }
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        if let Ok(album_id) = repo.create(trimmed) {
            if app.album_picker_open {
                let target_ids = app.album_picker_target_ids.clone();
                let _ = repo.add_photos(album_id, &target_ids);
                app.album_picker_open = false;
                app.album_picker_target_ids.clear();
                app.album_picker_new_name.clear();
                app.album_picker_creating = false;
                app.selected_timeline_photo_ids.clear();
            } else {
                app.album_picker_new_name.clear();
                app.album_picker_creating = false;
            }
        }
    }

    app.load_albums()
}

pub(crate) fn rename_album(app: &mut PhotoVault, album_id: i64, name: String) -> Task<Message> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Task::none();
    }

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        let _ = repo.rename(album_id, trimmed);
    }

    if let Some(album) = app.albums.iter_mut().find(|a| a.id == album_id) {
        album.name = trimmed.to_string();
    }
    app.editing_album_id = None;
    app.edit_album_name.clear();
    Task::none()
}

pub(crate) fn delete_album(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        let _ = repo.delete(album_id);
    }

    if app.selected_album_id == Some(album_id) {
        app.selected_album_id = None;
        app.album_photos.clear();
        app.current_view = View::Albums;
    }

    app.load_albums()
}

pub(crate) fn set_album_cover(app: &mut PhotoVault, album_id: i64, photo_id: i64) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        let _ = repo.set_cover(album_id, photo_id);
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
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        let _ = repo.add_photos(album_id, &photo_ids);
    }

    app.selected_timeline_photo_ids.clear();
    app.album_picker_open = false;
    app.album_picker_target_ids.clear();
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;

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
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        let _ = repo.remove_photos(album_id, &photo_ids);
    }

    app.selected_timeline_photo_ids.clear();
    Task::batch(vec![app.load_albums(), app.load_album_photos(album_id)])
}

pub(crate) fn open_album_picker(app: &mut PhotoVault, photo_ids: Vec<i64>) -> Task<Message> {
    app.album_picker_open = true;
    app.album_picker_target_ids = photo_ids;
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    app.load_albums()
}

pub(crate) fn album_picker_select(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    let ids = app.album_picker_target_ids.clone();
    super::handle(app, Message::AddPhotosToAlbum(album_id, ids))
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

    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let target_ids = app.album_picker_target_ids.clone();

    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        if let Ok(album_id) = repo.create(&name) {
            let _ = repo.add_photos(album_id, &target_ids);
        }
    }

    app.album_picker_open = false;
    app.album_picker_target_ids.clear();
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    app.selected_timeline_photo_ids.clear();

    app.load_albums()
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
    super::handle(app, Message::RenameAlbum(album_id, name))
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
            return Task::batch(vec![app.load_albums(), app.load_album_photos(album_id)]);
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
