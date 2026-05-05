//! Modal preview for album suggestions — lets the user peek at the
//! thumbnails before deciding to accept or dismiss the suggestion.

use iced::widget::{button, column, container, image as iced_image, row, scrollable, text, Space};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::AlbumSuggestionRecord;
use crate::theme::colors;

/// Render the modal overlay. Returns `None` when there's no active
/// preview, so the caller can `if let Some(o) = …` and stack it.
pub fn preview_modal(
    suggestion: Option<&AlbumSuggestionRecord>,
    thumbs: &[String],
    theme: AppTheme,
) -> Option<Element<'static, Message>> {
    let sug = suggestion?;
    let p = colors::palette(theme);
    let sug_id = sug.id;
    let title = sug.title.clone();
    let count = sug.photo_count();

    let header = column![
        text(title).size(18).color(p.text_primary),
        text(format!("{} photos", count))
            .size(12)
            .color(p.text_secondary),
    ]
    .spacing(4);

    // Horizontal thumbnail strip — shows up to ~40 photos so even a
    // big trip suggestion is browseable without scrolling forever.
    let mut strip = row![].spacing(8);
    for path in thumbs.iter() {
        strip = strip.push(
            container(
                iced_image(path.clone())
                    .content_fit(ContentFit::Cover)
                    .width(Length::Fixed(140.0))
                    .height(Length::Fixed(140.0)),
            )
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(p.bg_elevated.into()),
                border: iced::Border {
                    color: p.border_subtle,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );
    }

    let strip_or_loading: Element<'static, Message> = if thumbs.is_empty() {
        text("Loading photos…")
            .size(12)
            .color(p.text_secondary)
            .into()
    } else {
        scrollable(strip)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(6),
            ))
            .width(Length::Fill)
            .into()
    };

    let bg_hover = p.bg_hover;
    let bg_el = p.bg_elevated;
    let border = p.border_subtle;
    let accent = p.accent_primary;
    let tc_sec = p.text_secondary;

    let accept_btn = button(text("Save as album").size(13).color(accent))
        .padding(Padding::from([8, 16]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover.into()),
                _ => Some(bg_el.into()),
            },
            border: iced::Border {
                color: border,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::BeginAcceptSuggestion(sug_id));

    let dismiss_btn = button(text("Not for me").size(13).color(tc_sec))
        .padding(Padding::from([8, 16]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(bg_hover.into()),
                _ => None,
            },
            border: iced::Border {
                color: border,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::DismissSuggestion(sug_id));

    let close_btn = button(text("Close").size(13).color(tc_sec))
        .padding(Padding::from([8, 16]))
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
        .on_press(Message::ClosePreviewSuggestion);

    let actions = row![
        accept_btn,
        dismiss_btn,
        Space::with_width(Length::Fill),
        close_btn
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let card = container(
        column![
            header,
            Space::with_height(12),
            strip_or_loading,
            Space::with_height(16),
            actions
        ]
        .spacing(0),
    )
    .width(Length::Fixed(800.0))
    .padding(Padding::from(20))
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(p.bg_secondary.into()),
        border: iced::Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    });

    // Centred over a darkened backdrop. Clicking the backdrop also
    // closes the modal.
    let backdrop = button(Space::new(Length::Fill, Length::Fill))
        .style(move |_t: &iced::Theme, _s| button::Style {
            background: Some(
                iced::Color {
                    a: 0.6,
                    ..iced::Color::BLACK
                }
                .into(),
            ),
            ..Default::default()
        })
        .on_press(Message::ClosePreviewSuggestion);

    let centred = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    Some(
        iced::widget::stack![backdrop, centred]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}
