//! Photo detail view — full-size photo viewer
//!
//! Shows the original photo with a compact metadata panel.
//! Supports navigation, rotation, people tags, and full EXIF display.

use std::path::Path;

use iced::widget::{button, column, container, row, text, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::theme::colors::{Backgrounds, Border, Semantic, Text as TC};

pub struct PhotoDetailView;

impl PhotoDetailView {
    pub fn view(
        photo: &Photo,
        has_prev: bool,
        has_next: bool,
        drive_path: &Path,
        people: &[String],
        rotation: i32,
    ) -> Element<'static, Message> {
        // Header: filename + actions
        let header = Self::header_bar(photo);

        // Navigation
        let prev_btn = Self::nav_btn("\u{2039}", Message::PreviousPhoto, has_prev);
        let next_btn = Self::nav_btn("\u{203A}", Message::NextPhoto, has_next);

        // Main image
        let image_area = Self::image_area(photo, drive_path, rotation);
        let image_with_nav = row![prev_btn, image_area, next_btn].align_y(Alignment::Center);

        // Metadata panel
        let metadata = Self::metadata_panel(photo, people);

        let content = column![header, image_with_nav, metadata];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    fn header_bar(photo: &Photo) -> Element<'static, Message> {
        let photo_id = photo.id;

        let close_btn = Self::icon_btn("\u{00D7}", 18, Message::ClosePhotoDetail);
        let rotate_btn = Self::icon_btn("\u{21BB}", 14, Message::RotatePhoto);
        let trash_btn = button(text("Trash").size(11).color(Semantic::DANGER))
            .padding(Padding::from([4, 10]))
            .style(|_theme: &iced::Theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(iced::Color { a: 0.12, ..Semantic::DANGER }.into()),
                    _ => None,
                },
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            })
            .on_press(Message::TrashPhotos(vec![photo_id]));

        row![
            Space::with_width(16),
            rotate_btn,
            Space::with_width(Length::Fill),
            trash_btn,
            Space::with_width(8),
            close_btn,
            Space::with_width(12),
        ]
        .align_y(Alignment::Center)
        .height(40)
        .into()
    }

    fn image_area(photo: &Photo, drive_path: &Path, _rotation: i32) -> Element<'static, Message> {
        // Always show original file
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

        // Fallback: thumbnail
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

        let fname = photo.file_name.clone();
        container(text(fname).size(14).color(TC::TERTIARY))
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

    fn nav_btn(label: &str, message: Message, enabled: bool) -> Element<'static, Message> {
        let color = if enabled { TC::SECONDARY } else { TC::TERTIARY };
        let label = label.to_owned();
        let btn = button(text(label).size(26).color(color))
            .padding(Padding::from([14, 10]))
            .style(move |_theme: &iced::Theme, status| button::Style {
                background: if enabled {
                    match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => None,
                    }
                } else {
                    None
                },
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            });

        if enabled { btn.on_press(message).into() } else { btn.into() }
    }

    fn icon_btn(icon: &str, size: u16, message: Message) -> Element<'static, Message> {
        let icon = icon.to_owned();
        button(text(icon).size(size).color(TC::SECONDARY))
            .padding(Padding::from([4, 8]))
            .style(|_theme: &iced::Theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                },
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            })
            .on_press(message)
            .into()
    }

    /// Compact metadata panel — two rows of key info
    fn metadata_panel(photo: &Photo, people: &[String]) -> Element<'static, Message> {
        let mut top_items: Vec<Element<'static, Message>> = Vec::new();
        let mut bottom_items: Vec<Element<'static, Message>> = Vec::new();

        // Top row: Date | Location | People
        if let Some(date) = &photo.date_taken {
            top_items.push(Self::chip(&date.format("%b %d, %Y  %H:%M").to_string()));
        }

        if let Some(loc) = photo.location_string() {
            top_items.push(Self::chip(&loc));
        } else if photo.has_location() {
            top_items.push(Self::chip(&format!(
                "{:.4}, {:.4}",
                photo.gps_latitude.unwrap_or(0.0),
                photo.gps_longitude.unwrap_or(0.0)
            )));
        }

        if let Some(alt) = photo.gps_altitude {
            top_items.push(Self::chip(&format!("{:.0}m alt", alt)));
        }

        if !people.is_empty() {
            top_items.push(Self::chip(&people.join(", ")));
        }

        // Bottom row: Camera + Exposure + Dimensions + File
        let mut camera_parts: Vec<String> = Vec::new();
        if let Some(ref make) = photo.camera_make {
            if let Some(ref model) = photo.camera_model {
                camera_parts.push(format!("{} {}", make, model));
            } else {
                camera_parts.push(make.clone());
            }
        }
        if let Some(ref lens) = photo.lens_model {
            camera_parts.push(lens.clone());
        }
        if !camera_parts.is_empty() {
            bottom_items.push(Self::meta_text(&camera_parts.join(" \u{2022} ")));
        }

        // Exposure string: 50mm  f/1.8  1/125  ISO 64  Flash: Off
        let mut exp_parts: Vec<String> = Vec::new();
        if let Some(ref fl) = photo.focal_length { exp_parts.push(fl.clone()); }
        if let Some(ref ap) = photo.aperture { exp_parts.push(ap.clone()); }
        if let Some(ref ss) = photo.shutter_speed { exp_parts.push(ss.clone()); }
        if let Some(iso) = photo.iso { exp_parts.push(format!("ISO {}", iso)); }
        if let Some(ref flash) = photo.flash { exp_parts.push(format!("Flash: {}", flash)); }
        if !exp_parts.is_empty() {
            bottom_items.push(Self::meta_text(&exp_parts.join("  ")));
        }

        // Dimensions + file size
        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            let mp = (w as f64 * h as f64) / 1_000_000.0;
            bottom_items.push(Self::meta_text(&format!(
                "{}x{} ({:.1}MP)  {}",
                w, h, mp,
                Self::format_size(photo.file_size)
            )));
        } else {
            bottom_items.push(Self::meta_text(&Self::format_size(photo.file_size)));
        }

        let top_row = Row::with_children(top_items).spacing(8).align_y(Alignment::Center);
        let bottom_row = Row::with_children(bottom_items).spacing(16).align_y(Alignment::Center);

        container(
            column![top_row, bottom_row].spacing(6),
        )
        .width(Length::Fill)
        .padding(Padding::from([10, 20]))
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

    /// Info chip — small rounded pill with text
    fn chip(label: &str) -> Element<'static, Message> {
        let label = label.to_string();
        container(text(label).size(11).color(TC::PRIMARY))
            .padding(Padding::from([3, 8]))
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// Simple metadata text
    fn meta_text(value: &str) -> Element<'static, Message> {
        text(value.to_string()).size(11).color(TC::SECONDARY).into()
    }

    fn format_size(bytes: i64) -> String {
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
