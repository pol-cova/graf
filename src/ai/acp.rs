//! Agent Client Protocol (ACP) specification implementation (https://github.com/agentclientprotocol/agent-client-protocol).
//!
//! Provides JSON-RPC 2.0 client and protocol types for connecting Graf to any ACP-compliant agent.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Current supported Agent Client Protocol version.
pub const ACP_PROTOCOL_VERSION: u32 = 1;

static NEXT_RPC_ID: AtomicU64 = AtomicU64::new(1);

/// Standard JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: T,
}

impl<T: Serialize> JsonRpcRequest<T> {
    pub fn new(method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: NEXT_RPC_ID.fetch_add(1, Ordering::Relaxed),
            method: method.into(),
            params,
        }
    }
}

/// Standard JSON-RPC 2.0 Response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Standard JSON-RPC 2.0 Error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// `initialize` request params sent by client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InitializeParams {
    pub protocol_version: u32,
    pub client_info: ClientInfo,
    pub capabilities: ClientCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            name: "Graf".to_string(),
            version: "0.1.0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClientCapabilities {
    pub file_system: bool,
    pub terminal: bool,
}

/// `initialize` response returned by agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitializeResult {
    pub protocol_version: u32,
    pub agent_info: AgentInfo,
    pub capabilities: AgentCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub tool_calls: bool,
}

/// `session/new` request params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionNewParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

/// `session/new` response result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionNewResult {
    pub session_id: String,
}

/// Content block in `session/prompt`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
}

/// `session/prompt` request params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionPromptParams {
    pub session_id: String,
    pub content: Vec<ContentBlock>,
}

/// `session/prompt` response result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionPromptResult {
    pub response_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

/// A connected ACP Client handler.
pub struct AcpClient;

impl AcpClient {
    /// Encodes an initialize JSON-RPC message.
    pub fn build_initialize_message() -> String {
        let req = JsonRpcRequest::new(
            "initialize",
            InitializeParams {
                protocol_version: ACP_PROTOCOL_VERSION,
                client_info: ClientInfo::default(),
                capabilities: ClientCapabilities::default(),
            },
        );
        serde_json::to_string(&req).unwrap_or_default()
    }

    /// Encodes a session/new JSON-RPC message.
    pub fn build_session_new_message(system_prompt: Option<String>) -> String {
        let req = JsonRpcRequest::new("session/new", SessionNewParams { system_prompt });
        serde_json::to_string(&req).unwrap_or_default()
    }

    /// Encodes a session/prompt JSON-RPC message.
    pub fn build_prompt_message(
        session_id: impl Into<String>,
        prompt: impl Into<String>,
    ) -> String {
        let req = JsonRpcRequest::new(
            "session/prompt",
            SessionPromptParams {
                session_id: session_id.into(),
                content: vec![ContentBlock::Text {
                    text: prompt.into(),
                }],
            },
        );
        serde_json::to_string(&req).unwrap_or_default()
    }

    /// Parses a JSON-RPC response message for prompt completion.
    pub fn parse_prompt_response(raw_json: &str) -> Result<String, String> {
        let resp: JsonRpcResponse<SessionPromptResult> = serde_json::from_str(raw_json)
            .map_err(|e| format!("Failed to parse ACP JSON-RPC response: {e}"))?;

        if let Some(err) = resp.error {
            return Err(format!(
                "ACP Agent Error (code {}): {}",
                err.code, err.message
            ));
        }

        if let Some(res) = resp.result {
            Ok(res.response_text)
        } else {
            Err("Empty ACP result payload".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_message_builders_and_parsers() {
        let init_json = AcpClient::build_initialize_message();
        assert!(init_json.contains("\"method\":\"initialize\""));
        assert!(init_json.contains("\"protocol_version\":1"));

        let session_json = AcpClient::build_session_new_message(Some("System prompt".to_string()));
        assert!(session_json.contains("\"method\":\"session/new\""));

        let prompt_json = AcpClient::build_prompt_message("sess-123", "Write a paper intro");
        assert!(prompt_json.contains("\"method\":\"session/prompt\""));
        assert!(prompt_json.contains("\"session_id\":\"sess-123\""));

        let mock_response = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "response_text": "Here is the academic draft.",
                "stop_reason": "end_turn"
            }
        }"#;

        let parsed = AcpClient::parse_prompt_response(mock_response).unwrap();
        assert_eq!(parsed, "Here is the academic draft.");
    }

    #[test]
    fn test_acp_json_rpc_schema_roundtrip() {
        let init_json = AcpClient::build_initialize_message();
        let parsed_req: JsonRpcRequest<InitializeParams> =
            serde_json::from_str(&init_json).expect("Must deserialize InitializeParams");
        assert_eq!(parsed_req.jsonrpc, "2.0");
        assert_eq!(parsed_req.method, "initialize");
        assert_eq!(parsed_req.params.protocol_version, ACP_PROTOCOL_VERSION);

        // Error response handling
        let err_response = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "error": {
                "code": -32600,
                "message": "Invalid session ID"
            }
        }"#;
        let err_res = AcpClient::parse_prompt_response(err_response);
        assert!(err_res.is_err());
        assert!(err_res.unwrap_err().contains("Invalid session ID"));
    }
}
