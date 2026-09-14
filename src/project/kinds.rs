//! Single source of truth for classifying files by extension. `tree` and
//! `document` derive their public kinds from here so the taxonomies can
//! never drift apart again.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Latex,
    Typst,
    Bibtex,
    Style,
    Image,
    Pdf,
    GrafCanvas,
    Other,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("tex") => Self::Latex,
            Some("typ") => Self::Typst,
            Some("bib") => Self::Bibtex,
            Some("sty") | Some("cls") => Self::Style,
            Some("png") | Some("jpg") | Some("jpeg") | Some("svg") => Self::Image,
            Some("pdf") => Self::Pdf,
            Some("graf") => Self::GrafCanvas,
            _ => Self::Other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Latex => "TEX",
            Self::Typst => "TYP",
            Self::Bibtex => "BIB",
            Self::Style => "STY",
            Self::Image => "IMG",
            Self::Pdf => "PDF",
            Self::GrafCanvas => "GRF",
            Self::Other => "",
        }
    }

    /// Engine the file compiles with, if any. One classifier for root
    /// detection and compile routing.
    pub fn as_engine(self) -> Option<crate::compiler::EngineKind> {
        match self {
            Self::Latex => Some(crate::compiler::EngineKind::Latex),
            Self::Typst => Some(crate::compiler::EngineKind::Typst),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_mapping_follows_extension() {
        assert_eq!(
            FileKind::from_path(Path::new("paper.tex")).as_engine(),
            Some(crate::compiler::EngineKind::Latex)
        );
        assert_eq!(FileKind::from_path(Path::new("refs.bib")).as_engine(), None);
        assert_eq!(FileKind::from_path(Path::new("fig.png")).as_engine(), None);
        assert_eq!(
            FileKind::from_path(Path::new("chart.graf")).as_engine(),
            None
        );
    }
}
