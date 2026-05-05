//! Welcome view — shown when no drive is selected
//!
//! A calm, confident landing that guides users to their photo library.

use iced::widget::{column, container, stack, text, Space};
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
    pub fn view(
        drives: &[DriveInfo],
        drop_target_active: bool,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let logo = text("PhotoVault").size(38).color(p.text_primary);

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

        let base = container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            });

        if drop_target_active {
            // Visible affordance for the drag-drop pathway: a tinted
            // overlay with a clear "drop here" label, dismissed when
            // the OS reports FilesHoveredLeft / FileDropped.
            let accent_muted = p.accent_muted;
            let accent_primary = p.accent_primary;
            let text_primary = p.text_primary;

            let hint = container(
                column![
                    text("Drop a folder here").size(28).color(text_primary),
                    Space::with_height(8),
                    text("PhotoVault will index it as a new library.")
                        .size(14)
                        .color(p.text_secondary),
                ]
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(
                    iced::Color {
                        a: 0.85,
                        ..accent_muted
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: accent_primary,
                    width: 3.0,
                    radius: 12.0.into(),
                },
                ..Default::default()
            });

            stack![base, hint].into()
        } else {
            base.into()
        }
    }
}
