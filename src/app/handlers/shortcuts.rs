//! Keyboard shortcut handlers: overlay toggle + context-aware undo.

use iced::Task;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn toggle_overlay(app: &mut PhotoVault) -> Task<Message> {
    app.shortcuts_overlay_open = !app.shortcuts_overlay_open;
    Task::none()
}

pub(crate) fn undo_last_action(app: &mut PhotoVault) -> Task<Message> {
    match app.current_view {
        View::Cull => super::handle(app, Message::CullUndo),
        View::FaceReview => super::handle(app, Message::FaceReviewUndo),
        _ => super::handle(
            app,
            Message::ToastShow(crate::components::toast::Toast::info(
                "Nothing to undo here",
            )),
        ),
    }
}

/// Navigate the sidebar by index (Cmd+1..9).
pub(crate) fn navigate_by_index(app: &mut PhotoVault, index: usize) -> Task<Message> {
    let views = [
        View::Timeline,
        View::Map,
        View::Memories,
        View::Albums,
        View::Insights,
        View::Search,
        View::People,
        View::Duplicates,
        View::Bursts,
    ];
    if let Some(v) = views.get(index) {
        return super::handle(app, Message::NavigateTo(v.clone()));
    }
    Task::none()
}

/// Close the current detail-ish view back to its parent.
pub(crate) fn close_current_detail(app: &mut PhotoVault) -> Task<Message> {
    match app.current_view {
        View::PhotoDetail => super::handle(app, Message::ClosePhotoDetail),
        View::AlbumDetail => super::handle(app, Message::BackToAlbums),
        View::ClusterDetail => super::handle(app, Message::BackToPeople),
        View::MemoryDetail => super::handle(app, Message::CloseMemoryDetail),
        View::DuplicateDetail => super::handle(app, Message::CloseDuplicateDetail),
        View::BurstDetail => super::handle(app, Message::CloseBurstDetail),
        View::Cull => super::handle(app, Message::ExitCullMode),
        _ => Task::none(),
    }
}
