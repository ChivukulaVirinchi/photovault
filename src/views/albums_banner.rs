//! Timeline suggestions banner for albums.

use iced::widget::{button, container, image as iced_image, row, scrollable, text, Space};
use iced::{ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::AlbumSuggestionRecord;
use crate::theme::colors;

/// Horizontal strip of fresh suggestions (seen_count < 3) for the Timeline.
/// Returns None when empty.
pub fn suggestions_banner(
    suggestions: &[AlbumSuggestionRecord],
    theme: AppTheme,
) -> Option<Element<'static, Message>> {
    let fresh: Vec<&AlbumSuggestionRecord> = suggestions
        .iter()
        .filter(|s| s.seen_count < 3)
        .take(5)
        .collect();

    if fresh.is_empty() {
        return None;
    }

    let p = colors::palette(theme);
    let mut strip = row![].spacing(12);

    for sug in fresh {
        strip = strip.push(suggestion_banner_card(sug, p));
    }

    let banner = container(
        scrollable(strip).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(6),
        )),
    )
    .padding(Padding {
        top: 8.0,
        right: 24.0,
        bottom: 8.0,
        left: 24.0,
    });

    Some(banner.into())
}

fn suggestion_banner_card(
    sug: &AlbumSuggestionRecord,
    p: &'static colors::Palette,
) -> Element<'static, Message> {
    let card_w: f32 = 280.0;
    let thumb_h: f32 = 100.0;
    let sug_id = sug.id;

    let cover: Element<'static, Message> = match &sug.cover_thumbnail_path {
        Some(path) => iced_image(path.clone())
            .content_fit(ContentFit::Cover)
            .width(Length::Fixed(card_w))
            .height(Length::Fixed(thumb_h))
            .into(),
        None => {
            let bg = p.bg_elevated;
            container(Space::new(Length::Fixed(card_w), Length::Fixed(thumb_h)))
                .style(move |_t: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    ..Default::default()
                })
                .into()
        }
    };

    let accent = p.accent_primary;
    let tc_sec = p.text_secondary;
    let bg_hover_c = p.bg_hover;
    let bg_el = p.bg_elevated;
    let border = p.border_subtle;

    let accept_btn = button(text("Accept").size(10).color(accent))
        .padding(Padding::from([3, 8]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover_c.into()),
                _ => Some(bg_el.into()),
            },
            border: iced::Border {
                color: border,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::BeginAcceptSuggestion(sug_id));

    let dismiss_btn = button(text("Dismiss").size(10).color(tc_sec))
        .padding(Padding::from([3, 8]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover_c.into()),
                _ => None,
            },
            border: iced::Border {
                color: border,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::DismissSuggestion(sug_id));

    let card = container(
        iced::widget::column![
            cover,
            iced::widget::column![
                text(sug.title.clone()).size(12).color(p.text_primary),
                text(format!("{} photos", sug.photo_ids().len()))
                    .size(10)
                    .color(tc_sec),
                row![accept_btn, dismiss_btn].spacing(6),
            ]
            .spacing(4)
            .padding(Padding::from([8, 10])),
        ]
        .spacing(0),
    )
    .width(Length::Fixed(card_w))
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(p.bg_elevated.into()),
        border: iced::Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    });

    card.into()
}
