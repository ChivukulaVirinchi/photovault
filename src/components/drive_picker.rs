//! Drive picker component
//!
//! Allows users to select a drive or folder to index.
//! Design: clean list with subtle status indicators.

use iced::widget::{button, column, container, scrollable, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::services::DriveInfo;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Drive picker component
pub struct DrivePicker;

impl DrivePicker {
    /// Render the drive picker
    pub fn view(drives: &[DriveInfo]) -> Element<'static, Message> {
        let title = text("Select a folder to index")
            .size(16)
            .color(Text::PRIMARY);

        let subtitle = text("Choose a drive or folder containing your photos")
            .size(13)
            .color(Text::SECONDARY);

        let drive_list: Element<'static, Message> = if drives.is_empty() {
            container(
                text("No drives detected")
                    .size(13)
                    .color(Text::TERTIARY),
            )
            .padding(24)
            .into()
        } else {
            // Show indexed drives first, then unindexed
            let mut sorted_drives: Vec<&DriveInfo> = drives.iter().collect();
            sorted_drives.sort_by(|a, b| b.has_photovault_db.cmp(&a.has_photovault_db));

            let items: Vec<Element<'static, Message>> = sorted_drives
                .iter()
                .map(|drive: &&DriveInfo| Self::drive_item((*drive).clone()))
                .collect();

            scrollable(column(items).spacing(6))
                .height(Length::Fixed(280.0))
                .into()
        };

        let browse_button = button(
            text("Browse for folder...")
                .size(13)
                .color(Text::SECONDARY),
        )
        .padding(Padding::from([10, 18]))
        .style(|_theme: &iced::Theme, status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                button::Status::Pressed => Some(Backgrounds::ACTIVE.into()),
                _ => None,
            };

            button::Style {
                background,
                text_color: Text::SECONDARY,
                border: iced::Border {
                    color: Border::VISIBLE,
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
    fn drive_item(drive: DriveInfo) -> Element<'static, Message> {
        let status_text = if drive.has_photovault_db {
            text("Previously indexed")
                .size(11)
                .color(Accent::PRIMARY)
        } else {
            text("Not indexed").size(11).color(Text::TERTIARY)
        };

        let name = drive.name.clone();
        let path_str = drive.path.to_string_lossy().to_string();

        let info = column![
            text(name).size(13).color(Text::PRIMARY),
            text(path_str).size(11).color(Text::SECONDARY),
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
        .style(|_theme: &iced::Theme, status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                button::Status::Pressed => Some(Backgrounds::ACTIVE.into()),
                _ => Some(Backgrounds::ELEVATED.into()),
            };

            button::Style {
                background,
                border: iced::Border {
                    color: Border::SUBTLE,
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
