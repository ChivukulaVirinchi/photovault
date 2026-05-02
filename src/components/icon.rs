//! Lucide icon font wrapper.
//!
//! All UI icons render through this module. The Lucide TTF is embedded
//! into the binary at compile time and registered as a font family
//! named `"lucide"` by `iced::application(...).font(LUCIDE_FONT_BYTES)`
//! in `main.rs`. Codepoints come from `lucide-static`'s `info.json` —
//! they live in the Unicode private-use area `U+E000..U+F8FF`.
//!
//! Adding a new icon: look up its `encodedCode` in the Lucide info.json,
//! pick a variant name on `Lucide`, and add a match arm to `glyph()`.

use iced::widget::text;
use iced::{Color, Element, Font};

use crate::app::Message;

pub const LUCIDE_FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/lucide.ttf");
pub const LUCIDE: Font = Font::with_name("lucide");

/// Named subset of the Lucide icon set. Only icons we actually use
/// belong here — keep this enum small so the call sites stay obvious.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // enum surface intentionally larger than current call sites
pub enum Lucide {
    // Navigation
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    ChevronLeft,
    ChevronRight,
    ChevronDoubleLeft,
    ChevronDoubleRight,
    // Status / state
    Check,
    Circle,
    Close,
    // Content
    Person,
    People,
    Clock,
    Folder,
    MapPin,
    Trash,
    Play,
    Pause,
    Flash,
}

impl Lucide {
    /// The single-codepoint glyph string in Lucide's private-use area.
    /// Codes follow `lucide-static`'s `info.json` — bumped by the upstream
    /// release we ship in `assets/fonts/lucide.ttf`.
    fn glyph(self) -> &'static str {
        match self {
            // Navigation
            Lucide::ArrowLeft => "\u{e048}",
            Lucide::ArrowRight => "\u{e049}",
            Lucide::ArrowUp => "\u{e04a}",
            Lucide::ArrowDown => "\u{e042}",
            Lucide::ChevronLeft => "\u{e06e}",
            Lucide::ChevronRight => "\u{e06f}",
            // Lucide ships chevrons-left / chevrons-right (plural) for double arrows.
            Lucide::ChevronDoubleLeft => "\u{e072}",
            Lucide::ChevronDoubleRight => "\u{e073}",
            // Status
            Lucide::Check => "\u{e06c}",
            Lucide::Circle => "\u{e076}",
            Lucide::Close => "\u{e1b2}",
            // Content
            Lucide::Person => "\u{e19f}",
            Lucide::People => "\u{e1a4}",
            Lucide::Clock => "\u{e087}",
            Lucide::Folder => "\u{e0d7}",
            Lucide::MapPin => "\u{e111}",
            Lucide::Trash => "\u{e18e}",
            Lucide::Play => "\u{e13c}",
            Lucide::Pause => "\u{e12e}",
            Lucide::Flash => "\u{e1b4}",
        }
    }
}

/// Render an icon at the given pixel size and color. Returns a plain
/// `text` widget bound to the Lucide font, so it composes with any
/// layout the caller already uses (rows, buttons, containers).
pub fn icon(kind: Lucide, size: u16, color: Color) -> Element<'static, Message> {
    text(kind.glyph().to_string())
        .size(size)
        .color(color)
        .font(LUCIDE)
        .into()
}
