//! Compiler subsystem for multi-format document typesetting (spec §25–31, M6).

#[allow(dead_code)]
pub mod controller;
#[allow(dead_code)]
pub mod diagnostics;
#[allow(dead_code)]
pub mod engine;
#[allow(dead_code)]
pub mod tectonic;
#[allow(dead_code)]
pub mod typst;

/// Supported typesetting engine kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineKind {
    #[default]
    Latex,
    Typst,
}

impl EngineKind {
    /// Display name of the engine.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Latex => "LaTeX (Tectonic)",
            Self::Typst => "Typst",
        }
    }

    /// Display icon for this engine.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Latex => "📄",
            Self::Typst => "⚡",
        }
    }
}
