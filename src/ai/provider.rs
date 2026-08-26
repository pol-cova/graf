use std::sync::Arc;

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

pub fn create_default_provider() -> Arc<dyn AiProvider> {
    Arc::new(AcpAiProvider::new(AcpConfig::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_provider_returns_an_error() {
        let provider = AcpAiProvider::new(AcpConfig {
            agent_command: None,
            server_url: None,
        });
        let request = AiRequest::new("system", "prompt");

        let error = provider.complete(&request).unwrap_err();

        assert_eq!(error.message, "No ACP agent is configured");
    }
}
