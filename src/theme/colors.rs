//! Color Palette for PhotoVault
//!
//! Design Philosophy:
//! - Dark theme optimized for photo viewing (photos should pop)
//! - Warm neutral background (not pure black, not cold gray)
//! - Subtle accent color for interactive elements
//! - High contrast for text readability

use iced::Color;

/// Background colors - warm dark neutrals
pub struct Backgrounds;

impl Backgrounds {
    /// Main app background - warm charcoal
    pub const PRIMARY: Color = Color::from_rgb(
        0x12 as f32 / 255.0,
        0x12 as f32 / 255.0,
        0x14 as f32 / 255.0,
    ); // #121214

    /// Sidebar and panels - slightly lighter
    pub const SECONDARY: Color = Color::from_rgb(
        0x1A as f32 / 255.0,
        0x1A as f32 / 255.0,
        0x1E as f32 / 255.0,
    ); // #1A1A1E

    /// Cards and elevated surfaces
    pub const ELEVATED: Color = Color::from_rgb(
        0x24 as f32 / 255.0,
        0x24 as f32 / 255.0,
        0x2A as f32 / 255.0,
    ); // #24242A

    /// Hover states
    pub const HOVER: Color = Color::from_rgb(
        0x2E as f32 / 255.0,
        0x2E as f32 / 255.0,
        0x36 as f32 / 255.0,
    ); // #2E2E36

    /// Selected/highlighted states (between ELEVATED and ACTIVE)
    pub const SELECTED: Color = Color::from_rgb(
        0x2A as f32 / 255.0,
        0x2A as f32 / 255.0,
        0x32 as f32 / 255.0,
    ); // #2A2A32

    /// Selected/Active states
    pub const ACTIVE: Color = Color::from_rgb(
        0x38 as f32 / 255.0,
        0x38 as f32 / 255.0,
        0x42 as f32 / 255.0,
    ); // #383842
}

/// Text colors - warm whites and grays
pub struct Text;

impl Text {
    /// Primary text - warm white
    pub const PRIMARY: Color = Color::from_rgb(
        0xF5 as f32 / 255.0,
        0xF5 as f32 / 255.0,
        0xF3 as f32 / 255.0,
    ); // #F5F5F3

    /// Secondary text - muted
    pub const SECONDARY: Color = Color::from_rgb(
        0xA0 as f32 / 255.0,
        0xA0 as f32 / 255.0,
        0x9C as f32 / 255.0,
    ); // #A0A09C

    /// Tertiary/Disabled text
    pub const TERTIARY: Color = Color::from_rgb(
        0x6B as f32 / 255.0,
        0x6B as f32 / 255.0,
        0x67 as f32 / 255.0,
    ); // #6B6B67
}

/// Accent colors - warm amber/gold
pub struct Accent;

impl Accent {
    /// Primary accent - warm amber
    pub const PRIMARY: Color = Color::from_rgb(
        0xE5 as f32 / 255.0,
        0xA8 as f32 / 255.0,
        0x3B as f32 / 255.0,
    ); // #E5A83B

    /// Accent hover - lighter
    pub const HOVER: Color = Color::from_rgb(
        0xF0 as f32 / 255.0,
        0xB8 as f32 / 255.0,
        0x4B as f32 / 255.0,
    ); // #F0B84B

    /// Accent muted - for subtle highlights
    pub const MUTED: Color = Color {
        r: 0xE5 as f32 / 255.0,
        g: 0xA8 as f32 / 255.0,
        b: 0x3B as f32 / 255.0,
        a: 0.15,
    }; // #E5A83B at 15%
}

/// Semantic colors
pub struct Semantic;

impl Semantic {
    /// Success - muted green
    pub const SUCCESS: Color = Color::from_rgb(
        0x4A as f32 / 255.0,
        0xB8 as f32 / 255.0,
        0x7D as f32 / 255.0,
    ); // #4AB87D

    /// Warning - warm yellow
    pub const WARNING: Color = Color::from_rgb(
        0xE5 as f32 / 255.0,
        0xC0 as f32 / 255.0,
        0x7B as f32 / 255.0,
    ); // #E5C07B

    /// Error - muted red
    pub const ERROR: Color = Color::from_rgb(
        0xE0 as f32 / 255.0,
        0x6C as f32 / 255.0,
        0x75 as f32 / 255.0,
    ); // #E06C75
}

/// Border colors
pub struct Border;

impl Border {
    /// Subtle border for separation
    pub const SUBTLE: Color = Color::from_rgb(
        0x2A as f32 / 255.0,
        0x2A as f32 / 255.0,
        0x30 as f32 / 255.0,
    ); // #2A2A30

    /// Visible border for interactive elements
    pub const VISIBLE: Color = Color::from_rgb(
        0x3A as f32 / 255.0,
        0x3A as f32 / 255.0,
        0x42 as f32 / 255.0,
    ); // #3A3A42
}

/// Helper trait for color manipulation
pub trait ColorExt {
    fn scale_alpha(self, factor: f32) -> Color;
}

impl ColorExt for Color {
    fn scale_alpha(self, factor: f32) -> Color {
        Color {
            a: self.a * factor,
            ..self
        }
    }
}
