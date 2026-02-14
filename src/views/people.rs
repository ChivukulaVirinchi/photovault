//! People view - face clusters
//!
//! Placeholder for Phase 4 implementation.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::theme::colors::{Backgrounds, Text};

/// People view component
pub struct PeopleView;

impl PeopleView {
    /// Render the people view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("People").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Recognized faces will appear here.")
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
