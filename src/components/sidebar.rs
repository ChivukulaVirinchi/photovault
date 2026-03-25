//! Navigation sidebar component
//!
//! A refined, minimal sidebar with icon-based navigation.
//! Aesthetic: Clean vertical bar with subtle hover states.

use iced::widget::{button, column, container, text, Space};
use iced::{Element, Length, Padding};

use crate::app::{Message, View};
use crate::config::AppTheme;
use crate::theme::colors::{Backgrounds, Border, Text};

/// Sidebar navigation component
pub struct Sidebar;

impl Sidebar {
    /// Render the sidebar
    pub fn view(current_view: &View, app_theme: AppTheme) -> Element<'static, Message> {
        let nav_items = column![
            Self::nav_button("Timeline", View::Timeline, current_view, app_theme),
            Self::nav_button("People", View::People, current_view, app_theme),
            Self::nav_button("Duplicates", View::Duplicates, current_view, app_theme),
            Self::nav_button("Bursts", View::Bursts, current_view, app_theme),
            Self::nav_button("Search", View::Search, current_view, app_theme),
            Self::nav_button("Trash", View::Trash, current_view, app_theme),
            Space::with_height(Length::Fill),
            Self::nav_button("Settings", View::Settings, current_view, app_theme),
        ]
        .spacing(4)
        .padding(Padding::from([16, 8]));

        container(nav_items)
            .width(Length::Fixed(200.0))
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(
                    if matches!(app_theme, AppTheme::Light) {
                        iced::Color::from_rgb(0.96, 0.96, 0.96)
                    } else {
                        Backgrounds::SECONDARY
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if matches!(app_theme, AppTheme::Light) {
                        iced::Color::from_rgb(0.84, 0.84, 0.84)
                    } else {
                        Border::SUBTLE
                    },
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Create a navigation button
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
                iced::Color::from_rgb(0.15, 0.15, 0.15)
            } else {
                iced::Color::from_rgb(0.35, 0.35, 0.35)
            }
        } else if is_active {
            Text::PRIMARY
        } else {
            Text::SECONDARY
        };

        let btn = button(text(label.to_owned()).size(14).color(label_color))
            .padding(Padding::from([10, 12]))
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, status| {
                let background = match status {
                    button::Status::Active if is_active => Some(
                        if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.88, 0.88, 0.88)
                        } else {
                            Backgrounds::ACTIVE
                        }
                        .into(),
                    ),
                    button::Status::Active => None,
                    button::Status::Hovered => Some(
                        if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.92, 0.92, 0.92)
                        } else {
                            Backgrounds::HOVER
                        }
                        .into(),
                    ),
                    button::Status::Pressed => Some(
                        if matches!(app_theme, AppTheme::Light) {
                            iced::Color::from_rgb(0.88, 0.88, 0.88)
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
            .on_press(Message::NavigateTo(target));

        btn.into()
    }
}
