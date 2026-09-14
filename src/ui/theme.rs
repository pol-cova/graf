use gpui::Rgba;

/// `gpui::rgb` is not const-compatible; these local const fns keep the
/// palette a plain table of typed constants instead of runtime values.
#[inline(always)]
const fn rgb(hex: u32) -> Rgba {
    let [_, r, g, b] = hex.to_be_bytes();
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

#[inline(always)]
const fn rgba(hex: u32) -> Rgba {
    let [r, g, b, a] = hex.to_be_bytes();
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}

pub const BG: Rgba = rgb(0x181818);
pub const BG_SURFACE: Rgba = rgb(0x1e1e1e);
pub const BG_CANVAS: Rgba = rgb(0x1a1d24);
pub const BG_BAR: Rgba = rgb(0x141414);
pub const TAB_ACTIVE: Rgba = rgb(0x181818);
pub const BORDER: Rgba = rgb(0x2b2b2b);
pub const LINE_HIGHLIGHT: Rgba = rgb(0x202020);
pub const TEXT: Rgba = rgb(0xe0e0e0);
pub const TEXT_MUTED: Rgba = rgb(0x9a9a9a);
pub const ACCENT_GREEN: Rgba = rgb(0x49aa63);
pub const ACCENT_ORANGE: Rgba = rgb(0xe59c38);
pub const ACCENT_RED: Rgba = rgb(0xe05555);
pub const ACCENT_BLUE: Rgba = rgb(0x4f8cc9);
pub const HOVER_BG: Rgba = rgb(0x292929);
pub const SELECTION: Rgba = rgba(0x264f7880);
pub const SYNTAX_COMMAND: Rgba = rgb(0x569cd6);
pub const SYNTAX_MATH: Rgba = rgb(0xdcdcaa);
pub const SYNTAX_COMMENT: Rgba = rgb(0x6a9955);
pub const SYNTAX_PUNCTUATION: Rgba = rgb(0x8a8a8a);

/// Bottom layer under a modal; lets the dimmed window show through.
pub const OVERLAY: Rgba = rgba(0x00000080);
pub const WHITE: Rgba = rgb(0xffffff);

/// Editor typography tokens: every view and paint site takes metrics from
/// here so the editor cannot drift between layout and paint.
pub const EDITOR_FONT_FAMILY: &str = "Menlo";
pub const EDITOR_FONT_SIZE: f32 = 14.0;
pub const EDITOR_LINE_HEIGHT: f32 = 23.0;

pub const GUTTER_CHAR_WIDTH: f32 = 9.0;
pub const GUTTER_BASE_WIDTH: f32 = 26.0;
pub const GUTTER_MIN_WIDTH: f32 = 48.0;
/// Gap between the right gutter edge and each painted line number.
pub const GUTTER_NUMBER_MARGIN: f32 = 10.0;

/// Width of the completion popup; `completion_anchor` clamps against the
/// same value so the popup never spills past the right edge.
pub const COMPLETION_POPUP_WIDTH: f32 = 320.0;
