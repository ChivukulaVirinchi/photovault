//! Photo detail view — immersive photo viewer
//!
//! Full-screen image display with zoom/pan, instant rotation,
//! and togglable metadata panel.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::models::Photo;
use crate::theme::colors;

pub struct PhotoDetailView;

impl PhotoDetailView {
    pub fn view(
        photo: &Photo,
        has_prev: bool,
        has_next: bool,
        people: &[String],
        image_handle: Option<&iced::widget::image::Handle>,
        show_metadata: bool,
        zoom_to_fit: bool,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let photo_id = photo.id;

        // === Top bar: minimal, transparent feel ===
        let rotate_btn = Self::tool_btn("\u{21BB}", Message::RotatePhoto, p);
        let info_btn = Self::tool_btn("i", Message::ToggleMetadataPanel, p);
        let trash_btn = {
            let danger = p.semantic_danger;
            button(
                text("Trash").size(11).color(danger),
            )
            .padding(Padding::from([5, 12]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: match s {
                    button::Status::Hovered => Some(iced::Color { a: 0.12, ..danger }.into()),
                    _ => None,
                },
                border: iced::Border { radius: 6.0.into(), ..Default::default() },
                ..Default::default()
            })
            .on_press(Message::TrashPhotos(vec![photo_id]))
        };

        let close_btn = Self::tool_btn("\u{00D7}", Message::ClosePhotoDetail, p);

        let top_bg = p.bg_primary;
        let top_bar = container(
            row![
                rotate_btn,
                Space::with_width(4),
                info_btn,
                Space::with_width(Length::Fill),
                trash_btn,
                Space::with_width(8),
                close_btn,
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([6, 16])),
        )
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(top_bg.into()),
            ..Default::default()
        });

        // === Navigation arrows ===
        let prev_btn = Self::nav_arrow("\u{2039}", Message::PreviousPhoto, has_prev, p);
        let next_btn = Self::nav_arrow("\u{203A}", Message::NextPhoto, has_next, p);

        // === Image area — takes all available space ===
        let image_widget: Element<'static, Message> = if let Some(handle) = image_handle {
            let handle = handle.clone();
            if zoom_to_fit {
                // Fit-to-window mode: simple image widget
                container(
                    iced::widget::image(handle)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced::ContentFit::Contain),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
            } else {
                // Free zoom mode: viewer widget with scroll-to-zoom + drag-to-pan
                container(
                    iced::widget::image::viewer(handle)
                        .min_scale(0.1)
                        .max_scale(15.0)
                        .scale_step(0.10)
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        } else {
            let fname = photo.file_name.clone();
            let tc = p.text_tertiary;
            container(text(fname).size(14).color(tc))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        };

        // Double-click hint (zoom toggle)
        let zoom_hint = {
            let hint_text = if zoom_to_fit { "Scroll to zoom" } else { "Fit (Esc zoom)" };
            let tc = p.text_tertiary;
            container(text(hint_text).size(9).color(tc))
                .padding(Padding::from([2, 8]))
        };

        let image_row = row![
            prev_btn,
            column![image_widget, zoom_hint].width(Length::Fill),
            next_btn,
        ]
        .align_y(Alignment::Center)
        .height(Length::Fill);

        // === Metadata panel ===
        let meta_panel: Element<'static, Message> = if show_metadata {
            Self::build_metadata(photo, people, p)
        } else {
            Space::with_height(0).into()
        };

        let viewer_bg = p.bg_primary;
        container(column![top_bar, image_row, meta_panel])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(viewer_bg.into()),
                ..Default::default()
            })
            .into()
    }

    fn nav_arrow(
        symbol: &str,
        msg: Message,
        enabled: bool,
        p: &'static colors::Palette,
    ) -> Element<'static, Message> {
        let color = if enabled { p.text_secondary } else { p.text_tertiary };
        let hover_bg = p.bg_hover;
        let symbol = symbol.to_owned();
        let btn = button(text(symbol).size(28).color(color))
            .padding(Padding::from([20, 8]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: if enabled {
                    match s {
                        button::Status::Hovered => Some(hover_bg.into()),
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

    fn tool_btn(
        icon: &str,
        msg: Message,
        p: &'static colors::Palette,
    ) -> Element<'static, Message> {
        let icon = icon.to_owned();
        let tc = p.text_secondary;
        let hover = p.bg_hover;
        button(text(icon).size(15).color(tc))
            .padding(Padding::from([5, 8]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: match s {
                    button::Status::Hovered => Some(hover.into()),
                    _ => None,
                },
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            })
            .on_press(msg)
            .into()
    }

    /// Structured metadata panel with grouped info
    fn build_metadata(
        photo: &Photo,
        people: &[String],
        p: &'static colors::Palette,
    ) -> Element<'static, Message> {
        let label_color = p.text_tertiary;
        let value_color = p.text_primary;
        let secondary_color = p.text_secondary;

        // --- Group 1: Date & Location ---
        let mut date_loc_items: Vec<Element<'static, Message>> = Vec::new();

        if let Some(date) = &photo.date_taken {
            date_loc_items.push(
                column![
                    text("DATE").size(9).color(label_color),
                    text(date.format("%b %d, %Y  %H:%M").to_string()).size(12).color(value_color),
                ].spacing(1).into()
            );
        }

        let location_text = photo.location_string()
            .or_else(|| {
                if photo.has_location() {
                    Some(format!("{:.4}, {:.4}",
                        photo.gps_latitude.unwrap_or(0.0),
                        photo.gps_longitude.unwrap_or(0.0)))
                } else {
                    None
                }
            });

        if let Some(loc) = location_text {
            date_loc_items.push(
                column![
                    text("LOCATION").size(9).color(label_color),
                    text(loc).size(12).color(value_color),
                ].spacing(1).into()
            );
        }

        if let Some(alt) = photo.gps_altitude {
            date_loc_items.push(
                text(format!("{:.0}m", alt)).size(11).color(secondary_color).into()
            );
        }

        if !people.is_empty() {
            date_loc_items.push(
                column![
                    text("PEOPLE").size(9).color(label_color),
                    text(people.join(", ")).size(12).color(value_color),
                ].spacing(1).into()
            );
        }

        // --- Group 2: Camera ---
        let mut camera_items: Vec<Element<'static, Message>> = Vec::new();

        let camera_name = photo.camera_model.clone()
            .or_else(|| photo.camera_make.clone());
        if let Some(cam) = camera_name {
            camera_items.push(
                column![
                    text("CAMERA").size(9).color(label_color),
                    text(cam).size(12).color(value_color),
                ].spacing(1).into()
            );
        }

        if let Some(ref lens) = photo.lens_model {
            camera_items.push(
                column![
                    text("LENS").size(9).color(label_color),
                    text(lens.clone()).size(11).color(secondary_color),
                ].spacing(1).into()
            );
        }

        // --- Group 3: Exposure ---
        let mut exp_parts: Vec<String> = Vec::new();
        if let Some(ref fl) = photo.focal_length { exp_parts.push(fl.clone()); }
        if let Some(ref ap) = photo.aperture { exp_parts.push(ap.clone()); }
        if let Some(ref ss) = photo.shutter_speed { exp_parts.push(ss.clone()); }
        if let Some(iso) = photo.iso { exp_parts.push(format!("ISO {}", iso)); }

        let mut exposure_items: Vec<Element<'static, Message>> = Vec::new();
        if !exp_parts.is_empty() {
            exposure_items.push(
                column![
                    text("EXPOSURE").size(9).color(label_color),
                    text(exp_parts.join("  \u{B7}  ")).size(12).color(value_color),
                ].spacing(1).into()
            );
        }

        if let Some(ref flash) = photo.flash {
            if flash == "Fired" {
                exposure_items.push(
                    text("\u{26A1} Flash").size(11).color(secondary_color).into()
                );
            }
        }

        // --- Group 4: File ---
        let mut file_items: Vec<Element<'static, Message>> = Vec::new();

        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            let mp = (w as f64 * h as f64) / 1_000_000.0;
            file_items.push(
                text(format!("{}×{} ({:.1}MP)", w, h, mp)).size(11).color(secondary_color).into()
            );
        }
        file_items.push(
            text(Self::fmt_size(photo.file_size)).size(11).color(secondary_color).into()
        );

        // --- Assemble groups into a row ---
        let mut groups: Vec<Element<'static, Message>> = Vec::new();

        if !date_loc_items.is_empty() {
            let group = iced::widget::Column::with_children(date_loc_items).spacing(4);
            groups.push(group.into());
        }
        if !camera_items.is_empty() {
            let group = iced::widget::Column::with_children(camera_items).spacing(4);
            groups.push(group.into());
        }
        if !exposure_items.is_empty() {
            let group = iced::widget::Column::with_children(exposure_items).spacing(4);
            groups.push(group.into());
        }
        if !file_items.is_empty() {
            let group = iced::widget::Column::with_children(file_items).spacing(2);
            groups.push(group.into());
        }

        let meta_row = iced::widget::Row::with_children(groups)
            .spacing(32)
            .align_y(Alignment::Start);

        let panel_bg = p.bg_secondary;
        let border_color = p.border_subtle;

        container(meta_row)
            .width(Length::Fill)
            .padding(Padding::from([12, 24]))
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(panel_bg.into()),
                border: iced::Border {
                    color: border_color, width: 1.0, radius: 0.0.into(),
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
