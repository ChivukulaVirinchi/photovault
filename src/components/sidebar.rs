//! Navigation sidebar component
//!
//! A refined, slim sidebar with accent-bar active indicators.
//! Design: clean vertical navigation, generous spacing, whisper-quiet labels.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length, Padding};

use crate::app::{Message, View};
use crate::config::AppTheme;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Sidebar navigation component
pub struct Sidebar;

impl Sidebar {
    /// Render the sidebar
    pub fn view(current_view: &View, app_theme: AppTheme) -> Element<'static, Message> {
        let brand = container(
            text("PhotoVault")
                .size(13)
                .color(Text::SECONDARY),
        )
        .padding(Padding::from([24, 20]));

        let nav_items = column![
            Self::nav_button("Timeline", View::Timeline, current_view, app_theme),
            Self::nav_button("People", View::People, current_view, app_theme),
            Self::nav_button("Duplicates", View::Duplicates, current_view, app_theme),
            Self::nav_button("Bursts", View::Bursts, current_view, app_theme),
            Self::nav_button("Search", View::Search, current_view, app_theme),
            Self::nav_button("Trash", View::Trash, current_view, app_theme),
        ]
        .spacing(2)
        .padding(Padding::from([0, 8]));

        let settings = container(
            Self::nav_button("Settings", View::Settings, current_view, app_theme),
        )
        .padding(Padding::from([0, 8]));

        let content = column![
            brand,
            nav_items,
            Space::with_height(Length::Fill),
            settings,
            Space::with_height(16),
        ];

        container(content)
            .width(Length::Fixed(180.0))
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| {
                let bg = if matches!(app_theme, AppTheme::Light) {
                    iced::Color::from_rgb(0.96, 0.96, 0.96)
                } else {
                    Backgrounds::SECONDARY
                };
                container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        color: if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.88, 0.88, 0.88)
                        } else {
                            Border::SUBTLE
                        },
                        width: 0.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into()
    }

    /// Create a navigation button with accent bar for active state
    fn nav_button(
        label: &str,
        target: View,
        current: &View,
        app_theme: AppTheme,
    ) -> Element<'static, Message> {
        let is_active = std::mem::discriminant(&target) == std::mem::discriminant(current)
            || (matches!(target, View::People) && matches!(current, View::ClusterDetail))
            || (matches!(target, View::Duplicates) && matches!(current, View::DuplicateDetail))
            || (matches!(target, View::Bursts) && matches!(current, View::BurstDetail))
            || (matches!(target, View::Search) && matches!(current, View::Cull));

        let label_color = if matches!(app_theme, AppTheme::Light) {
            if is_active {
                iced::Color::from_rgb(0.1, 0.1, 0.1)
            } else {
                iced::Color::from_rgb(0.45, 0.45, 0.45)
            }
        } else if is_active {
            Text::PRIMARY
        } else {
            Text::SECONDARY
        };

        // Accent bar: 3px wide amber strip on the left for active items
        let accent_bar = container(Space::new(3, Length::Fill))
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: if is_active {
                    Some(Accent::PRIMARY.into())
                } else {
                    None
                },
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        let label_text = text(label.to_owned()).size(13).color(label_color);

        let inner = row![
            accent_bar,
            Space::with_width(if is_active { 12 } else { 15 }),
            label_text,
        ]
        .align_y(iced::Alignment::Center)
        .height(36);

        button(inner)
            .padding(0)
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, status| {
                let background = match status {
                    button::Status::Active if is_active => Some(
                        if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.92, 0.92, 0.92)
                        } else {
                            Backgrounds::ELEVATED
                        }
                        .into(),
                    ),
                    button::Status::Active => None,
                    button::Status::Hovered => Some(
                        if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.94, 0.94, 0.94)
                        } else {
                            Backgrounds::HOVER
                        }
                        .into(),
                    ),
                    button::Status::Pressed => Some(
                        if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.90, 0.90, 0.90)
                        } else {
                            Backgrounds::ACTIVE
                        }
                        .into(),
                    ),
                    button::Status::Disabled => None,
                };

                button::Style {
                    background,
                    text_color: label_color,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::NavigateTo(target))
            .into()
    }
}
