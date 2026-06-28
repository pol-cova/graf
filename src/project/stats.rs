//! Document statistics and conference page budget calculator.

/// Pre-configured conference submission limits.
#[allow(dead_code, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, PartialEq)]
pub enum ConferenceProfile {
    NeurIPS,
    ICML,
    ICLR,
    CVPR,
    Custom {
        name: String,
        max_pages: f32,
        words_per_page: usize,
    },
}

impl ConferenceProfile {
    /// Returns the conference name.
    pub fn name(&self) -> &str {
        match self {
            Self::NeurIPS => "NeurIPS",
            Self::ICML => "ICML",
            Self::ICLR => "ICLR",
            Self::CVPR => "CVPR",
            Self::Custom { name, .. } => name.as_str(),
        }
    }

    /// Maximum allowed main-body pages (excluding references/appendix).
    pub fn max_pages(&self) -> f32 {
        match self {
            Self::NeurIPS => 9.0,
            Self::ICML => 8.0,
            Self::ICLR => 9.0,
            Self::CVPR => 8.0,
            Self::Custom { max_pages, .. } => *max_pages,
        }
    }

    /// Approximate words per publication page.
    pub fn words_per_page(&self) -> usize {
        match self {
            Self::NeurIPS | Self::ICML | Self::ICLR => 550,
            Self::CVPR => 600,
            Self::Custom { words_per_page, .. } => *words_per_page,
        }
    }
}

/// Comprehensive document statistics.
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
    /// Analyzes document text and computes academic metrics.
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

            // Ignore full comment lines
            if (!is_typst && trimmed.starts_with('%')) || (is_typst && trimmed.starts_with("//")) {
                continue;
            }

            // Count math formulas
            equation_count += line.matches('$').count() / 2;
            if line.contains("\\begin{equation") || line.contains("\\begin{align") {
                equation_count += 1;
            }

            // Count citations
            if is_typst {
                citation_count += line.matches('@').count();
            } else {
                citation_count += line.matches("\\cite").count();
            }

            // Word extraction (ignoring commands)
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

    /// Returns a compact status bar summary (e.g. "1,240 words • ~2.3 pages / NeurIPS max 9.0").
    pub fn status_summary(&self, profile: &ConferenceProfile) -> String {
        let est_pages = self.word_count as f32 / profile.words_per_page() as f32;
        let max_p = profile.max_pages();
        let conf_name = profile.name();

        format!(
            "{} words • {:.1}/{:.0} pages ({conf_name})",
            format_thousands(self.word_count),
            est_pages,
            max_p
        )
    }
}

fn format_thousands(num: usize) -> String {
    let s = num.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();

    for (i, &ch) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
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

    #[test]
    fn test_conference_budget_summary() {
        let mut stats = DocumentStats::compute("sample", false);
        stats.word_count = 2200;

        let summary = stats.status_summary(&ConferenceProfile::NeurIPS);
        assert!(summary.contains("2,200 words"));
        assert!(summary.contains("4.0/9 pages (NeurIPS)"));
    }
}
