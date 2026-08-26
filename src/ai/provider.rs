use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq)]
pub struct AiResponse {
    pub text: String,
    pub model: String,
}

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

pub trait AiProvider: Send + Sync {
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

impl OpenAiConfig {
    pub fn from_env(base_url: Option<String>, model: Option<String>) -> Self {
        Self {
            base_url: base_url
                .or_else(|| std::env::var("GRAF_AI_BASE_URL").ok())
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("GRAF_AI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty()),
            model: model
                .or_else(|| std::env::var("GRAF_AI_MODEL").ok())
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

pub struct OpenAiCompatibleProvider {
    config: OpenAiConfig,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn chat_request<'a>(&'a self, request: &'a AiRequest) -> ChatCompletionRequest<'a> {
        ChatCompletionRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &request.system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: &request.user_prompt,
                },
            ],
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        }
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError> {
        let Some(api_key) = &self.config.api_key else {
            return Err(AiError {
                message: "No AI provider configured. Set GRAF_AI_API_KEY to use an \
                          OpenAI-compatible endpoint."
                    .to_string(),
            });
        };

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .timeout(Duration::from_secs(120))
            .send_json(self.chat_request(request))
            .map_err(|error| AiError {
                message: format!("AI request failed: {error}"),
            })?;

        let completion: ChatCompletionResponse = response.into_json().map_err(|error| AiError {
            message: format!("Failed to parse AI response: {error}"),
        })?;

        let content = completion
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| AiError {
                message: "AI response contained no choices".to_string(),
            })?;

        Ok(AiResponse {
            text: content.trim().to_string(),
            model: self.config.model.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AcpConfig {
    pub agent_command: Option<String>,
    pub server_url: Option<String>,
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            agent_command: std::env::var("GRAF_ACP_COMMAND").ok(),
            server_url: std::env::var("GRAF_ACP_SERVER").ok(),
        }
    }
}

pub struct AcpAiProvider {
    config: AcpConfig,
}

impl AcpAiProvider {
    pub fn new(config: AcpConfig) -> Self {
        Self { config }
    }
}

impl AiProvider for AcpAiProvider {
    fn complete(&self, _request: &AiRequest) -> Result<AiResponse, AiError> {
        Err(AiError {
            message: if self.config.agent_command.is_none() && self.config.server_url.is_none() {
                "No ACP agent is configured".to_string()
            } else {
                "ACP transport is not implemented".to_string()
            },
        })
    }
}

pub fn create_default_provider(
    base_url: Option<String>,
    model: Option<String>,
) -> Arc<dyn AiProvider> {
    let config = OpenAiConfig::from_env(base_url, model);
    if config.api_key.is_some() {
        Arc::new(OpenAiCompatibleProvider::new(config))
    } else {
        Arc::new(AcpAiProvider::new(AcpConfig::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_chat_completion_request() {
        let provider = OpenAiCompatibleProvider::new(OpenAiConfig {
            base_url: "https://example.com/v1".to_string(),
            api_key: Some("key".to_string()),
            model: "test-model".to_string(),
        });
        let request = AiRequest::new("system prompt", "user prompt");

        let body = serde_json::to_value(provider.chat_request(&request)).expect("serialize");

        assert_eq!(body["model"], "test-model");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "system prompt");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "user prompt");
        assert_eq!(body["max_tokens"], 2048);
    }

    #[test]
    fn parses_chat_completion_response() {
        let json = r#"{
            "choices": [
                {"message": {"role": "assistant", "content": "  rewritten text  "}}
            ]
        }"#;
        let parsed: ChatCompletionResponse = serde_json::from_str(json).expect("parse");

        assert_eq!(parsed.choices[0].message.content.trim(), "rewritten text");
    }

    #[test]
    fn unconfigured_provider_reports_missing_key() {
        let provider = OpenAiCompatibleProvider::new(OpenAiConfig {
            base_url: "https://example.com/v1".to_string(),
            api_key: None,
            model: "test-model".to_string(),
        });
        let request = AiRequest::new("system", "prompt");

        let error = provider.complete(&request).unwrap_err();

        assert!(error.message.contains("GRAF_AI_API_KEY"));
    }
}
