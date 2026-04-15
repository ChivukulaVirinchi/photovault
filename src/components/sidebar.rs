//! Navigation sidebar component
//!
//! A refined, slim sidebar with accent-bar active indicators.
//! Design: clean vertical navigation, generous spacing, whisper-quiet labels.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length, Padding};

use crate::app::{Message, View};
use crate::config::AppTheme;
use crate::theme::colors;

/// Sidebar navigation component
pub struct Sidebar;

impl Sidebar {
    /// Render the sidebar. `review_pending` is the face-review queue size and
    /// drives the badge on the "Review Faces" entry.
    pub fn view(
        current_view: &View,
        app_theme: AppTheme,
        review_pending: i64,
    ) -> Element<'static, Message> {
        let p = colors::palette(app_theme);

        let hover_bg = p.bg_hover;
        let tc = p.text_tertiary;
        let collapse_btn = button(
            text("\u{00AB}").size(14).color(tc), // « double left arrow
        )
        .padding(Padding::from([2, 6]))
        .style(move |_t: &iced::Theme, s| button::Style {
            background: match s {
                button::Status::Hovered => Some(hover_bg.into()),
                _ => None,
            },
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::ToggleSidebar);

        let brand = container(
            row![
                text("PhotoVault").size(13).color(p.text_secondary),
                Space::with_width(Length::Fill),
                collapse_btn,
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding(Padding::from([24, 16]));

        let review_label = if review_pending > 0 {
            format!("Review Faces · {}", review_pending)
        } else {
            "Review Faces".to_string()
        };

        let nav_items = column![
            Self::nav_button("Timeline", View::Timeline, current_view, app_theme),
            Self::nav_button("Map", View::Map, current_view, app_theme),
            Self::nav_button("Memories", View::Memories, current_view, app_theme),
            Self::nav_button("Albums", View::Albums, current_view, app_theme),
            Self::nav_button("Search", View::Search, current_view, app_theme),
            Space::with_height(12),
            Self::group_header("People", app_theme),
            Self::nav_button("All People", View::People, current_view, app_theme),
            Self::nav_button(&review_label, View::FaceReview, current_view, app_theme),
            Space::with_height(12),
            Self::group_header("Cleanup", app_theme),
            Self::nav_button("Duplicates", View::Duplicates, current_view, app_theme),
            Self::nav_button("Bursts", View::Bursts, current_view, app_theme),
            Self::nav_button("Trash", View::Trash, current_view, app_theme),
            Space::with_height(12),
            Self::group_header("Library", app_theme),
            Self::nav_button("Documents", View::Documents, current_view, app_theme),
        ]
        .spacing(2)
        .padding(Padding::from([0, 8]));

        let settings = container(Self::nav_button(
            "Settings",
            View::Settings,
            current_view,
            app_theme,
        ))
        .padding(Padding::from([0, 8]));

        let content = column![
            brand,
            nav_items,
            Space::with_height(Length::Fill),
            settings,
            Space::with_height(16),
        ];

        let bg = p.bg_secondary;
        let border_color = p.border_subtle;

        container(content)
            .width(Length::Fixed(180.0))
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    color: border_color,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Small caps-ish section header between nav groups.
    fn group_header(label: &str, app_theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(app_theme);
        container(text(label.to_uppercase()).size(10).color(p.text_tertiary))
            .padding(Padding::from([4, 15]))
            .into()
    }

    /// Create a navigation button with accent bar for active state
    fn nav_button(
        label: &str,
        target: View,
        current: &View,
        app_theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(app_theme);

        let is_active = std::mem::discriminant(&target) == std::mem::discriminant(current)
            || (matches!(target, View::People) && matches!(current, View::ClusterDetail))
            || (matches!(target, View::Duplicates) && matches!(current, View::DuplicateDetail))
            || (matches!(target, View::Bursts) && matches!(current, View::BurstDetail))
            || (matches!(target, View::Search) && matches!(current, View::Cull))
            || (matches!(target, View::Albums) && matches!(current, View::AlbumDetail));

        let label_color = if is_active {
            p.text_primary
        } else {
            p.text_secondary
        };

        // Accent bar: 3px wide amber strip on the left for active items
        let accent_bar = container(Space::new(3, Length::Fill))
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: if is_active {
                    Some(colors::DARK.accent_primary.into())
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

        let bg_elevated = p.bg_elevated;
        let bg_hover = p.bg_hover;
        let bg_active = p.bg_active;

        button(inner)
            .padding(0)
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, status| {
                let background = match status {
                    button::Status::Active if is_active => Some(bg_elevated.into()),
                    button::Status::Active => None,
                    button::Status::Hovered => Some(bg_hover.into()),
                    button::Status::Pressed => Some(bg_active.into()),
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
