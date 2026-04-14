//! Cluster detail view — single person with their photos.

use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::photo_grid::photo_grid_simple;
use crate::config::AppTheme;
use crate::db::FaceClusterRecord;
use crate::models::Photo;
use crate::theme::colors;

/// Render cluster detail view showing photos for a specific person
pub fn view_cluster_detail(
    cluster: &FaceClusterRecord,
    photos: &[Photo],
    editing: bool,
    edit_name: &str,
    columns: usize,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let bg_primary = p.bg_primary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;

    let cluster_id = cluster.id;

    // Back button
    let back_btn = button(
        row![
            text("\u{2190}").size(16).color(text_primary),
            text("People").size(14).color(text_secondary)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 12]))
    .style(move |_theme: &iced::Theme, status: button::Status| {
        let background = match status {
            button::Status::Hovered => Some(bg_hover.into()),
            _ => None,
        };
        button::Style {
            background,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .on_press(Message::BackToPeople);

    // Face thumbnail for header
    let face_avatar: Element<'static, Message> =
        if let Some(ref thumb_path) = cluster.face_thumbnail_path {
            let path = std::path::PathBuf::from(thumb_path);
            container(
                iced::widget::image(iced::widget::image::Handle::from_path(&path))
                    .width(64)
                    .height(64)
                    .content_fit(iced::ContentFit::Cover),
            )
            .width(64)
            .height(64)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 32.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            container(text("?").size(24).color(text_secondary))
                .width(64)
                .height(64)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg_elevated.into()),
                    border: iced::Border {
                        radius: 32.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

    // Person name (editable)
    let display_name = cluster
        .name
        .clone()
        .unwrap_or_else(|| format!("Unknown Person {}", cluster.id));

    let name_element: Element<'static, Message> = if editing {
        let edit_name_owned = edit_name.to_string();
        row![text_input("Enter name...", &edit_name_owned)
            .on_input(move |s| Message::EditClusterName(cluster_id, s))
            .on_submit(Message::SaveClusterName(cluster_id))
            .size(22)
            .width(Length::Fixed(300.0)),]
        .into()
    } else {
        button(text(display_name.clone()).size(24).color(text_primary))
            .padding(Padding::from([4, 8]))
            .style(move |_theme: &iced::Theme, status: button::Status| {
                let background = match status {
                    button::Status::Hovered => Some(bg_hover.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::StartEditClusterName(cluster_id))
            .into()
    };

    // Face count subtitle
    let subtitle = text(format!(
        "{} {}",
        photos.len(),
        if photos.len() == 1 { "photo" } else { "photos" }
    ))
    .size(14)
    .color(text_secondary);

    // Photo grid
    let grid: Element<'static, Message> = if photos.is_empty() {
        container(
            text("No photos found for this person.")
                .size(14)
                .color(text_tertiary),
        )
        .padding(32)
        .into()
    } else {
        photo_grid_simple(photos, 160.0, columns, None, None, theme)
    };

    let content = column![
        back_btn,
        Space::with_height(16),
        row![
            face_avatar,
            Space::with_width(16),
            column![name_element, subtitle,].spacing(4),
        ]
        .align_y(Alignment::Center),
        Space::with_height(24),
        scrollable(grid),
    ]
    .padding(32);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        })
        .into()
}
