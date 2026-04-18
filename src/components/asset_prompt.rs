use iced::widget::{button, column, container, mouse_area, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::bootstrap::AssetHealth;
use crate::config::AppTheme;
use crate::theme::colors;

pub fn overlay(
    health: &AssetHealth,
    install_in_progress: bool,
    install_error: Option<&str>,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let title = text("Install optional AI assets?")
        .size(20)
        .color(p.text_primary);

    let details = text(format!(
        "PhotoVault can run without them, but People and map geocoding need extra files. {}",
        health.summary()
    ))
    .size(13)
    .color(p.text_secondary);

    let accent_primary = p.accent_primary;
    let accent_hover = p.accent_hover;
    let text_primary = p.text_primary;
    let border_subtle = p.border_subtle;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let error_color = p.semantic_danger;

    let install_label = if install_in_progress {
        "Installing..."
    } else {
        "Install Assets"
    };

    let install_btn = if install_in_progress {
        button(text(install_label).size(13).color(text_primary))
            .padding(Padding::from([9, 18]))
            .style(move |_t, _status| button::Style {
                background: Some(bg_hover.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
    } else {
        button(text(install_label).size(13).color(iced::Color::WHITE))
            .padding(Padding::from([9, 18]))
            .style(move |_t, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => accent_hover.into(),
                    _ => accent_primary.into(),
                }),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::InstallAssetPack)
    };

    let later_btn = button(text("Not now").size(13).color(text_primary))
        .padding(Padding::from([9, 18]))
        .style(move |_t, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => bg_hover.into(),
                _ => iced::Color::TRANSPARENT.into(),
            }),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::DismissAssetInstallPrompt);

    let mut card_col = column![
        title,
        Space::with_height(10),
        details,
        Space::with_height(18),
        row![later_btn, Space::with_width(10), install_btn].align_y(Alignment::Center),
    ]
    .spacing(0);

    if let Some(err) = install_error {
        card_col = card_col.push(Space::with_height(12)).push(
            text(format!("Install failed: {}", err))
                .size(12)
                .color(error_color),
        );
    }

    let card = container(card_col)
        .padding(22)
        .max_width(520.0)
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
    .on_press(Message::DismissAssetInstallPrompt);

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    iced::widget::stack![backdrop, centered].into()
}
