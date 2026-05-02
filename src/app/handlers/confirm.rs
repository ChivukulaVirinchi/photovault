//! Unified confirmation-dialog handlers for destructive actions.

use iced::Task;

use super::super::messages::Message;
use super::super::state::{PendingConfirmation, PhotoVault};

pub(crate) fn request(app: &mut PhotoVault, c: PendingConfirmation) -> Task<Message> {
    app.pending_confirmation = Some(c);
    Task::none()
}

pub(crate) fn confirm(app: &mut PhotoVault) -> Task<Message> {
    let Some(c) = app.pending_confirmation.take() else {
        return Task::none();
    };
    match c {
        PendingConfirmation::DeleteAlbum(id) => super::handle(app, Message::DeleteAlbum(id)),
        PendingConfirmation::EmptyTrash => super::handle(app, Message::ConfirmEmptyTrash),
        PendingConfirmation::PermanentlyDeletePhoto(id) => {
            super::handle(app, Message::ConfirmPermanentlyDeletePhoto(id))
        }
        PendingConfirmation::RebuildFaces => super::handle(app, Message::RebuildFaceClusters),
    }
}

pub(crate) fn cancel(app: &mut PhotoVault) -> Task<Message> {
    app.pending_confirmation = None;
    Task::none()
}
