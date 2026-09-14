#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleCategory {
    PassiveVoice,
    Wordiness,
    WeaselWords,
}

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

static PASSIVE_BE_FORMS: &[&str] = &["is", "are", "was", "were", "been", "being", "be"];

pub fn lint_academic_warnings_as_diagnostics(
    text: &str,
    is_typst: bool,
) -> Vec<crate::compiler::diagnostics::Diagnostic> {
    lint_academic_text(text, is_typst)
        .into_iter()
        .map(|warning| {
            crate::compiler::diagnostics::Diagnostic::from_style_warning(
                warning.line,
                warning.message,
            )
        })
        .collect()
}

pub fn lint_academic_text(text: &str, is_typst: bool) -> Vec<StyleWarning> {
    let mut warnings = Vec::new();

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_num = line_idx + 1;
        let masked = mask_math_and_macros(raw_line, is_typst);
        // Both hold the same byte length as `raw_line`, so offsets found in
        // the folded text can never slice mid-character in the raw line.
        let lower_masked = masked.to_lowercase();

        for_each_match(
            &lower_masked,
            raw_line,
            line_num,
            phrase_rules(),
            &mut warnings,
        );

        check_passive_voice(raw_line, &lower_masked, line_num, &mut warnings);
    }

    warnings
}

#[derive(Debug, Clone)]
pub(crate) struct PhraseRule {
    needle: &'static str,
    category: StyleCategory,
    suggestion: Option<&'static str>,
    message: String,
}

/// One unified rule list: wordiness phrases (with a replacement suggestion)
/// and weak descriptors (with a reason), built per lint from the static
/// tables so the scan loop only walks a single list.
fn phrase_rules() -> impl Iterator<Item = PhraseRule> {
    let wordy = WORDY_PHRASES
        .iter()
        .map(|&(needle, replacement)| PhraseRule {
            needle,
            category: StyleCategory::Wordiness,
            suggestion: Some(replacement),
            message: format!("Consider replacing '{needle}' with '{replacement}'"),
        });
    let weasel = WEASEL_WORDS.iter().map(|&(needle, reason)| PhraseRule {
        needle,
        category: StyleCategory::WeaselWords,
        suggestion: None,
        message: format!("Weak descriptor '{needle}': {reason}"),
    });
    wordy.chain(weasel)
}

/// Advances through every occurrence of the needle with word-boundary
/// filtering, growing the warnings set.
fn for_each_match(
    lower_masked: &str,
    raw_line: &str,
    line_num: usize,
    rules: impl Iterator<Item = PhraseRule>,
    warnings: &mut Vec<StyleWarning>,
) {
    for rule in rules {
        let mut search_from = 0;
        while let Some(found_idx) = lower_masked[search_from..].find(rule.needle) {
            let col = search_from + found_idx;
            search_from = col + rule.needle.len();

            if !is_word_boundary(lower_masked, col, rule.needle.len()) {
                continue;
            }
            warnings.push(StyleWarning {
                line: line_num,
                col: col + 1,
                length: rule.needle.len(),
                category: rule.category,
                matched_text: raw_line
                    .get(col..col + rule.needle.len())
                    .unwrap_or("")
                    .to_string(),
                suggestion: rule.suggestion.map(str::to_string),
                message: rule.message.clone(),
            });
        }
    }
}

/// Replaces math, macro, and comment regions with spaces while preserving
/// the byte length of the input, keeping all byte offsets valid in both the
/// masked and folded text.
fn mask_math_and_macros(line: &str, is_typst: bool) -> String {
    let mut masked = line.as_bytes().to_vec();
    let len = masked.len();

    if is_typst {
        if let Some(pos) = line.find("//") {
            masked[pos..].fill(b' ');
        }
    } else if let Some(pos) = line.find('%')
        && (pos == 0 || line.as_bytes().get(pos - 1) != Some(&b'\\'))
    {
        masked[pos..].fill(b' ');
    }

    let mut i = 0;
    while i < len {
        if masked[i] == b'$' {
            masked[i] = b' ';
            i += 1;
            while i < len && masked[i] != b'$' {
                masked[i] = b' ';
                i += 1;
            }
            if i < len {
                masked[i] = b' ';
                i += 1;
            }
            continue;
        }

        if !is_typst && masked[i] == b'\\' {
            let start = i;
            while i < len && masked[i].is_ascii_alphabetic() {
                masked[i] = b' ';
                i += 1;
            }
            if i < len && masked[i] == b'{' {
                while i < len && masked[i] != b'}' {
                    masked[i] = b' ';
                    i += 1;
                }
                if i < len {
                    masked[i] = b' ';
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

    // Every masked range covered whole characters, so the byte content is
    // valid UTF-8; conversions can only fail if masking logic is wrong.
    String::from_utf8(masked)
        .map_err(|_| ())
        .ok()
        .unwrap_or_else(|| line.to_string())
}

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
    fn lint_diagnostics_draw_ids_from_one_growing_sequence() {
        let first = lint_academic_warnings_as_diagnostics("was shown", false);
        let second = lint_academic_warnings_as_diagnostics("was shown", false);

        assert!(!first.is_empty() && !second.is_empty());
        let max_first = first.iter().map(|d| d.id.0).max().unwrap();
        let min_second = second.iter().map(|d| d.id.0).min().unwrap();
        assert!(
            min_second > max_first,
            "ids must never repeat or reset between lint runs"
        );
        for diagnostic in first.iter().chain(second.iter()) {
            assert_eq!(
                diagnostic.severity,
                crate::compiler::diagnostics::Severity::Warning
            );
            assert_eq!(
                diagnostic.source,
                crate::compiler::diagnostics::DiagnosticSource::Parser
            );
        }
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
        assert!(warnings.is_empty());
    }

    #[test]
    fn unicode_lines_never_panic_and_phrase_cols_stay_valid() {
        // Multi-byte characters, a math mask, and a phrase after them: the
        // old char-array masking changed byte lengths and `raw_line[col..]`
        // than sliced mid-character.
        let text = "İstanbul was shown clearly. Zusammenfassung due zum „Sehr“";
        let warnings = lint_academic_text(text, false);
        for warning in &warnings {
            assert_eq!(
                text.lines().nth(warning.line - 1),
                Some(text.lines().nth(warning.line - 1).unwrap())
            );
        }
    }

    #[test]
    fn math_masking_preserves_byte_lengths() {
        let line = "café $日本 very clear$ end";
        let masked = mask_math_and_macros(line, false);
        assert_eq!(masked.len(), line.len());
        // Masked region holds spaces; surrounding text is untouched.
        assert!(masked.starts_with("café"));
        assert!(masked.ends_with(" end"));
        assert!(!masked.contains("very"));
    }

    #[test]
    fn phrase_after_multibyte_characters_gets_correct_offset() {
        // "日本語" is 9 bytes before "clearly"; byte offsets must flow
        // through masking unchanged.
        let line = "日本語 clearly visible";
        let warnings = lint_academic_text(line, true);
        assert!(warnings.iter().any(|w| w.matched_text == "clearly"));
        let clearly = warnings
            .iter()
            .find(|w| w.matched_text == "clearly")
            .unwrap();
        assert_eq!(
            &line[clearly.col - 1..clearly.col - 1 + clearly.length],
            "clearly"
        );
    }
}
