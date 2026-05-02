//! Photo detail view — immersive photo viewer
//!
//! Full-screen image display with zoom/pan, instant rotation,
//! and togglable metadata panel.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::{Message, PhotoVault};
use crate::components::icon::{icon, Lucide};
use crate::components::tooltip::with_tooltip;
use crate::config::AppTheme;
use crate::models::Photo;
use crate::theme::colors;

pub struct PhotoDetailView;

impl PhotoDetailView {
    pub fn view(
        app: &PhotoVault,
        photo: &Photo,
        has_prev: bool,
        has_next: bool,
        people: &[(i64, String)],
        face_count: usize,
        image_handle: Option<&iced::widget::image::Handle>,
        show_metadata: bool,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let photo_id = photo.id;

        // === Top bar: labeled tool buttons ===
        let rotate_btn = Self::tool_btn("Rotate", Message::RotatePhoto, "Rotate 90° (R)", p);
        let info_btn = Self::tool_btn(
            "Info",
            Message::ToggleMetadataPanel,
            "Toggle metadata (I)",
            p,
        );
        let album_btn = Self::tool_btn(
            "Album",
            Message::OpenAlbumPicker(vec![photo_id]),
            "Add to album",
            p,
        );
        let trash_btn = {
            let danger = p.semantic_danger;
            let btn = button(text("Delete").size(11).color(danger))
                .padding(Padding::from([5, 12]))
                .style(move |_t: &iced::Theme, s| button::Style {
                    background: match s {
                        button::Status::Hovered => Some(iced::Color { a: 0.12, ..danger }.into()),
                        _ => None,
                    },
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Message::TrashPhotos(vec![photo_id]));
            with_tooltip(btn.into(), "Delete photo")
        };

        let close_btn = Self::tool_btn("Close", Message::ClosePhotoDetail, "Close (Esc)", p);

        let top_bg = p.bg_primary;
        let top_bar = container(
            row![
                rotate_btn,
                Space::with_width(4),
                info_btn,
                Space::with_width(4),
                album_btn,
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
        let prev_btn = Self::nav_arrow(
            Lucide::ChevronLeft,
            Message::PreviousPhoto,
            "Previous photo (←)",
            has_prev,
            p,
        );
        let next_btn = Self::nav_arrow(
            Lucide::ChevronRight,
            Message::NextPhoto,
            "Next photo (→)",
            has_next,
            p,
        );

        // === Image area — viewer widget with scroll-to-zoom + drag-to-pan ===
        let image_widget: Element<'static, Message> = if let Some(handle) = image_handle {
            let handle = handle.clone();
            container(
                iced::widget::image::viewer(handle)
                    .min_scale(1.0)
                    .max_scale(10.0)
                    .scale_step(0.15)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            let tc = p.text_tertiary;
            container(text("Loading photo...").size(14).color(tc))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        };

        let image_row = row![prev_btn, image_widget, next_btn,]
            .align_y(Alignment::Center)
            .height(Length::Fill);

        // === Metadata panel ===
        let meta_panel: Element<'static, Message> = if show_metadata {
            Self::build_metadata(app, photo, people, face_count, p)
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
        kind: Lucide,
        msg: Message,
        tooltip_label: &str,
        enabled: bool,
        p: &'static colors::Palette,
    ) -> Element<'static, Message> {
        let color = if enabled {
            p.text_secondary
        } else {
            p.text_tertiary
        };
        let hover_bg = p.bg_hover;
        let btn = button(icon(kind, 28, color))
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
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        if enabled {
            with_tooltip(btn.on_press(msg).into(), tooltip_label)
        } else {
            btn.into()
        }
    }

    fn tool_btn(
        icon: &str,
        msg: Message,
        tooltip_label: &str,
        p: &'static colors::Palette,
    ) -> Element<'static, Message> {
        let icon = icon.to_owned();
        let tc = p.text_secondary;
        let hover = p.bg_hover;
        let btn = button(text(icon).size(15).color(tc))
            .padding(Padding::from([5, 8]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: match s {
                    button::Status::Hovered => Some(hover.into()),
                    _ => None,
                },
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(msg);
        with_tooltip(btn.into(), tooltip_label)
    }

    /// Structured metadata panel with grouped info
    fn build_metadata(
        app: &PhotoVault,
        photo: &Photo,
        people: &[(i64, String)],
        face_count: usize,
        p: &'static colors::Palette,
    ) -> Element<'static, Message> {
        let label_color = p.text_tertiary;
        let value_color = p.text_primary;
        let secondary_color = p.text_secondary;

        // --- Group 1: Date & Location ---
        let mut date_loc_items: Vec<Element<'static, Message>> = Vec::new();

        if let Some(date) = &photo.date_taken {
            // Show a small caveat for dates that didn't come from a
            // capture-time EXIF tag — filename and file-metadata-derived
            // dates can be off, especially for copied/exported photos.
            let source_hint = match photo.date_taken_source.as_deref() {
                Some("filename") => Some("from filename"),
                Some("file_meta") => Some("from file metadata — may be inaccurate"),
                Some("mtime") => Some("from file date — may be inaccurate"),
                _ => None,
            };
            let mut entry = column![
                text("DATE").size(9).color(label_color),
                text(date.format("%b %d, %Y  %H:%M").to_string())
                    .size(12)
                    .color(value_color),
            ]
            .spacing(1);
            if let Some(hint) = source_hint {
                entry = entry.push(text(hint).size(10).color(label_color));
            }
            date_loc_items.push(entry.into());
        }

        // Use the pre-resolved location name (from DB or on-demand geocode),
        // falling back to raw coordinates only as a last resort.
        let location_text = app.current_photo_location.clone().or_else(|| {
            if photo.has_location() {
                Some("Resolving location...".to_string())
            } else {
                None
            }
        });

        if let Some(loc) = location_text {
            date_loc_items.push(
                column![
                    text("LOCATION").size(9).color(label_color),
                    text(loc).size(12).color(value_color),
                ]
                .spacing(1)
                .into(),
            );
        }

        // Mini-map is placed in its own group (rightmost) so it doesn't
        // crowd the EXIF columns. Capture it now, push to groups later.
        let mini_map = crate::views::photo_detail_map::photo_mini_map(app, photo);

        if let Some(alt) = photo.gps_altitude {
            date_loc_items.push(
                text(format!("{:.0}m altitude", alt))
                    .size(11)
                    .color(secondary_color)
                    .into(),
            );
        }

        if !people.is_empty() || face_count > 0 {
            let mut people_col: Vec<Element<'static, Message>> = Vec::new();
            people_col.push(text("PEOPLE").size(9).color(label_color).into());

            if !people.is_empty() {
                // Clickable people links that navigate to their cluster detail
                let accent = p.accent_primary;
                let hover_bg = p.bg_hover;
                let mut people_row: Vec<Element<'static, Message>> = Vec::new();
                for (cluster_id, name) in people.iter() {
                    let name = name.clone();
                    let cid = *cluster_id;
                    people_row.push(
                        button(text(name).size(12).color(accent))
                            .padding(Padding::from([2, 6]))
                            .style(move |_t: &iced::Theme, s| button::Style {
                                background: match s {
                                    button::Status::Hovered => Some(hover_bg.into()),
                                    _ => None,
                                },
                                border: iced::Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .on_press(Message::SelectCluster(cid))
                            .into(),
                    );
                }
                people_col.push(
                    iced::widget::Row::with_children(people_row)
                        .spacing(4)
                        .wrap()
                        .into(),
                );
            } else {
                people_col.push(
                    text(format!(
                        "{} face{} detected",
                        face_count,
                        if face_count == 1 { "" } else { "s" }
                    ))
                    .size(12)
                    .color(value_color)
                    .into(),
                );
            }

            date_loc_items.push(
                iced::widget::Column::with_children(people_col)
                    .spacing(2)
                    .into(),
            );
        }

        // --- Album membership ---
        if !app.current_photo_albums.is_empty() {
            let mut album_col: Vec<Element<'static, Message>> = Vec::new();
            album_col.push(text("ALBUMS").size(9).color(label_color).into());

            let accent = p.accent_primary;
            let hover_bg = p.bg_hover;
            let mut album_row: Vec<Element<'static, Message>> = Vec::new();
            for (album_id, name) in app.current_photo_albums.iter() {
                let name = name.clone();
                let aid = *album_id;
                album_row.push(
                    button(text(name).size(12).color(accent))
                        .padding(Padding::from([2, 6]))
                        .style(move |_t: &iced::Theme, s| button::Style {
                            background: match s {
                                button::Status::Hovered => Some(hover_bg.into()),
                                _ => None,
                            },
                            border: iced::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::OpenAlbum(aid))
                        .into(),
                );
            }
            album_col.push(
                iced::widget::Row::with_children(album_row)
                    .spacing(4)
                    .wrap()
                    .into(),
            );

            date_loc_items.push(
                iced::widget::Column::with_children(album_col)
                    .spacing(2)
                    .into(),
            );
        }

        // --- Group 2: Camera ---
        let mut camera_items: Vec<Element<'static, Message>> = Vec::new();

        let camera_name = photo
            .camera_model
            .clone()
            .or_else(|| photo.camera_make.clone());
        if let Some(cam) = camera_name {
            camera_items.push(
                column![
                    text("CAMERA").size(9).color(label_color),
                    text(cam).size(12).color(value_color),
                ]
                .spacing(1)
                .into(),
            );
        }

        if let Some(ref lens) = photo.lens_model {
            camera_items.push(
                column![
                    text("LENS").size(9).color(label_color),
                    text(lens.clone()).size(11).color(secondary_color),
                ]
                .spacing(1)
                .into(),
            );
        }

        // --- Group 3: Exposure ---
        let mut exp_parts: Vec<String> = Vec::new();
        if let Some(ref fl) = photo.focal_length {
            exp_parts.push(fl.clone());
        }
        if let Some(ref ap) = photo.aperture {
            exp_parts.push(ap.clone());
        }
        if let Some(ref ss) = photo.shutter_speed {
            exp_parts.push(ss.clone());
        }
        if let Some(iso) = photo.iso {
            exp_parts.push(format!("ISO {}", iso));
        }

        let mut exposure_items: Vec<Element<'static, Message>> = Vec::new();
        if !exp_parts.is_empty() {
            exposure_items.push(
                column![
                    text("EXPOSURE").size(9).color(label_color),
                    text(exp_parts.join("  \u{B7}  "))
                        .size(12)
                        .color(value_color),
                ]
                .spacing(1)
                .into(),
            );
        }

        if let Some(ref flash) = photo.flash {
            if flash == "Fired" {
                exposure_items.push(
                    row![
                        icon(Lucide::Flash, 11, secondary_color),
                        Space::with_width(4),
                        text("Flash").size(11).color(secondary_color),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
        }

        // --- Group 4: File ---
        let mut file_items: Vec<Element<'static, Message>> = Vec::new();

        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            let mp = (w as f64 * h as f64) / 1_000_000.0;
            file_items.push(
                text(format!("{}×{} ({:.1}MP)", w, h, mp))
                    .size(11)
                    .color(secondary_color)
                    .into(),
            );
        }
        file_items.push(
            text(Self::fmt_size(photo.file_size))
                .size(11)
                .color(secondary_color)
                .into(),
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

        // Flexible spacer pushes the mini-map to the right edge.
        if mini_map.is_some() {
            groups.push(Space::with_width(Length::Fill).into());
        }
        if let Some(mini) = mini_map {
            groups.push(mini);
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
                    color: border_color,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn fmt_size(bytes: i64) -> String {
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
