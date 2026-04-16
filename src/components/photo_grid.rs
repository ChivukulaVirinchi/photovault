//! Photo grid component
//!
//! Renders photos in a responsive grid layout with day headers.
//! Design: gallery-style cards with subtle rounded corners and clean hover.

use std::collections::HashSet;

use iced::widget::{button, column, container, mouse_area, text, Column, Row, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::models::Photo;
use crate::theme::colors;

/// Render a photo grid using standard iced widgets
pub fn photo_grid_simple(
    photos: &[Photo],
    thumbnail_size: f32,
    columns: usize,
    selected_photo_ids: Option<&HashSet<i64>>,
    hovered_photo_id: Option<i64>,
    highlighted_photo_id: Option<i64>,
    theme: AppTheme,
) -> Element<'static, Message> {
    let mut rows: Vec<Element<'static, Message>> = Vec::new();
    let mut current_row: Vec<Element<'static, Message>> = Vec::new();

    for photo in photos {
        let card = photo_card(
            photo,
            thumbnail_size,
            selected_photo_ids,
            hovered_photo_id,
            highlighted_photo_id,
            theme,
        );
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
fn photo_card(
    photo: &Photo,
    size: f32,
    selected_photo_ids: Option<&HashSet<i64>>,
    hovered_photo_id: Option<i64>,
    highlighted_photo_id: Option<i64>,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let photo_id = photo.id;
    let file_name = photo.file_name.clone();
    let is_selected = selected_photo_ids
        .map(|ids| ids.contains(&photo_id))
        .unwrap_or(false);
    let in_select_mode = selected_photo_ids
        .map(|ids| !ids.is_empty())
        .unwrap_or(false);
    let is_hovered = hovered_photo_id == Some(photo_id);
    let is_highlighted = highlighted_photo_id == Some(photo_id);

    let bg_elevated = p.bg_elevated;
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
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        placeholder_card(&file_name, size, theme)
    };

    let accent_primary = p.accent_primary;
    let primary_action = if in_select_mode {
        Message::ToggleTimelinePhotoSelection(photo_id)
    } else {
        Message::SelectPhoto(photo_id)
    };

    let mut tile_button = button(content)
        .padding(0)
        .style(move |_theme: &iced::Theme, status| {
            let (border_color, border_width) = match status {
                _ if is_highlighted => (accent_primary, 3.0),
                button::Status::Hovered => (accent_primary, 2.0),
                _ if is_selected => (accent_primary, 2.0),
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
        });
    tile_button = tile_button.on_press(primary_action);

    if selected_photo_ids.is_some() {
        let show_selector = is_hovered || is_selected;
        let marker_label = if is_selected { "x" } else { "o" };
        let marker_text_color = if is_selected {
            p.text_primary
        } else {
            p.text_tertiary
        };
        let marker_border = p.border_subtle;
        let marker_hover = p.bg_hover;
        let selector: Element<'static, Message> = if show_selector {
            button(text(marker_label).size(12).color(marker_text_color))
                .padding(Padding::from([2, 7]))
                .style(move |_theme: &iced::Theme, status| button::Style {
                    background: match status {
                        button::Status::Hovered => Some(marker_hover.into()),
                        _ => None,
                    },
                    border: iced::Border {
                        color: marker_border,
                        width: 1.0,
                        radius: 999.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(Message::ToggleTimelinePhotoSelection(photo_id))
                .into()
        } else {
            Space::with_width(24).height(24).into()
        };

        let card: Element<'static, Message> = container(
            column![
                Row::with_children(vec![selector, Space::with_width(Length::Fill).into()])
                    .width(Length::Fill),
                tile_button,
            ]
            .spacing(4.0),
        )
        .width(size)
        .style(move |_theme: &iced::Theme| container::Style {
            border: iced::Border {
                color: if is_selected {
                    accent_primary
                } else {
                    iced::Color::TRANSPARENT
                },
                width: if is_selected { 2.0 } else { 0.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into();

        return mouse_area(card)
            .on_enter(Message::TimelinePhotoHover(Some(photo_id)))
            .on_exit(Message::TimelinePhotoHover(None))
            .into();
    }

    mouse_area(tile_button)
        .on_enter(Message::TimelinePhotoHover(Some(photo_id)))
        .on_exit(Message::TimelinePhotoHover(None))
        .into()
}

/// Placeholder card shown while thumbnail is loading
fn placeholder_card(file_name: &str, size: f32, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let display_name = if file_name.chars().count() > 14 {
        let truncated: String = file_name.chars().take(11).collect();
        format!("{}...", truncated)
    } else {
        file_name.to_string()
    };

    let bg_elevated = p.bg_elevated;
    container(
        column![text(display_name).size(9).color(p.text_tertiary),]
            .align_x(iced::Alignment::Center)
            .spacing(4),
    )
    .width(size)
    .height(size)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(bg_elevated.into()),
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// Render a day header for the timeline
pub fn day_header(
    day_key: &str,
    date: &str,
    count: usize,
    selected_count: usize,
    is_hovered: bool,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let date_text = text(date.to_owned()).size(14).color(p.text_primary);

    let mut header_items: Vec<Element<'static, Message>> = vec![];

    let show_day_selector = is_hovered || selected_count > 0;
    if show_day_selector {
        let all_selected = selected_count >= count && count > 0;
        let marker = if all_selected { "x" } else { "o" };
        let marker_hover = p.bg_hover;
        let marker_border = p.border_subtle;
        let selector = button(text(marker).size(11).color(if all_selected {
            p.text_primary
        } else {
            p.text_tertiary
        }))
        .padding(Padding::from([1, 6]))
        .style(move |_theme: &iced::Theme, status| button::Style {
            background: match status {
                button::Status::Hovered => Some(marker_hover.into()),
                _ => None,
            },
            border: iced::Border {
                color: marker_border,
                width: 1.0,
                radius: 999.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::ToggleTimelineDaySelection(day_key.to_owned()));

        header_items.push(selector.into());
        header_items.push(Space::with_width(10).into());
    }

    header_items.push(date_text.into());
    header_items.push(Space::with_width(Length::Fill).into());

    header_items.push(
        text(format!("{}", count))
            .size(11)
            .color(p.text_tertiary)
            .into(),
    );

    if selected_count > 0 {
        header_items.push(Space::with_width(12).into());
        header_items.push(
            text(format!("{} selected", selected_count))
                .size(11)
                .color(p.accent_primary)
                .into(),
        );
    }

    let header_row = Row::with_children(header_items).align_y(iced::Alignment::Center);

    let header: Element<'static, Message> = container(header_row)
        .width(Length::Fill)
        .padding(Padding::from([16, 20]))
        .into();

    mouse_area(header)
        .on_enter(Message::TimelineDayHover(Some(day_key.to_owned())))
        .on_exit(Message::TimelineDayHover(None))
        .into()
}

/// Render a grid of placeholder boxes for skeleton-screen loading states.
pub fn skeleton_grid(
    rows_count: usize,
    cols: usize,
    cell_size: f32,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let bg = p.bg_elevated;
    let border = p.border_subtle;
    let mut rows_vec: Vec<Element<'static, Message>> = Vec::new();
    for _ in 0..rows_count {
        let mut cells: Vec<Element<'static, Message>> = Vec::new();
        for _ in 0..cols {
            cells.push(
                container(Space::new(
                    Length::Fixed(cell_size),
                    Length::Fixed(cell_size),
                ))
                .style(move |_t| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        color: border,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                })
                .into(),
            );
        }
        rows_vec.push(Row::with_children(cells).spacing(8).into());
    }
    Column::with_children(rows_vec)
        .spacing(8)
        .padding(Padding::from([0, 20]))
        .into()
}
