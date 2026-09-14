//! Built-in document templates. Templates are plain data so the picker UI,
//! the welcome screen, and project scaffolding all render from one registry
//! instead of hardcoded string literals scattered through the workspace.

use super::document::DocumentKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTemplate {
    /// Stable identifier, e.g. `latex-article`.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub kind: DocumentKind,
    /// Suggested file name for a new document or project root file.
    pub file_name: &'static str,
    pub content: &'static str,
}

pub const DEFAULT_LATEX_STARTER: &str = "\\documentclass{article}\n\\title{Untitled}\n\\author{}\n\n\\begin{document}\n\\maketitle\n\n\\section{Introduction}\nStart writing here.\n\n\\end{document}\n";

const LATEX_ARTICLE: &str = DEFAULT_LATEX_STARTER;

const LATEX_PAPER: &str = r"\documentclass[11pt]{article}
\usepackage{graphicx}
\usepackage{hyperref}

\title{Paper Title}
\author{Author Name}
\date{}

\begin{document}
\maketitle

\begin{abstract}
Summarize the problem, approach, and results in one paragraph.
\end{abstract}

\section{Introduction}
Motivate the problem and state the contribution.

\section{Related Work}
Position this paper against prior work.

\section{Method}
Describe the approach. Reference figures with \verb|\figurename| and
equations as $E = mc^2$.

\section{Results}
Present and discuss the results.

\section{Conclusion}
Restate the contribution and outline future work.

\begin{thebibliography}{9}
\bibitem{example} A. Author, ``Example Title,'' Journal of Examples, 2026.
\end{thebibliography}

\end{document}
";

const LATEX_BEAMER: &str = r"\documentclass{beamer}

\title{Presentation Title}
\author{Author Name}
\date{}

\begin{document}
\maketitle

\begin{frame}{Outline}
\tableofcontents
\end{frame}

\section{Introduction}
\begin{frame}{Introduction}
One idea per frame; keep slides scannable.
\end{frame}

\begin{frame}{Results}
\begin{itemize}
\item First finding
\item Second finding
\end{itemize}
\end{frame}

\end{document}
";

const TYPST_BLANK: &str = "= Untitled\n\nStart writing here.\n";

const TYPST_ARTICLE: &str = r#"#set document(title: "Article Title", author: "Author Name")

= Article Title

#author("Author Name")

*Abstract* — Summarize the problem, approach, and results in one paragraph.

= Introduction

Motivate the problem and state the contribution.

= Method

Describe the approach. Inline math looks like $E = mc^2$.

= Results

Present and discuss the results.

= Conclusion

Restate the contribution and outline future work.
"#;

const TYPST_REPORT: &str = r#"#set document(title: "Report Title", author: "Author Name")
#set page(numbering: "1")
#set heading(numbering: "1.")

#align(center)[
  = Report Title
  #v(0.5em)
  Author Name \
  #datetime.today().display("[month long] [day], [year]")
]

#outline()
#pagebreak()

= Introduction

Summarize the scope and structure of the report.

= Findings

Present the findings with numbered sections.

= Conclusion

State the outcome and recommended next steps.
"#;

/// All built-in templates in picker display order.
pub fn builtin_templates() -> &'static [DocumentTemplate] {
    &[
        DocumentTemplate {
            id: "latex-article",
            name: "Blank Article",
            description: "A minimal LaTeX article with title and sections.",
            kind: DocumentKind::Latex,
            file_name: "main.tex",
            content: LATEX_ARTICLE,
        },
        DocumentTemplate {
            id: "latex-paper",
            name: "Academic Paper",
            description: "Abstract, sections, and a bibliography skeleton.",
            kind: DocumentKind::Latex,
            file_name: "paper.tex",
            content: LATEX_PAPER,
        },
        DocumentTemplate {
            id: "latex-beamer",
            name: "Presentation",
            description: "A Beamer slide deck with an outline and frames.",
            kind: DocumentKind::Latex,
            file_name: "slides.tex",
            content: LATEX_BEAMER,
        },
        DocumentTemplate {
            id: "typst-blank",
            name: "Blank Note",
            description: "A minimal Typst document to start from scratch.",
            kind: DocumentKind::Typst,
            file_name: "main.typ",
            content: TYPST_BLANK,
        },
        DocumentTemplate {
            id: "typst-article",
            name: "Typst Article",
            description: "Title block, abstract, and numbered sections.",
            kind: DocumentKind::Typst,
            file_name: "article.typ",
            content: TYPST_ARTICLE,
        },
        DocumentTemplate {
            id: "typst-report",
            name: "Typst Report",
            description: "Table of contents, page numbers, and headings.",
            kind: DocumentKind::Typst,
            file_name: "report.typ",
            content: TYPST_REPORT,
        },
    ]
}

pub fn template_by_id(id: &str) -> Option<&'static DocumentTemplate> {
    builtin_templates()
        .iter()
        .find(|template| template.id == id)
}

/// Templates matching `query` (case-insensitive substring on name and
/// description) and, when `kind` is given, only that document kind. Shared
/// by the picker view and Enter-accept so the visible list and the accepted
/// result agree.
pub fn filter_templates(
    query: &str,
    kind: Option<DocumentKind>,
) -> impl Iterator<Item = &'static DocumentTemplate> {
    let query_lower = query.trim().to_lowercase();
    builtin_templates().iter().filter(move |template| {
        kind.is_none_or(|wanted| template.kind == wanted)
            && (query_lower.is_empty()
                || template.name.to_lowercase().contains(&query_lower)
                || template.description.to_lowercase().contains(&query_lower))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::document::kind_for_title;

    #[test]
    fn template_ids_and_file_names_are_unique() {
        let templates = builtin_templates();
        for (i, a) in templates.iter().enumerate() {
            for b in &templates[i + 1..] {
                assert_ne!(a.id, b.id, "duplicate template id");
                assert_ne!(a.file_name, b.file_name, "duplicate template file name");
            }
        }
    }

    #[test]
    fn template_kinds_match_file_extensions() {
        for template in builtin_templates() {
            let derived = kind_for_title(template.file_name);
            assert_eq!(derived, template.kind, "kind mismatch for {}", template.id);
            assert!(
                template.kind.is_compilable(),
                "templates must be compilable"
            );
            assert!(!template.content.is_empty());
        }
    }

    #[test]
    fn lookup_by_id_round_trips() {
        for template in builtin_templates() {
            assert_eq!(template_by_id(template.id), Some(template));
        }
        assert_eq!(template_by_id("missing"), None);
    }

    #[test]
    fn filter_matches_name_and_description_case_insensitively() {
        let all: Vec<_> = filter_templates("", None).collect();
        assert_eq!(all.len(), builtin_templates().len());

        let latex: Vec<_> = filter_templates("", Some(DocumentKind::Latex)).collect();
        assert_eq!(latex.len(), 3);
        assert!(latex.iter().all(|t| t.kind == DocumentKind::Latex));

        assert!(filter_templates("beamer", None).count() == 1);
        assert!(filter_templates("BIBLIOGRAPHY", None).count() == 1);
        assert!(
            filter_templates("nothing matches this", None)
                .next()
                .is_none()
        );
    }

    #[test]
    fn latex_templates_compile_standalone() {
        // Every LaTeX template must open and close the document environment;
        // a template that cannot compile on first try is a broken first run.
        for template in filter_templates("", Some(DocumentKind::Latex)) {
            assert!(template.content.contains("\\begin{document}"));
            assert!(template.content.contains("\\end{document}"));
        }
    }
}
