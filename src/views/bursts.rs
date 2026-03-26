//! Bursts review view

use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::{BurstGroupMemberRecord, BurstGroupRecord};
use crate::models::Photo;
use crate::theme::colors;

/// Bursts view
pub struct BurstsView;

impl BurstsView {
    /// Render the bursts overview
    pub fn view(
        groups: &[BurstGroupRecord],
        total_saveable: usize,
        is_loading: bool,
        drive_path: Option<&Path>,
        photos: &[Photo],
        previews: &[(i64, Vec<i64>)],
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);

        if is_loading {
            return Self::loading_view(theme);
        }

        if groups.is_empty() {
            return Self::empty_view(theme);
        }

        let title = text("Burst Photos").size(28).color(p.text_primary);

        let subtitle = text(format!(
            "{} bursts found \u{2014} {} photos could be removed",
            groups.len(),
            total_saveable
        ))
        .size(14)
        .color(p.text_secondary);

        // Group list
        let group_list: Vec<Element<'static, Message>> = groups
            .iter()
            .map(|g| Self::group_card(g, drive_path, photos, previews, theme))
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
            text("Burst Photos").size(28).color(p.text_primary),
            Space::with_height(16),
            text("Detecting bursts...").size(16).color(p.text_secondary),
            Space::with_height(8),
            text("Grouping photos taken within 3 seconds and scoring quality.")
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

    /// Empty state
    fn empty_view(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let content = column![
            text("Burst Photos").size(28).color(p.text_primary),
            Space::with_height(16),
            text("No burst photos found!")
                .size(16)
                .color(p.text_secondary),
            Space::with_height(8),
            text("Bursts are photos taken within 3 seconds of each other.")
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

    /// Render a burst group card
    fn group_card(
        group: &BurstGroupRecord,
        drive_path: Option<&Path>,
        photos: &[Photo],
        previews: &[(i64, Vec<i64>)],
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let group_id = group.id;

        let text_primary = p.text_primary;
        let text_tertiary = p.text_tertiary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_primary = p.bg_primary;
        let bg_hover = p.bg_hover;
        let bg_elevated = p.bg_elevated;
        let border_subtle = p.border_subtle;

        // Parse and format time range safely
        let start_display = if group.start_time.len() >= 19 {
            &group.start_time[..19]
        } else {
            &group.start_time
        };
        let end_display = if group.end_time.len() >= 19 && group.end_time.len() > 11 {
            &group.end_time[11..19.min(group.end_time.len())]
        } else {
            &group.end_time
        };
        let time_range = format!("{} - {}", start_display, end_display);

        let header = row![
            text(format!("{} photos", group.photo_count))
                .size(16)
                .color(text_primary),
            Space::with_width(Length::Fill),
            text(time_range).size(12).color(text_tertiary),
        ]
        .align_y(Alignment::Center);

        let actions = row![
            button(text("Keep Best").size(12).color(text_primary))
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
                .on_press(Message::KeepBestFromBurst(group_id)),
            Space::with_width(8),
            button(text("Review All").size(12).color(text_primary))
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
                .on_press(Message::OpenBurstGroup(group_id)),
            Space::with_width(8),
            button(text("Keep All").size(12).color(text_tertiary))
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
                .on_press(Message::DismissBurstGroup(group_id)),
        ];

        let ids = previews
            .iter()
            .find(|(gid, _)| *gid == group.id)
            .map(|(_, ids)| ids.clone())
            .unwrap_or_default();

        let strip_items: Vec<Element<'static, Message>> = if let Some(root) = drive_path {
            ids.iter()
                .map(|pid| Self::burst_preview_thumb(*pid, root, photos, theme))
                .collect()
        } else {
            vec![text("No previews").size(11).color(text_tertiary).into()]
        };

        let thumbnail_strip = container(Row::with_children(strip_items).spacing(6))
            .width(Length::Fill)
            .height(60)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        let content = column![
            header,
            Space::with_height(12),
            thumbnail_strip,
            Space::with_height(12),
            actions,
        ];

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

    fn burst_preview_thumb(
        photo_id: i64,
        drive_path: &Path,
        photos: &[Photo],
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let pal = colors::palette(theme);
        let bg_elevated = pal.bg_elevated;
        let text_tertiary = pal.text_tertiary;

        if let Some(photo) = photos.iter().find(|p| p.id == photo_id) {
            if let Some(ref thumb) = photo.thumbnail_path {
                let p = std::path::PathBuf::from(thumb);
                if p.exists() {
                    return container(
                        iced::widget::image(iced::widget::image::Handle::from_path(p))
                            .width(48)
                            .height(48)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .width(48)
                    .height(48)
                    .style(move |_theme| container::Style {
                        background: Some(bg_elevated.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into();
                }
            }

            let full = drive_path.join(&photo.file_path);
            if full.exists() {
                return container(
                    iced::widget::image(iced::widget::image::Handle::from_path(full))
                        .width(48)
                        .height(48)
                        .content_fit(iced::ContentFit::Cover),
                )
                .width(48)
                .height(48)
                .style(move |_theme| container::Style {
                    background: Some(bg_elevated.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into();
            }
        }

        container(text("IMG").size(10).color(text_tertiary))
            .width(48)
            .height(48)
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

    /// Render detailed burst review
    pub fn group_detail_view(
        group: &BurstGroupRecord,
        members: &[BurstGroupMemberRecord],
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let group_id = group.id;

        let text_primary = p.text_primary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_primary = p.bg_primary;

        let header = row![
            button(text("<").size(16).color(text_primary))
                .padding(8)
                .style(|_theme, _status| button::Style::default())
                .on_press(Message::CloseBurstDetail),
            Space::with_width(16),
            text(format!("Burst \u{2014} {} photos", members.len()))
                .size(20)
                .color(text_primary),
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        // Member grid with quality scores — build rows of 4 directly
        let mut rows: Vec<Element<'static, Message>> = Vec::new();
        let mut current_row: Vec<Element<'static, Message>> = Vec::new();

        for m in members {
            current_row.push(Self::member_card(group_id, m, theme));
            if current_row.len() == 4 {
                let row_items: Vec<Element<'static, Message>> = current_row.drain(..).collect();
                rows.push(Row::with_children(row_items).spacing(12).into());
            }
        }

        // Push any remaining items
        if !current_row.is_empty() {
            let row_items: Vec<Element<'static, Message>> = current_row.drain(..).collect();
            rows.push(Row::with_children(row_items).spacing(12).into());
        }

        let content = column![
            header,
            Space::with_height(24),
            scrollable(Column::with_children(rows).spacing(12)),
            Space::with_height(16),
            row![
                button(text("Keep Only Selected").size(14).color(text_primary))
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
                    .on_press(Message::TrashNonBestFromBurst(group_id)),
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

    /// Render a single burst member card
    fn member_card(
        group_id: i64,
        member: &BurstGroupMemberRecord,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let photo_id = member.photo_id;
        let is_best = member.is_suggested_best;

        let text_primary = p.text_primary;
        let text_tertiary = p.text_tertiary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_primary = p.bg_primary;
        let bg_hover = p.bg_hover;
        let bg_elevated = p.bg_elevated;
        let bg_selected = p.bg_selected;
        let border_subtle = p.border_subtle;

        let sharpness = member.sharpness_score.unwrap_or(0.0);
        let blur = member.blur_score.unwrap_or(0.0);

        // Quality bar
        let quality = (sharpness * 0.5 + blur * 0.5) * 100.0;

        let quality_indicator = container(Space::new(Length::Fixed(quality), Length::Fixed(3.0)))
            .width(Length::Fixed(100.0))
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            });

        let best_badge = if is_best {
            container(text("BEST").size(9).color(bg_primary))
                .padding(Padding::from([2, 6]))
                .style(move |_theme| container::Style {
                    background: Some(accent_primary.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
        } else {
            container(Space::new(Length::Shrink, Length::Shrink))
        };

        let content = column![
            // Image placeholder
            container(
                column![best_badge, Space::with_height(Length::Shrink),]
                    .width(Length::Fill)
                    .padding(4)
            )
            .width(Length::Fixed(140.0))
            .height(Length::Fixed(100.0))
            .style(move |_theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            Space::with_height(8),
            // Quality bar
            column![
                text(format!("Quality: {:.0}%", quality))
                    .size(10)
                    .color(text_tertiary),
                Space::with_height(2),
                container(quality_indicator).style(move |_theme| container::Style {
                    background: Some(bg_elevated.into()),
                    ..Default::default()
                }),
            ],
            Space::with_height(8),
            // Select button
            button(
                text(if is_best { "Selected" } else { "Select" })
                    .size(11)
                    .color(if is_best {
                        accent_primary
                    } else {
                        text_primary
                    })
            )
            .padding(Padding::from([4, 8]))
            .width(Length::Fill)
            .style(move |_theme, status| {
                let background = if is_best {
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
                        color: if is_best {
                            accent_primary
                        } else {
                            border_subtle
                        },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetBestFromBurst(group_id, photo_id)),
        ]
        .width(Length::Fixed(140.0));

        container(content)
            .padding(8)
            .style(move |_theme| container::Style {
                background: Some(
                    if is_best {
                        bg_selected
                    } else {
                        bg_elevated
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if is_best {
                        accent_primary
                    } else {
                        border_subtle
                    },
                    width: if is_best { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
