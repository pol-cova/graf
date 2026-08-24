//! Structured technical-writing AI operations (spec §M5.3, §M5.4, §M5.6).

use crate::ai::provider::{AiProvider, AiRequest};
use crate::canvas::scene::CanvasDocument;

/// Technical writing operation kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiOperationKind {
    /// Polish prose to improve academic clarity, flow, and rigorous tone.
    RewriteAcademic,
    /// Shorten and condense technical paragraphs.
    Shorten,
    /// Explain mathematical notation or LaTeX environment.
    Explain,
    /// Fix a compiler diagnostic error given surrounding LaTeX lines.
    FixDiagnostic {
        message: String,
        line: Option<usize>,
    },
    /// Generate a vector diagram scene graph from natural language.
    GenerateDiagram { prompt: String },
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

/// Executes a technical writing AI operation against the selected context.
pub fn execute_operation(
    provider: &dyn AiProvider,
    kind: &AiOperationKind,
    context: &str,
) -> Result<String, String> {
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

    let request = AiRequest::new(system_prompt, user_prompt);
    let response = provider
        .complete(&request)
        .map_err(|e| format!("AI generation failed: {e}"))?;

    Ok(response.text.trim().to_string())
}

/// Parses an AI response into a valid [`CanvasDocument`] if possible.
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
    use crate::ai::provider::{AiError, AiResponse};

    struct StubProvider;

    impl AiProvider for StubProvider {
        fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError> {
            let text = if request.user_prompt.contains("Generate .graf") {
                CanvasDocument::new().to_json().unwrap()
            } else if request.user_prompt.contains("Fix LaTeX") {
                "\\begin{equation}\nE = mc^2\n\\end{equation}".to_string()
            } else {
                "Rewritten text".to_string()
            };
            Ok(AiResponse {
                text,
                model: "test".to_string(),
            })
        }
    }

    #[test]
    fn test_execute_ai_operations() {
        let provider = StubProvider;

        let rewritten = execute_operation(
            &provider,
            &AiOperationKind::RewriteAcademic,
            "we did this because it is faster",
        )
        .unwrap();
        assert_eq!(rewritten, "Rewritten text");

        let fixed = execute_operation(
            &provider,
            &AiOperationKind::FixDiagnostic {
                message: "Undefined control sequence".to_string(),
                line: Some(12),
            },
            "\\begin{equation}\n ... \\end{equation}",
        )
        .unwrap();
        assert!(fixed.contains("\\begin{equation}"));
    }

    #[test]
    fn test_generate_canvas_diagram_parsing() {
        let provider = StubProvider;
        let json = execute_operation(
            &provider,
            &AiOperationKind::GenerateDiagram {
                prompt: "Transformer architecture".to_string(),
            },
            "",
        )
        .unwrap();

        let document = parse_canvas_response(&json).expect("valid canvas document");
        assert!(document.elements.is_empty());
    }
}
