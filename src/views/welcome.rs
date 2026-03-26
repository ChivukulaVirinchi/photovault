//! Welcome view — shown when no drive is selected
//!
//! A calm, confident landing that guides users to their photo library.

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
        let logo = text("PhotoVault")
            .size(38)
            .color(Text::PRIMARY);

        let tagline = text("Your photos. Your drive. Your privacy.")
            .size(14)
            .color(Text::TERTIARY);

        let content = column![
            logo,
            Space::with_height(6),
            tagline,
            Space::with_height(56),
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
