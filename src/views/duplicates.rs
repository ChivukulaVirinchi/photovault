//! Duplicates review view

use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::{DuplicateGroupMemberRecord, DuplicateGroupRecord};
use crate::models::Photo;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};
use crate::utils::format_bytes;

/// Duplicates view state
pub struct DuplicatesView;

impl DuplicatesView {
    /// Render the duplicates overview
    pub fn view(
        groups: &[DuplicateGroupRecord],
        wasted_space: u64,
        is_loading: bool,
        drive_path: Option<&Path>,
        photos: &[Photo],
        overview: &[(i64, u64, Option<i64>)],
    ) -> Element<'static, Message> {
        if is_loading {
            return Self::loading_view();
        }

        if groups.is_empty() {
            return Self::empty_view();
        }

        let title = text("Duplicates").size(28).color(Text::PRIMARY);

        let subtitle = text(format!(
            "{} duplicate groups found \u{2014} {} wasted",
            groups.len(),
            format_bytes(wasted_space)
        ))
        .size(14)
        .color(Text::SECONDARY);

        // Group list
        let group_list: Vec<Element<'static, Message>> = groups
            .iter()
            .map(|g| Self::group_row(g, drive_path, photos, overview))
            .collect();

        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(24),
            scrollable(Column::with_children(group_list).spacing(12)),
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

    fn loading_view() -> Element<'static, Message> {
        let content = column![
            text("Duplicates").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Detecting duplicates...")
                .size(16)
                .color(Text::SECONDARY),
            Space::with_height(8),
            text("Analyzing file hashes and building groups.")
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

    /// Empty state when no duplicates
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Duplicates").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("No duplicates found!").size(16).color(Text::SECONDARY),
            Space::with_height(8),
            text("Your photo library has no exact duplicate files.")
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

    /// Render a single duplicate group row
    fn group_row(
        group: &DuplicateGroupRecord,
        drive_path: Option<&Path>,
        photos: &[Photo],
        overview: &[(i64, u64, Option<i64>)],
    ) -> Element<'static, Message> {
        let group_id = group.id;

        let (recoverable, preview_photo) = overview
            .iter()
            .find(|(id, _, _)| *id == group.id)
            .map(|(_, bytes, pid)| (*bytes, *pid))
            .unwrap_or((0, None));

        let preview = Self::group_preview(preview_photo, drive_path, photos);

        let header = row![
            text(format!("Group #{}", group.id))
                .size(14)
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            text(format!(
                "{} identical files · {} recoverable",
                group.member_count,
                format_bytes(recoverable)
            ))
            .size(12)
            .color(Text::SECONDARY),
        ]
        .align_y(Alignment::Center);

        let actions = row![
            button(text("Keep Suggested").size(12).color(Text::PRIMARY))
                .padding(Padding::from([6, 12]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::PRIMARY.into()),
                        _ => Some(Accent::MUTED.into()),
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
                .on_press(Message::KeepSuggestedDuplicate(group_id)),
            Space::with_width(8),
            button(text("Review").size(12).color(Text::PRIMARY))
                .padding(Padding::from([6, 12]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: Border::SUBTLE,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::OpenDuplicateGroup(group_id)),
            Space::with_width(8),
            button(text("Dismiss").size(12).color(Text::TERTIARY))
                .padding(Padding::from([6, 12]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => None,
                    };
                    button::Style {
                        background,
                        border: iced::Border::default(),
                        ..Default::default()
                    }
                })
                .on_press(Message::DismissDuplicateGroup(group_id)),
        ];

        let content = column![
            header,
            Space::with_height(10),
            preview,
            Space::with_height(10),
            actions,
        ]
        .spacing(4);

        container(content)
            .padding(16)
            .width(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn group_preview(
        preview_photo_id: Option<i64>,
        drive_path: Option<&Path>,
        photos: &[Photo],
    ) -> Element<'static, Message> {
        let image_box: Element<'static, Message> = if let (Some(pid), Some(root)) =
            (preview_photo_id, drive_path)
        {
            if let Some(photo) = photos.iter().find(|p| p.id == pid) {
                if let Some(ref thumb_path) = photo.thumbnail_path {
                    let path = std::path::PathBuf::from(thumb_path);
                    if path.exists() {
                        container(
                            iced::widget::image(iced::widget::image::Handle::from_path(path))
                                .width(80)
                                .height(80)
                                .content_fit(iced::ContentFit::Cover),
                        )
                        .width(80)
                        .height(80)
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
                        let full = root.join(&photo.file_path);
                        if full.exists() {
                            container(
                                iced::widget::image(iced::widget::image::Handle::from_path(full))
                                    .width(80)
                                    .height(80)
                                    .content_fit(iced::ContentFit::Cover),
                            )
                            .width(80)
                            .height(80)
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
                            Self::preview_placeholder()
                        }
                    }
                } else {
                    let full = root.join(&photo.file_path);
                    if full.exists() {
                        container(
                            iced::widget::image(iced::widget::image::Handle::from_path(full))
                                .width(80)
                                .height(80)
                                .content_fit(iced::ContentFit::Cover),
                        )
                        .width(80)
                        .height(80)
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
                        Self::preview_placeholder()
                    }
                }
            } else {
                Self::preview_placeholder()
            }
        } else {
            Self::preview_placeholder()
        };

        container(row![
            image_box,
            Space::with_width(12),
            text("Representative photo").size(12).color(Text::TERTIARY)
        ])
        .width(Length::Fill)
        .padding(Padding::from([4, 0]))
        .into()
    }

    fn preview_placeholder() -> Element<'static, Message> {
        container(text("IMG").size(12).color(Text::TERTIARY))
            .width(80)
            .height(80)
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

    /// Render detailed view of a duplicate group
    pub fn group_detail_view(
        group: &DuplicateGroupRecord,
        members: &[DuplicateGroupMemberRecord],
        drive_path: &Path,
    ) -> Element<'static, Message> {
        let group_id = group.id;

        let header = row![
            button(text("<").size(16).color(Text::PRIMARY))
                .padding(8)
                .style(|_theme, _status| button::Style::default())
                .on_press(Message::CloseDuplicateDetail),
            Space::with_width(16),
            text(format!("Duplicate Group #{}", group.id))
                .size(20)
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            text(format!("{} files", members.len()))
                .size(14)
                .color(Text::SECONDARY),
        ]
        .align_y(Alignment::Center);

        // Member list
        let member_list: Vec<Element<'static, Message>> = members
            .iter()
            .map(|m| Self::member_row(group_id, m, drive_path))
            .collect();

        let content = column![
            header,
            Space::with_height(24),
            scrollable(Column::with_children(member_list).spacing(8)),
            Space::with_height(16),
            row![
                button(text("Trash Non-Suggested").size(14).color(Text::PRIMARY))
                    .padding(Padding::from([10, 20]))
                    .style(|_theme, status| {
                        let background = match status {
                            button::Status::Hovered => Some(Accent::PRIMARY.into()),
                            _ => Some(Accent::MUTED.into()),
                        };
                        button::Style {
                            background,
                            border: iced::Border {
                                radius: 8.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    })
                    .on_press(Message::TrashNonSuggestedDuplicates(group_id)),
            ],
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

    /// Render a single member in the detail view
    fn member_row(
        group_id: i64,
        member: &DuplicateGroupMemberRecord,
        drive_path: &Path,
    ) -> Element<'static, Message> {
        let photo_id = member.photo_id;
        let is_keep = member.is_suggested_keep;

        let path = member
            .file_path
            .as_deref()
            .unwrap_or("Unknown path")
            .to_string();
        let size = member
            .file_size
            .map(|s| format_bytes(s as u64))
            .unwrap_or_default();
        let date = member
            .date_taken
            .as_deref()
            .unwrap_or("Unknown date")
            .to_string();

        let keep_indicator = if is_keep {
            text("KEEP").size(10).color(Accent::PRIMARY)
        } else {
            text("").size(10)
        };

        let thumb = Self::thumbnail_button(member, drive_path);

        let content = row![
            thumb,
            Space::with_width(16),
            column![
                text(path).size(13).color(Text::PRIMARY),
                Space::with_height(4),
                row![
                    text(size).size(12).color(Text::SECONDARY),
                    Space::with_width(16),
                    text(date).size(12).color(Text::TERTIARY),
                ],
            ]
            .width(Length::Fill),
            keep_indicator,
            Space::with_width(16),
            button(
                text(if is_keep { "Keeping" } else { "Keep This" })
                    .size(12)
                    .color(if is_keep {
                        Accent::PRIMARY
                    } else {
                        Text::PRIMARY
                    })
            )
            .padding(Padding::from([6, 12]))
            .style(move |_theme, status| {
                let background = if is_keep {
                    Some(Accent::MUTED.into())
                } else {
                    match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    }
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: if is_keep {
                            Accent::PRIMARY
                        } else {
                            Border::SUBTLE
                        },
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetKeepDuplicate(group_id, photo_id)),
        ]
        .align_y(Alignment::Center);

        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(
                    if is_keep {
                        Backgrounds::SELECTED
                    } else {
                        Backgrounds::ELEVATED
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if is_keep {
                        Accent::PRIMARY
                    } else {
                        Border::SUBTLE
                    },
                    width: if is_keep { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn thumbnail_button(
        member: &DuplicateGroupMemberRecord,
        drive_path: &Path,
    ) -> Element<'static, Message> {
        let photo_id = member.photo_id;

        let image_container: Element<'static, Message> =
            if let Some(ref thumb_path) = member.thumbnail_path {
                let thumb_full = drive_path.join(thumb_path);
                if thumb_full.exists() {
                    container(
                        iced::widget::image(iced::widget::image::Handle::from_path(thumb_full))
                            .width(60)
                            .height(60)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .width(60)
                    .height(60)
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
                    Self::thumbnail_placeholder()
                }
            } else if let Some(ref file_path) = member.file_path {
                let full_path = drive_path.join(file_path);
                if full_path.exists() {
                    container(
                        iced::widget::image(iced::widget::image::Handle::from_path(full_path))
                            .width(60)
                            .height(60)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .width(60)
                    .height(60)
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
                    Self::thumbnail_placeholder()
                }
            } else {
                Self::thumbnail_placeholder()
            };

        button(image_container)
            .padding(0)
            .style(|_theme, status| {
                let border_color = match status {
                    button::Status::Hovered => Border::VISIBLE,
                    _ => Border::SUBTLE,
                };
                button::Style {
                    background: None,
                    border: iced::Border {
                        color: border_color,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SelectPhoto(photo_id))
            .into()
    }

    fn thumbnail_placeholder() -> Element<'static, Message> {
        container(text("IMG").size(12).color(Text::TERTIARY))
            .width(60)
            .height(60)
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
