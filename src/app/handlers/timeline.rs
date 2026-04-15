//! Timeline view and photo detail handlers.

use std::collections::HashSet;

use iced::keyboard;
use iced::Task;

use crate::models::Photo;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn scrolled(
    app: &mut PhotoVault,
    offset: iced::widget::scrollable::AbsoluteOffset,
) -> Task<Message> {
    app.timeline_scroll_offset = offset;
    Task::none()
}

pub(crate) fn photos_loaded(app: &mut PhotoVault, photos: Vec<Photo>) -> Task<Message> {
    tracing::info!("Loaded {} photos for timeline", photos.len());
    app.begin_thumbnail_generation_epoch();
    app.photos = photos;
    app.photo_count = app.photos.len() as i64;

    let valid_ids: HashSet<i64> = app.photos.iter().map(|p| p.id).collect();
    app.selected_timeline_photo_ids
        .retain(|photo_id| valid_ids.contains(photo_id));

    // Prioritize visible timeline region first, then continue incrementally.
    app.seed_thumbnail_queue_for_timeline();
    app.schedule_thumbnail_chunk();

    tracing::info!(
        "Queued {} thumbnails ({} photos total)",
        app.thumbnail_queue.len(),
        app.photos.len()
    );

    if !app.thumbnail_queue.is_empty() {
        // Start processing the first batch
        app.start_thumbnail_generation()
    } else if app.thumbnail_scan_cursor < app.photos.len() {
        let epoch = app.thumbnail_generation_epoch;
        Task::perform(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            },
            move |_| Message::ContinueThumbnailScheduling(epoch),
        )
    } else {
        Task::none()
    }
}

pub(crate) fn select_photo(app: &mut PhotoVault, photo_id: i64) -> Task<Message> {
    // Find the photo index
    let resolved_idx = app
        .photos
        .iter()
        .position(|p| p.id == photo_id)
        .or_else(|| {
            app.documents
                .iter()
                .find(|p| p.id == photo_id)
                .and_then(|doc| app.photos.iter().position(|p| p.id == doc.id))
        });

    if let Some(idx) = resolved_idx {
        if app.current_view != View::PhotoDetail {
            app.previous_view = Some(app.current_view.clone());
        }
        app.selected_photo_index = Some(idx);
        app.current_view = View::PhotoDetail;
        return app.load_photo_detail_for_index(idx);
    }
    Task::none()
}

pub(crate) fn toggle_timeline_photo_selection(
    app: &mut PhotoVault,
    photo_id: i64,
) -> Task<Message> {
    if !app.selected_timeline_photo_ids.insert(photo_id) {
        app.selected_timeline_photo_ids.remove(&photo_id);
    }
    Task::none()
}

pub(crate) fn toggle_timeline_day_selection(
    app: &mut PhotoVault,
    day_key: String,
) -> Task<Message> {
    let source_photos = if app.current_view == View::Documents {
        &app.documents
    } else {
        &app.photos
    };

    let day_photo_ids: Vec<i64> = source_photos
        .iter()
        .filter(|p| {
            p.date_taken
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "0000-00-00".to_string())
                == day_key
        })
        .map(|p| p.id)
        .collect();

    if !day_photo_ids.is_empty()
        && day_photo_ids
            .iter()
            .all(|id| app.selected_timeline_photo_ids.contains(id))
    {
        for photo_id in day_photo_ids {
            app.selected_timeline_photo_ids.remove(&photo_id);
        }
    } else {
        for photo_id in day_photo_ids {
            app.selected_timeline_photo_ids.insert(photo_id);
        }
    }
    Task::none()
}

pub(crate) fn timeline_photo_hover(app: &mut PhotoVault, photo_id: Option<i64>) -> Task<Message> {
    app.hovered_timeline_photo_id = photo_id;
    Task::none()
}

pub(crate) fn timeline_day_hover(app: &mut PhotoVault, day_key: Option<String>) -> Task<Message> {
    app.hovered_timeline_day_key = day_key;
    Task::none()
}

pub(crate) fn clear_timeline_photo_selection(app: &mut PhotoVault) -> Task<Message> {
    app.selected_timeline_photo_ids.clear();
    Task::none()
}

pub(crate) fn close_photo_detail(app: &mut PhotoVault) -> Task<Message> {
    app.selected_photo_index = None;
    // Return to whatever view we came from
    app.current_view = app.previous_view.take().unwrap_or(View::Timeline);
    if app.current_view == View::Timeline {
        return iced::widget::scrollable::scroll_to(
            iced::widget::scrollable::Id::new("timeline"),
            app.timeline_scroll_offset,
        );
    }
    Task::none()
}

pub(crate) fn previous_photo(app: &mut PhotoVault) -> Task<Message> {
    let target_photo_id = if let Some(current_idx) = app.selected_photo_index {
        if let Some(current_photo) = app.photos.get(current_idx) {
            let current_id = current_photo.id;
            let nav_list = app.photo_detail_navigation_list();
            if let Some(nav_idx) = nav_list.iter().position(|p| p.id == current_id) {
                if nav_idx > 0 {
                    Some(nav_list[nav_idx - 1].id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(photo_id) = target_photo_id {
        if let Some(global_idx) = app.photos.iter().position(|p| p.id == photo_id) {
            app.selected_photo_index = Some(global_idx);
            return app.load_photo_detail_for_index(global_idx);
        }
    }
    Task::none()
}

pub(crate) fn next_photo(app: &mut PhotoVault) -> Task<Message> {
    let target_photo_id = if let Some(current_idx) = app.selected_photo_index {
        if let Some(current_photo) = app.photos.get(current_idx) {
            let current_id = current_photo.id;
            let nav_list = app.photo_detail_navigation_list();
            if let Some(nav_idx) = nav_list.iter().position(|p| p.id == current_id) {
                if nav_idx + 1 < nav_list.len() {
                    Some(nav_list[nav_idx + 1].id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(photo_id) = target_photo_id {
        if let Some(global_idx) = app.photos.iter().position(|p| p.id == photo_id) {
            app.selected_photo_index = Some(global_idx);
            return app.load_photo_detail_for_index(global_idx);
        }
    }
    Task::none()
}

pub(crate) fn key_pressed(app: &mut PhotoVault, key: keyboard::Key) -> Task<Message> {
    // --- View-specific shortcuts ---
    if app.current_view == View::PhotoDetail {
        match key {
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                return super::handle(app, Message::PreviousPhoto);
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                return super::handle(app, Message::NextPhoto);
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                return super::handle(app, Message::ClosePhotoDetail);
            }
            keyboard::Key::Named(keyboard::key::Named::Delete)
            | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                // Trash current photo
                if let Some(idx) = app.selected_photo_index {
                    if let Some(photo) = app.photos.get(idx) {
                        return super::handle(app, Message::TrashPhotos(vec![photo.id]));
                    }
                }
            }
            keyboard::Key::Character(ref ch) => {
                let lower = ch.to_lowercase();
                if lower == "r" {
                    return super::handle(app, Message::RotatePhoto);
                }
                if lower == "i" {
                    return super::handle(app, Message::ToggleMetadataPanel);
                }
            }
            _ => {}
        }
    } else if app.current_view == View::Cull {
        match key {
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                return super::handle(app, Message::CullPrev);
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                return super::handle(app, Message::CullNext);
            }
            keyboard::Key::Named(keyboard::key::Named::Enter) => {
                return super::handle(app, Message::CullFinish);
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                return super::handle(app, Message::ExitCullMode);
            }
            keyboard::Key::Character(ref ch) => {
                let lower = ch.to_lowercase();
                if lower == "x" {
                    return super::handle(app, Message::CullToggleTrash);
                }
                if lower == "u" {
                    return super::handle(app, Message::CullUndo);
                }
            }
            _ => {}
        }
    } else if app.current_view == View::ClusterDetail {
        if let keyboard::Key::Named(keyboard::key::Named::Escape) = key {
            return super::handle(app, Message::BackToPeople);
        }
    } else if app.current_view == View::MemoryDetail {
        match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                return super::handle(app, Message::CloseMemoryDetail);
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                return super::handle(app, Message::MemorySlideshowPrev);
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                return super::handle(app, Message::MemorySlideshowNext);
            }
            keyboard::Key::Named(keyboard::key::Named::Space) => {
                return super::handle(app, Message::MemorySlideshowTogglePause);
            }
            _ => {}
        }
    } else if app.current_view == View::FaceReview {
        match key {
            keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                return super::handle(app, Message::FaceReviewSame);
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                return super::handle(app, Message::FaceReviewDifferent);
            }
            keyboard::Key::Named(keyboard::key::Named::Enter) => {
                return super::handle(app, Message::FaceReviewFinish);
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                return super::handle(app, Message::FaceReviewFinish);
            }
            keyboard::Key::Named(keyboard::key::Named::Space) => {
                return super::handle(app, Message::FaceReviewSkip);
            }
            keyboard::Key::Character(ref ch) => {
                let lower = ch.to_lowercase();
                if lower == "y" {
                    return super::handle(app, Message::FaceReviewSame);
                }
                if lower == "n" {
                    return super::handle(app, Message::FaceReviewDifferent);
                }
                if lower == "s" {
                    return super::handle(app, Message::FaceReviewSkip);
                }
                if lower == "u" {
                    return super::handle(app, Message::FaceReviewUndo);
                }
            }
            _ => {}
        }
    } else if app.current_view == View::Map {
        match key {
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                return super::handle(app, Message::MapPanBy { dx: 50.0, dy: 0.0 });
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                return super::handle(app, Message::MapPanBy { dx: -50.0, dy: 0.0 });
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                return super::handle(app, Message::MapPanBy { dx: 0.0, dy: 50.0 });
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                return super::handle(app, Message::MapPanBy { dx: 0.0, dy: -50.0 });
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                if !app.open_popovers.is_empty() {
                    return super::handle(app, Message::MapClosePopover);
                }
            }
            keyboard::Key::Character(ref ch) => {
                let lower = ch.to_lowercase();
                if lower == "+" || lower == "=" {
                    return super::handle(
                        app,
                        Message::MapZoomAt {
                            x: app.window_width / 2.0,
                            y: app.window_height / 2.0,
                            delta: 1,
                        },
                    );
                }
                if lower == "-" || lower == "_" {
                    return super::handle(
                        app,
                        Message::MapZoomAt {
                            x: app.window_width / 2.0,
                            y: app.window_height / 2.0,
                            delta: -1,
                        },
                    );
                }
            }
            _ => {}
        }
    } else if app.current_view == View::DuplicateDetail {
        if let keyboard::Key::Named(keyboard::key::Named::Escape) = key {
            return super::handle(app, Message::CloseDuplicateDetail);
        }
    } else if app.current_view == View::BurstDetail {
        if let keyboard::Key::Named(keyboard::key::Named::Escape) = key {
            return super::handle(app, Message::CloseBurstDetail);
        }
    } else if app.current_view == View::Documents {
        match key {
            keyboard::Key::Named(keyboard::key::Named::Delete)
            | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                if !app.selected_timeline_photo_ids.is_empty() {
                    let ids = app
                        .selected_timeline_photo_ids
                        .iter()
                        .copied()
                        .collect::<Vec<_>>();
                    return super::handle(app, Message::TrashPhotos(ids));
                }
            }
            _ => {}
        }
    } else if app.current_view == View::Timeline {
        match key {
            keyboard::Key::Named(keyboard::key::Named::Delete)
            | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                if !app.selected_timeline_photo_ids.is_empty() {
                    let ids = app
                        .selected_timeline_photo_ids
                        .iter()
                        .copied()
                        .collect::<Vec<_>>();
                    return super::handle(app, Message::TrashPhotos(ids));
                }
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                if !app.selected_timeline_photo_ids.is_empty() {
                    return super::handle(app, Message::ClearTimelinePhotoSelection);
                }
            }
            _ => {}
        }
    }

    // --- Global shortcuts (work from any non-detail view) ---
    match key {
        keyboard::Key::Character(ref ch) => {
            let lower = ch.to_lowercase();
            // '/' or 'f' → focus search (unless in a text-entry context)
            if (lower == "/" || lower == "f")
                && !matches!(
                    app.current_view,
                    View::PhotoDetail | View::Cull | View::Search
                )
                && app.editing_cluster_id.is_none()
            {
                return super::handle(app, Message::NavigateTo(View::Search));
            }
            // '[' → toggle sidebar
            if lower == "[" {
                return super::handle(app, Message::ToggleSidebar);
            }
        }
        _ => {}
    }

    Task::none()
}

pub(crate) fn rotate_photo(app: &mut PhotoVault) -> Task<Message> {
    app.photo_rotation = (app.photo_rotation + 90) % 360;

    // Rotate the in-memory display image (~1ms for 1000px thumbnail)
    if let Some(ref img) = app.current_display_image {
        app.current_display_image = Some(img.rotate90());
    }
    Task::none()
}

pub(crate) fn toggle_metadata_panel(app: &mut PhotoVault) -> Task<Message> {
    app.show_metadata_panel = !app.show_metadata_panel;
    Task::none()
}

pub(crate) fn display_image_ready(
    app: &mut PhotoVault,
    bytes_opt: Option<Vec<u8>>,
    w: u32,
    h: u32,
) -> Task<Message> {
    if let Some(bytes) = bytes_opt {
        if w > 0 && h > 0 {
            if let Some(rgba) = image::RgbaImage::from_raw(w, h, bytes) {
                app.current_display_image = Some(image::DynamicImage::ImageRgba8(rgba));
            }
        }
    }
    Task::none()
}
