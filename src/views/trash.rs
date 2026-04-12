//! Trash view.

use std::collections::HashSet;
use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::TrashedPhotoRecord;
use crate::services::trash::TrashStats;
use crate::theme::colors;
use crate::utils::format_bytes;

/// Trash view component.
pub struct TrashView;

impl TrashView {
    pub fn view(
        items: &[TrashedPhotoRecord],
        stats: &TrashStats,
        selected: &HashSet<i64>,
        drive_path: Option<&Path>,
        confirm_empty_trash: bool,
        confirm_delete_photo_id: Option<i64>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);

        if items.is_empty() {
            return Self::empty_view(theme);
        }

        let title = text("Trash").size(28).color(p.text_primary);
        let subtitle = text(format!(
            "{} photos - {}",
            stats.count,
            format_bytes(stats.total_size)
        ))
        .size(14)
        .color(p.text_secondary);

        let bg_elevated = p.bg_elevated;
        let warning = container(
            text("Photos in trash will be permanently deleted after 30 days")
                .size(12)
                .color(p.text_tertiary),
        )
        .padding(Padding::from([8, 12]))
        .style(move |_theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let item_list: Vec<Element<'static, Message>> = items
            .iter()
            .map(|item| {
                Self::trash_item(
                    item,
                    selected.contains(&item.photo_id),
                    drive_path,
                    confirm_delete_photo_id,
                    theme,
                )
            })
            .collect();

        let has_selection = !selected.is_empty();
        let text_primary = p.text_primary;
        let text_tertiary = p.text_tertiary;
        let semantic_danger = p.semantic_danger;
        let actions = row![
            button(
                text(if has_selection {
                    format!("Restore Selected ({})", selected.len())
                } else {
                    "Restore Selected".to_string()
                })
                .size(14)
                .color(if has_selection {
                    text_primary
                } else {
                    text_tertiary
                })
            )
            .padding(Padding::from([10, 18]))
            .on_press(Message::RestoreSelected),
            Space::with_width(Length::Fill),
            button(text("Empty Trash").size(14).color(semantic_danger))
                .padding(Padding::from([10, 18]))
                .style(move |_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(
                            iced::Color {
                                a: 0.15,
                                ..semantic_danger
                            }
                            .into(),
                        ),
                        _ => None,
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: semantic_danger,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(if confirm_empty_trash {
                    Message::ConfirmEmptyTrash
                } else {
                    Message::EmptyTrash
                }),
        ]
        .align_y(Alignment::Center);

        let confirm_text: Element<'static, Message> = if confirm_empty_trash {
            text("Click Empty Trash again to confirm permanent deletion")
                .size(12)
                .color(semantic_danger)
                .into()
        } else {
            Space::with_height(Length::Shrink).into()
        };

        let bg_primary = p.bg_primary;
        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(16),
            warning,
            Space::with_height(20),
            scrollable(Column::with_children(item_list).spacing(8))
                .id(iced::widget::scrollable::Id::new("trash"))
                .height(Length::Fill),
            Space::with_height(16),
            confirm_text,
            Space::with_height(8),
            actions,
        ]
        .padding(32)
        .height(Length::Fill);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into()
    }

    fn empty_view(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let content = column![
            text("Trash").size(28).color(p.text_primary),
            Space::with_height(16),
            text("Trash is empty").size(16).color(p.text_secondary),
            Space::with_height(8),
            text("Deleted photos will appear here")
                .size(14)
                .color(p.text_tertiary),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into()
    }

    fn trash_item(
        item: &TrashedPhotoRecord,
        is_selected: bool,
        drive_path: Option<&Path>,
        confirm_delete_photo_id: Option<i64>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let photo_id = item.photo_id;
        let size = item
            .file_size
            .map(|s| format_bytes(s as u64))
            .unwrap_or_default();
        let trashed = item.trashed_at.get(..10).unwrap_or(&item.trashed_at);

        let thumb: Element<'static, Message> =
            if let (Some(root), Some(tp)) = (drive_path, item.thumbnail_path.as_ref()) {
                let full = root.join(tp);
                if full.exists() {
                    let bg_elevated = p.bg_elevated;
                    container(
                        iced::widget::image(iced::widget::image::Handle::from_path(full))
                            .width(50)
                            .height(50)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .width(50)
                    .height(50)
                    .style(move |_theme| container::Style {
                        background: Some(bg_elevated.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
                } else {
                    Self::thumb_placeholder(theme)
                }
            } else {
                Self::thumb_placeholder(theme)
            };

        let accent_primary = p.accent_primary;
        let text_tertiary = p.text_tertiary;
        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;
        let semantic_danger = p.semantic_danger;
        let content = row![
            button(
                text(if is_selected { "+" } else { "" })
                    .size(12)
                    .color(if is_selected {
                        accent_primary
                    } else {
                        text_tertiary
                    })
            )
            .width(24)
            .height(24)
            .padding(0)
            .on_press(Message::ToggleTrashSelection(photo_id)),
            Space::with_width(14),
            thumb,
            Space::with_width(14),
            column![
                text(item.original_path.clone())
                    .size(13)
                    .color(text_primary),
                Space::with_height(4),
                row![
                    text(size).size(12).color(text_secondary),
                    Space::with_width(16),
                    text(format!("Deleted {}", trashed))
                        .size(12)
                        .color(text_tertiary),
                ],
            ]
            .width(Length::Fill),
            button(text("Restore").size(12).color(accent_primary))
                .padding(Padding::from([6, 12]))
                .on_press(Message::RestorePhoto(photo_id)),
            Space::with_width(8),
            button(
                text(if confirm_delete_photo_id == Some(photo_id) {
                    "Confirm Delete"
                } else {
                    "Delete"
                })
                .size(12)
                .color(semantic_danger)
            )
            .padding(Padding::from([6, 12]))
            .on_press(if confirm_delete_photo_id == Some(photo_id) {
                Message::ConfirmPermanentlyDeletePhoto(photo_id)
            } else {
                Message::PermanentlyDeletePhoto(photo_id)
            }),
        ]
        .align_y(Alignment::Center);

        let bg_selected = p.bg_selected;
        let bg_elevated = p.bg_elevated;
        let border_subtle = p.border_subtle;
        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(
                    if is_selected {
                        bg_selected
                    } else {
                        bg_elevated
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if is_selected {
                        accent_primary
                    } else {
                        border_subtle
                    },
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn thumb_placeholder(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_elevated = p.bg_elevated;
        container(text("IMG").size(10).color(p.text_tertiary))
            .width(50)
            .height(50)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(move |_theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }
}
