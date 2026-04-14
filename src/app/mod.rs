//! Main application state and logic

mod messages;
mod state;
mod views;
mod handlers;

pub use messages::Message;
#[allow(unused_imports)]
pub use messages::ScanResult;
pub use state::{PhotoVault, View};
#[allow(unused_imports)]
pub use state::ScanState;

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

        // Keyboard events for all views (shortcuts)
        if self.selected_drive.is_some() {
            subs.push(event::listen_with(|event, _status, _id| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                    Some(Message::KeyPressed(key))
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
