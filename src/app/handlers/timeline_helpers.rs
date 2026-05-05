use iced::widget::scrollable;
use iced::Task;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(super) fn move_documents_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.documents.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.documents_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.documents_highlight_index = Some(next as usize);
    Task::none()
}

pub(super) fn move_sidebar_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = 13_i32;

    let current = app
        .sidebar_highlight_index
        .or_else(|| sidebar_index_for_view(&app.current_view))
        .unwrap_or(0) as i32;
    let mut next = current + delta;
    if next < 0 {
        next = total - 1;
    } else if next >= total {
        next = 0;
    }
    app.sidebar_highlight_index = Some(next as usize);

    Task::none()
}

pub(super) fn navigate_to_sidebar_highlight(app: &mut PhotoVault) -> Task<Message> {
    let target = app.sidebar_highlight_index.and_then(sidebar_view_for_index);
    if let Some(view) = target {
        return super::handle(app, Message::NavigateTo(view));
    }
    Task::none()
}

pub(super) fn sidebar_target_differs_current(app: &PhotoVault) -> bool {
    let target = app.sidebar_highlight_index;
    let current = sidebar_index_for_view(&app.current_view);
    match (target, current) {
        (Some(t), Some(c)) => t != c,
        (Some(_), None) => true,
        _ => false,
    }
}

fn sidebar_view_for_index(index: usize) -> Option<View> {
    Some(match index {
        0 => View::Timeline,
        1 => View::Map,
        2 => View::Memories,
        3 => View::Albums,
        4 => View::Insights,
        5 => View::Search,
        6 => View::People,
        7 => View::FaceReview,
        8 => View::Duplicates,
        9 => View::Bursts,
        10 => View::Trash,
        11 => View::Documents,
        12 => View::Settings,
        _ => return None,
    })
}

fn sidebar_index_for_view(view: &View) -> Option<usize> {
    Some(match view {
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
        _ => return None,
    })
}

pub(super) fn move_people_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.face_clusters.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.people_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.people_highlight_index = Some(next as usize);
    Task::none()
}

pub(super) fn move_memory_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.memories.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.memory_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.memory_highlight_index = Some(next as usize);
    Task::none()
}

pub(super) fn move_albums_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.albums.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.albums_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.albums_highlight_index = Some(next as usize);
    Task::none()
}

pub(super) fn move_duplicates_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.duplicate_groups.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.duplicates_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.duplicates_highlight_index = Some(next as usize);
    Task::none()
}

pub(super) fn move_bursts_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.burst_groups.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.bursts_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.bursts_highlight_index = Some(next as usize);
    Task::none()
}

pub(super) fn move_timeline_highlight(app: &mut PhotoVault, delta: i32) -> Task<Message> {
    let total = app.photos.len();
    if total == 0 {
        return Task::none();
    }
    let current = app.timeline_highlight_index.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, total as i32 - 1);
    app.timeline_highlight_index = Some(next as usize);

    if delta.abs() > 1 {
        scrollable::scroll_by(
            scrollable::Id::new("timeline"),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: if delta > 0 { 196.0 } else { -196.0 },
            },
        )
    } else {
        Task::none()
    }
}

pub(super) fn rotate_photo(app: &mut PhotoVault) -> Task<Message> {
    app.photo_rotation = (app.photo_rotation + 90) % 360;
    if let Some(ref img) = app.current_display_image {
        app.current_display_image = Some(img.rotate90());
    }
    Task::none()
}

pub(super) fn toggle_metadata_panel(app: &mut PhotoVault) -> Task<Message> {
    app.show_metadata_panel = !app.show_metadata_panel;
    Task::none()
}

pub(super) fn display_image_ready(
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

pub(super) fn photo_location_resolved(
    app: &mut PhotoVault,
    photo_id: i64,
    location: Option<String>,
) -> Task<Message> {
    if app.current_view != View::PhotoDetail {
        return Task::none();
    }
    let Some(idx) = app.selected_photo_index else {
        return Task::none();
    };
    let Some(photo) = app.photos.get(idx) else {
        return Task::none();
    };
    if photo.id == photo_id {
        app.current_photo_location = location;
        app.current_photo_location_resolved = true;
    }
    Task::none()
}
