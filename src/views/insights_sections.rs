//! Extracted Insights sections to keep insights.rs focused and smaller.

use iced::widget::{button, column, container, image as iced_image, row, text, Space};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::services::insights::InsightsData;
use crate::theme::colors;

fn format_number(n: i64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

pub fn top_people(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let border_card = p.border_subtle;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let bg_hover = p.bg_hover;

    let mut people_col = column![].spacing(8);

    for person in &data.top_people {
        let face: Element<'static, Message> = if let Some(ref path) = person.face_crop_path {
            container(
                iced_image(path.clone())
                    .content_fit(ContentFit::Cover)
                    .width(Length::Fixed(48.0))
                    .height(Length::Fixed(48.0)),
            )
            .width(Length::Fixed(48.0))
            .height(Length::Fixed(48.0))
            .clip(true)
            .style(move |_theme: &iced::Theme| container::Style {
                border: iced::Border {
                    radius: 24.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            container(Space::new(48, 48))
                .width(Length::Fixed(48.0))
                .height(Length::Fixed(48.0))
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg_hover.into()),
                    border: iced::Border {
                        radius: 24.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

        let name = person.name.clone();
        let count_str = format!("{} photos", person.photo_count);
        let cluster_id = person.cluster_id;

        let person_row = button(
            row![
                face,
                Space::with_width(12),
                column![
                    text(name).size(14).color(text_primary),
                    text(count_str).size(11).color(text_secondary),
                ]
                .spacing(2),
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([4, 8])),
        )
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => bg_hover,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectCluster(cluster_id));

        people_col = people_col.push(person_row);
    }

    container(people_col.padding(Padding::from([12, 12])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn top_locations(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let border_card = p.border_subtle;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let bg_hover = p.bg_hover;

    let mut loc_col = column![].spacing(6);

    for loc in &data.top_locations {
        let label = if loc.country.is_empty() {
            loc.city.clone()
        } else {
            format!("{}, {}", loc.city, loc.country)
        };
        let count_str = format!("{} photos", loc.photo_count);
        let city_clone = loc.city.clone();

        let loc_row = button(
            row![column![
                text(label).size(14).color(text_primary),
                text(count_str).size(11).color(text_secondary),
            ]
            .spacing(2),]
            .align_y(Alignment::Center)
            .padding(Padding::from([6, 12])),
        )
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => bg_hover,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::InsightsSearchCity(city_clone));

        loc_col = loc_col.push(loc_row);
    }

    container(loc_col.padding(Padding::from([12, 12])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn camera_breakdown(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let max_count = data
        .top_cameras
        .iter()
        .map(|c| c.photo_count)
        .max()
        .unwrap_or(1)
        .max(1);

    let accent = p.accent_primary;
    let bg_elevated = p.bg_elevated;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let border_card = p.border_subtle;

    let mut bars = column![].spacing(6);

    for cam in &data.top_cameras {
        let bar_width = (cam.photo_count as f32 / max_count as f32 * 260.0).max(4.0);

        let bar_color = accent;
        let bar = container(Space::new(Length::Fixed(bar_width), Length::Fixed(16.0))).style(
            move |_theme: &iced::Theme| container::Style {
                background: Some(bar_color.into()),
                border: iced::Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let bar_row = row![
            container(text(cam.camera.clone()).size(12).color(text_secondary),)
                .width(Length::Fixed(160.0)),
            bar,
            Space::with_width(8),
            text(format_number(cam.photo_count))
                .size(11)
                .color(text_tertiary),
        ]
        .align_y(Alignment::Center)
        .spacing(4);

        bars = bars.push(bar_row);
    }

    container(bars.padding(Padding::from([12, 16])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}
