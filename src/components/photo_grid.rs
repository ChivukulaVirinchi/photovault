//! Photo grid component
//!
//! Renders photos in a responsive grid layout with day headers.
//! Design: gallery-style cards with subtle rounded corners and clean hover.

use iced::widget::{button, column, container, text, Column, Row, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::theme::colors::{Accent, Backgrounds, Text as TextColors};

/// Render a photo grid using standard iced widgets
pub fn photo_grid_simple(
    photos: &[Photo],
    thumbnail_size: f32,
    columns: usize,
) -> Element<'static, Message> {
    let mut rows: Vec<Element<'static, Message>> = Vec::new();
    let mut current_row: Vec<Element<'static, Message>> = Vec::new();

    for photo in photos {
        let card = photo_card(photo, thumbnail_size);
        current_row.push(card);

        if current_row.len() >= columns {
            rows.push(Row::with_children(current_row).spacing(6.0).into());
            current_row = Vec::new();
        }
    }

    if !current_row.is_empty() {
        while current_row.len() < columns {
            current_row.push(Space::with_width(thumbnail_size).into());
        }
        rows.push(Row::with_children(current_row).spacing(6.0).into());
    }

    Column::with_children(rows)
        .spacing(6.0)
        .padding(Padding::from([0, 20]))
        .into()
}

/// Render a single photo card as a clickable thumbnail
fn photo_card(photo: &Photo, size: f32) -> Element<'static, Message> {
    let photo_id = photo.id;
    let file_name = photo.file_name.clone();

    let content: Element<'static, Message> = if let Some(ref thumb_path) = photo.thumbnail_path {
        let path = std::path::PathBuf::from(thumb_path);
        container(
            iced::widget::image(iced::widget::image::Handle::from_path(&path))
                .width(size)
                .height(size)
                .content_fit(iced::ContentFit::Cover),
        )
        .width(size)
        .height(size)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(Backgrounds::ELEVATED.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        placeholder_card(&file_name, size)
    };

    button(content)
        .padding(0)
        .style(|_theme: &iced::Theme, status| {
            let (border_color, border_width) = match status {
                button::Status::Hovered => (Accent::PRIMARY, 2.0),
                _ => (iced::Color::TRANSPARENT, 0.0),
            };
            button::Style {
                background: None,
                border: iced::Border {
                    color: border_color,
                    width: border_width,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectPhoto(photo_id))
        .into()
}

/// Placeholder card shown while thumbnail is loading
fn placeholder_card(file_name: &str, size: f32) -> Element<'static, Message> {
    let display_name = if file_name.chars().count() > 14 {
        let truncated: String = file_name.chars().take(11).collect();
        format!("{}...", truncated)
    } else {
        file_name.to_string()
    };

    container(
        column![text(display_name).size(9).color(TextColors::TERTIARY),]
            .align_x(iced::Alignment::Center)
            .spacing(4),
    )
    .width(size)
    .height(size)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(Backgrounds::ELEVATED.into()),
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// Render a day header for the timeline
pub fn day_header(date: &str, location: Option<&str>, count: usize) -> Element<'static, Message> {
    let date_text = text(date.to_owned())
        .size(14)
        .color(TextColors::PRIMARY);

    let mut header_items: Vec<Element<'static, Message>> =
        vec![date_text.into(), Space::with_width(Length::Fill).into()];

    if let Some(loc) = location {
        header_items.push(
            text(loc.to_owned())
                .size(12)
                .color(TextColors::SECONDARY)
                .into(),
        );
        header_items.push(Space::with_width(16).into());
    }

    header_items.push(
        text(format!("{}", count))
            .size(11)
            .color(TextColors::TERTIARY)
            .into(),
    );

    let header_row = Row::with_children(header_items).align_y(iced::Alignment::Center);

    container(header_row)
        .width(Length::Fill)
        .padding(Padding::from([16, 20]))
        .into()
}
