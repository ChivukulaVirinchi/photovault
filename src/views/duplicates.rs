//! Duplicates review view

use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::{DuplicateGroupMemberRecord, DuplicateGroupRecord};
use crate::models::Photo;
use crate::theme::colors;
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
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);

        if is_loading {
            return Self::loading_view(theme);
        }

        if groups.is_empty() {
            return Self::empty_view(theme);
        }

        let title = text("Duplicates").size(28).color(p.text_primary);

        let subtitle = text(format!(
            "{} duplicate groups found \u{2014} {} wasted",
            groups.len(),
            format_bytes(wasted_space)
        ))
        .size(14)
        .color(p.text_secondary);

        // Group list
        let group_list: Vec<Element<'static, Message>> = groups
            .iter()
            .map(|g| Self::group_row(g, drive_path, photos, overview, theme))
            .collect();

        let bg_primary = p.bg_primary;
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
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into()
    }

    fn loading_view(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let content = column![
            text("Duplicates").size(28).color(p.text_primary),
            Space::with_height(16),
            text("Detecting duplicates...")
                .size(16)
                .color(p.text_secondary),
            Space::with_height(8),
            text("Analyzing file hashes and building groups.")
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

    /// Empty state when no duplicates
    fn empty_view(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let content = column![
            text("Duplicates").size(28).color(p.text_primary),
            Space::with_height(16),
            text("No duplicates found!").size(16).color(p.text_secondary),
            Space::with_height(8),
            text("Your photo library has no exact duplicate files.")
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

    /// Render a single duplicate group row
    fn group_row(
        group: &DuplicateGroupRecord,
        drive_path: Option<&Path>,
        photos: &[Photo],
        overview: &[(i64, u64, Option<i64>)],
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let group_id = group.id;

        let (recoverable, preview_photo) = overview
            .iter()
            .find(|(id, _, _)| *id == group.id)
            .map(|(_, bytes, pid)| (*bytes, *pid))
            .unwrap_or((0, None));

        let preview = Self::group_preview(preview_photo, drive_path, photos, theme);

        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;
        let text_tertiary = p.text_tertiary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_hover = p.bg_hover;
        let bg_elevated = p.bg_elevated;
        let border_subtle = p.border_subtle;

        let header = row![
            text(format!("Group #{}", group.id))
                .size(14)
                .color(text_primary),
            Space::with_width(Length::Fill),
            text(format!(
                "{} identical files · {} recoverable",
                group.member_count,
                format_bytes(recoverable)
            ))
            .size(12)
            .color(text_secondary),
        ]
        .align_y(Alignment::Center);

        let actions = row![
            button(text("Keep Suggested").size(12).color(text_primary))
                .padding(Padding::from([6, 12]))
                .style(move |_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(accent_primary.into()),
                        _ => Some(accent_muted.into()),
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
            button(text("Review").size(12).color(text_primary))
                .padding(Padding::from([6, 12]))
                .style(move |_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(bg_hover.into()),
                        _ => Some(bg_elevated.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: border_subtle,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::OpenDuplicateGroup(group_id)),
            Space::with_width(8),
            button(text("Dismiss").size(12).color(text_tertiary))
                .padding(Padding::from([6, 12]))
                .style(move |_theme, status| {
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
            .style(move |_theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    color: border_subtle,
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
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_elevated = p.bg_elevated;
        let text_tertiary = p.text_tertiary;

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
                            Self::preview_placeholder(theme)
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
                        Self::preview_placeholder(theme)
                    }
                }
            } else {
                Self::preview_placeholder(theme)
            }
        } else {
            Self::preview_placeholder(theme)
        };

        container(row![
            image_box,
            Space::with_width(12),
            text("Representative photo").size(12).color(text_tertiary)
        ])
        .width(Length::Fill)
        .padding(Padding::from([4, 0]))
        .into()
    }

    fn preview_placeholder(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_elevated = p.bg_elevated;
        container(text("IMG").size(12).color(p.text_tertiary))
            .width(80)
            .height(80)
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

    /// Render detailed view of a duplicate group
    pub fn group_detail_view(
        group: &DuplicateGroupRecord,
        members: &[DuplicateGroupMemberRecord],
        drive_path: &Path,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let group_id = group.id;

        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_primary = p.bg_primary;

        let header = row![
            button(text("<").size(16).color(text_primary))
                .padding(8)
                .style(|_theme, _status| button::Style::default())
                .on_press(Message::CloseDuplicateDetail),
            Space::with_width(16),
            text(format!("Duplicate Group #{}", group.id))
                .size(20)
                .color(text_primary),
            Space::with_width(Length::Fill),
            text(format!("{} files", members.len()))
                .size(14)
                .color(text_secondary),
        ]
        .align_y(Alignment::Center);

        // Member list
        let member_list: Vec<Element<'static, Message>> = members
            .iter()
            .map(|m| Self::member_row(group_id, m, drive_path, theme))
            .collect();

        let content = column![
            header,
            Space::with_height(24),
            scrollable(Column::with_children(member_list).spacing(8)),
            Space::with_height(16),
            row![
                button(text("Trash Non-Suggested").size(14).color(text_primary))
                    .padding(Padding::from([10, 20]))
                    .style(move |_theme, status| {
                        let background = match status {
                            button::Status::Hovered => Some(accent_primary.into()),
                            _ => Some(accent_muted.into()),
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
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single member in the detail view
    fn member_row(
        group_id: i64,
        member: &DuplicateGroupMemberRecord,
        drive_path: &Path,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let photo_id = member.photo_id;
        let is_keep = member.is_suggested_keep;

        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;
        let text_tertiary = p.text_tertiary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_hover = p.bg_hover;
        let bg_elevated = p.bg_elevated;
        let bg_selected = p.bg_selected;
        let border_subtle = p.border_subtle;

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
            text("KEEP").size(10).color(accent_primary)
        } else {
            text("").size(10)
        };

        let thumb = Self::thumbnail_button(member, drive_path, theme);

        let content = row![
            thumb,
            Space::with_width(16),
            column![
                text(path).size(13).color(text_primary),
                Space::with_height(4),
                row![
                    text(size).size(12).color(text_secondary),
                    Space::with_width(16),
                    text(date).size(12).color(text_tertiary),
                ],
            ]
            .width(Length::Fill),
            keep_indicator,
            Space::with_width(16),
            button(
                text(if is_keep { "Keeping" } else { "Keep This" })
                    .size(12)
                    .color(if is_keep {
                        accent_primary
                    } else {
                        text_primary
                    })
            )
            .padding(Padding::from([6, 12]))
            .style(move |_theme, status| {
                let background = if is_keep {
                    Some(accent_muted.into())
                } else {
                    match status {
                        button::Status::Hovered => Some(bg_hover.into()),
                        _ => Some(bg_elevated.into()),
                    }
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: if is_keep {
                            accent_primary
                        } else {
                            border_subtle
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
                        bg_selected
                    } else {
                        bg_elevated
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if is_keep {
                        accent_primary
                    } else {
                        border_subtle
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
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let photo_id = member.photo_id;
        let bg_elevated = p.bg_elevated;
        let border_subtle = p.border_subtle;
        let border_visible = p.border_visible;

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
                    Self::thumbnail_placeholder(theme)
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
                    Self::thumbnail_placeholder(theme)
                }
            } else {
                Self::thumbnail_placeholder(theme)
            };

        button(image_container)
            .padding(0)
            .style(move |_theme, status| {
                let border_color = match status {
                    button::Status::Hovered => border_visible,
                    _ => border_subtle,
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

    fn thumbnail_placeholder(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_elevated = p.bg_elevated;
        container(text("IMG").size(12).color(p.text_tertiary))
            .width(60)
            .height(60)
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
