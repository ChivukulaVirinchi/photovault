//! Search view
//!
//! Placeholder for Phase 6 implementation.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::theme::colors::{Backgrounds, Text};

/// Search view component
pub struct SearchView;

impl SearchView {
    /// Render the search view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("Search").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Search by date, location, or people.")
                .size(14)
                .color(Text::SECONDARY),
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
