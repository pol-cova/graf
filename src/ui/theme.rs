use gpui::{Rgba, rgb};

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

#[inline(always)]
pub fn color(hex: u32) -> Rgba {
    rgb(hex)
}
