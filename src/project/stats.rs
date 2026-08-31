#[derive(Debug, Clone, PartialEq)]
pub struct DocumentStats {
    pub word_count: usize,
    pub char_count: usize,
    pub equation_count: usize,
    pub citation_count: usize,
    pub reading_time_mins: f32,
    pub estimated_pages: f32,
}

impl DocumentStats {
    pub fn compute(text: &str, is_typst: bool) -> Self {
        let mut words = 0;
        let mut char_count = 0;
        let mut equation_count = 0;
        let mut citation_count = 0;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if (!is_typst && trimmed.starts_with('%')) || (is_typst && trimmed.starts_with("//")) {
                continue;
            }

            equation_count += line.matches('$').count() / 2;
            if line.contains("\\begin{equation") || line.contains("\\begin{align") {
                equation_count += 1;
            }

            if is_typst {
                citation_count += line.matches('@').count();
            } else {
                citation_count += line.matches("\\cite").count();
            }

            for raw_word in line.split_whitespace() {
                let clean = raw_word.trim_matches(|c: char| !c.is_alphanumeric());
                if !clean.is_empty() && !clean.starts_with('\\') {
                    words += 1;
                    char_count += clean.len();
                }
            }
        }

        let reading_time_mins = (words as f32 / 200.0).max(0.1);
        let estimated_pages = (words as f32 / 550.0).max(0.1);

        Self {
            word_count: words,
            char_count,
            equation_count,
            citation_count,
            reading_time_mins,
            estimated_pages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_stats_calculation() {
        let text = r#"
\section{Introduction}
Deep learning has revolutionized artificial intelligence.
We propose a novel framework for scalable training \cite{vaswani2017attention}.

\begin{equation}
E = mc^2
\end{equation}

The empirical results demonstrate significant improvements over baselines.
"#;
        let stats = DocumentStats::compute(text, false);
        assert!(stats.word_count >= 15);
        assert_eq!(stats.citation_count, 1);
        assert_eq!(stats.equation_count, 1);
    }
}
