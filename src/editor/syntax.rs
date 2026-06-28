//! Fast zero-allocation lexical tokenizer and syntax highlighting for LaTeX and Typst source files.

use gpui::{Font, TextRun};

use crate::ui::theme;

/// Category of syntax token for document formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Command,
    Math,
    Comment,
    Punctuation,
    Plain,
}

impl TokenKind {
    /// Returns the theme color for this syntax token.
    pub fn color(self) -> gpui::Rgba {
        match self {
            Self::Command => theme::color(theme::SYNTAX_COMMAND),
            Self::Math => theme::color(theme::SYNTAX_MATH),
            Self::Comment => theme::color(theme::SYNTAX_COMMENT),
            Self::Punctuation => theme::color(theme::SYNTAX_PUNCTUATION),
            Self::Plain => theme::color(theme::TEXT),
        }
    }
}

/// Tokenizes a single line of LaTeX text into stylized [`TextRun`]s with safe zero-allocation `char_indices` scanning.
pub fn highlight_latex_line(line: &str, font: Font) -> Vec<TextRun> {
    if line.is_empty() {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some(&(start, ch)) = chars.peek() {
        if ch == '%' {
            let byte_len = line.len() - start;
            runs.push(TextRun {
                len: byte_len,
                font: font.clone(),
                color: TokenKind::Comment.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            break;
        }

        if ch == '\\' {
            chars.next();
            let mut end = line.len();
            if let Some(&(_, next_ch)) = chars.peek() {
                if next_ch.is_alphabetic() {
                    while let Some(&(c_idx, c)) = chars.peek() {
                        if !c.is_alphabetic() {
                            end = c_idx;
                            break;
                        }
                        chars.next();
                    }
                } else {
                    chars.next();
                    end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
                }
            }
            let byte_len = chars.peek().map_or(line.len(), |&(idx, _)| idx.min(end)) - start;
            runs.push(TextRun {
                len: byte_len,
                font: font.clone(),
                color: TokenKind::Command.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        if ch == '$' {
            chars.next();
            let is_double = chars.peek().is_some_and(|&(_, c)| c == '$');
            if is_double {
                chars.next();
            }

            while let Some(&(_, c)) = chars.peek() {
                if c == '$' {
                    chars.next();
                    if is_double && chars.peek().is_some_and(|&(_, c2)| c2 == '$') {
                        chars.next();
                    }
                    break;
                } else if c == '\\' {
                    chars.next();
                    chars.next();
                } else {
                    chars.next();
                }
            }

            let end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: TokenKind::Math.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        if ch == '{' || ch == '}' || ch == '[' || ch == ']' {
            chars.next();
            runs.push(TextRun {
                len: ch.len_utf8(),
                font: font.clone(),
                color: TokenKind::Punctuation.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        while let Some(&(_, next_ch)) = chars.peek() {
            if next_ch == '\\'
                || next_ch == '%'
                || next_ch == '$'
                || next_ch == '{'
                || next_ch == '}'
                || next_ch == '['
                || next_ch == ']'
            {
                break;
            }
            chars.next();
        }

        let end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
        if end > start {
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: TokenKind::Plain.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
    }

    runs
}

/// Tokenizes a single line of Typst text into stylized [`TextRun`]s with safe zero-allocation `char_indices` scanning.
pub fn highlight_typst_line(line: &str, font: Font) -> Vec<TextRun> {
    if line.is_empty() {
        return Vec::new();
    }

    if line.starts_with('=') {
        let eq_count = line.bytes().take_while(|&b| b == b'=').count();
        if line.as_bytes().get(eq_count) == Some(&b' ') {
            return vec![TextRun {
                len: line.len(),
                font,
                color: TokenKind::Command.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
        }
    }

    let mut runs = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some(&(start, ch)) = chars.peek() {
        if ch == '/' {
            let mut clone_chars = chars.clone();
            clone_chars.next();
            if clone_chars.peek().is_some_and(|&(_, c)| c == '/') {
                let byte_len = line.len() - start;
                runs.push(TextRun {
                    len: byte_len,
                    font: font.clone(),
                    color: TokenKind::Comment.color().into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
                break;
            }
        }

        if ch == '#' {
            chars.next();
            while let Some(&(_, c)) = chars.peek() {
                if !c.is_alphanumeric() && c != '_' {
                    break;
                }
                chars.next();
            }
            let end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: TokenKind::Command.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        if ch == '$' {
            chars.next();
            while let Some(&(_, c)) = chars.peek() {
                if c == '$' {
                    chars.next();
                    break;
                } else if c == '\\' {
                    chars.next();
                    chars.next();
                } else {
                    chars.next();
                }
            }
            let end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: TokenKind::Math.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        if ch == '"' {
            chars.next();
            while let Some(&(_, c)) = chars.peek() {
                if c == '"' {
                    chars.next();
                    break;
                } else if c == '\\' {
                    chars.next();
                    chars.next();
                } else {
                    chars.next();
                }
            }
            let end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: TokenKind::Punctuation.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        if ch == '[' || ch == ']' || ch == '{' || ch == '}' || ch == '(' || ch == ')' {
            chars.next();
            runs.push(TextRun {
                len: ch.len_utf8(),
                font: font.clone(),
                color: TokenKind::Punctuation.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            continue;
        }

        while let Some(&(_, next_ch)) = chars.peek() {
            if next_ch == '#'
                || next_ch == '$'
                || next_ch == '"'
                || next_ch == '['
                || next_ch == ']'
                || next_ch == '{'
                || next_ch == '}'
                || next_ch == '('
                || next_ch == ')'
            {
                break;
            }
            if next_ch == '/' {
                let mut clone_chars = chars.clone();
                clone_chars.next();
                if clone_chars.peek().is_some_and(|&(_, c)| c == '/') {
                    break;
                }
            }
            chars.next();
        }

        let end = chars.peek().map_or(line.len(), |&(idx, _)| idx);
        if end > start {
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: TokenKind::Plain.color().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
    }

    runs
}

/// Highlight a line of text based on language type (LaTeX vs Typst).
pub fn highlight_line(line: &str, font: Font, is_typst: bool) -> Vec<TextRun> {
    if is_typst {
        highlight_typst_line(line, font)
    } else {
        highlight_latex_line(line, font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_latex_line_empty() {
        let font = Font::default();
        let runs = highlight_latex_line("", font);
        assert!(runs.is_empty());
    }

    #[test]
    fn test_highlight_latex_line_command_and_comment() {
        let font = Font::default();
        let line = r#"\documentclass{article} % main class"#;
        let runs = highlight_latex_line(line, font);

        assert_eq!(runs.len(), 6);
        assert_eq!(runs[0].len, 14);
        assert_eq!(runs[1].len, 1);
        assert_eq!(runs[2].len, 7);
        assert_eq!(runs[3].len, 1);
        assert_eq!(runs[4].len, 1);
        assert_eq!(runs[5].len, 12);
    }

    #[test]
    fn test_highlight_latex_line_math() {
        let font = Font::default();
        let line = r#"Formula: $E=mc^2$ is famous."#;
        let runs = highlight_latex_line(line, font);

        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].len, 9);
        assert_eq!(runs[1].len, 8);
        assert_eq!(runs[2].len, 11);
    }

    #[test]
    fn test_highlight_typst_line() {
        let font = Font::default();
        let line = "= Introduction to Typst";
        let runs = highlight_typst_line(line, font.clone());
        assert_eq!(runs.len(), 1);

        let code_line = "#set page(paper: \"a4\") // page setup";
        let code_runs = highlight_typst_line(code_line, font.clone());
        assert!(!code_runs.is_empty());

        let math_line = "The formula is $x^2 + y^2 = r^2$ in 2D.";
        let math_runs = highlight_typst_line(math_line, font);
        assert_eq!(math_runs.len(), 3);
    }

    #[test]
    fn test_pathological_syntax_stress() {
        let font = Font::default();

        let unclosed_math = r#"Some text $x + y = z with no closing dollar"#;
        let runs = highlight_latex_line(unclosed_math, font.clone());
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].len, 10);
        assert_eq!(runs[1].color, TokenKind::Math.color().into());

        let trailing_slash = r#"Command trailing \"#;
        let runs = highlight_latex_line(trailing_slash, font.clone());
        assert!(!runs.is_empty());

        let unclosed_typst = r#"#let name = "unclosed string without end"#;
        let runs_typst = highlight_typst_line(unclosed_typst, font);
        assert!(!runs_typst.is_empty());
    }
}
