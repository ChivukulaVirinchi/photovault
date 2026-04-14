//! Trash handlers.

use iced::Task;

use crate::db::{Database, TrashedPhotoRecord};
use crate::services::{TrashService, TrashStats};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn load_trash(app: &mut PhotoVault) -> Task<Message> {
    app.load_trash()
}

pub(crate) fn trash_loaded(
    app: &mut PhotoVault,
    items: Vec<TrashedPhotoRecord>,
    stats: TrashStats,
) -> Task<Message> {
    app.trash_items = items;
    app.trash_stats = stats;
    app.selected_trash_ids.clear();
    app.confirm_empty_trash = false;
    app.confirm_delete_photo_id = None;
    Task::none()
}

pub(crate) fn trash_photos(app: &mut PhotoVault, photo_ids: Vec<i64>) -> Task<Message> {
    if photo_ids.is_empty() {
        return super::handle(app, Message::ExitCullMode);
    }
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();

    let task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let _ = TrashService::trash_photos(&db.conn, &photo_ids);
            }
        },
        |_| Message::LoadTrash,
    );

    // If trashing from photo detail, navigate back to previous view
    // and advance to the next photo (or close if last)
    if app.current_view == View::PhotoDetail {
        if let Some(idx) = app.selected_photo_index {
            // Will be shown after reload; adjust index
            if idx + 1 < app.photos.len() {
                // Stay at same index (next photo slides in)
            } else if idx > 0 {
                app.selected_photo_index = Some(idx - 1);
            } else {
                // Last photo trashed, go back to previous view
                app.selected_photo_index = None;
                app.current_view = app.previous_view.take().unwrap_or(View::Timeline);
            }
        }
    } else {
        if app.current_view == View::Timeline {
            // Timeline multi-delete stays in timeline (Google Photos style)
            app.selected_timeline_photo_ids.clear();
        } else if app.current_view == View::Documents {
            // Documents multi-delete stays in documents view
            app.selected_timeline_photo_ids.clear();
        } else {
            // From other views, go to Trash
            app.cull_state = None;
            app.cull_confirm_pending = false;
            app.current_view = View::Trash;
        }
    }
    let reload = app.load_photos();
    Task::batch([task, reload])
}

pub(crate) fn restore_photo(app: &mut PhotoVault, photo_id: i64) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    let task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let _ = TrashService::restore_photos(&db.conn, &[photo_id]);
            }
        },
        |_| Message::LoadTrash,
    );
    let reload = app.load_photos();
    Task::batch([task, reload])
}

pub(crate) fn restore_selected(app: &mut PhotoVault) -> Task<Message> {
    let ids = app.selected_trash_ids.iter().copied().collect::<Vec<_>>();
    if ids.is_empty() {
        return Task::none();
    }
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    let task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let _ = TrashService::restore_photos(&db.conn, &ids);
            }
        },
        |_| Message::LoadTrash,
    );
    let reload = app.load_photos();
    Task::batch([task, reload])
}

pub(crate) fn toggle_trash_selection(app: &mut PhotoVault, photo_id: i64) -> Task<Message> {
    if !app.selected_trash_ids.insert(photo_id) {
        app.selected_trash_ids.remove(&photo_id);
    }
    Task::none()
}

pub(crate) fn permanently_delete_photo(app: &mut PhotoVault, photo_id: i64) -> Task<Message> {
    app.confirm_delete_photo_id = Some(photo_id);
    Task::none()
}

pub(crate) fn confirm_permanently_delete_photo(
    app: &mut PhotoVault,
    photo_id: i64,
) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    let task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let _ = TrashService::permanent_delete(&db.conn, &[photo_id], &drive_path);
            }
        },
        |_| Message::LoadTrash,
    );
    let reload = app.load_photos();
    Task::batch([task, reload])
}

pub(crate) fn empty_trash(app: &mut PhotoVault) -> Task<Message> {
    app.confirm_empty_trash = true;
    Task::none()
}

pub(crate) fn confirm_empty_trash(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    let task = Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                let _ = TrashService::empty_trash(&db.conn, &drive_path);
            }
        },
        |_| Message::LoadTrash,
    );
    let reload = app.load_photos();
    Task::batch([task, reload])
}
