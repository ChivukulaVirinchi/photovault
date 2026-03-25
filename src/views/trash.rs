//! Trash view.

use std::collections::HashSet;
use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::TrashedPhotoRecord;
use crate::services::trash::TrashStats;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};
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
    ) -> Element<'static, Message> {
        if items.is_empty() {
            return Self::empty_view();
        }

        let title = text("Trash").size(28).color(Text::PRIMARY);
        let subtitle = text(format!(
            "{} photos - {}",
            stats.count,
            format_bytes(stats.total_size)
        ))
        .size(14)
        .color(Text::SECONDARY);

        let warning = container(
            text("Photos in trash will be permanently deleted after 30 days")
                .size(12)
                .color(Text::TERTIARY),
        )
        .padding(Padding::from([8, 12]))
        .style(|_theme| container::Style {
            background: Some(Backgrounds::ELEVATED.into()),
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
                )
            })
            .collect();

        let has_selection = !selected.is_empty();
        let actions = row![
            button(
                text(if has_selection {
                    format!("Restore Selected ({})", selected.len())
                } else {
                    "Restore Selected".to_string()
                })
                .size(14)
                .color(if has_selection {
                    Text::PRIMARY
                } else {
                    Text::TERTIARY
                })
            )
            .padding(Padding::from([10, 18]))
            .on_press(Message::RestoreSelected),
            Space::with_width(Length::Fill),
            button(
                text("Empty Trash")
                    .size(14)
                    .color(iced::Color::from_rgb(0.8, 0.2, 0.2))
            )
            .padding(Padding::from([10, 18]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => {
                        Some(iced::Color::from_rgba(0.8, 0.2, 0.2, 0.2).into())
                    }
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: iced::Color::from_rgb(0.8, 0.2, 0.2),
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
                .color(iced::Color::from_rgb(0.8, 0.2, 0.2))
                .into()
        } else {
            Space::with_height(Length::Shrink).into()
        };

        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(16),
            warning,
            Space::with_height(20),
            scrollable(Column::with_children(item_list).spacing(8)).height(Length::Fill),
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
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Trash").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Trash is empty").size(16).color(Text::SECONDARY),
            Space::with_height(8),
            text("Deleted photos will appear here")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    fn trash_item(
        item: &TrashedPhotoRecord,
        is_selected: bool,
        drive_path: Option<&Path>,
        confirm_delete_photo_id: Option<i64>,
    ) -> Element<'static, Message> {
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
                    container(
                        iced::widget::image(iced::widget::image::Handle::from_path(full))
                            .width(50)
                            .height(50)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .width(50)
                    .height(50)
                    .style(|_theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
                } else {
                    Self::thumb_placeholder()
                }
            } else {
                Self::thumb_placeholder()
            };

        let content = row![
            button(
                text(if is_selected { "+" } else { "" })
                    .size(12)
                    .color(if is_selected {
                        Accent::PRIMARY
                    } else {
                        Text::TERTIARY
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
                    .color(Text::PRIMARY),
                Space::with_height(4),
                row![
                    text(size).size(12).color(Text::SECONDARY),
                    Space::with_width(16),
                    text(format!("Deleted {}", trashed))
                        .size(12)
                        .color(Text::TERTIARY),
                ],
            ]
            .width(Length::Fill),
            button(text("Restore").size(12).color(Accent::PRIMARY))
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
                .color(iced::Color::from_rgb(0.8, 0.2, 0.2))
            )
            .padding(Padding::from([6, 12]))
            .on_press(if confirm_delete_photo_id == Some(photo_id) {
                Message::ConfirmPermanentlyDeletePhoto(photo_id)
            } else {
                Message::PermanentlyDeletePhoto(photo_id)
            }),
        ]
        .align_y(Alignment::Center);

        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(
                    if is_selected {
                        Backgrounds::SELECTED
                    } else {
                        Backgrounds::ELEVATED
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if is_selected {
                        Accent::PRIMARY
                    } else {
                        Border::SUBTLE
                    },
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn thumb_placeholder() -> Element<'static, Message> {
        container(text("IMG").size(10).color(Text::TERTIARY))
            .width(50)
            .height(50)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }
}
