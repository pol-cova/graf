//! Graf visual theme system with Light, Dark, and High Contrast palettes (spec §7.2, M7).

use gpui::{Rgba, rgb};
use serde::{Deserialize, Serialize};

/// Supported visual theme modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    HighContrast,
}

impl ThemeMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Dark => "Zed Dark",
            Self::Light => "Zed Light",
            Self::HighContrast => "High Contrast",
        }
    }

    pub fn palette(&self) -> ThemePalette {
        match self {
            Self::Dark => ThemePalette::dark(),
            Self::Light => ThemePalette::light(),
            Self::HighContrast => ThemePalette::high_contrast(),
        }
    }
}

/// A complete color palette for the Graf workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePalette {
    pub bg: u32,
    pub bg_surface: u32,
    pub bg_canvas: u32,
    pub bg_bar: u32,
    pub tab_active: u32,
    pub border: u32,
    pub line_highlight: u32,
    pub text: u32,
    pub text_muted: u32,
    pub accent_green: u32,
    pub accent_orange: u32,
    pub accent_red: u32,
    pub accent_blue: u32,
    pub hover_bg: u32,
    pub syntax_command: u32,
    pub syntax_math: u32,
    pub syntax_comment: u32,
    pub syntax_punctuation: u32,
}

impl ThemePalette {
    /// The default dark palette (Zed Dark).
    pub fn dark() -> Self {
        Self {
            bg: 0x181818,
            bg_surface: 0x1e1e1e,
            bg_canvas: 0x1a1d24,
            bg_bar: 0x141414,
            tab_active: 0x181818,
            border: 0x2b2b2b,
            line_highlight: 0x202020,
            text: 0xe0e0e0,
            text_muted: 0x9a9a9a,
            accent_green: 0x49aa63,
            accent_orange: 0xe59c38,
            accent_red: 0xe05555,
            accent_blue: 0x4f8cc9,
            hover_bg: 0x292929,
            syntax_command: 0x569cd6,
            syntax_math: 0xdcdcaa,
            syntax_comment: 0x6a9955,
            syntax_punctuation: 0x8a8a8a,
        }
    }

    /// The light palette (Zed Light).
    pub fn light() -> Self {
        Self {
            bg: 0xfafafa,
            bg_surface: 0xf0f0f0,
            bg_canvas: 0xf5f5f7,
            bg_bar: 0xe6e6e6,
            tab_active: 0xfafafa,
            border: 0xd0d0d0,
            line_highlight: 0xeaeaea,
            text: 0x24292e,
            text_muted: 0x6a737d,
            accent_green: 0x22863a,
            accent_orange: 0xd73a49,
            accent_red: 0xcb2431,
            accent_blue: 0x005cc5,
            hover_bg: 0xe1e4e8,
            syntax_command: 0x005cc5,
            syntax_math: 0x6f42c1,
            syntax_comment: 0x6a737d,
            syntax_punctuation: 0x444d56,
        }
    }

    /// The high contrast accessibility palette.
    pub fn high_contrast() -> Self {
        Self {
            bg: 0x000000,
            bg_surface: 0x0d0d0d,
            bg_canvas: 0x050505,
            bg_bar: 0x000000,
            tab_active: 0x1a1a1a,
            border: 0xffffff,
            line_highlight: 0x262626,
            text: 0xffffff,
            text_muted: 0xcccccc,
            accent_green: 0x00ff66,
            accent_orange: 0xffaa00,
            accent_red: 0xff3333,
            accent_blue: 0x3399ff,
            hover_bg: 0x333333,
            syntax_command: 0x66b2ff,
            syntax_math: 0xffff66,
            syntax_comment: 0x80ff80,
            syntax_punctuation: 0xffffff,
        }
    }
}

// Canonical dark palette color constants for backward compatibility
pub const BG: u32 = 0x181818;
pub const BG_SURFACE: u32 = 0x1e1e1e;
pub const BG_CANVAS: u32 = 0x1a1d24;
pub const BG_BAR: u32 = 0x141414;
pub const TAB_ACTIVE: u32 = 0x181818;
pub const BORDER: u32 = 0x2b2b2b;
pub const LINE_HIGHLIGHT: u32 = 0x202020;
pub const TEXT: u32 = 0xe0e0e0;
pub const TEXT_MUTED: u32 = 0x9a9a9a;
pub const ACCENT_GREEN: u32 = 0x49aa63;
pub const ACCENT_ORANGE: u32 = 0xe59c38;
pub const ACCENT_RED: u32 = 0xe05555;
pub const ACCENT_BLUE: u32 = 0x4f8cc9;
pub const HOVER_BG: u32 = 0x292929;
pub const SELECTION: u32 = 0x264f7880;
pub const SYNTAX_COMMAND: u32 = 0x569cd6;
pub const SYNTAX_MATH: u32 = 0xdcdcaa;
pub const SYNTAX_COMMENT: u32 = 0x6a9955;
pub const SYNTAX_PUNCTUATION: u32 = 0x8a8a8a;

/// Converts a `u32` hex colour constant to a GPUI [`Rgba`].
#[inline(always)]
pub fn color(hex: u32) -> Rgba {
    rgb(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_palettes() {
        let dark = ThemeMode::Dark.palette();
        assert_eq!(dark.bg, BG);
        assert_eq!(dark.text, TEXT);

        let light = ThemeMode::Light.palette();
        assert_ne!(light.bg, dark.bg);
        assert_eq!(light.bg, 0xfafafa);

        let hc = ThemeMode::HighContrast.palette();
        assert_eq!(hc.bg, 0x000000);
        assert_eq!(hc.border, 0xffffff);
    }
}
