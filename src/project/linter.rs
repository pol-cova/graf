//! Zero-cloud local academic writing style and grammar linter.

/// Category of style violation in technical writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleCategory {
    PassiveVoice,
    Wordiness,
    WeaselWords,
    Cliché,
}

impl StyleCategory {
    /// Returns a human-readable title for the category.
    pub fn title(self) -> &'static str {
        match self {
            Self::PassiveVoice => "Passive Voice",
            Self::Wordiness => "Wordiness / Redundancy",
            Self::WeaselWords => "Weak / Weasel Word",
            Self::Cliché => "Cliché / Informal",
        }
    }
}

/// A specific style warning emitted by the linter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleWarning {
    pub line: usize,
    pub col: usize,
    pub length: usize,
    pub category: StyleCategory,
    pub matched_text: String,
    pub suggestion: Option<String>,
    pub message: String,
}

/// Known wordy phrases and their concise replacements.
static WORDY_PHRASES: &[(&str, &str)] = &[
    ("in order to", "to"),
    ("due to the fact that", "because"),
    ("at this point in time", "now"),
    ("at the present time", "now"),
    ("a large number of", "many"),
    ("a small number of", "few"),
    ("a significant number of", "many"),
    ("has the ability to", "can"),
    ("is able to", "can"),
    ("take into consideration", "consider"),
    ("give consideration to", "consider"),
    ("conduct an investigation of", "investigate"),
    ("perform an analysis of", "analyze"),
    ("in the event that", "if"),
    ("for the purpose of", "to"),
    ("utilize", "use"),
    ("utilizes", "uses"),
    ("utilized", "used"),
    ("utilizing", "using"),
    ("utilization", "use"),
    ("prior to", "before"),
    ("subsequent to", "after"),
];

/// Known weak or weasel words that degrade academic rigor.
static WEASEL_WORDS: &[(&str, &str)] = &[
    ("very", "Omit or use a precise quantitative descriptor"),
    ("extremely", "Omit or specify exact magnitude"),
    ("obviously", "Omit; state the empirical evidence directly"),
    ("clearly", "Omit or demonstrate via experimental results"),
    ("quite", "Omit or specify exact degree"),
    ("fairly", "Specify measurable bounds or threshold"),
    ("somewhat", "Specify measurable bounds"),
    ("sort of", "Use precise terminology"),
    ("kind of", "Use precise terminology"),
];

/// Common irregular passive past participles following forms of "to be".
static PASSIVE_BE_FORMS: &[&str] = &["is", "are", "was", "were", "been", "being", "be"];

/// Checks a complete document text for academic style improvements.
pub fn lint_academic_text(text: &str, is_typst: bool) -> Vec<StyleWarning> {
    let mut warnings = Vec::new();

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_num = line_idx + 1;
        let masked = mask_math_and_macros(raw_line, is_typst);
        let lower_masked = masked.to_lowercase();

        // 1. Check for wordy phrases
        for &(phrase, replacement) in WORDY_PHRASES {
            let mut search_from = 0;
            while let Some(found_idx) = lower_masked[search_from..].find(phrase) {
                let col = search_from + found_idx;
                search_from = col + phrase.len();

                if is_word_boundary(&lower_masked, col, phrase.len()) {
                    warnings.push(StyleWarning {
                        line: line_num,
                        col: col + 1,
                        length: phrase.len(),
                        category: StyleCategory::Wordiness,
                        matched_text: raw_line[col..col + phrase.len()].to_string(),
                        suggestion: Some(replacement.to_string()),
                        message: format!("Consider replacing '{phrase}' with '{replacement}'"),
                    });
                }
            }
        }

        // 2. Check for weasel words
        for &(weasel, reason) in WEASEL_WORDS {
            let mut search_from = 0;
            while let Some(found_idx) = lower_masked[search_from..].find(weasel) {
                let col = search_from + found_idx;
                search_from = col + weasel.len();

                if is_word_boundary(&lower_masked, col, weasel.len()) {
                    warnings.push(StyleWarning {
                        line: line_num,
                        col: col + 1,
                        length: weasel.len(),
                        category: StyleCategory::WeaselWords,
                        matched_text: raw_line[col..col + weasel.len()].to_string(),
                        suggestion: None,
                        message: format!("Weak descriptor '{weasel}': {reason}"),
                    });
                }
            }
        }

        // 3. Check for passive voice patterns (e.g. "was observed", "is evaluated", "were performed")
        check_passive_voice(raw_line, &lower_masked, line_num, &mut warnings);
    }

    warnings
}

/// Masks math blocks `$ ... $`, comments `%`, and macros `\cite{...}` with spaces to avoid false positives.
fn mask_math_and_macros(line: &str, is_typst: bool) -> String {
    let mut out: Vec<char> = line.chars().collect();
    let len = out.len();
    let mut i = 0;

    // Mask comments to end of line
    if is_typst {
        if let Some(pos) = line.find("//") {
            for c in out.iter_mut().skip(pos) {
                *c = ' ';
            }
        }
    } else if let Some(pos) = line.find('%') {
        // Only if not escaped \%
        if pos == 0 || line.as_bytes().get(pos - 1) != Some(&b'\\') {
            for c in out.iter_mut().skip(pos) {
                *c = ' ';
            }
        }
    }

    // Mask math $...$
    while i < len {
        if out[i] == '$' {
            out[i] = ' ';
            i += 1;
            while i < len && out[i] != '$' {
                out[i] = ' ';
                i += 1;
            }
            if i < len && out[i] == '$' {
                out[i] = ' ';
                i += 1;
            }
            continue;
        }

        // Mask LaTeX macro commands like \cite{...} or \ref{...}
        if !is_typst && out[i] == '\\' {
            let start = i;
            while i < len && out[i].is_alphabetic() {
                out[i] = ' ';
                i += 1;
            }
            if i < len && out[i] == '{' {
                while i < len && out[i] != '}' {
                    out[i] = ' ';
                    i += 1;
                }
                if i < len && out[i] == '}' {
                    out[i] = ' ';
                    i += 1;
                }
            }
            if i == start {
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    out.into_iter().collect()
}

/// Checks if a slice in `line` starts and ends on ASCII word boundaries.
fn is_word_boundary(line: &str, start: usize, len: usize) -> bool {
    let bytes = line.as_bytes();
    let before_ok = if start == 0 {
        true
    } else {
        !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_'
    };

    let end = start + len;
    let after_ok = if end >= bytes.len() {
        true
    } else {
        !bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_'
    };

    before_ok && after_ok
}

/// Identifies passive voice occurrences (e.g. "was analyzed", "is calculated").
fn check_passive_voice(
    raw_line: &str,
    lower_line: &str,
    line_num: usize,
    warnings: &mut Vec<StyleWarning>,
) {
    let words: Vec<(usize, &str)> = lower_line
        .split_whitespace()
        .map(|w| {
            let offset = (w.as_ptr() as usize) - (lower_line.as_ptr() as usize);
            (offset, w.trim_matches(|c: char| !c.is_alphabetic()))
        })
        .filter(|(_, w)| !w.is_empty())
        .collect();

    for i in 0..words.len().saturating_sub(1) {
        let (be_offset, be_word) = words[i];
        let (verb_offset, next_word) = words[i + 1];

        if PASSIVE_BE_FORMS.contains(&be_word) {
            // Check if next word ends in 'ed' or is common irregular past participle
            let is_past_participle = next_word.ends_with("ed")
                || matches!(
                    next_word,
                    "shown"
                        | "seen"
                        | "found"
                        | "done"
                        | "made"
                        | "given"
                        | "taken"
                        | "known"
                        | "chosen"
                        | "written"
                );

            if is_past_participle && next_word.len() > 3 {
                let total_len = (verb_offset + next_word.len()) - be_offset;
                let matched = raw_line
                    .get(be_offset..be_offset + total_len)
                    .unwrap_or("")
                    .to_string();

                warnings.push(StyleWarning {
                    line: line_num,
                    col: be_offset + 1,
                    length: total_len,
                    category: StyleCategory::PassiveVoice,
                    matched_text: matched,
                    suggestion: Some(format!("Consider active voice: 'We {next_word}'")),
                    message: format!("Passive voice construct '{be_word} {next_word}'"),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_wordiness_and_weasel_words() {
        let text =
            "In order to improve performance, it is clearly evident that we utilize caching.";
        let warnings = lint_academic_text(text, false);

        assert!(warnings.iter().any(|w| w.matched_text == "In order to"));
        assert!(warnings.iter().any(|w| w.matched_text == "clearly"));
        assert!(warnings.iter().any(|w| w.matched_text == "utilize"));
    }

    #[test]
    fn test_lint_passive_voice() {
        let text = "The experiment was performed and data was analyzed accurately.";
        let warnings = lint_academic_text(text, false);

        let passive_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.category == StyleCategory::PassiveVoice)
            .collect();
        assert_eq!(passive_warnings.len(), 2);
        assert!(passive_warnings[0].matched_text.contains("was performed"));
        assert!(passive_warnings[1].matched_text.contains("was analyzed"));
    }

    #[test]
    fn test_math_and_comment_masking() {
        let text = "Formula $x = \\text{very clear}$ is good. % in order to ignore this";
        let warnings = lint_academic_text(text, false);
        // Math content and comment content must be ignored
        assert!(warnings.is_empty());
    }
}
