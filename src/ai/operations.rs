use crate::ai::provider::{AiProvider, AiRequest};
use crate::canvas::scene::CanvasDocument;

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

pub fn parse_canvas_response(response: &str) -> Result<CanvasDocument, String> {
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    CanvasDocument::from_json(cleaned).map_err(|e| format!("Invalid generated .graf JSON: {e}"))
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
}
