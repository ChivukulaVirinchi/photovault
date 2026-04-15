//! Albums views: grid, detail, and picker overlay.

use std::collections::HashSet;

use iced::widget::{
    button, column, container, row, scrollable, text, text_input, Column, Row, Space,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::AlbumRecord;
use crate::models::Photo;
use crate::theme::colors;

pub fn albums_view(
    albums: &[AlbumRecord],
    _drive_path: Option<&std::path::Path>,
    creating: bool,
    create_name: &str,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let title = text("Albums").size(28).color(p.text_primary);
    let create_btn = button(text("Create Album").size(12).color(p.text_primary))
        .padding([6, 12])
        .style(move |_t: &iced::Theme, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => p.accent_hover.into(),
                _ => p.accent_muted.into(),
            }),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::AlbumPickerToggleCreate);

    let mut root = column![row![title, Space::with_width(Length::Fill), create_btn]
        .align_y(Alignment::Center)
        .padding(Padding::from([24, 24])),];

    if creating {
        let name_owned = create_name.to_string();
        root = root.push(
            row![
                text_input("Album name", &name_owned)
                    .on_input(Message::AlbumPickerNameChanged)
                    .on_submit(Message::CreateAlbum(name_owned.clone()))
                    .padding([8, 10])
                    .size(14)
                    .width(Length::Fixed(260.0)),
                button(text("Create").size(12).color(p.text_primary))
                    .padding([8, 12])
                    .on_press(Message::CreateAlbum(name_owned)),
            ]
            .spacing(8)
            .padding(Padding {
                top: 0.0,
                right: 24.0,
                bottom: 16.0,
                left: 24.0,
            })
            .align_y(Alignment::Center),
        );
    }

    if albums.is_empty() {
        root = root.push(
            container(
                text("No albums yet. Create one to get started.")
                    .size(14)
                    .color(p.text_secondary),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        );
        return container(root)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_t| container::Style {
                background: Some(p.bg_primary.into()),
                ..Default::default()
            })
            .into();
    }

    let mut rows: Vec<Element<'static, Message>> = Vec::new();
    let mut current: Vec<Element<'static, Message>> = Vec::new();
    for album in albums {
        current.push(album_card(album, theme));
        if current.len() == 4 {
            rows.push(Row::with_children(current).spacing(16).into());
            current = Vec::new();
        }
    }
    if !current.is_empty() {
        rows.push(Row::with_children(current).spacing(16).into());
    }

    let grid = scrollable(Column::with_children(rows).spacing(16).padding(Padding {
        top: 0.0,
        right: 24.0,
        bottom: 24.0,
        left: 24.0,
    }));
    root = root.push(grid);

    container(root)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t| container::Style {
            background: Some(p.bg_primary.into()),
            ..Default::default()
        })
        .into()
}

fn album_card(album: &AlbumRecord, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let cover: Element<'static, Message> = if let Some(path) = &album.cover_thumbnail_path {
        container(
            iced::widget::image(iced::widget::image::Handle::from_path(path))
                .width(160)
                .height(160)
                .content_fit(ContentFit::Cover),
        )
        .width(160)
        .height(160)
        .style(move |_t| container::Style {
            background: Some(p.bg_elevated.into()),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        container(text("No cover").size(11).color(p.text_tertiary))
            .width(160)
            .height(160)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_t| container::Style {
                background: Some(p.bg_elevated.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    };

    let meta = format!(
        "{} photos{}",
        album.photo_count,
        date_range_suffix(
            album.date_range_start.as_deref(),
            album.date_range_end.as_deref()
        )
    );

    let content = column![
        cover,
        text(album.name.clone()).size(12).color(p.text_primary),
        text(meta).size(10).color(p.text_tertiary),
    ]
    .spacing(6)
    .width(160);

    button(content)
        .padding(8)
        .style(move |_t, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => p.bg_hover.into(),
                _ => p.bg_primary.into(),
            }),
            border: iced::Border {
                color: p.border_subtle,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::OpenAlbum(album.id))
        .into()
}

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

    let back_btn = button(text("< Albums").size(12).color(p.text_secondary))
        .padding([6, 10])
        .on_press(Message::BackToAlbums);

    let title: Element<'static, Message> = if is_editing_name {
        let edit_name_owned = edit_name.to_string();
        text_input("Album name", &edit_name_owned)
            .on_input(Message::EditAlbumName)
            .on_submit(Message::SaveAlbumName(album.id))
            .size(24)
            .into()
    } else {
        button(text(album.name.clone()).size(24).color(p.text_primary))
            .padding(0)
            .style(|_, _| button::Style::default())
            .on_press(Message::StartEditAlbumName(album.id))
            .into()
    };

    let subtitle = text(format!(
        "{} photos{}",
        photos.len(),
        date_range_suffix(
            album.date_range_start.as_deref(),
            album.date_range_end.as_deref()
        )
    ))
    .size(12)
    .color(p.text_tertiary);

    let cover_hint = if album.cover_auto_picked {
        "Auto cover"
    } else {
        "Manual cover"
    };
    let updated_line = text(format!(
        "{} · created {} · updated {}",
        cover_hint, album.created_at, album.updated_at
    ))
    .size(11)
    .color(p.text_tertiary);

    let rename_btn = button(text("Rename").size(12).color(p.text_primary))
        .padding([6, 12])
        .on_press(Message::StartEditAlbumName(album.id));
    let set_cover_btn: Element<'static, Message> = if let Some(photo) = photos.first() {
        button(text("Set Cover to First").size(12).color(p.text_primary))
            .padding([6, 12])
            .on_press(Message::SetAlbumCover(album.id, photo.id))
            .into()
    } else {
        button(text("Set Cover to First").size(12).color(p.text_tertiary))
            .padding([6, 12])
            .into()
    };
    let delete_btn = button(text("Delete").size(12).color(p.semantic_danger))
        .padding([6, 12])
        .on_press(Message::DeleteAlbum(album.id));

    let mut content = column![
        back_btn,
        Space::with_height(8),
        row![
            column![title, subtitle, updated_line].spacing(4),
            Space::with_width(Length::Fill),
            rename_btn,
            Space::with_width(8),
            set_cover_btn,
            Space::with_width(8),
            delete_btn,
        ]
        .align_y(Alignment::Center),
        Space::with_height(16),
    ]
    .padding(Padding::from([24, 24]));

    if !selected_photo_ids.is_empty() {
        let selected_ids = selected_photo_ids.iter().copied().collect::<Vec<_>>();
        let bar = container(
            row![
                text(format!("{} selected", selected_ids.len()))
                    .size(12)
                    .color(p.text_secondary),
                Space::with_width(Length::Fill),
                button(text("Add to Album").size(12).color(p.text_primary))
                    .padding([6, 12])
                    .on_press(Message::OpenAlbumPicker(selected_ids.clone())),
                Space::with_width(8),
                button(text("Remove from Album").size(12).color(p.semantic_danger))
                    .padding([6, 12])
                    .on_press(Message::RemovePhotosFromAlbum(album.id, selected_ids)),
            ]
            .align_y(Alignment::Center)
            .spacing(8),
        )
        .padding([8, 12])
        .style(move |_t| container::Style {
            background: Some(p.bg_secondary.into()),
            border: iced::Border {
                color: p.border_subtle,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });
        content = content.push(bar).push(Space::with_height(12));
    }

    if photos.is_empty() {
        content = content.push(
            container(
                text("This album is empty. Add photos from the timeline.")
                    .size(14)
                    .color(p.text_secondary),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        );
    } else {
        let grid = crate::components::photo_grid::photo_grid_simple(
            photos,
            160.0,
            columns,
            Some(selected_photo_ids),
            hovered_photo_id,
            theme,
        );
        content = content.push(scrollable(grid));
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t| container::Style {
            background: Some(p.bg_primary.into()),
            ..Default::default()
        })
        .into()
}

pub fn album_picker_overlay(
    albums: &[AlbumRecord],
    target_count: usize,
    creating: bool,
    create_name: &str,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let backdrop = iced::widget::mouse_area(
        container(Space::with_height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_t| container::Style {
                background: Some(
                    iced::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.45,
                    }
                    .into(),
                ),
                ..Default::default()
            }),
    )
    .on_press(Message::CloseAlbumPicker);

    let mut list = column![
        button(text("+ Create new album").size(13).color(p.text_primary))
            .padding([8, 10])
            .width(Length::Fill)
            .on_press(Message::AlbumPickerToggleCreate)
    ]
    .spacing(8);

    if creating {
        let name_owned = create_name.to_string();
        list = list.push(
            row![
                text_input("Album name", &name_owned)
                    .on_input(Message::AlbumPickerNameChanged)
                    .on_submit(Message::AlbumPickerCreateAndAdd)
                    .padding([8, 10])
                    .size(13)
                    .width(Length::Fill),
                button(text("Create").size(12).color(p.text_primary))
                    .padding([8, 12])
                    .on_press(Message::AlbumPickerCreateAndAdd),
            ]
            .spacing(8),
        );
    }

    for album in albums {
        list = list.push(album_picker_row(album, theme));
    }

    let panel = container(
        column![
            row![
                text(format!("Add {} photos to album", target_count))
                    .size(18)
                    .color(p.text_primary),
                Space::with_width(Length::Fill),
                button(text("x").size(16).color(p.text_secondary))
                    .padding([2, 8])
                    .on_press(Message::CloseAlbumPicker),
            ]
            .align_y(Alignment::Center),
            Space::with_height(12),
            scrollable(list),
        ]
        .spacing(4)
        .padding(Padding::from([16, 16]))
        .height(Length::Fixed(500.0)),
    )
    .width(Length::Fixed(420.0))
    .style(move |_t| container::Style {
        background: Some(p.bg_primary.into()),
        border: iced::Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    });

    iced::widget::stack![
        backdrop,
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(Padding::from([40, 0]))
            .style(|_t| container::Style::default())
    ]
    .into()
}

fn album_picker_row(album: &AlbumRecord, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let mini_cover: Element<'static, Message> = if let Some(path) = &album.cover_thumbnail_path {
        iced::widget::image(iced::widget::image::Handle::from_path(path))
            .width(32)
            .height(32)
            .content_fit(ContentFit::Cover)
            .into()
    } else {
        container(text("-").size(11).color(p.text_tertiary))
            .width(32)
            .height(32)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    };

    button(
        row![
            mini_cover,
            Space::with_width(8),
            column![
                text(album.name.clone()).size(13).color(p.text_primary),
                text(format!("{} photos", album.photo_count))
                    .size(11)
                    .color(p.text_tertiary)
            ]
            .spacing(2),
        ]
        .spacing(0)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([8, 8])
    .style(move |_t, status| button::Style {
        background: Some(match status {
            button::Status::Hovered => p.bg_hover.into(),
            _ => p.bg_secondary.into(),
        }),
        border: iced::Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .on_press(Message::AlbumPickerSelect(album.id))
    .into()
}

fn date_range_suffix(start: Option<&str>, end: Option<&str>) -> String {
    let s = start.and_then(short_date);
    let e = end.and_then(short_date);
    match (s, e) {
        (Some(a), Some(b)) if a == b => format!(" · {}", a),
        (Some(a), Some(b)) => format!(" · {} - {}", a, b),
        (Some(a), None) => format!(" · {}", a),
        (None, Some(b)) => format!(" · {}", b),
        (None, None) => String::new(),
    }
}

fn short_date(input: &str) -> Option<String> {
    if input.len() >= 10 {
        Some(input[..10].to_string())
    } else {
        None
    }
}
