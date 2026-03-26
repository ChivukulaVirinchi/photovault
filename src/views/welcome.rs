//! Welcome view — shown when no drive is selected
//!
//! A calm, confident landing that guides users to their photo library.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::components::DrivePicker;
use crate::config::AppTheme;
use crate::services::DriveInfo;
use crate::theme::colors;

/// Welcome view component
pub struct WelcomeView;

impl WelcomeView {
    /// Render the welcome view
    pub fn view(drives: &[DriveInfo], theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let logo = text("PhotoVault")
            .size(38)
            .color(p.text_primary);

        let tagline = text("Your photos. Your drive. Your privacy.")
            .size(14)
            .color(p.text_tertiary);

        let content = column![
            logo,
            Space::with_height(6),
            tagline,
            Space::with_height(56),
            DrivePicker::view(drives, theme),
        ]
        .align_x(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into()
    }
}
