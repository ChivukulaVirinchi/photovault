//! Color Palette for PhotoVault
//!
//! Design Philosophy: "Editorial Dark" + Clean Light
//! - Deep, warm dark backgrounds that let photos breathe
//! - Soft text hierarchy — whispers, not shouts
//! - Amber accent used surgically — only for active/interactive moments
//! - Every surface is intentional: base → elevated → active
//! - Light theme mirrors the hierarchy with warm paper tones

use iced::Color;

use crate::config::AppTheme;

/// Complete color palette for a theme variant
pub struct Palette {
    // Backgrounds
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub bg_selected: Color,
    pub bg_active: Color,

    // Text
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,

    // Accent
    pub accent_primary: Color,
    pub accent_hover: Color,
    pub accent_muted: Color,

    // Semantic
    pub semantic_success: Color,
    pub semantic_warning: Color,
    pub semantic_danger: Color,

    // Border
    pub border_subtle: Color,
    pub border_visible: Color,
}

/// Helper: build an RGB color from hex bytes
const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// Helper: build an RGBA color from hex bytes + alpha
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

// ── Dark Palette ───────────────────────────────────────────────

pub static DARK: Palette = Palette {
    bg_primary:    rgb(0x0F, 0x0F, 0x11), // #0F0F11
    bg_secondary:  rgb(0x16, 0x16, 0x19), // #161619
    bg_elevated:   rgb(0x1E, 0x1E, 0x22), // #1E1E22
    bg_hover:      rgb(0x28, 0x28, 0x2E), // #28282E
    bg_selected:   rgb(0x24, 0x24, 0x2A), // #24242A
    bg_active:     rgb(0x30, 0x30, 0x38), // #303038


    text_primary:   rgb(0xEC, 0xEC, 0xEA), // #ECECEA
    text_secondary: rgb(0x8A, 0x8A, 0x8E), // #8A8A8E
    text_tertiary:  rgb(0x55, 0x55, 0x5A), // #55555A

    accent_primary: rgb(0xD4, 0x9E, 0x3C), // #D49E3C
    accent_hover:   rgb(0xE0, 0xAE, 0x50), // #E0AE50
    accent_muted:   rgba(0xD4, 0x9E, 0x3C, 0.10),

    semantic_success: rgb(0x5C, 0xB8, 0x85), // #5CB885
    semantic_warning: rgb(0xD4, 0xB0, 0x6A), // #D4B06A
    semantic_danger:  rgb(0xC4, 0x4E, 0x52), // #C44E52

    border_subtle:  rgb(0x22, 0x22, 0x28), // #222228
    border_visible: rgb(0x34, 0x34, 0x3C), // #34343C
};

// ── Light Palette ──────────────────────────────────────────────

pub static LIGHT: Palette = Palette {
    bg_primary:    rgb(0xFA, 0xFA, 0xF9), // #FAFAF9 warm white
    bg_secondary:  rgb(0xF2, 0xF2, 0xF0), // #F2F2F0 sidebar panels
    bg_elevated:   rgb(0xFF, 0xFF, 0xFF), // #FFFFFF cards
    bg_hover:      rgb(0xEB, 0xEB, 0xE8), // #EBEBE8
    bg_selected:   rgb(0xE5, 0xE5, 0xE2), // #E5E5E2
    bg_active:     rgb(0xDF, 0xDF, 0xDC), // #DFDFDC


    text_primary:   rgb(0x1A, 0x1A, 0x1C), // #1A1A1C
    text_secondary: rgb(0x6B, 0x6B, 0x70), // #6B6B70
    text_tertiary:  rgb(0x9A, 0x9A, 0x9F), // #9A9A9F

    accent_primary: rgb(0xB8, 0x86, 0x2D), // #B8862D deeper amber for contrast
    accent_hover:   rgb(0xA0, 0x74, 0x22), // #A07422
    accent_muted:   rgba(0xB8, 0x86, 0x2D, 0.08),

    semantic_success: rgb(0x2E, 0x8B, 0x57), // #2E8B57
    semantic_warning: rgb(0xB8, 0x86, 0x2D), // #B8862D
    semantic_danger:  rgb(0xC0, 0x39, 0x2B), // #C0392B

    border_subtle:  rgb(0xE0, 0xE0, 0xDD), // #E0E0DD
    border_visible: rgb(0xCC, 0xCC, 0xC8), // #CCCCC8
};

/// Get the correct palette for the given theme
pub fn palette(theme: AppTheme) -> &'static Palette {
    match theme {
        AppTheme::Dark => &DARK,
        AppTheme::Light | AppTheme::System => &LIGHT,
    }
}

