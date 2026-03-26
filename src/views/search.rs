//! Search view.

use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, text_input, Column, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::services::search::SearchResultGroup;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Search view component.
pub struct SearchView;

impl SearchView {
    pub fn view(
        query: &str,
        suggestions: &[String],
        results: Option<&Vec<SearchResultGroup>>,
        is_loading: bool,
        drive_path: Option<&Path>,
        photos: &[Photo],
    ) -> Element<'static, Message> {
        let input = text_input("Search photos...", query)
            .on_input(Message::SearchInputChanged)
            .on_submit(Message::ExecuteSearch)
            .size(14)
            .padding(12)
            .width(Length::Fill);

        let search_bar = row![
            input,
            Space::with_width(12),
            button(text("Search").size(14).color(Backgrounds::PRIMARY))
                .padding(Padding::from([12, 20]))
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
                .on_press(Message::ExecuteSearch),
        ]
        .align_y(Alignment::Center);

        let suggestions_row: Element<'static, Message> =
            if !query.is_empty() && !suggestions.is_empty() {
                let chips: Vec<Element<'static, Message>> = suggestions
                    .iter()
                    .take(5)
                    .map(|s| {
                        let value = s.clone();
                        button(text(s.clone()).size(12).color(Text::PRIMARY))
                            .padding(Padding::from([4, 10]))
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
                                        radius: 12.0.into(),
                                    },
                                    ..Default::default()
                                }
                            })
                            .on_press(Message::SearchSuggestionSelected(value))
                            .into()
                    })
                    .collect();

                row(chips).spacing(8).into()
            } else {
                Space::with_height(Length::Shrink).into()
            };

        let body: Element<'static, Message> = if is_loading {
            container(text("Searching...").size(14).color(Text::SECONDARY))
                .width(Length::Fill)
                .padding(24)
                .into()
        } else if let Some(groups) = results {
            if groups.is_empty() {
                container(
                    column![
                        text("No results found").size(16).color(Text::SECONDARY),
                        Space::with_height(8),
                        text("Try another query").size(13).color(Text::TERTIARY),
                    ]
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .padding(24)
                .into()
            } else {
                let total: usize = groups.iter().map(|g| g.results.len()).sum();
                let group_views: Vec<Element<'static, Message>> = groups
                    .iter()
                    .map(|g| Self::render_result_group(g, drive_path, photos))
                    .collect();

                column![
                    row![
                        text(format!("{} photos found", total))
                            .size(14)
                            .color(Text::SECONDARY),
                        Space::with_width(Length::Fill),
                        button(text("Quick Cull").size(13).color(Text::PRIMARY))
                            .padding(Padding::from([8, 14]))
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
                            .on_press(Message::EnterCullFromSearch),
                    ],
                    Space::with_height(12),
                    scrollable(Column::with_children(group_views).spacing(20)).height(Length::Fill),
                ]
                .into()
            }
        } else {
            column![
                text("Search Examples").size(16).color(Text::PRIMARY),
                Space::with_height(12),
                Self::search_tip("Dad", "Find by person"),
                Self::search_tip("Tokyo", "Find by location"),
                Self::search_tip("March 2019", "Find by date"),
                Self::search_tip("Dad in Tokyo", "Combine filters"),
                Self::search_tip("last summer", "Natural date query"),
            ]
            .spacing(8)
            .into()
        };

        let content = column![
            text("Search").size(22).color(Text::PRIMARY),
            Space::with_height(20),
            search_bar,
            Space::with_height(10),
            suggestions_row,
            Space::with_height(14),
            body,
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

    fn search_tip(example: &str, description: &str) -> Element<'static, Message> {
        let example = example.to_string();
        let description = description.to_string();
        row![
            container(text(example).size(12).color(Text::PRIMARY))
                .padding(Padding::from([4, 8]))
                .style(|_theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            Space::with_width(10),
            text(description).size(12).color(Text::TERTIARY),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    fn render_result_group(
        group: &SearchResultGroup,
        drive_path: Option<&Path>,
        photos: &[Photo],
    ) -> Element<'static, Message> {
        let header = row![
            text(group.date.clone()).size(14).color(Text::PRIMARY),
            Space::with_width(10),
            if let Some(loc) = &group.location {
                text(loc.clone()).size(12).color(Text::TERTIARY)
            } else {
                text("").size(12)
            },
            Space::with_width(Length::Fill),
            text(format!("{} photos", group.results.len()))
                .size(12)
                .color(Text::SECONDARY),
        ]
        .align_y(Alignment::Center);

        let mut items: Vec<Element<'static, Message>> = Vec::new();
        for r in group.results.iter().take(16) {
            let thumb: Element<'static, Message> = if let (Some(root), Some(photo)) =
                (drive_path, photos.iter().find(|p| p.id == r.photo_id))
            {
                if let Some(ref t) = photo.thumbnail_path {
                    let p = std::path::PathBuf::from(t);
                    if p.exists() {
                        container(
                            iced::widget::image(iced::widget::image::Handle::from_path(p))
                                .width(56)
                                .height(56)
                                .content_fit(iced::ContentFit::Cover),
                        )
                        .width(56)
                        .height(56)
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
                                    .width(56)
                                    .height(56)
                                    .content_fit(iced::ContentFit::Cover),
                            )
                            .width(56)
                            .height(56)
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
                    }
                } else {
                    Self::thumb_placeholder()
                }
            } else {
                Self::thumb_placeholder()
            };

            items.push(
                button(thumb)
                    .padding(0)
                    .style(|_theme, _status| button::Style::default())
                    .on_press(Message::SelectPhoto(r.photo_id))
                    .into(),
            );
        }

        column![header, Space::with_height(10), row(items).spacing(8)]
            .spacing(2)
            .into()
    }

    fn thumb_placeholder() -> Element<'static, Message> {
        container(text("IMG").size(10).color(Text::TERTIARY))
            .width(56)
            .height(56)
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
