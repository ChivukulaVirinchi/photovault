//! Individual person card rendering — normal mode and merge-selection mode.

use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::icon::{icon, Lucide};
use crate::config::AppTheme;
use crate::db::FaceClusterRecord;
use crate::theme::colors;

/// Render a person card in merge selection mode
pub fn person_card_merge(
    cluster: &FaceClusterRecord,
    is_selected: bool,
    is_highlighted: bool,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let accent_primary = p.accent_primary;
    let accent_muted = p.accent_muted;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let border_subtle = p.border_subtle;

    let cluster_id = cluster.id;

    // Face thumbnail — same as normal card
    let face_circle: Element<'static, Message> =
        if let Some(ref thumb_path) = cluster.face_thumbnail_path {
            let path = std::path::PathBuf::from(thumb_path);
            container(
                iced::widget::image(iced::widget::image::Handle::from_path(&path))
                    .width(80)
                    .height(80)
                    .content_fit(iced::ContentFit::Cover),
            )
            .width(80)
            .height(80)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 40.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            let initials = cluster
                .name
                .as_deref()
                .and_then(|n| n.chars().next())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());

            container(text(initials).size(28).color(text_secondary))
                .width(80)
                .height(80)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg_elevated.into()),
                    border: iced::Border {
                        radius: 40.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

    // Selection indicator
    let check_indicator: Element<'static, Message> = if is_selected {
        icon(Lucide::Check, 16, accent_primary)
    } else {
        icon(Lucide::Circle, 16, text_tertiary)
    };

    let display_name = cluster
        .name
        .clone()
        .unwrap_or_else(|| format!("Unknown Person {}", cluster.id));

    let card_content = column![
        row![Space::with_width(Length::Fill), check_indicator,],
        face_circle,
        Space::with_height(8),
        text(display_name).size(14).color(text_primary),
        text(format!(
            "{} {}",
            cluster.photo_count,
            if cluster.photo_count == 1 {
                "photo"
            } else {
                "photos"
            }
        ))
        .size(12)
        .color(text_tertiary),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let border_color = if is_highlighted || is_selected {
        accent_primary
    } else {
        border_subtle
    };
    let border_width = if is_highlighted || is_selected {
        2.0
    } else {
        1.0
    };

    button(
        container(card_content)
            .padding(16)
            .width(Length::Fixed(160.0)),
    )
    .style(move |_theme: &iced::Theme, status: button::Status| {
        let background = match status {
            button::Status::Hovered => Some(bg_hover.into()),
            _ => {
                if is_selected {
                    Some(accent_muted.into())
                } else {
                    Some(bg_elevated.into())
                }
            }
        };
        button::Style {
            background,
            border: iced::Border {
                color: border_color,
                width: border_width,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    })
    .on_press(Message::ToggleMergeSelect(cluster_id))
    .into()
}

/// Render a person card
pub fn person_card(
    cluster: &FaceClusterRecord,
    is_editing: bool,
    edit_name: &str,
    is_highlighted: bool,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let border_subtle = p.border_subtle;
    let accent_primary = p.accent_primary;

    let cluster_id = cluster.id;

    // Face thumbnail — show actual face crop if available, otherwise initials
    let face_circle: Element<'static, Message> =
        if let Some(ref thumb_path) = cluster.face_thumbnail_path {
            let path = std::path::PathBuf::from(thumb_path);
            container(
                iced::widget::image(iced::widget::image::Handle::from_path(&path))
                    .width(80)
                    .height(80)
                    .content_fit(iced::ContentFit::Cover),
            )
            .width(80)
            .height(80)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 40.0.into(), // Circular
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            // Fallback: show initials
            let initials = cluster
                .name
                .as_deref()
                .and_then(|n| n.chars().next())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());

            container(text(initials).size(28).color(text_secondary))
                .width(80)
                .height(80)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg_elevated.into()),
                    border: iced::Border {
                        radius: 40.0.into(), // Circular
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

    // Name (editable on click)
    let name_element: Element<'static, Message> = if is_editing {
        let edit_name_owned = edit_name.to_string();
        text_input("Enter name...", &edit_name_owned)
            .id(text_input::Id::new(format!("cluster-edit-{}", cluster_id)))
            .on_input(move |s| Message::EditClusterName(cluster_id, s))
            .on_submit(Message::SaveClusterName(cluster_id))
            .size(14)
            .width(Length::Fixed(140.0))
            .into()
    } else {
        let display_name = cluster
            .name
            .clone()
            .unwrap_or_else(|| format!("Unknown Person {}", cluster.id));

        button(text(display_name).size(14).color(text_primary))
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

    // Photo count
    let count = text(format!(
        "{} {}",
        cluster.photo_count,
        if cluster.photo_count == 1 {
            "photo"
        } else {
            "photos"
        }
    ))
    .size(12)
    .color(text_tertiary);

    let card_content = column![face_circle, Space::with_height(12), name_element, count,]
        .spacing(4)
        .align_x(Alignment::Center);

    button(
        container(card_content)
            .padding(16)
            .width(Length::Fixed(160.0)),
    )
    .style(move |_theme: &iced::Theme, status: button::Status| {
        let background = match status {
            button::Status::Hovered => Some(bg_hover.into()),
            _ => Some(bg_elevated.into()),
        };
        button::Style {
            background,
            border: iced::Border {
                color: if is_highlighted {
                    accent_primary
                } else {
                    border_subtle
                },
                width: if is_highlighted { 2.0 } else { 1.0 },
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    })
    .on_press(Message::SelectCluster(cluster_id))
    .into()
}
