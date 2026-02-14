//! Welcome view - shown when no drive is selected
//!
//! A clean, inviting screen that guides users to select their photo library.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::components::DrivePicker;
use crate::services::DriveInfo;
use crate::theme::colors::{Backgrounds, Text};

/// Welcome view component
pub struct WelcomeView;

impl WelcomeView {
    /// Render the welcome view
    pub fn view(drives: &[DriveInfo]) -> Element<'static, Message> {
        let logo = text("PhotoVault").size(48).color(Text::PRIMARY);

        let tagline = text("Your photos. Your drive. Your privacy.")
            .size(16)
            .color(Text::SECONDARY);

        let content = column![
            logo,
            Space::with_height(8),
            tagline,
            Space::with_height(48),
            DrivePicker::view(drives),
        ]
        .align_x(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }
}
