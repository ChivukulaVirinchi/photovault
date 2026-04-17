//! Main application state and logic

mod handlers;
mod messages;
pub mod state;
mod view_status;
mod views;

#[allow(unused_imports)]
pub use messages::ScanResult;
pub use messages::{MapPopover, Message};
#[allow(unused_imports)]
pub use state::ScanState;
pub use state::{PhotoVault, View};

use iced::keyboard;
use iced::{event, Element, Subscription, Task};

use crate::config::AppTheme;

impl PhotoVault {
    /// Application title
    pub fn title(&self) -> String {
        match &self.selected_drive {
            Some(path) => format!("PhotoVault - {}", path.display()),
            None => "PhotoVault".to_string(),
        }
    }

    /// Current app theme.
    pub fn theme(&self) -> iced::Theme {
        match self.config.theme {
            AppTheme::Dark => iced::Theme::Dark,
            AppTheme::Light => iced::Theme::Light,
            AppTheme::System => iced::Theme::default(),
        }
    }

    /// Subscription for polling scan progress, keyboard events, and window resize
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        // Background progress polling (scan, face processing, or any active background op)
        let has_background_ops = self.scan_state.is_some()
            || self.face_processing_active
            || self.duplicate_detection_running
            || self.burst_detection_running
            || self.document_analysis_active;

        if has_background_ops {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(120))
                    .map(|_| Message::PollScanChannels),
            );
        }

        // Spinner animation — tick at ~8 fps while anything is loading.
        if self.is_anything_loading() {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(120))
                    .map(|_| Message::SpinnerTick),
            );
        }

        // Toast auto-dismiss check — half-second resolution is plenty.
        if !self.toasts.is_empty() {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(500))
                    .map(|_| Message::ToastTick),
            );
        }

        // Memory day-rollover tick: cheap NaiveDate compare every 60s.
        if self.selected_drive.is_some() && self.memories_enabled {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(60))
                    .map(|_| Message::MemoriesTick),
            );
        }

        // Memory slideshow auto-advance — only when actually viewing a
        // memory slideshow and not paused.
        if self.current_view == state::View::MemoryDetail
            && !self.memory_slideshow_paused
            && !self.memory_photos.is_empty()
        {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(4))
                    .map(|_| Message::MemorySlideshowTick),
            );
        }

        // Keyboard events for all views (shortcuts/focus nav)
        if self.selected_drive.is_some() {
            subs.push(event::listen_with(|event, _status, _id| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    Some(Message::KeyPressed(key, modifiers))
                }
                iced::Event::Window(iced::window::Event::Resized(size)) => {
                    Some(Message::WindowResized {
                        width: size.width,
                        height: size.height,
                    })
                }
                _ => None,
            }));
        }

        Subscription::batch(subs)
    }

    /// Handle messages
    pub fn update(&mut self, message: Message) -> Task<Message> {
        handlers::handle(self, message)
    }

    /// Render the application
    pub fn view(&self) -> Element<'_, Message> {
        views::view(self)
    }
}
