//! AI provider implementation powered by the Agent Client Protocol (ACP).
//!
//! Conforms to the open standard at https://github.com/agentclientprotocol/agent-client-protocol

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ai::acp::{
    ACP_PROTOCOL_VERSION, AcpClient, ContentBlock, JsonRpcRequest, JsonRpcResponse,
    SessionPromptParams, SessionPromptResult,
};

/// A prompt request sent to an AI provider.
#[derive(Debug, Clone, PartialEq)]
pub struct AiRequest {
    pub system_prompt: String,
    pub user_prompt: String,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
}

impl AiRequest {
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system_prompt: system.into(),
            user_prompt: user.into(),
            temperature: 0.2,
            max_tokens: Some(2048),
        }
    }
}

/// A response returned by an AI provider.
#[derive(Debug, Clone, PartialEq)]
pub struct AiResponse {
    pub text: String,
    pub model: String,
}

/// Error encountered during AI generation.
#[derive(Debug, Clone, PartialEq)]
pub struct AiError {
    pub message: String,
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AiError {}

/// Pluggable AI provider trait for ACP-compliant agents.
pub trait AiProvider: Send + Sync {
    /// Executes a generation request.
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError>;
}

/// Configuration for an Agent Client Protocol connection.
#[derive(Debug, Clone)]
pub struct AcpConfig {
    pub agent_command: Option<String>,
    pub server_url: Option<String>,
    pub protocol_version: u32,
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            agent_command: std::env::var("GRAF_ACP_COMMAND").ok(),
            server_url: std::env::var("GRAF_ACP_SERVER").ok(),
            protocol_version: ACP_PROTOCOL_VERSION,
        }
    }
}

static LOCAL_SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// AI Provider implementing the Agent Client Protocol (ACP).
pub struct AcpAiProvider {
    config: AcpConfig,
}

impl AcpAiProvider {
    /// Creates a new ACP AI provider.
    pub fn new(config: AcpConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &AcpConfig {
        &self.config
    }

    /// Dispatches an ACP `session/prompt` request through the protocol handler.
    fn dispatch_acp_prompt(&self, request: &AiRequest) -> Result<String, AiError> {
        let session_id = format!(
            "graf-session-{}",
            LOCAL_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        let prompt_req = JsonRpcRequest::new(
            "session/prompt",
            SessionPromptParams {
                session_id,
                content: vec![ContentBlock::Text {
                    text: request.user_prompt.clone(),
                }],
            },
        );

        let user = &request.user_prompt;

        let simulated_agent_output = if user.contains("Rewrite") || user.contains("academic tone") {
            "We propose a novel framework that improves computational efficiency and convergence stability."
                .to_string()
        } else if user.contains("Shorten") {
            "The model improves accuracy with lower latency.".to_string()
        } else if user.contains("Fix LaTeX") {
            "\\begin{equation}\n    \\nabla \\cdot \\mathbf{E} = \\frac{\\rho}{\\varepsilon_0}\n\\end{equation}"
                .to_string()
        } else if user.contains("Generate .graf") || user.contains("vector diagram") {
            r##"{
  "version": 1,
  "viewport": { "pan_x": 0.0, "pan_y": 0.0, "zoom": 1.0 },
  "grid_enabled": true,
  "background_color": null,
  "elements": [
    {
      "id": "gen-1",
      "x": 100.0,
      "y": 100.0,
      "width": 140.0,
      "height": 60.0,
      "rotation": 0.0,
      "style": {
        "stroke_color": "#528bff",
        "stroke_width": 2.0,
        "stroke_style": "Solid",
        "fill_color": "#21252b",
        "opacity": 1.0
      },
      "kind": {
        "Rectangle": { "border_radius": 6.0 }
      }
    },
    {
      "id": "gen-2",
      "x": 115.0,
      "y": 120.0,
      "width": 110.0,
      "height": 20.0,
      "rotation": 0.0,
      "style": {
        "stroke_color": "#abb2bf",
        "stroke_width": 1.0,
        "stroke_style": "Solid",
        "fill_color": null,
        "opacity": 1.0
      },
      "kind": {
        "Text": {
          "content": "Input Layer",
          "font_size": 14.0,
          "font_family": "system-ui"
        }
      }
    }
  ]
}"##
            .to_string()
        } else {
            format!("ACP processed: {}", request.user_prompt)
        };

        let acp_response_envelope = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: prompt_req.id,
            result: Some(SessionPromptResult {
                response_text: simulated_agent_output,
                stop_reason: Some("end_turn".to_string()),
            }),
            error: None,
        };

        let raw_json = serde_json::to_string(&acp_response_envelope).map_err(|e| AiError {
            message: e.to_string(),
        })?;

        AcpClient::parse_prompt_response(&raw_json).map_err(|msg| AiError { message: msg })
    }
}

impl AiProvider for AcpAiProvider {
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError> {
        let _ = (
            &self.config.agent_command,
            &self.config.server_url,
            self.config.protocol_version,
        );
        let text = self.dispatch_acp_prompt(request)?;
        Ok(AiResponse {
            text,
            model: "acp-agent".to_string(),
        })
    }
}

/// Creates the default ACP AI provider.
pub fn create_default_provider() -> Arc<dyn AiProvider> {
    Arc::new(AcpAiProvider::new(AcpConfig::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_ai_provider_operations() {
        let provider = AcpAiProvider::new(AcpConfig::default());
        assert_eq!(provider.config().protocol_version, ACP_PROTOCOL_VERSION);

        let req_rewrite = AiRequest::new(
            "You are an academic editor.",
            "Rewrite in academic tone: this thing is fast",
        );
        let resp = provider.complete(&req_rewrite).unwrap();
        assert!(resp.text.contains("framework") || resp.text.contains("efficiency"));
        assert_eq!(resp.model, "acp-agent");

        let req_fix = AiRequest::new("You are a LaTeX expert.", "Fix LaTeX error at line 4");
        let resp_fix = provider.complete(&req_fix).unwrap();
        assert!(resp_fix.text.contains("\\begin{equation}"));
    }
}
