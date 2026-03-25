//! Photo grid component
//!
//! Renders photos in a responsive grid layout with day headers.
//! Uses standard iced widgets with scrollable for initial implementation.
//! For 100k+ photos, a custom virtual scrolling widget would be needed.

use iced::widget::{button, column, container, text, Column, Row, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::theme::colors::{Backgrounds, Border, Text as TextColors};

/// Render a photo grid using standard iced widgets
///
/// Takes owned photo data to avoid lifetime issues with closures.
/// Each photo card is a clickable button that sends SelectPhoto(id).
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
            rows.push(Row::with_children(current_row).spacing(8.0).into());
            current_row = Vec::new();
        }
    }

    // Add remaining photos with padding
    if !current_row.is_empty() {
        while current_row.len() < columns {
            current_row.push(Space::with_width(thumbnail_size).into());
        }
        rows.push(Row::with_children(current_row).spacing(8.0).into());
    }

    Column::with_children(rows)
        .spacing(8.0)
        .padding(Padding::from([0, 16]))
        .into()
}

/// Render a single photo card as a clickable thumbnail placeholder
fn photo_card(photo: &Photo, size: f32) -> Element<'static, Message> {
    let photo_id = photo.id;
    let file_name = photo.file_name.clone();

    // If we have a thumbnail path, show the actual image.
    // Trust the thumbnail_path field — if set, the file should exist.
    // Avoids synchronous .exists() stat on every re-render.
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
                color: Border::SUBTLE,
                width: 1.0,
                radius: 4.0.into(),
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
            let border_color = match status {
                button::Status::Hovered => Border::VISIBLE,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: None,
                border: iced::Border {
                    color: border_color,
                    width: 2.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectPhoto(photo_id))
        .into()
}

/// Placeholder card shown while thumbnail is loading or not yet generated
fn placeholder_card(file_name: &str, size: f32) -> Element<'static, Message> {
    // Truncate filename for display using char-based indexing (safe for multi-byte UTF-8)
    let display_name = if file_name.chars().count() > 16 {
        let truncated: String = file_name.chars().take(13).collect();
        format!("{}...", truncated)
    } else {
        file_name.to_string()
    };

    container(
        column![
            text("\u{1F4F7}").size(24),
            text(display_name).size(9).color(TextColors::TERTIARY),
        ]
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
            color: Border::SUBTLE,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Render a day header for the timeline
pub fn day_header(date: &str, location: Option<&str>, count: usize) -> Element<'static, Message> {
    let date_text = text(date.to_owned()).size(16).color(TextColors::PRIMARY);

    let mut header_items: Vec<Element<'static, Message>> =
        vec![date_text.into(), Space::with_width(Length::Fill).into()];

    if let Some(loc) = location {
        header_items.push(
            text(loc.to_owned())
                .size(14)
                .color(TextColors::SECONDARY)
                .into(),
        );
        header_items.push(Space::with_width(16).into());
    }

    header_items.push(
        text(format!("{} photos", count))
            .size(12)
            .color(TextColors::TERTIARY)
            .into(),
    );

    let header_row = Row::with_children(header_items).align_y(iced::Alignment::Center);

    container(header_row)
        .width(Length::Fill)
        .padding(Padding::from([16, 16]))
        .style(|_theme: &iced::Theme| container::Style {
            border: iced::Border {
                color: Border::SUBTLE,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
