use std::path::Path;

/// Language/home kind of a document, derived once from its filename.
/// Delegates to the shared `kinds` classifier so tree labels and document
/// kinds can never disagree about what an extension means.
///
/// `PlainText` is a distinct variant rather than a missing engine because the
/// scaffolder offers plain-text templates alongside the two languages Graf can
/// build, and needs to tell "no engine" from "LaTeX".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Latex,
    Typst,
    PlainText,
}

pub fn kind_for_title(title: &str) -> DocumentKind {
    match super::kinds::FileKind::from_path(Path::new(title)) {
        super::kinds::FileKind::Latex => DocumentKind::Latex,
        super::kinds::FileKind::Typst => DocumentKind::Typst,
        _ => DocumentKind::PlainText,
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;

    #[test]
    fn title_kinds() {
        assert_eq!(kind_for_title("paper.tex"), DocumentKind::Latex);
        assert_eq!(kind_for_title("notes.typ"), DocumentKind::Typst);
        assert_eq!(kind_for_title("README.md"), DocumentKind::PlainText);
        assert_eq!(kind_for_title("makefile"), DocumentKind::PlainText);
        assert_eq!(kind_for_title("dotted.name.typ"), DocumentKind::Typst);
        assert_eq!(kind_for_title(""), DocumentKind::PlainText);
        assert_eq!(kind_for_title("typst-like.tex"), DocumentKind::Latex);
    }
}
