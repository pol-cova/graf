//! Context-aware LaTeX autocompletion engine (spec §M3.5–M3.7).

use crate::project::bibtex::{BibtexIndex, LabelIndex};

/// A single autocompletion candidate item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: String,
    pub insert_text: String,
    pub kind: CompletionKind,
}

/// Category of completion proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Citation,
    Reference,
    Environment,
    Command,
}

impl CompletionKind {
    /// Returns a compact label for this completion kind.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Citation => "CITE",
            Self::Reference => "REF",
            Self::Environment => "ENV",
            Self::Command => "CMD",
        }
    }
}

/// Computes contextual completions for the cursor position in `buffer`.
pub fn compute_completions(
    buffer_content: &str,
    cursor_offset: usize,
    bib_index: &BibtexIndex,
    label_index: &LabelIndex,
) -> Vec<CompletionItem> {
    let safe_offset = cursor_offset.min(buffer_content.len());
    let mut end = safe_offset;
    while end > 0 && !buffer_content.is_char_boundary(end) {
        end -= 1;
    }
    let safe_prefix = &buffer_content[..end];
    let line_start = safe_prefix.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let prefix = &safe_prefix[line_start..];

    if let Some(cite_pos) = prefix.rfind("\\cite{") {
        let after = &prefix[cite_pos + 6..];
        if !after.contains('}') && !after.contains('\n') {
            return bib_index
                .search(after)
                .into_iter()
                .map(|e| CompletionItem {
                    label: e.key.clone(),
                    detail: e.display_summary(),
                    insert_text: format!("{}}}", completion_suffix(&e.key, after)),
                    kind: CompletionKind::Citation,
                })
                .collect();
        }
    }

    let ref_prefixes = ["\\ref{", "\\eqref{", "\\autoref{", "\\pageref{"];
    for ref_prefix in ref_prefixes {
        if let Some(ref_pos) = prefix.rfind(ref_prefix) {
            let after = &prefix[ref_pos + ref_prefix.len()..];
            if !after.contains('}') && !after.contains('\n') {
                return label_index
                    .search(after)
                    .into_iter()
                    .map(|l| CompletionItem {
                        label: l.to_string(),
                        detail: "Cross-reference label".to_string(),
                        insert_text: format!("{}}}", completion_suffix(l, after)),
                        kind: CompletionKind::Reference,
                    })
                    .collect();
            }
        }
    }

    if let Some(begin_pos) = prefix.rfind("\\begin{") {
        let after = &prefix[begin_pos + 7..];
        if !after.contains('}') && !after.contains('\n') {
            let common_envs = [
                ("equation", "Numbered mathematical equation"),
                ("align", "Aligned equations"),
                ("figure", "Floating figure with caption"),
                ("table", "Floating table"),
                ("itemize", "Bulleted list"),
                ("enumerate", "Numbered list"),
                ("abstract", "Paper abstract section"),
                ("proof", "Mathematical proof block"),
                ("theorem", "Theorem statement block"),
                ("lemma", "Lemma statement block"),
                ("lstlisting", "Source code listing"),
            ];

            let query_lower = after.to_lowercase();
            return common_envs
                .into_iter()
                .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
                .map(|(name, detail)| CompletionItem {
                    label: name.to_string(),
                    detail: detail.to_string(),
                    insert_text: format!(
                        "{}}}\n    \n\\end{{{name}}}",
                        completion_suffix(name, after)
                    ),
                    kind: CompletionKind::Environment,
                })
                .collect();
        }
    }

    if let Some(slash_pos) = prefix.rfind('\\') {
        let after = &prefix[slash_pos + 1..];
        if !after.contains(' ')
            && !after.contains('{')
            && !after.contains('}')
            && !after.contains('\n')
            && after.chars().all(|c| c.is_alphabetic())
        {
            let common_commands = [
                ("section", "Section heading", "section{}"),
                ("subsection", "Subsection heading", "subsection{}"),
                ("subsubsection", "Subsubsection heading", "subsubsection{}"),
                ("textbf", "Bold font weight", "textbf{}"),
                ("textit", "Italic font slant", "textit{}"),
                ("usepackage", "Include LaTeX package", "usepackage{}"),
                (
                    "newcommand",
                    "Define custom command macro",
                    "newcommand{}{}",
                ),
                ("frac", "Fraction numerator over denominator", "frac{}{}"),
                ("sqrt", "Square root", "sqrt{}"),
                ("label", "Cross-reference anchor label", "label{}"),
                ("caption", "Figure or table caption", "caption{}"),
            ];

            let query_lower = after.to_lowercase();
            return common_commands
                .into_iter()
                .filter(|(name, _, _)| name.to_lowercase().starts_with(&query_lower))
                .map(|(name, detail, snippet)| CompletionItem {
                    label: format!("\\{name}"),
                    detail: detail.to_string(),
                    insert_text: completion_suffix(snippet, after).to_string(),
                    kind: CompletionKind::Command,
                })
                .collect();
        }
    }

    Vec::new()
}

fn completion_suffix<'a>(candidate: &'a str, typed: &str) -> &'a str {
    candidate.strip_prefix(typed).unwrap_or(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_citation_completion() {
        let mut bib = BibtexIndex::new();
        bib.parse_and_load(
            r#"
@article{vaswani2017attention,
  title = {Attention Is All You Need},
  author = {Ashish Vaswani}
}
"#,
        );
        let labels = LabelIndex::default();

        let content = "See paper \\cite{vas";
        let completions = compute_completions(content, content.len(), &bib, &labels);

        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].label, "vaswani2017attention");
        assert_eq!(completions[0].kind, CompletionKind::Citation);
    }

    #[test]
    fn test_ref_completion() {
        let bib = BibtexIndex::new();
        let mut labels = LabelIndex::default();
        labels.parse_and_load(
            "\\section{Intro}\\label{sec:intro}\n\\begin{equation}\\label{eq:maxwell}",
        );

        let content = "As seen in equation \\eqref{eq:";
        let completions = compute_completions(content, content.len(), &bib, &labels);

        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].label, "eq:maxwell");
        assert_eq!(completions[0].kind, CompletionKind::Reference);
    }

    #[test]
    fn test_environment_completion() {
        let bib = BibtexIndex::new();
        let labels = LabelIndex::default();

        let content = "\\begin{equa";
        let completions = compute_completions(content, content.len(), &bib, &labels);

        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].label, "equation");
        assert_eq!(completions[0].kind, CompletionKind::Environment);
    }
}
