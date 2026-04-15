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
pub fn memories_banner(cards: &[MemoryCard], theme: AppTheme) -> Option<Element<'static, Message>> {
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
        scrollable(strip).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(6),
        )),
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

    let inner = column![hero, container(caption).padding(Padding::from([8, 12])),].spacing(0);

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
pub fn memories_view(cards: &[MemoryCard], theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let header = container(text("Memories").size(28).color(p.text_primary)).padding(Padding {
        top: 24.0,
        right: 32.0,
        bottom: 8.0,
        left: 32.0,
    });

    if cards.is_empty() {
        let body = column![
            header,
            Space::with_height(48),
            text("Nothing to show right now.")
                .size(16)
                .color(p.text_secondary),
            Space::with_height(8),
            text("Memories appear when your library has photos from prior years. Newer libraries (under six months old) are skipped silently.")
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

/// Slideshow detail for a single memory. Shows one photo at a time
/// big-and-centered, auto-advancing every 4 seconds (toggleable).
pub fn memory_detail_view(
    card: &MemoryCard,
    photos: &[crate::models::Photo],
    drive_path: Option<&std::path::Path>,
    current_index: usize,
    paused: bool,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let total = photos.len();

    let back_btn = button(text("← Back").size(13).color(p.text_primary))
        .on_press(Message::CloseMemoryDetail)
        .padding(Padding::from([8, 14]));

    let counter_text = if total == 0 {
        "0 / 0".to_string()
    } else {
        format!("{} / {}", current_index + 1, total)
    };

    let header = row![
        back_btn,
        Space::with_width(16),
        column![
            text(card.title.clone()).size(22).color(p.text_primary),
            Space::with_height(2),
            text(counter_text).size(12).color(p.text_secondary),
        ],
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([16, 24]));

    if total == 0 {
        let body = column![
            header,
            Space::with_height(64),
            text("No photos in this memory.")
                .size(14)
                .color(p.text_secondary),
        ]
        .align_x(Alignment::Center);
        return container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    // Prefer original file for the slideshow; iced_image handles
    // downscaling. Fall back to thumbnail if original is unavailable.
    let current = &photos[current_index.min(total - 1)];
    let abs_path: Option<String> = drive_path
        .map(|root| root.join(&current.file_path))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| current.thumbnail_path.clone());

    let hero: Element<'static, Message> = match abs_path {
        Some(path) => container(
            iced_image(path)
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into(),
        None => container(text("(image unavailable)").color(p.text_tertiary))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
    };

    let prev_btn = button(text("◀").size(20).color(p.text_primary))
        .on_press(Message::MemorySlideshowPrev)
        .padding(Padding::from([10, 16]));

    let pause_label = if paused { "▶ Play" } else { "❚❚ Pause" };
    let pause_btn = button(text(pause_label).size(13).color(p.text_primary))
        .on_press(Message::MemorySlideshowTogglePause)
        .padding(Padding::from([10, 18]));

    let next_btn = button(text("\u{25B6}").size(20).color(p.text_primary))
        .on_press(Message::MemorySlideshowNext)
        .padding(Padding::from([10, 16]));

    let save_album_btn = button(text("Save as Album").size(12).color(p.text_primary))
        .on_press(Message::SaveMemoryAsAlbum)
        .padding(Padding::from([8, 14]));

    let controls = container(
        row![
            prev_btn,
            Space::with_width(12),
            pause_btn,
            Space::with_width(12),
            next_btn,
            Space::with_width(24),
            save_album_btn,
        ]
        .align_y(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding(Padding::from([12, 0]));

    container(column![header, hero, controls])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
