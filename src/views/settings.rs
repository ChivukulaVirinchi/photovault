//! Settings view
//!
//! Application settings and preferences.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::theme::colors::{Backgrounds, Text};

/// Settings view component
pub struct SettingsView;

impl SettingsView {
    /// Render the settings view
    pub fn view() -> Element<'static, Message> {
        let content = column![
            text("Settings").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Configure PhotoVault preferences.")
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
