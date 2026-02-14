//! Timeline view - main photo browsing interface
//!
//! Placeholder for Phase 2 implementation.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::theme::colors::{Backgrounds, Text};

/// Timeline view component
pub struct TimelineView;

impl TimelineView {
    /// Render the timeline view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("Timeline").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Your photos will appear here, organized by date.")
                .size(14)
                .color(Text::SECONDARY),
            Space::with_height(32),
            text("Select a folder to start indexing...")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }
}
