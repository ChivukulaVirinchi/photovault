//! Bursts review view

use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::{BurstGroupMemberRecord, BurstGroupRecord};
use crate::models::Photo;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

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
    ) -> Element<'static, Message> {
        if is_loading {
            return Self::loading_view();
        }

        if groups.is_empty() {
            return Self::empty_view();
        }

        let title = text("Burst Photos").size(28).color(Text::PRIMARY);

        let subtitle = text(format!(
            "{} bursts found \u{2014} {} photos could be removed",
            groups.len(),
            total_saveable
        ))
        .size(14)
        .color(Text::SECONDARY);

        // Group list
        let group_list: Vec<Element<'static, Message>> = groups
            .iter()
            .map(|g| Self::group_card(g, drive_path, photos, previews))
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
            text("Burst Photos").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Detecting bursts...").size(16).color(Text::SECONDARY),
            Space::with_height(8),
            text("Grouping photos taken within 3 seconds and scoring quality.")
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

    /// Empty state
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Burst Photos").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("No burst photos found!")
                .size(16)
                .color(Text::SECONDARY),
            Space::with_height(8),
            text("Bursts are photos taken within 3 seconds of each other.")
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

    /// Render a burst group card
    fn group_card(
        group: &BurstGroupRecord,
        drive_path: Option<&Path>,
        photos: &[Photo],
        previews: &[(i64, Vec<i64>)],
    ) -> Element<'static, Message> {
        let group_id = group.id;

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
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            text(time_range).size(12).color(Text::TERTIARY),
        ]
        .align_y(Alignment::Center);

        let actions = row![
            button(text("Keep Best").size(12).color(Text::PRIMARY))
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
                .on_press(Message::KeepBestFromBurst(group_id)),
            Space::with_width(8),
            button(text("Review All").size(12).color(Text::PRIMARY))
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
                .on_press(Message::OpenBurstGroup(group_id)),
            Space::with_width(8),
            button(text("Keep All").size(12).color(Text::TERTIARY))
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
                .on_press(Message::DismissBurstGroup(group_id)),
        ];

        let ids = previews
            .iter()
            .find(|(gid, _)| *gid == group.id)
            .map(|(_, ids)| ids.clone())
            .unwrap_or_default();

        let strip_items: Vec<Element<'static, Message>> = if let Some(root) = drive_path {
            ids.iter()
                .map(|pid| Self::burst_preview_thumb(*pid, root, photos))
                .collect()
        } else {
            vec![text("No previews").size(11).color(Text::TERTIARY).into()]
        };

        let thumbnail_strip = container(Row::with_children(strip_items).spacing(6))
            .width(Length::Fill)
            .height(60)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
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

    fn burst_preview_thumb(
        photo_id: i64,
        drive_path: &Path,
        photos: &[Photo],
    ) -> Element<'static, Message> {
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
                    .style(|_theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
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
                .style(|_theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into();
            }
        }

        container(text("IMG").size(10).color(Text::TERTIARY))
            .width(48)
            .height(48)
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

    /// Render detailed burst review
    pub fn group_detail_view(
        group: &BurstGroupRecord,
        members: &[BurstGroupMemberRecord],
    ) -> Element<'static, Message> {
        let group_id = group.id;

        let header = row![
            button(text("<").size(16).color(Text::PRIMARY))
                .padding(8)
                .style(|_theme, _status| button::Style::default())
                .on_press(Message::CloseBurstDetail),
            Space::with_width(16),
            text(format!("Burst \u{2014} {} photos", members.len()))
                .size(20)
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        // Member grid with quality scores — build rows of 4 directly
        let mut rows: Vec<Element<'static, Message>> = Vec::new();
        let mut current_row: Vec<Element<'static, Message>> = Vec::new();

        for m in members {
            current_row.push(Self::member_card(group_id, m));
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
                button(text("Keep Only Selected").size(14).color(Text::PRIMARY))
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
                    .on_press(Message::TrashNonBestFromBurst(group_id)),
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

    /// Render a single burst member card
    fn member_card(group_id: i64, member: &BurstGroupMemberRecord) -> Element<'static, Message> {
        let photo_id = member.photo_id;
        let is_best = member.is_suggested_best;

        let sharpness = member.sharpness_score.unwrap_or(0.0);
        let blur = member.blur_score.unwrap_or(0.0);

        // Quality bar
        let quality = (sharpness * 0.5 + blur * 0.5) * 100.0;

        let quality_indicator = container(Space::new(Length::Fixed(quality), Length::Fixed(3.0)))
            .width(Length::Fixed(100.0))
            .style(move |_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            });

        let best_badge = if is_best {
            container(text("BEST").size(9).color(Backgrounds::PRIMARY))
                .padding(Padding::from([2, 6]))
                .style(|_theme| container::Style {
                    background: Some(Accent::PRIMARY.into()),
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
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
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
                    .color(Text::TERTIARY),
                Space::with_height(2),
                container(quality_indicator).style(|_theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    ..Default::default()
                }),
            ],
            Space::with_height(8),
            // Select button
            button(
                text(if is_best { "Selected" } else { "Select" })
                    .size(11)
                    .color(if is_best {
                        Accent::PRIMARY
                    } else {
                        Text::PRIMARY
                    })
            )
            .padding(Padding::from([4, 8]))
            .width(Length::Fill)
            .style(move |_theme, status| {
                let background = if is_best {
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
                        color: if is_best {
                            Accent::PRIMARY
                        } else {
                            Border::SUBTLE
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
                        Backgrounds::SELECTED
                    } else {
                        Backgrounds::ELEVATED
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if is_best {
                        Accent::PRIMARY
                    } else {
                        Border::SUBTLE
                    },
                    width: if is_best { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
