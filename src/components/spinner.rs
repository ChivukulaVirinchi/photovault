//! Animated loading spinner — rotating Lucide loader-circle SVG.
//!
//! `phase` is the subscription tick counter (~8 fps). We map it to a
//! rotation angle so the spinner spins smoothly without depending on
//! a font's glyph coverage. The SVG itself is inlined as bytes so
//! there's no asset-loading hop and no possibility of missing files.

use iced::widget::{row, svg, text, Space};
use iced::{Alignment, Element, Length, Rotation};
use std::sync::OnceLock;

use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

// Lucide loader-circle (single-line spinner, MIT-licensed asset). The
// `currentColor` stroke is overridden by the iced svg color filter.
const SPINNER_SVG_BYTES: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#888888" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>"##;

fn spinner_handle() -> svg::Handle {
    static H: OnceLock<svg::Handle> = OnceLock::new();
    H.get_or_init(|| svg::Handle::from_memory(SPINNER_SVG_BYTES.to_vec()))
        .clone()
}

pub fn spinner_with_label(phase: u32, label: &str, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    // 8 fps subscription * ~45° per tick = full rotation in ~1 s.
    let radians = (phase as f32) * (std::f32::consts::TAU / 8.0);
    let secondary = p.text_secondary;

    let glyph = svg(spinner_handle())
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .rotation(Rotation::Floating(iced::Radians(radians)))
        .style(move |_t: &iced::Theme, _s| svg::Style {
            color: Some(secondary),
        });

    row![
        glyph,
        Space::with_width(10),
        text(label.to_string()).size(13).color(p.text_secondary),
    ]
    .align_y(Alignment::Center)
    .into()
}
