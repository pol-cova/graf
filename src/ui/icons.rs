use gpui::{Rgba, Styled, Svg, svg};

use super::theme;

#[derive(Clone, Copy)]
pub enum Icon {
    PanelLeft,
    PanelRight,
    PanelBottom,
    MoreHorizontal,
    Close,
    ChevronUp,
    ChevronDown,
    ChevronRight,
    Minus,
    Plus,
    Alert,
}

pub fn icon(kind: Icon) -> Svg {
    icon_colored(kind, theme::color(theme::TEXT_MUTED))
}

pub fn icon_colored(kind: Icon, color: Rgba) -> Svg {
    let data: &'static [u8] = match kind {
        Icon::PanelLeft => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/></svg>"#,
        Icon::PanelRight => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/></svg>"#,
        Icon::PanelBottom => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 14h18"/></svg>"#,
        Icon::MoreHorizontal => br#"<svg viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="19" cy="12" r="1.5"/></svg>"#,
        Icon::Close => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="m7 7 10 10M17 7 7 17"/></svg>"#,
        Icon::ChevronUp => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="m7 14 5-5 5 5"/></svg>"#,
        Icon::ChevronDown => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="m7 10 5 5 5-5"/></svg>"#,
        Icon::ChevronRight => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="m10 7 5 5-5 5"/></svg>"#,
        Icon::Minus => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="M6 12h12"/></svg>"#,
        Icon::Plus => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="M12 6v12M6 12h12"/></svg>"#,
        Icon::Alert => br#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4 3.5 19h17L12 4Z"/><path d="M12 9v4M12 16.5h.01"/></svg>"#,
    };

    svg().data(data).size_full().text_color(color)
}
