//! Color Palette for PhotoVault
//!
//! Design Philosophy: "Editorial Dark"
//! - Deep, warm dark backgrounds that let photos breathe
//! - Soft text hierarchy — whispers, not shouts
//! - Amber accent used surgically — only for active/interactive moments
//! - Every surface is intentional: base → elevated → active

use iced::Color;

/// Background colors — warm, deep neutrals with subtle layering
pub struct Backgrounds;

impl Backgrounds {
    /// Base canvas — the deepest layer
    pub const PRIMARY: Color = Color::from_rgb(
        0x0F as f32 / 255.0,
        0x0F as f32 / 255.0,
        0x11 as f32 / 255.0,
    ); // #0F0F11

    /// Sidebar and panels — one step up
    pub const SECONDARY: Color = Color::from_rgb(
        0x16 as f32 / 255.0,
        0x16 as f32 / 255.0,
        0x19 as f32 / 255.0,
    ); // #161619

    /// Cards and elevated surfaces
    pub const ELEVATED: Color = Color::from_rgb(
        0x1E as f32 / 255.0,
        0x1E as f32 / 255.0,
        0x22 as f32 / 255.0,
    ); // #1E1E22

    /// Hover state — subtle lift
    pub const HOVER: Color = Color::from_rgb(
        0x28 as f32 / 255.0,
        0x28 as f32 / 255.0,
        0x2E as f32 / 255.0,
    ); // #28282E

    /// Selected/highlighted state
    pub const SELECTED: Color = Color::from_rgb(
        0x24 as f32 / 255.0,
        0x24 as f32 / 255.0,
        0x2A as f32 / 255.0,
    ); // #24242A

    /// Active/pressed state
    pub const ACTIVE: Color = Color::from_rgb(
        0x30 as f32 / 255.0,
        0x30 as f32 / 255.0,
        0x38 as f32 / 255.0,
    ); // #303038

    /// Surface for input fields and wells
    pub const WELL: Color = Color::from_rgb(
        0x0C as f32 / 255.0,
        0x0C as f32 / 255.0,
        0x0E as f32 / 255.0,
    ); // #0C0C0E
}

/// Text colors — warm whites with careful hierarchy
pub struct Text;

impl Text {
    /// Primary text — warm, not harsh white
    pub const PRIMARY: Color = Color::from_rgb(
        0xEC as f32 / 255.0,
        0xEC as f32 / 255.0,
        0xEA as f32 / 255.0,
    ); // #ECECEA

    /// Secondary text — soft gray
    pub const SECONDARY: Color = Color::from_rgb(
        0x8A as f32 / 255.0,
        0x8A as f32 / 255.0,
        0x8E as f32 / 255.0,
    ); // #8A8A8E

    /// Tertiary/disabled — barely there
    pub const TERTIARY: Color = Color::from_rgb(
        0x55 as f32 / 255.0,
        0x55 as f32 / 255.0,
        0x5A as f32 / 255.0,
    ); // #55555A
}

/// Accent colors — warm amber, used sparingly
pub struct Accent;

impl Accent {
    /// Primary accent — warm amber gold
    pub const PRIMARY: Color = Color::from_rgb(
        0xD4 as f32 / 255.0,
        0x9E as f32 / 255.0,
        0x3C as f32 / 255.0,
    ); // #D49E3C

    /// Accent hover — lighter amber
    pub const HOVER: Color = Color::from_rgb(
        0xE0 as f32 / 255.0,
        0xAE as f32 / 255.0,
        0x50 as f32 / 255.0,
    ); // #E0AE50

    /// Accent muted — for subtle tinted backgrounds
    pub const MUTED: Color = Color {
        r: 0xD4 as f32 / 255.0,
        g: 0x9E as f32 / 255.0,
        b: 0x3C as f32 / 255.0,
        a: 0.10,
    }; // #D49E3C at 10%
}

/// Semantic colors
pub struct Semantic;

impl Semantic {
    /// Success — muted sage green
    pub const SUCCESS: Color = Color::from_rgb(
        0x5C as f32 / 255.0,
        0xB8 as f32 / 255.0,
        0x85 as f32 / 255.0,
    ); // #5CB885

    /// Warning — soft gold
    pub const WARNING: Color = Color::from_rgb(
        0xD4 as f32 / 255.0,
        0xB0 as f32 / 255.0,
        0x6A as f32 / 255.0,
    ); // #D4B06A

    /// Danger — muted rose
    pub const DANGER: Color = Color::from_rgb(
        0xC4 as f32 / 255.0,
        0x4E as f32 / 255.0,
        0x52 as f32 / 255.0,
    ); // #C44E52
}

/// Border colors — barely visible separation
pub struct Border;

impl Border {
    /// Subtle border — the default
    pub const SUBTLE: Color = Color::from_rgb(
        0x22 as f32 / 255.0,
        0x22 as f32 / 255.0,
        0x28 as f32 / 255.0,
    ); // #222228

    /// Visible border — for interactive elements
    pub const VISIBLE: Color = Color::from_rgb(
        0x34 as f32 / 255.0,
        0x34 as f32 / 255.0,
        0x3C as f32 / 255.0,
    ); // #34343C
}
