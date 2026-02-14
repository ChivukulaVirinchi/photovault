//! Typography definitions for PhotoVault
//!
//! Font Stack:
//! - Display/Headers: Inter (clean, modern, highly legible)
//! - Body: Inter
//! - Monospace: JetBrains Mono (for file paths, technical info)

use iced::Font;

/// Font weights as embedded bytes
pub struct Fonts;

impl Fonts {
    /// Inter Regular (400)
    pub const INTER_REGULAR: &'static [u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");

    /// Inter Medium (500)
    pub const INTER_MEDIUM: &'static [u8] = include_bytes!("../../assets/fonts/Inter-Medium.ttf");

    /// Inter SemiBold (600)
    pub const INTER_SEMIBOLD: &'static [u8] =
        include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");

    /// JetBrains Mono Regular
    pub const JETBRAINS_MONO: &'static [u8] =
        include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
}

/// Font family definitions
pub struct FontFamily;

impl FontFamily {
    pub const INTER: Font = Font::with_name("Inter");
    pub const MONO: Font = Font::with_name("JetBrains Mono");
}

/// Text size scale (in pixels)
pub struct TextSize;

impl TextSize {
    /// Tiny labels, badges
    pub const XS: f32 = 11.0;
    /// Small captions, metadata
    pub const SM: f32 = 12.0;
    /// Body text
    pub const BASE: f32 = 14.0;
    /// Emphasized body, small headers
    pub const LG: f32 = 16.0;
    /// Section headers
    pub const XL: f32 = 20.0;
    /// Page titles
    pub const XXL: f32 = 28.0;
    /// Hero text
    pub const XXXL: f32 = 36.0;
}

/// Line heights
pub struct LineHeight;

impl LineHeight {
    pub const TIGHT: f32 = 1.2;
    pub const NORMAL: f32 = 1.5;
    pub const RELAXED: f32 = 1.75;
}
