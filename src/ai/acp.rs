use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};

const PROTOCOL_VERSION: u16 = 1;
const MAX_STDERR_BYTES: usize = 8 * 1024;

pub struct AcpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    messages: Receiver<Result<Value, String>>,
    next_id: u64,
    stderr: std::thread::JoinHandle<String>,
    timeout: Duration,
}

impl AcpClient {
    pub fn connect(command: &Path, args: &[String], timeout: Duration) -> Result<Self, String> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Failed to start ACP agent {}: {error}", command.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or("ACP agent stdin was unavailable")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("ACP agent stdout was unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("ACP agent stderr was unavailable")?;
        let (sender, messages) = mpsc::channel();

        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let message = line
                    .map_err(|error| format!("Failed to read ACP output: {error}"))
                    .and_then(|line| {
                        serde_json::from_str(&line)
                            .map_err(|error| format!("Invalid ACP JSON-RPC message: {error}"))
                    });
                if sender.send(message).is_err() {
                    return;
                }
            }
        });

        let stderr = std::thread::spawn(move || {
            let mut output = String::new();
            let _ = BufReader::new(stderr)
                .take(MAX_STDERR_BYTES as u64)
                .read_to_string(&mut output);
            output
        });

        Ok(Self {
            child,
            stdin,
            messages,
            next_id: 1,
            stderr,
            timeout,
        })
    }

    pub fn initialize(&mut self) -> Result<String, String> {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {},
                "clientInfo": { "name": "graf", "version": env!("CARGO_PKG_VERSION") }
            }),
        )?;
        let version = response
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .ok_or("ACP initialize response omitted protocolVersion")?;
        if version != u64::from(PROTOCOL_VERSION) {
            return Err(format!(
                "ACP agent selected protocol version {version}; graf supports version {PROTOCOL_VERSION}"
            ));
        }
        Ok(response
            .pointer("/agentInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("ACP agent")
            .to_string())
    }

    pub fn complete(
        &mut self,
        cwd: &Path,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, String> {
        let session = self.request("session/new", json!({ "cwd": cwd, "mcpServers": [] }))?;
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or("ACP session/new response omitted sessionId")?
            .to_string();
        let prompt = format!("{system_prompt}\n\n{user_prompt}");
        let id = self.send_request(
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": prompt }]
            }),
        )?;
        let mut output = String::new();
        self.wait_for_response(id, &mut output)?;
        if output.trim().is_empty() {
            return Err("ACP agent completed without a text response".to_string());
        }
        Ok(output)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send_request(method, params)?;
        let mut ignored = String::new();
        self.wait_for_response(id, &mut ignored)
    }

    fn send_request(&mut self, method: &str, params: Value) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        serde_json::to_writer(&mut self.stdin, &message)
            .map_err(|error| format!("Failed to serialize ACP request: {error}"))?;
        self.stdin
            .write_all(b"\n")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("Failed to send ACP request: {error}"))?;
        Ok(id)
    }

    fn wait_for_response(&mut self, id: u64, output: &mut String) -> Result<Value, String> {
        loop {
            let message = self.messages.recv_timeout(self.timeout).map_err(|error| {
                format!(
                    "ACP agent did not respond within {} seconds: {error}",
                    self.timeout.as_secs()
                )
            })??;
            if let Some(method) = message.get("method").and_then(Value::as_str) {
                if method == "session/update" {
                    if let Some(text) = message
                        .pointer("/params/update/content/text")
                        .and_then(Value::as_str)
                        .filter(|_| {
                            message
                                .pointer("/params/update/sessionUpdate")
                                .and_then(Value::as_str)
                                == Some("agent_message_chunk")
                        })
                    {
                        output.push_str(text);
                    }
                } else if let Some(request_id) = message.get("id").and_then(Value::as_u64) {
                    self.send_error(
                        request_id,
                        -32601,
                        "Graf does not support ACP agent requests",
                    )?;
                }
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown ACP error");
                return Err(format!("ACP agent error: {detail}"));
            }
            return message
                .get("result")
                .cloned()
                .ok_or("ACP response omitted result".to_string());
        }
    }

    fn send_error(&mut self, id: u64, code: i64, message: &str) -> Result<(), String> {
        serde_json::to_writer(
            &mut self.stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message }
            }),
        )
        .map_err(|error| format!("Failed to serialize ACP response: {error}"))?;
        self.stdin
            .write_all(b"\n")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("Failed to send ACP response: {error}"))
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let stderr = std::mem::replace(&mut self.stderr, std::thread::spawn(String::new));
        let _ = stderr.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_and_collects_agent_text() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agent = temp.path().join("agent.py");
        std::fs::write(
            &agent,
            r#"import json, sys
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":1,"agentInfo":{"name":"test-agent"}}}), flush=True)
    elif method == "session/new":
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":{"sessionId":"session-1"}}), flush=True)
    elif method == "session/prompt":
        print(json.dumps({"jsonrpc":"2.0","id":99,"method":"fs/read_text_file","params":{"path":"/private/document.tex"}}), flush=True)
        denied = json.loads(next(sys.stdin))
        assert denied["error"]["code"] == -32601
        print(json.dumps({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello "}}}}), flush=True)
        print(json.dumps({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"world"}}}}), flush=True)
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":{"stopReason":"end_turn"}}), flush=True)
"#,
        )
        .expect("write agent");
        let mut client = AcpClient::connect(
            Path::new("python3"),
            &[agent.display().to_string()],
            Duration::from_secs(2),
        )
        .expect("connect");

        assert_eq!(client.initialize().expect("initialize"), "test-agent");
        assert_eq!(
            client
                .complete(temp.path(), "system", "user")
                .expect("complete"),
            "hello world"
        );
    }
}
