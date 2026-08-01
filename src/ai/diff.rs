//! AI proposal diff review and change staging (spec §M5.5).

/// A staged AI text mutation proposal for user review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReview {
    pub title: String,
    pub original: String,
    pub replacement: String,
}

impl DiffReview {
    /// Creates a new diff review staging object.
    pub fn new(
        title: impl Into<String>,
        original: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            original: original.into(),
            replacement: replacement.into(),
        }
    }

    /// Computes line-count metrics for the diff.
    pub fn line_metrics(&self) -> (usize, usize) {
        let orig_lines = self.original.lines().count().max(1);
        let repl_lines = self.replacement.lines().count().max(1);
        (orig_lines, repl_lines)
    }

    /// Returns a short label indicating the diff size.
    pub fn diff_summary(&self) -> String {
        let (orig_lines, repl_lines) = self.line_metrics();
        format!("{orig_lines} lines → {repl_lines} lines")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_review_metrics() {
        let review = DiffReview::new(
            "Rewrite Academic",
            "This is original line 1\nLine 2",
            "This is replaced line 1\nLine 2\nLine 3",
        );

        assert_eq!(review.line_metrics(), (2, 3));
        assert_eq!(review.diff_summary(), "2 lines → 3 lines");
    }
}
