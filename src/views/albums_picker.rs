//! Album picker overlay and rows.

use iced::widget::{button, column, container, row, scrollable, stack, text, text_input, Space};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::AlbumRecord;
use crate::theme::colors;

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

    let header = row![title, Space::with_width(Length::Fill), close_btn].align_y(Alignment::Center);

    let create_section: Element<'static, Message> = if creating {
        let name_owned = create_name.to_owned();
        let input = text_input("New album name...", &name_owned)
            .id(text_input::Id::new("album-picker-new"))
            .on_input(|s| Message::AlbumPickerNameChanged(s))
            .on_submit(Message::AlbumPickerCreateAndAdd)
            .size(13)
            .width(Length::Fill);

        let submit = button(text("Create").size(12).color(iced::Color::WHITE))
            .padding(Padding::from([5, 12]))
            .style(move |_t: &iced::Theme, s| button::Style {
                background: Some(match s {
                    button::Status::Hovered => iced::Color { a: 0.9, ..accent }.into(),
                    _ => accent.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::AlbumPickerCreateAndAdd);

        container(row![input, Space::with_width(8), submit].align_y(Alignment::Center))
            .padding(Padding::from([8, 0]))
            .into()
    } else {
        let new_btn = button(text("+ Create new album").size(13).color(accent))
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

    let backdrop = button(Space::new(Length::Fill, Length::Fill))
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme, _s| button::Style {
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
        })
        .on_press(Message::CloseAlbumPicker);

    let centered_card = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    stack![backdrop, centered_card].into()
}

fn picker_album_row(album: &AlbumRecord, p: &'static colors::Palette) -> Element<'static, Message> {
    let album_id = album.id;
    let bg_hover = p.bg_hover;
    let text_primary = p.text_primary;
    let text_tertiary = p.text_tertiary;

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

    let inner = row![thumb, Space::with_width(10), info].align_y(Alignment::Center);

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
