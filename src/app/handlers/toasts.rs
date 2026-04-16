//! Toast notification handlers.

use iced::Task;

use crate::components::toast::Toast;

use super::super::messages::Message;
use super::super::state::PhotoVault;

pub(crate) fn show(app: &mut PhotoVault, mut toast: Toast) -> Task<Message> {
    toast.id = app.toast_next_id;
    app.toast_next_id = app.toast_next_id.wrapping_add(1);
    app.toasts.push(toast);
    // Cap stack depth at 5 — drop oldest
    while app.toasts.len() > 5 {
        app.toasts.remove(0);
    }
    Task::none()
}

pub(crate) fn dismiss(app: &mut PhotoVault, id: u64) -> Task<Message> {
    app.toasts.retain(|t| t.id != id);
    Task::none()
}

pub(crate) fn tick(app: &mut PhotoVault) -> Task<Message> {
    app.toasts.retain(|t| !t.is_expired());
    Task::none()
}

pub(crate) fn spinner_tick(app: &mut PhotoVault) -> Task<Message> {
    app.spinner_phase = app.spinner_phase.wrapping_add(1);
    Task::none()
}
