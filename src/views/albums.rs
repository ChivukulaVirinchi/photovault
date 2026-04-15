//! Albums — grid view, album detail, and album picker overlay.

use std::collections::HashSet;

use iced::widget::{
    button, column, container, row, scrollable, stack, text, text_input, Space,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::components::photo_grid::photo_grid_simple;
use crate::config::AppTheme;
use crate::db::AlbumRecord;
use crate::models::Photo;
use crate::theme::colors;

// ---------------------------------------------------------------------------
// Albums grid view
// ---------------------------------------------------------------------------

pub fn albums_view(
    albums: &[AlbumRecord],
    _drive_path: Option<&std::path::Path>,
    creating: bool,
    create_name: &str,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let bg_primary = p.bg_primary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let accent = p.accent_primary;
    let border_subtle = p.border_subtle;

    // Title bar with create button
    let title = text("Albums").size(28).color(text_primary);

    let create_btn = button(text("+ Create Album").size(12).color(accent))
        .padding(Padding::from([6, 14]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover.into()),
                _ => Some(bg_elevated.into()),
            },
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::AlbumPickerToggleCreate);

    let header = row![title, Space::with_width(Length::Fill), create_btn]
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 24.0,
            right: 32.0,
            bottom: 8.0,
            left: 32.0,
        });

    // Inline "create new" input (shown when creating is true)
    let create_input: Element<'static, Message> = if creating {
        let name_owned = create_name.to_owned();
        let input = text_input("Album name...", &name_owned)
            .on_input(|s| Message::AlbumPickerNameChanged(s))
            .on_submit(Message::CreateAlbum(create_name.to_owned()))
            .size(14)
            .width(Length::Fixed(300.0));

        let submit_btn = button(text("Create").size(12).color(iced::Color::WHITE))
            .padding(Padding::from([6, 14]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: Some(match s {
                    button::Status::Hovered => iced::Color {
                        a: 0.9,
                        ..accent
                    }
                    .into(),
                    _ => accent.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::CreateAlbum(create_name.to_owned()));

        let cancel_btn = button(text("Cancel").size(12).color(text_secondary))
            .padding(Padding::from([6, 12]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: match s {
                    button::Status::Hovered => Some(bg_hover.into()),
                    _ => None,
                },
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::AlbumPickerToggleCreate);

        container(
            row![input, Space::with_width(8), submit_btn, Space::with_width(4), cancel_btn]
                .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 4.0,
            right: 32.0,
            bottom: 12.0,
            left: 32.0,
        })
        .into()
    } else {
        Space::with_height(0).into()
    };

    if albums.is_empty() {
        let body = column![
            header,
            create_input,
            container(
                column![
                    text("No albums yet.").size(16).color(text_secondary),
                    Space::with_height(4),
                    text("Create one to start organising your photos.")
                        .size(13)
                        .color(text_tertiary),
                ]
                .align_x(Alignment::Center),
            )
            .padding(Padding::from([80, 0]))
            .center_x(Length::Fill),
        ];

        return container(scrollable(body))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into();
    }

    // Album card grid
    let mut grid_rows: Vec<Element<'static, Message>> = Vec::new();
    let mut current_row: Vec<Element<'static, Message>> = Vec::new();
    let cols = 4;

    for album in albums {
        current_row.push(album_card(album, p));
        if current_row.len() >= cols {
            grid_rows.push(
                iced::widget::Row::with_children(std::mem::take(&mut current_row))
                    .spacing(16)
                    .into(),
            );
        }
    }
    if !current_row.is_empty() {
        grid_rows.push(
            iced::widget::Row::with_children(std::mem::take(&mut current_row))
                .spacing(16)
                .into(),
        );
    }

    let grid = iced::widget::Column::with_children(grid_rows)
        .spacing(16)
        .padding(Padding {
            top: 8.0,
            right: 32.0,
            bottom: 32.0,
            left: 32.0,
        });

    container(scrollable(column![header, create_input, grid]))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        })
        .into()
}

fn album_card(album: &AlbumRecord, p: &'static colors::Palette) -> Element<'static, Message> {
    let card_w: f32 = 180.0;
    let thumb_h: f32 = 140.0;

    let cover: Element<'static, Message> = match &album.cover_thumbnail_path {
        Some(path) => iced::widget::image(path.clone())
            .content_fit(ContentFit::Cover)
            .width(Length::Fixed(card_w))
            .height(Length::Fixed(thumb_h))
            .into(),
        None => {
            let bg = p.bg_elevated;
            let tc = p.text_tertiary;
            container(text("No cover").size(11).color(tc))
                .width(Length::Fixed(card_w))
                .height(Length::Fixed(thumb_h))
                .center_x(Length::Fixed(card_w))
                .center_y(Length::Fixed(thumb_h))
                .style(move |_t: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    ..Default::default()
                })
                .into()
        }
    };

    let name = text(album.name.clone()).size(13).color(p.text_primary);

    // Subtitle: photo count + optional date range
    let mut sub_parts: Vec<String> = Vec::new();
    sub_parts.push(format!(
        "{} {}",
        album.photo_count,
        if album.photo_count == 1 {
            "photo"
        } else {
            "photos"
        }
    ));
    if let (Some(ref start), Some(ref end)) = (&album.date_range_start, &album.date_range_end) {
        // Show year range if different, else just one year
        let sy = start.get(..4).unwrap_or("");
        let ey = end.get(..4).unwrap_or("");
        if !sy.is_empty() {
            if sy == ey {
                sub_parts.push(sy.to_string());
            } else {
                sub_parts.push(format!("{} - {}", sy, ey));
            }
        }
    }
    let subtitle = text(sub_parts.join(" · ")).size(10).color(p.text_tertiary);

    let caption = column![name, subtitle].spacing(2).padding(Padding::from([6, 4]));

    let inner = column![cover, caption].spacing(0);

    let album_id = album.id;
    let border_color = p.border_subtle;
    let bg = p.bg_elevated;
    let bg_hover = p.bg_hover;

    button(inner)
        .padding(0)
        .width(Length::Fixed(card_w))
        .on_press(Message::OpenAlbum(album_id))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: Some(match s {
                button::Status::Hovered => bg_hover.into(),
                _ => bg.into(),
            }),
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Album detail view
// ---------------------------------------------------------------------------

pub fn album_detail_view(
    album: &AlbumRecord,
    photos: &[Photo],
    columns: usize,
    is_editing_name: bool,
    edit_name: &str,
    selected_photo_ids: &HashSet<i64>,
    hovered_photo_id: Option<i64>,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let album_id = album.id;

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let bg_primary = p.bg_primary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;

    // Back button
    let back_btn = button(
        row![
            text("\u{2190}").size(16).color(text_primary),
            text("Albums").size(14).color(text_secondary)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 12]))
    .style(move |_t: &iced::Theme, status: button::Status| {
        let background = match status {
            button::Status::Hovered => Some(bg_hover.into()),
            _ => None,
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
    .on_press(Message::BackToAlbums);

    // Album name (editable)
    let name_element: Element<'static, Message> = if is_editing_name {
        let edit_name_owned = edit_name.to_owned();
        row![text_input("Album name...", &edit_name_owned)
            .on_input(|s| Message::EditAlbumName(s))
            .on_submit(Message::SaveAlbumName(album_id))
            .size(22)
            .width(Length::Fixed(300.0)),]
        .into()
    } else {
        button(text(album.name.clone()).size(24).color(text_primary))
            .padding(Padding::from([4, 8]))
            .style(move |_t: &iced::Theme, status: button::Status| {
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
            .on_press(Message::StartEditAlbumName(album_id))
            .into()
    };

    // Subtitle
    let mut sub_parts: Vec<String> = Vec::new();
    sub_parts.push(format!(
        "{} {}",
        photos.len(),
        if photos.len() == 1 { "photo" } else { "photos" }
    ));
    if let (Some(ref start), Some(ref end)) = (&album.date_range_start, &album.date_range_end) {
        let sy = start.get(..4).unwrap_or("");
        let ey = end.get(..4).unwrap_or("");
        if !sy.is_empty() {
            if sy == ey {
                sub_parts.push(sy.to_string());
            } else {
                sub_parts.push(format!("{} - {}", sy, ey));
            }
        }
    }
    let subtitle = text(sub_parts.join(" · ")).size(14).color(text_secondary);

    // Action buttons
    let danger_color = p.semantic_danger;
    let delete_btn = button(text("Delete Album").size(12).color(danger_color))
        .padding(Padding::from([6, 12]))
        .style(move |_t: &iced::Theme, status: button::Status| {
            let background = match status {
                button::Status::Hovered => Some(iced::Color { a: 0.12, ..danger_color }.into()),
                _ => Some(bg_elevated.into()),
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
        .on_press(Message::DeleteAlbum(album_id));

    // Photo grid
    let grid: Element<'static, Message> = if photos.is_empty() {
        container(
            column![
                text("This album is empty.").size(14).color(text_secondary),
                Space::with_height(4),
                text("Add photos from the timeline or photo detail view.")
                    .size(12)
                    .color(text_tertiary),
            ]
            .align_x(Alignment::Center),
        )
        .padding(Padding::from([80, 0]))
        .center_x(Length::Fill)
        .into()
    } else {
        let sel = if selected_photo_ids.is_empty() {
            None
        } else {
            Some(selected_photo_ids)
        };
        photo_grid_simple(photos, 160.0, columns, sel, hovered_photo_id, theme)
    };

    // Selection action bar
    let selection_bar: Element<'static, Message> = if !selected_photo_ids.is_empty() {
        let count = selected_photo_ids.len();
        let ids_vec: Vec<i64> = selected_photo_ids.iter().copied().collect();

        let clear_hover = bg_hover;
        let clear_border = p.border_subtle;
        let clear_text = text_secondary;
        let clear_btn = button(text("x").size(14).color(clear_text))
            .padding([2, 8])
            .style(move |_t: &iced::Theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(clear_hover.into()),
                    _ => None,
                },
                border: iced::Border {
                    color: clear_border,
                    width: 1.0,
                    radius: 999.0.into(),
                },
                ..Default::default()
            })
            .on_press(Message::ClearTimelinePhotoSelection);

        let remove_btn = button(text("Remove from Album").size(12).color(danger_color))
            .padding([6, 12])
            .style(move |_t: &iced::Theme, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => iced::Color {
                        r: (danger_color.r * 0.9).clamp(0.0, 1.0),
                        g: (danger_color.g * 0.9).clamp(0.0, 1.0),
                        b: (danger_color.b * 0.9).clamp(0.0, 1.0),
                        a: 1.0,
                    }
                    .into(),
                    _ => danger_color.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                text_color: iced::Color::WHITE,
                ..Default::default()
            })
            .on_press(Message::RemovePhotosFromAlbum(album_id, ids_vec));

        let bar_bg = p.bg_secondary;
        let bar_border = p.border_subtle;
        container(
            row![
                clear_btn,
                text(format!("{} selected", count))
                    .size(12)
                    .color(text_secondary),
                Space::with_width(Length::Fill),
                remove_btn,
            ]
            .align_y(Alignment::Center)
            .spacing(10),
        )
        .padding([8, 20])
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bar_bg.into()),
            border: iced::Border {
                color: bar_border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    } else {
        Space::with_height(0).into()
    };

    let content = column![
        selection_bar,
        scrollable(
            column![
                back_btn,
                Space::with_height(16),
                row![
                    column![name_element, subtitle].spacing(4),
                    Space::with_width(Length::Fill),
                    delete_btn,
                ]
                .align_y(Alignment::Center),
                Space::with_height(24),
                grid,
            ]
            .padding(32)
        ),
    ];

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Album picker overlay (modal)
// ---------------------------------------------------------------------------

pub fn album_picker_overlay(
    albums: &[AlbumRecord],
    target_count: usize,
    creating: bool,
    create_name: &str,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let border_subtle = p.border_subtle;
    let accent = p.accent_primary;

    // Title
    let title = text(format!(
        "Add {} {} to album",
        target_count,
        if target_count == 1 { "photo" } else { "photos" }
    ))
    .size(18)
    .color(text_primary);

    let close_btn = button(text("X").size(14).color(text_secondary))
        .padding(Padding::from([4, 8]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover.into()),
                _ => None,
            },
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::CloseAlbumPicker);

    let header = row![title, Space::with_width(Length::Fill), close_btn]
        .align_y(Alignment::Center);

    // "Create new album" row
    let create_section: Element<'static, Message> = if creating {
        let name_owned = create_name.to_owned();
        let input = text_input("New album name...", &name_owned)
            .on_input(|s| Message::AlbumPickerNameChanged(s))
            .on_submit(Message::AlbumPickerCreateAndAdd)
            .size(13)
            .width(Length::Fill);

        let submit = button(text("Create").size(12).color(iced::Color::WHITE))
            .padding(Padding::from([5, 12]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: Some(match s {
                    button::Status::Hovered => iced::Color {
                        a: 0.9,
                        ..accent
                    }
                    .into(),
                    _ => accent.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::AlbumPickerCreateAndAdd);

        container(
            row![input, Space::with_width(8), submit]
                .align_y(Alignment::Center),
        )
        .padding(Padding::from([8, 0]))
        .into()
    } else {
        let new_btn = button(
            text("+ Create new album")
                .size(13)
                .color(accent),
        )
        .padding(Padding::from([8, 0]))
        .width(Length::Fill)
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover.into()),
                _ => None,
            },
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::AlbumPickerToggleCreate);
        new_btn.into()
    };

    // Album list
    let mut list_items: Vec<Element<'static, Message>> = Vec::new();

    if albums.is_empty() {
        list_items.push(
            container(text("No albums yet.").size(13).color(text_tertiary))
                .padding(Padding::from([16, 0]))
                .into(),
        );
    } else {
        for album in albums {
            list_items.push(picker_album_row(album, p));
        }
    }

    let list = iced::widget::Column::with_children(list_items).spacing(2);

    let card_content = column![
        header,
        Space::with_height(8),
        create_section,
        Space::with_height(8),
        scrollable(list).height(Length::Fixed(300.0)),
    ]
    .spacing(0)
    .width(Length::Fixed(380.0));

    let card_bg = bg_elevated;
    let card = container(card_content)
        .padding(Padding::from([20, 24]))
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(card_bg.into()),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        });

    // Backdrop: semi-transparent overlay. Click to dismiss.
    let backdrop = button(Space::new(Length::Fill, Length::Fill))
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme, _s| button::Style {
            background: Some(iced::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.45 }.into()),
            ..Default::default()
        })
        .on_press(Message::CloseAlbumPicker);

    let centered_card = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    stack![backdrop, centered_card].into()
}

fn picker_album_row(
    album: &AlbumRecord,
    p: &'static colors::Palette,
) -> Element<'static, Message> {
    let album_id = album.id;
    let bg_hover = p.bg_hover;
    let text_primary = p.text_primary;
    let text_tertiary = p.text_tertiary;

    // Mini cover thumb
    let thumb: Element<'static, Message> = match &album.cover_thumbnail_path {
        Some(path) => iced::widget::image(path.clone())
            .content_fit(ContentFit::Cover)
            .width(32)
            .height(32)
            .into(),
        None => {
            let bg = p.bg_active;
            container(Space::new(32, 32))
                .style(move |_t: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        }
    };

    let info = column![
        text(album.name.clone()).size(13).color(text_primary),
        text(format!(
            "{} {}",
            album.photo_count,
            if album.photo_count == 1 {
                "photo"
            } else {
                "photos"
            }
        ))
        .size(10)
        .color(text_tertiary),
    ]
    .spacing(1);

    let inner = row![thumb, Space::with_width(10), info]
        .align_y(Alignment::Center);

    button(inner)
        .padding(Padding::from([6, 8]))
        .width(Length::Fill)
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover.into()),
                _ => None,
            },
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::AlbumPickerSelect(album_id))
        .into()
}
