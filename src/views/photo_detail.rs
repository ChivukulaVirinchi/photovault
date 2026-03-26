//! Photo detail view — immersive photo viewer
//!
//! Full-screen image display with floating metadata overlay.
//! Image takes maximum space. Metadata shown as an elegant bottom panel.

use std::path::Path;

use iced::widget::{button, column, container, row, text, Space};
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
        rotated_path: Option<&std::path::PathBuf>,
    ) -> Element<'static, Message> {
        let photo_id = photo.id;

        // === Top bar: minimal, transparent feel ===
        let rotate_btn = Self::tool_btn("\u{21BB}", "Rotate (R)", Message::RotatePhoto);
        let trash_btn = button(
            text("Trash").size(11).color(Semantic::DANGER),
        )
        .padding(Padding::from([5, 12]))
        .style(|_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(iced::Color { a: 0.12, ..Semantic::DANGER }.into()),
                _ => None,
            },
            border: iced::Border { radius: 6.0.into(), ..Default::default() },
            ..Default::default()
        })
        .on_press(Message::TrashPhotos(vec![photo_id]));

        let close_btn = Self::tool_btn("\u{00D7}", "Close (Esc)", Message::ClosePhotoDetail);

        let top_bar = container(
            row![
                rotate_btn,
                Space::with_width(Length::Fill),
                trash_btn,
                Space::with_width(8),
                close_btn,
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([6, 16])),
        )
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(Backgrounds::PRIMARY.into()),
            ..Default::default()
        });

        // === Navigation arrows ===
        let prev_btn = Self::nav_arrow("\u{2039}", Message::PreviousPhoto, has_prev);
        let next_btn = Self::nav_arrow("\u{203A}", Message::NextPhoto, has_next);

        // === Image area — takes all available space ===
        let image_widget = Self::render_image(photo, drive_path, rotated_path);

        let image_row = row![
            prev_btn,
            image_widget,
            next_btn,
        ]
        .align_y(Alignment::Center);

        // === Metadata panel — compact, two lines ===
        let meta_panel = Self::build_metadata(photo, people);

        container(column![top_bar, image_row, meta_panel])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_t: &iced::Theme| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..Default::default()
            })
            .into()
    }

    fn render_image(photo: &Photo, drive_path: &Path, rotated_path: Option<&std::path::PathBuf>) -> Element<'static, Message> {
        // If we have a rotated version, use that
        if let Some(rp) = rotated_path {
            if rp.exists() {
                return container(
                    iced::widget::image(iced::widget::image::Handle::from_path(rp))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced::ContentFit::Contain),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
            }
        }

        let original = drive_path.join(&photo.file_path);
        let path = if original.exists() {
            original
        } else if let Some(ref tp) = photo.thumbnail_path {
            std::path::PathBuf::from(tp)
        } else {
            let fname = photo.file_name.clone();
            return container(text(fname).size(14).color(TC::TERTIARY))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };

        container(
            iced::widget::image(iced::widget::image::Handle::from_path(&path))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(iced::ContentFit::Contain),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }

    fn nav_arrow(symbol: &str, msg: Message, enabled: bool) -> Element<'static, Message> {
        let color = if enabled { TC::SECONDARY } else { TC::TERTIARY };
        let symbol = symbol.to_owned();
        let btn = button(text(symbol).size(28).color(color))
            .padding(Padding::from([20, 8]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: if enabled {
                    match s {
                        button::Status::Hovered => Some(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.05).into()),
                        _ => None,
                    }
                } else {
                    None
                },
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            });

        if enabled { btn.on_press(msg).into() } else { btn.into() }
    }

    fn tool_btn(icon: &str, _tooltip: &str, msg: Message) -> Element<'static, Message> {
        let icon = icon.to_owned();
        button(text(icon).size(15).color(TC::SECONDARY))
            .padding(Padding::from([5, 8]))
            .style(|_t: &iced::Theme, s| button::Style {
                background: match s {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                },
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            })
            .on_press(msg)
            .into()
    }

    /// Compact two-line metadata panel
    fn build_metadata(photo: &Photo, people: &[String]) -> Element<'static, Message> {
        // Line 1: When, Where, Who — as readable text
        let mut line1_parts: Vec<String> = Vec::new();

        if let Some(date) = &photo.date_taken {
            line1_parts.push(date.format("%b %d, %Y  %H:%M").to_string());
        }

        if let Some(loc) = photo.location_string() {
            line1_parts.push(loc);
        } else if photo.has_location() {
            line1_parts.push(format!(
                "{:.4}, {:.4}",
                photo.gps_latitude.unwrap_or(0.0),
                photo.gps_longitude.unwrap_or(0.0)
            ));
        }

        if let Some(alt) = photo.gps_altitude {
            line1_parts.push(format!("{:.0}m", alt));
        }

        if !people.is_empty() {
            line1_parts.push(people.join(", "));
        }

        let line1 = if line1_parts.is_empty() {
            photo.file_name.clone()
        } else {
            line1_parts.join("   \u{2022}   ")
        };

        // Line 2: Camera + Exposure + Dimensions + Size
        let mut line2_parts: Vec<String> = Vec::new();

        // Camera
        if let Some(ref model) = photo.camera_model {
            line2_parts.push(model.clone());
        } else if let Some(ref make) = photo.camera_make {
            line2_parts.push(make.clone());
        }

        // Lens
        if let Some(ref lens) = photo.lens_model {
            line2_parts.push(lens.clone());
        }

        // Exposure: 50mm  f/1.8  1/125  ISO 64
        let mut exp: Vec<String> = Vec::new();
        if let Some(ref fl) = photo.focal_length { exp.push(fl.clone()); }
        if let Some(ref ap) = photo.aperture { exp.push(ap.clone()); }
        if let Some(ref ss) = photo.shutter_speed { exp.push(ss.clone()); }
        if let Some(iso) = photo.iso { exp.push(format!("ISO {}", iso)); }
        if !exp.is_empty() {
            line2_parts.push(exp.join("  "));
        }

        if let Some(ref flash) = photo.flash {
            if flash == "Fired" {
                line2_parts.push("Flash".to_string());
            }
        }

        // Dimensions + file
        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            let mp = (w as f64 * h as f64) / 1_000_000.0;
            line2_parts.push(format!("{}x{} ({:.1}MP)", w, h, mp));
        }
        line2_parts.push(Self::fmt_size(photo.file_size));

        let line2 = line2_parts.join("   \u{2022}   ");

        container(
            column![
                text(line1).size(12).color(TC::PRIMARY),
                text(line2).size(11).color(TC::SECONDARY),
            ]
            .spacing(3),
        )
        .width(Length::Fill)
        .padding(Padding::from([10, 20]))
        .style(|_t: &iced::Theme| container::Style {
            background: Some(Backgrounds::SECONDARY.into()),
            border: iced::Border {
                color: Border::SUBTLE, width: 1.0, radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    fn fmt_size(bytes: i64) -> String {
        if bytes < 1024 { format!("{} B", bytes) }
        else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
        else if bytes < 1024 * 1024 * 1024 { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
        else { format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0)) }
    }
}
