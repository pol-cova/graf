use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::project::settings::{AcpSettings, AiProviderKind, AiSettings};

use serde::{Deserialize, Serialize};

const DEFAULT_AI_TIMEOUT_SECONDS: u64 = 120;

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
            .timeout(Duration::from_secs(DEFAULT_AI_TIMEOUT_SECONDS))
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
    pub command: Option<PathBuf>,
    pub args: Vec<String>,
    pub timeout: Duration,
}

impl AcpConfig {
    fn from_settings(settings: &AcpSettings) -> Self {
        let command = std::env::var_os("GRAF_ACP_COMMAND")
            .map(PathBuf::from)
            .or_else(|| settings.command.clone());
        let args = std::env::var("GRAF_ACP_ARGS")
            .ok()
            .and_then(|args| serde_json::from_str(&args).ok())
            .unwrap_or_else(|| settings.args.clone());
        let timeout_seconds = if settings.timeout_seconds == 0 {
            DEFAULT_AI_TIMEOUT_SECONDS
        } else {
            settings.timeout_seconds
        };
        Self {
            command,
            args,
            timeout: Duration::from_secs(timeout_seconds),
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
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiError> {
        let command = self.config.command.as_deref().ok_or_else(|| AiError {
            message: "No ACP agent is configured. Set ai.acp.command or GRAF_ACP_COMMAND."
                .to_string(),
        })?;
        let cwd = std::env::current_dir().map_err(|error| AiError {
            message: format!("Failed to determine ACP working directory: {error}"),
        })?;
        let mut client =
            crate::ai::acp::AcpClient::connect(command, &self.config.args, self.config.timeout)
                .map_err(|message| AiError { message })?;
        let model = client.initialize().map_err(|message| AiError { message })?;
        let text = client
            .complete(&cwd, &request.system_prompt, &request.user_prompt)
            .map_err(|message| AiError { message })?;
        Ok(AiResponse { text, model })
    }
}

pub struct DisabledAiProvider;

impl AiProvider for DisabledAiProvider {
    fn complete(&self, _request: &AiRequest) -> Result<AiResponse, AiError> {
        Err(AiError {
            message: "AI is disabled in settings".to_string(),
        })
    }
}

pub fn create_provider(settings: &AiSettings) -> Arc<dyn AiProvider> {
    let provider = std::env::var("GRAF_AI_PROVIDER")
        .ok()
        .and_then(|value| match value.as_str() {
            "acp" => Some(AiProviderKind::Acp),
            "openai_compatible" => Some(AiProviderKind::OpenAiCompatible),
            "disabled" => Some(AiProviderKind::Disabled),
            _ => None,
        })
        .unwrap_or_else(|| settings.provider.clone());
    match provider {
        AiProviderKind::Acp => {
            Arc::new(AcpAiProvider::new(AcpConfig::from_settings(&settings.acp)))
        }
        AiProviderKind::OpenAiCompatible => Arc::new(OpenAiCompatibleProvider::new(
            OpenAiConfig::from_env(settings.base_url.clone(), settings.model.clone()),
        )),
        AiProviderKind::Disabled => Arc::new(DisabledAiProvider),
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
    fn reads_acp_settings() {
        let config = AcpConfig::from_settings(&AcpSettings {
            command: Some(PathBuf::from("/usr/local/bin/agent")),
            args: vec!["--acp".to_string()],
            timeout_seconds: 45,
        });

        assert_eq!(config.command, Some(PathBuf::from("/usr/local/bin/agent")));
        assert_eq!(config.args, vec!["--acp"]);
        assert_eq!(config.timeout, Duration::from_secs(45));
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
