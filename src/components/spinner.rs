//! Animated loading spinner using braille glyphs.

use iced::widget::{row, text, Space};
use iced::{Alignment, Element};

use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

const FRAMES: &[&str] = &[
    "\u{280B}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283C}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280F}",
];

pub fn spinner_with_label(phase: u32, label: &str, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let frame = FRAMES[(phase as usize) % FRAMES.len()];
    row![
        text(frame.to_string()).size(14).color(p.text_secondary),
        Space::with_width(10),
        text(label.to_string()).size(13).color(p.text_secondary),
    ]
    .align_y(Alignment::Center)
    .into()
}
