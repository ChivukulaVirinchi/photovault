//! Photo detail view - full-size photo viewer
//!
//! Modal overlay for viewing photos at full resolution.
//! Supports keyboard navigation between photos.

use std::path::Path;

use iced::widget::{button, column, container, row, text, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::theme::colors::{Backgrounds, Border, ColorExt, Text as TextColors};

/// Photo detail view component
pub struct PhotoDetailView;

impl PhotoDetailView {
    /// Render the photo detail view as a full-screen overlay
    ///
    /// `drive_path` is used to resolve the photo's original file for full-res display.
    pub fn view(
        photo: &Photo,
        has_prev: bool,
        has_next: bool,
        drive_path: &Path,
    ) -> Element<'static, Message> {
        // Close button (top right)
        let close_btn = button(text("\u{00D7}").size(24).color(TextColors::PRIMARY))
            .padding(Padding::from([8, 12]))
            .style(|_theme: &iced::Theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    text_color: TextColors::PRIMARY,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::ClosePhotoDetail);

        // Navigation buttons
        let prev_btn = Self::nav_button("<", Message::PreviousPhoto, has_prev);
        let next_btn = Self::nav_button(">", Message::NextPhoto, has_next);

        // Photo metadata bar
        let metadata = Self::metadata_bar(photo);

        // Main image area (shows original full-res photo)
        let image_area = Self::image_area(photo, drive_path);

        // Layout: header, image with nav buttons, metadata
        let header =
            row![Space::with_width(Length::Fill), close_btn,].padding(Padding::from([8, 16]));

        let image_with_nav = row![prev_btn, image_area, next_btn,].align_y(Alignment::Center);

        let content = column![header, image_with_nav, metadata,];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.scale_alpha(0.95).into()),
                ..Default::default()
            })
            .into()
    }

    /// Image area - shows the original full-resolution photo (not the 256px thumbnail)
    fn image_area(photo: &Photo, drive_path: &Path) -> Element<'static, Message> {
        let file_name = photo.file_name.clone();
        let width = photo.width.unwrap_or(0);
        let height = photo.height.unwrap_or(0);

        // Load the original full-resolution image via drive_path + file_path
        let original_path = drive_path.join(&photo.file_path);
        if original_path.exists() {
            return container(
                iced::widget::image(iced::widget::image::Handle::from_path(&original_path))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Contain),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..Default::default()
            })
            .into();
        }

        // Fallback: try thumbnail if original is missing
        if let Some(ref thumb_path) = photo.thumbnail_path {
            let path = std::path::PathBuf::from(thumb_path);
            return container(
                iced::widget::image(iced::widget::image::Handle::from_path(&path))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Contain),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..Default::default()
            })
            .into();
        }

        // Placeholder
        container(
            column![
                text(file_name).size(16).color(TextColors::PRIMARY),
                Space::with_height(8),
                text(format!("{}x{}", width, height))
                    .size(14)
                    .color(TextColors::SECONDARY),
            ]
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Color::BLACK.into()),
            ..Default::default()
        })
        .into()
    }

    /// Navigation button (prev/next)
    fn nav_button(label: &str, message: Message, enabled: bool) -> Element<'static, Message> {
        let label_color = if enabled {
            TextColors::PRIMARY
        } else {
            TextColors::TERTIARY
        };

        let label_owned = label.to_owned();

        let btn = button(text(label_owned).size(32).color(label_color))
            .padding(Padding::from([20, 16]))
            .style(move |_theme: &iced::Theme, status| {
                let background = if !enabled {
                    None
                } else {
                    match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => None,
                    }
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            });

        if enabled {
            btn.on_press(message).into()
        } else {
            btn.into()
        }
    }

    /// Metadata bar at bottom of detail view
    fn metadata_bar(photo: &Photo) -> Element<'static, Message> {
        let mut items: Vec<Element<'static, Message>> = Vec::new();

        // Date
        if let Some(date) = &photo.date_taken {
            items.push(Self::metadata_item(
                "Date",
                &date.format("%B %d, %Y %H:%M").to_string(),
            ));
        }

        // Location
        if let Some(location) = photo.location_string() {
            items.push(Self::metadata_item("Location", &location));
        }

        // Camera
        if let Some(ref make) = photo.camera_make {
            let camera = match &photo.camera_model {
                Some(model) => format!("{} {}", make, model),
                None => make.clone(),
            };
            items.push(Self::metadata_item("Camera", &camera));
        }

        // Dimensions
        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            items.push(Self::metadata_item("Size", &format!("{}x{}", w, h)));
        }

        // File size
        let size_str = Self::format_file_size(photo.file_size);
        items.push(Self::metadata_item("File", &size_str));

        if items.is_empty() {
            items.push(
                text("No metadata available")
                    .size(13)
                    .color(TextColors::TERTIARY)
                    .into(),
            );
        }

        let metadata_row = Row::with_children(items)
            .spacing(32)
            .align_y(Alignment::Center);

        container(metadata_row)
            .width(Length::Fill)
            .padding(Padding::from([16, 32]))
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::SECONDARY.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Single metadata item (label + value)
    fn metadata_item(label: &str, value: &str) -> Element<'static, Message> {
        column![
            text(label.to_owned()).size(11).color(TextColors::TERTIARY),
            text(value.to_owned()).size(13).color(TextColors::PRIMARY),
        ]
        .spacing(2)
        .into()
    }

    /// Format file size for display
    fn format_file_size(bytes: i64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}
