use crate::ai::provider::{AiProvider, AiRequest};
use crate::canvas::scene::CanvasDocument;

/// Upper bound on document text sent to a provider in one request. Without
/// a cap, a 100KB chapter is shipped per op at typing-speed latency.
const MAX_AI_CONTEXT_CHARS: usize = 12_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiOperationKind {
    RewriteAcademic,
    Shorten,
    Explain,
    FixDiagnostic {
        message: String,
        line: Option<usize>,
    },
    GenerateDiagram {
        prompt: String,
    },
}

impl AiOperationKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::RewriteAcademic => "Polish Academic Tone",
            Self::Shorten => "Shorten and Condense",
            Self::Explain => "Explain Formula or Section",
            Self::FixDiagnostic { .. } => "Fix Compiler Error",
            Self::GenerateDiagram { .. } => "Generate Vector Diagram",
        }
    }
}

fn request_for_operation(kind: &AiOperationKind, context: &str) -> AiRequest {
    let (system_prompt, user_prompt) = match kind {
        AiOperationKind::RewriteAcademic => (
            "You are an expert academic editor for peer-reviewed technical publications. Rewrite the selected text to enhance clarity, conciseness, and academic rigor while preserving all technical terminology and LaTeX equations verbatim. Return only the revised text.",
            format!("Rewrite the following passage in a formal academic tone:\n\n{context}"),
        ),
        AiOperationKind::Shorten => (
            "You are a technical editor. Condense the text significantly while maintaining all essential technical findings, formulas, and references.",
            format!("Shorten the following text:\n\n{context}"),
        ),
        AiOperationKind::Explain => (
            "You are a computer science and mathematics professor. Clearly explain the selected LaTeX expression, algorithm, or text in 2-3 concise paragraphs.",
            format!("Explain the following LaTeX / technical concept:\n\n{context}"),
        ),
        AiOperationKind::FixDiagnostic { message, line } => (
            "You are a LaTeX typesetting compiler expert. Analyze the compilation error and the surrounding source code, and provide the exact corrected LaTeX replacement block. Return only the corrected LaTeX snippet.",
            format!(
                "Fix LaTeX compilation error: \"{}\" (at line {:?})\nSurrounding code:\n{}",
                message, line, context
            ),
        ),
        AiOperationKind::GenerateDiagram { prompt } => (
            "You are a technical diagram designer. Output a valid JSON CanvasDocument scene graph (.graf format) matching the requested architecture.",
            format!("Generate .graf vector diagram JSON for: {prompt}"),
        ),
    };

    AiRequest::new(system_prompt, user_prompt)
}

pub fn execute_operation(
    provider: &dyn AiProvider,
    kind: &AiOperationKind,
    context: &str,
) -> Result<String, String> {
    let response = provider
        .complete(&request_for_operation(kind, context))
        .map_err(|e| format!("AI generation failed: {e}"))?;

    Ok(response.text.trim().to_string())
}

/// Builds the request payload with the provider-boundary context cap:
/// the selection when one exists, else the document's last
/// `MAX_AI_CONTEXT_CHARS` characters, with an elision marker.
pub fn build_ai_context(is_selection: bool, text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_AI_CONTEXT_CHARS {
        return trimmed.to_string();
    }
    let truncated = last_chars(trimmed, MAX_AI_CONTEXT_CHARS);
    if is_selection {
        format!("[selection truncated]\n{truncated}")
    } else {
        format!("[document truncated to last {MAX_AI_CONTEXT_CHARS} characters]\n{truncated}")
    }
}

/// Copies at most `max_chars` trailing characters of `text`, never cutting
/// mid-character, starting on a line break when one is handy.
fn last_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    // Byte offset where the last `max_chars` characters begin.
    let start = text
        .char_indices()
        .nth_back(max_chars - 1)
        .map(|(byte, _)| byte)
        .unwrap_or(0);
    let tail = &text[start..];
    // The cut may split a line in half; skip to the next line break when the
    // chopped-off fragment is short so the model sees whole lines.
    match tail.find('\n') {
        Some(breakpoint) if breakpoint < max_chars / 2 => tail[breakpoint + 1..].to_string(),
        _ => tail.to_string(),
    }
}

pub fn parse_canvas_response(response: &str) -> Result<CanvasDocument, String> {
    // Fenced blocks can sit anywhere in prose ("Here is your diagram: ...");
    // the old string-edge trims missed those cases entirely.
    let fenced = extract_fences(response);
    if fenced.is_empty() {
        return CanvasDocument::from_json(response.trim())
            .map_err(|e| format!("Invalid generated .graf JSON: {e}"));
    }

    for candidate in fenced {
        if let Ok(document) = CanvasDocument::from_json(candidate) {
            return Ok(document);
        }
    }
    Err(
        "Response contained no parseable .graf JSON (fenced blocks present but invalid)"
            .to_string(),
    )
}

/// All content inside ``` blocks, in order; unterminated closers treat the
/// remainder as body, matching how models often leave trailing fences.
fn extract_fences(response: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = response;
    while let Some(open) = rest.find("```") {
        let after_open = &rest[open + 3..];
        if let Some(newline) = after_open.find('\n') {
            let body = &after_open[newline + 1..];
            if let Some(close) = body.find("```") {
                blocks.push(body[..close].trim());
                rest = &body[close + 3..];
            } else {
                blocks.push(body.trim());
                break;
            }
        } else {
            blocks.push(after_open.trim());
            break;
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_operation_request_from_context() {
        let request = request_for_operation(
            &AiOperationKind::FixDiagnostic {
                message: "Undefined control sequence".to_string(),
                line: Some(12),
            },
            "\\begin{equation}\n ... \\end{equation}",
        );

        assert!(request.user_prompt.contains("Undefined control sequence"));
        assert!(request.user_prompt.contains("\\begin{equation}"));
    }

    #[test]
    fn parses_fenced_canvas_document() {
        let json = CanvasDocument::new().to_json().expect("serialize canvas");
        let response = format!("```json\n{json}\n```");

        let document = parse_canvas_response(&response).expect("valid canvas document");

        assert!(document.elements.is_empty());
    }

    #[test]
    fn parses_fences_in_the_middle_of_prose() {
        let json = CanvasDocument::new().to_json().expect("serialize canvas");
        let response = format!("Here is your diagram:\n\n```json\n{json}\n```\n\nEnjoy.");

        let document = parse_canvas_response(&response).expect("fenced mid-prose block");

        assert!(document.elements.is_empty());
    }

    #[test]
    fn unfenced_canvas_still_parses_and_broken_fences_do_not_loose_data() {
        let json = CanvasDocument::new().to_json().expect("serialize canvas");
        assert!(parse_canvas_response(&json).is_ok());

        // Fences present but unparsable content: an explicit failure is
        // better than a half-parsed snapshot or an empty document.
        let bad = "```json\nthis is not json\n```";
        assert!(parse_canvas_response(bad).is_err());
    }

    #[test]
    fn short_documents_pass_through_uncapped() {
        let text = "\\section{Intro}\nA short note.\n";
        assert_eq!(build_ai_context(false, text), text.trim());
    }

    #[test]
    fn oversized_documents_are_truncated_to_the_cap() {
        let capped_text = "q".repeat(MAX_AI_CONTEXT_CHARS + 200);
        let context = build_ai_context(false, &capped_text);
        assert!(context.starts_with(&format!(
            "[document truncated to last {MAX_AI_CONTEXT_CHARS} characters]"
        )));

        let context_selection = build_ai_context(true, &"x".repeat(MAX_AI_CONTEXT_CHARS + 5));
        assert!(context_selection.starts_with("[selection truncated]"));
    }

    #[test]
    fn truncation_never_splits_characters() {
        let japan = "日本語".repeat(MAX_AI_CONTEXT_CHARS);
        let text = format!("{japan}XYZ");
        let context = build_ai_context(false, &text);
        // No replacement character can appear: the slice is char-aligned.
        assert!(!context.contains('\u{FFFD}'));
        assert!(context.ends_with("XYZ"));
    }
}
