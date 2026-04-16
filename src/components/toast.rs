//! Toast notification component for transient user feedback.

use std::time::SystemTime;

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToastAction {
    pub label: String,
    /// Boxed to break the recursive type cycle with `Message::ToastShow(Toast)`.
    pub message: Box<Message>,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub message: Option<String>,
    pub action: Option<ToastAction>,
    /// Unix millis when created. Used for auto-dismiss.
    pub created_at_ms: u128,
    /// Auto-dismiss after this many ms. 0 = sticky.
    pub ttl_ms: u128,
}

impl Toast {
    pub fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    pub fn is_expired(&self) -> bool {
        if self.ttl_ms == 0 {
            return false;
        }
        Self::now_ms().saturating_sub(self.created_at_ms) > self.ttl_ms
    }

    pub fn success(title: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: ToastKind::Success,
            title: title.into(),
            message: None,
            action: None,
            created_at_ms: Self::now_ms(),
            ttl_ms: 3000,
        }
    }

    pub fn info(title: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: ToastKind::Info,
            title: title.into(),
            message: None,
            action: None,
            created_at_ms: Self::now_ms(),
            ttl_ms: 3000,
        }
    }

    #[allow(dead_code)]
    pub fn warning(title: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: ToastKind::Warning,
            title: title.into(),
            message: None,
            action: None,
            created_at_ms: Self::now_ms(),
            ttl_ms: 5000,
        }
    }

    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: ToastKind::Error,
            title: title.into(),
            message: Some(message.into()),
            action: None,
            created_at_ms: Self::now_ms(),
            ttl_ms: 6000,
        }
    }

    pub fn with_action(mut self, label: impl Into<String>, msg: Message) -> Self {
        self.action = Some(ToastAction {
            label: label.into(),
            message: Box::new(msg),
        });
        self
    }

    #[allow(dead_code)]
    pub fn sticky(mut self) -> Self {
        self.ttl_ms = 0;
        self
    }
}

/// Render the toast stack (bottom-right corner).
pub fn toast_stack(toasts: &[Toast], theme: AppTheme) -> Element<'static, Message> {
    let mut col = column![].spacing(8);
    for t in toasts.iter().take(5) {
        col = col.push(toast_card(t, theme));
    }
    container(col)
        .padding(Padding::default().right(24.0).bottom(24.0))
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn toast_card(t: &Toast, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let accent = match t.kind {
        ToastKind::Success => p.semantic_success,
        ToastKind::Info => p.text_secondary,
        ToastKind::Warning => p.semantic_warning,
        ToastKind::Error => p.semantic_danger,
    };

    let title_text = text(t.title.clone()).size(13).color(p.text_primary);
    let mut col = column![title_text].spacing(4);
    if let Some(ref m) = t.message {
        col = col.push(text(m.clone()).size(11).color(p.text_secondary));
    }

    // Left accent bar — shown as a thin vertical strip.
    let accent_bar = container(Space::new(Length::Fixed(3.0), Length::Fixed(40.0)))
        .style(move |_| container::Style {
            background: Some(accent.into()),
            border: iced::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let mut card_row = row![accent_bar, Space::with_width(10), col,].align_y(Alignment::Center);

    if let Some(action) = &t.action {
        let action_msg = (*action.message).clone();
        let label = action.label.clone();
        let text_primary = p.text_primary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        card_row = card_row.push(Space::with_width(12)).push(
            button(text(label).size(11).color(text_primary))
                .padding(Padding::from([4, 10]))
                .style(move |_t, status| button::Style {
                    background: Some(match status {
                        button::Status::Hovered => accent_primary.into(),
                        _ => accent_muted.into(),
                    }),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(action_msg),
        );
    }

    let id = t.id;
    let text_tertiary = p.text_tertiary;
    let bg_hover = p.bg_hover;
    card_row = card_row.push(Space::with_width(8)).push(
        button(text("\u{00D7}").size(13).color(text_tertiary))
            .padding(Padding::from([4, 6]))
            .style(move |_t, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => bg_hover.into(),
                    _ => iced::Background::Color(iced::Color::TRANSPARENT),
                }),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::ToastDismiss(id)),
    );

    let bg_elevated = p.bg_elevated;
    let border_subtle = p.border_subtle;
    container(card_row)
        .padding(12)
        .width(Length::Fixed(360.0))
        .style(move |_| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}
