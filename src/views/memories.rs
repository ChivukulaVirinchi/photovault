//! Memories — "N years ago today" carousel banner + dedicated view + detail.

use iced::widget::{button, column, container, image as iced_image, row, scrollable, text, Space};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::MemoryCard;
use crate::theme::colors;

/// Horizontal carousel banner embedded above the Timeline grid.
/// Returns None when there are no memories so the caller can omit the
/// banner entirely (no empty space).
pub fn memories_banner(
    cards: &[MemoryCard],
    theme: AppTheme,
) -> Option<Element<'static, Message>> {
    if cards.is_empty() {
        return None;
    }

    let p = colors::palette(theme);
    let mut strip = row![].spacing(12);

    // Show top 5 on the banner; rest accessible via the sidebar "Memories" entry.
    for card in cards.iter().take(5) {
        strip = strip.push(banner_card(card, &p));
    }

    let banner = container(
        scrollable(strip).direction(
            scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(6)),
        ),
    )
    .padding(Padding {
        top: 12.0,
        right: 24.0,
        bottom: 8.0,
        left: 24.0,
    });

    Some(banner.into())
}

fn banner_card(card: &MemoryCard, p: &colors::Palette) -> Element<'static, Message> {
    let card_w: f32 = 340.0;
    let hero_h: f32 = 190.0;

    let hero: Element<'static, Message> = match &card.hero_thumbnail_path {
        Some(path) => iced_image(path.clone())
            .content_fit(ContentFit::Cover)
            .width(Length::Fixed(card_w))
            .height(Length::Fixed(hero_h))
            .into(),
        None => container(Space::new(Length::Fixed(card_w), Length::Fixed(hero_h)))
            .style({
                let bg = p.bg_elevated;
                move |_t: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    ..Default::default()
                }
            })
            .into(),
    };

    let caption = column![
        text(card.title.clone()).size(16).color(p.text_primary),
        Space::with_height(2),
        text(format!("{} photos", card.photo_count))
            .size(11)
            .color(p.text_secondary),
    ]
    .spacing(0);

    let inner = column![
        hero,
        container(caption).padding(Padding::from([8, 12])),
    ]
    .spacing(0);

    let id_clone = card.id.clone();
    let border_color = p.border_subtle;
    let bg = p.bg_elevated;
    let bg_hover = p.bg_hover;

    button(inner)
        .padding(0)
        .on_press(Message::OpenMemory(id_clone))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: Some(match s {
                button::Status::Hovered => bg_hover.into(),
                _ => bg.into(),
            }),
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Dedicated "Memories" sidebar view — all active memory cards as wide rows.
pub fn memories_view(
    cards: &[MemoryCard],
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let header = container(
        text("Memories").size(28).color(p.text_primary),
    )
    .padding(Padding {
        top: 24.0,
        right: 32.0,
        bottom: 8.0,
        left: 32.0,
    });

    if cards.is_empty() {
        let body = column![
            header,
            Space::with_height(48),
            text("No memories to show yet.")
                .size(16)
                .color(p.text_secondary),
            Space::with_height(8),
            text("Memories appear once your library has at least six months of history.")
                .size(13)
                .color(p.text_tertiary),
        ]
        .align_x(Alignment::Center);

        return container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(24)
            .into();
    }

    let mut list = column![].spacing(16).padding(Padding {
        top: 8.0,
        right: 32.0,
        bottom: 32.0,
        left: 32.0,
    });
    for card in cards {
        list = list.push(wide_card(card, &p));
    }

    container(scrollable(column![header, list]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn wide_card(card: &MemoryCard, p: &colors::Palette) -> Element<'static, Message> {
    let hero_w: f32 = 280.0;
    let hero_h: f32 = 160.0;

    let hero: Element<'static, Message> = match &card.hero_thumbnail_path {
        Some(path) => iced_image(path.clone())
            .content_fit(ContentFit::Cover)
            .width(Length::Fixed(hero_w))
            .height(Length::Fixed(hero_h))
            .into(),
        None => Space::new(Length::Fixed(hero_w), Length::Fixed(hero_h)).into(),
    };

    let info = column![
        text(card.title.clone()).size(18).color(p.text_primary),
        Space::with_height(4),
        text(format!("{} photos", card.photo_count))
            .size(13)
            .color(p.text_secondary),
    ]
    .padding(16);

    let id_clone = card.id.clone();
    let border_color = p.border_subtle;
    let bg = p.bg_elevated;
    let bg_hover = p.bg_hover;

    button(row![hero, info].align_y(Alignment::Center))
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::OpenMemory(id_clone))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: Some(match s {
                button::Status::Hovered => bg_hover.into(),
                _ => bg.into(),
            }),
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Filmstrip detail for a single memory.
pub fn memory_detail_view(
    card: &MemoryCard,
    photos: &[crate::models::Photo],
    columns: usize,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let back_btn = button(text("← Back").size(13).color(p.text_primary))
        .on_press(Message::CloseMemoryDetail)
        .padding(Padding::from([8, 14]));

    let header = row![
        back_btn,
        Space::with_width(16),
        column![
            text(card.title.clone()).size(22).color(p.text_primary),
            Space::with_height(2),
            text(format!("{} photos", card.photo_count))
                .size(13)
                .color(p.text_secondary),
        ],
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([16, 24]));

    let grid = crate::components::photo_grid::photo_grid_simple(
        photos,
        160.0,
        columns,
        None,
        None,
        theme,
    );

    container(scrollable(column![header, grid]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
