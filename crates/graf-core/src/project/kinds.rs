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
            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_follows_the_last_extension() {
        assert_eq!(FileKind::from_path(Path::new("paper.tex")), FileKind::Latex);
        assert_eq!(
            FileKind::from_path(Path::new("dotted.name.typ")),
            FileKind::Typst
        );
        assert_eq!(FileKind::from_path(Path::new("refs.bib")), FileKind::Bibtex);
        assert_eq!(
            FileKind::from_path(Path::new("chapter.cls")),
            FileKind::Style
        );
        assert_eq!(FileKind::from_path(Path::new("fig.png")), FileKind::Image);
        assert_eq!(FileKind::from_path(Path::new("out.pdf")), FileKind::Pdf);
        assert_eq!(FileKind::from_path(Path::new("notes")), FileKind::Other);
    }
}
