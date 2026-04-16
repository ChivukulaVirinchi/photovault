//! Unified confirmation-dialog overlay for destructive actions.

use iced::widget::{button, column, container, mouse_area, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::state::PendingConfirmation;
use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

pub fn overlay(pending: &PendingConfirmation, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let title = pending.title().to_string();
    let body = pending.body().to_string();
    let action_label = pending.action_label().to_string();

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let border_subtle = p.border_subtle;
    let danger = p.semantic_danger;
    let danger_hover = iced::Color {
        r: (danger.r * 0.9).clamp(0.0, 1.0),
        g: (danger.g * 0.9).clamp(0.0, 1.0),
        b: (danger.b * 0.9).clamp(0.0, 1.0),
        a: 1.0,
    };

    let cancel_btn = button(text("Cancel").size(13).color(text_primary))
        .padding(Padding::from([8, 18]))
        .style(move |_t, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => bg_hover.into(),
                _ => iced::Background::Color(iced::Color::TRANSPARENT),
            }),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::CancelPending);

    let action_btn = button(text(action_label).size(13).color(iced::Color::WHITE))
        .padding(Padding::from([8, 18]))
        .style(move |_t, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => danger_hover.into(),
                _ => danger.into(),
            }),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::ConfirmPending);

    let card = container(
        column![
            text(title).size(16).color(text_primary),
            Space::with_height(10),
            text(body).size(13).color(text_secondary),
            Space::with_height(20),
            row![
                Space::with_width(Length::Fill),
                cancel_btn,
                Space::with_width(10),
                action_btn,
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(0),
    )
    .padding(24)
    .max_width(420.0)
    .style(move |_| container::Style {
        background: Some(bg_elevated.into()),
        border: iced::Border {
            color: border_subtle,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    });

    let backdrop = mouse_area(
        container(Space::new(Length::Fill, Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.55,
                })),
                ..Default::default()
            }),
    )
    .on_press(Message::CancelPending);

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    iced::widget::stack![backdrop, centered].into()
}
