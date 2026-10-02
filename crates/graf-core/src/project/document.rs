use std::path::Path;

/// Language/home kind of a document, derived once from its filename.
/// Delegates to the shared `kinds` classifier so tree labels and document
/// kinds can never disagree about what an extension means.
///
/// This is what remains of the former `document` module. `Document` itself
/// described a workspace-owned document with its own `TextBuffer`, dirty
/// tracking, undo, and an external-change guard on save. All of that was the
/// GPUI front end's text ownership; Swift now holds the live document in a
/// single `NSTextStorage` and saves a snapshot through the bridge, so nothing
/// constructed a `Document` and the whole type was unreachable outside its
/// own tests. `DocumentKind` survives because the project scaffolder still
/// needs to know which engine a new template targets.
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

impl DocumentKind {
    /// Whether a compile engine runs on this kind. `PlainText` is not a
    /// language Graf can build, which is what distinguishes it from the two
    /// the scaffolder can target.
    pub fn is_compilable(self) -> bool {
        matches!(self, Self::Latex | Self::Typst)
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

    /// The only reason this enum is not simply `FileKind`: templates may be
    /// plain text, and the scaffolder needs to tell "no engine" from "LaTeX".
    #[test]
    fn plain_text_has_no_engine_and_latex_and_typst_do() {
        assert!(matches!(
            super::super::kinds::FileKind::from_path(Path::new("a.tex")).as_engine(),
            Some(crate::compiler::EngineKind::Latex)
        ));
        assert!(matches!(
            super::super::kinds::FileKind::from_path(Path::new("a.typ")).as_engine(),
            Some(crate::compiler::EngineKind::Typst)
        ));
        assert!(
            super::super::kinds::FileKind::from_path(Path::new("a.md"))
                .as_engine()
                .is_none()
        );
    }
}
