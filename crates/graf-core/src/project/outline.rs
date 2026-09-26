#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: usize,
    pub title: String,
    pub line_number: usize,
}

impl OutlineItem {
    pub fn display_prefix(&self) -> &'static str {
        match self.level {
            0 => "§",
            1 => "§§",
            2 => "•",
            _ => "·",
        }
    }
}

pub fn parse_latex_outline(source: &str) -> Vec<OutlineItem> {
    let mut items = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('%') {
            continue;
        }

        let section_kinds = [
            ("\\chapter{", 0),
            ("\\section{", 1),
            ("\\subsection{", 2),
            ("\\subsubsection{", 3),
            ("\\paragraph{", 4),
        ];

        for (pattern, level) in section_kinds {
            if let Some(pos) = trimmed.find(pattern) {
                let rest = &trimmed[pos + pattern.len()..];
                if let Some(end_brace) = rest.find('}') {
                    let title = rest[..end_brace].trim().to_string();
                    items.push(OutlineItem {
                        level,
                        title,
                        line_number: line_idx + 1,
                    });
                    break;
                }
            }
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_latex_outline() {
        let latex = r#"\documentclass{article}
\title{Paper}
\begin{document}
\section{Introduction}
Some text.
\subsection{Background}
Background details.
\section{Methods}
Methods description.
\subsubsection{Data Collection}
Data details.
\end{document}
"#;

        let outline = parse_latex_outline(latex);
        assert_eq!(outline.len(), 4);
        assert_eq!(outline[0].title, "Introduction");
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].line_number, 4);

        assert_eq!(outline[1].title, "Background");
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].line_number, 6);

        assert_eq!(outline[2].title, "Methods");
        assert_eq!(outline[2].level, 1);
        assert_eq!(outline[2].line_number, 8);

        assert_eq!(outline[3].title, "Data Collection");
        assert_eq!(outline[3].level, 3);
        assert_eq!(outline[3].line_number, 10);
    }
}
