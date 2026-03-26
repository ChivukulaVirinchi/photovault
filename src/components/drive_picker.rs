//! Drive picker component
//!
//! Allows users to select a drive or folder to index.
//! Design: clean list with subtle status indicators.

use iced::widget::{button, column, container, scrollable, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::DriveInfo;
use crate::theme::colors;

/// Drive picker component
pub struct DrivePicker;

impl DrivePicker {
    /// Render the drive picker
    pub fn view(drives: &[DriveInfo], theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);

        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;
        let text_tertiary = p.text_tertiary;
        let bg_hover = p.bg_hover;
        let bg_active = p.bg_active;
        let border_visible = p.border_visible;

        let title = text("Select a folder to index")
            .size(16)
            .color(text_primary);

        let subtitle = text("Choose a drive or folder containing your photos")
            .size(13)
            .color(text_secondary);

        let drive_list: Element<'static, Message> = if drives.is_empty() {
            container(
                text("No drives detected")
                    .size(13)
                    .color(text_tertiary),
            )
            .padding(24)
            .into()
        } else {
            // Show indexed drives first, then unindexed
            let mut sorted_drives: Vec<&DriveInfo> = drives.iter().collect();
            sorted_drives.sort_by(|a, b| b.has_photovault_db.cmp(&a.has_photovault_db));

            let items: Vec<Element<'static, Message>> = sorted_drives
                .iter()
                .map(|drive: &&DriveInfo| Self::drive_item((*drive).clone(), theme))
                .collect();

            scrollable(column(items).spacing(6))
                .height(Length::Fixed(280.0))
                .into()
        };

        let browse_button = button(
            text("Browse for folder...")
                .size(13)
                .color(text_secondary),
        )
        .padding(Padding::from([10, 18]))
        .style(move |_theme: &iced::Theme, status| {
            let background = match status {
                button::Status::Hovered => Some(bg_hover.into()),
                button::Status::Pressed => Some(bg_active.into()),
                _ => None,
            };

            button::Style {
                background,
                text_color: text_secondary,
                border: iced::Border {
                    color: border_visible,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::BrowseForFolder);

        let content = column![
            title,
            Space::with_height(6),
            subtitle,
            Space::with_height(28),
            drive_list,
            Space::with_height(16),
            browse_button,
        ]
        .align_x(Alignment::Center);

        container(content).padding(40).into()
    }

    /// Render a single drive item
    fn drive_item(drive: DriveInfo, theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);

        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;
        let text_tertiary = p.text_tertiary;
        let accent_primary = p.accent_primary;
        let bg_hover = p.bg_hover;
        let bg_active = p.bg_active;
        let bg_elevated = p.bg_elevated;
        let border_subtle = p.border_subtle;

        let status_text = if drive.has_photovault_db {
            text("Previously indexed")
                .size(11)
                .color(accent_primary)
        } else {
            text("Not indexed").size(11).color(text_tertiary)
        };

        let name = drive.name.clone();
        let path_str = drive.path.to_string_lossy().to_string();

        let info = column![
            text(name).size(13).color(text_primary),
            text(path_str).size(11).color(text_secondary),
            status_text,
        ]
        .spacing(3);

        let path = drive.path.clone();

        button(
            container(info)
                .padding(Padding::from([14, 18]))
                .width(Length::Fill),
        )
        .width(Length::Fixed(380.0))
        .style(move |_theme: &iced::Theme, status| {
            let background = match status {
                button::Status::Hovered => Some(bg_hover.into()),
                button::Status::Pressed => Some(bg_active.into()),
                _ => Some(bg_elevated.into()),
            };

            button::Style {
                background,
                border: iced::Border {
                    color: border_subtle,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectDrive(path))
        .into()
    }
}
