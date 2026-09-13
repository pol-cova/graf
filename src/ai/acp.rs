use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const PROTOCOL_VERSION: u16 = 1;
/// How much agent stderr is *kept* for error messages. The reader drains
/// without bound — stopping early would deadlock a chatty agent on a full
/// pipe — but only the first bytes are retained.
const MAX_STDERR_BYTES: usize = 8 * 1024;

pub struct AcpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    messages: Receiver<Result<Value, String>>,
    next_id: u64,
    stderr_tail: std::sync::Arc<std::sync::Mutex<String>>,
    _stderr_reader: Option<std::thread::JoinHandle<()>>,
    timeout: Duration,
}

/// A JSON-RPC id may be number or string; match either.
fn message_id(message: &Value) -> Option<u64> {
    match message.get("id") {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    }
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

        // The reader drains stderr forever — a bounded reader deadlocks a
        // chatty agent on a full pipe — while keeping only a head segment
        // under a shared mutex so failures can quote it.
        let stderr_tail: std::sync::Arc<std::sync::Mutex<String>> =
            std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let tail_for_thread = stderr_tail.clone();
        let stderr_reader = std::thread::spawn(move || {
            let mut kept_bytes = 0usize;
            let mut discarded = 0usize;
            let mut first_discarded = String::new();
            for line in BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if kept_bytes < MAX_STDERR_BYTES {
                    kept_bytes += line.len();
                    if let Ok(mut tail) = tail_for_thread.lock() {
                        tail.push_str(&line);
                        tail.push('\n');
                    }
                } else {
                    discarded += 1;
                    if discarded == 1 {
                        first_discarded = line;
                    }
                }
            }
            if discarded > 0
                && let Ok(mut tail) = tail_for_thread.lock()
            {
                tail.push_str(&format!(
                    "…[{discarded} more stderr lines discarded; first: {first_discarded}]"
                ));
            }
        });

        Ok(Self {
            child,
            stdin,
            messages,
            next_id: 1,
            stderr_tail,
            _stderr_reader: Some(stderr_reader),
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

    /// Error suffix quoting what the agent printed to stderr, when anything
    /// is available. Used for timeout and crash reports.
    fn stderr_suffix(&self) -> String {
        let tail = self
            .stderr_tail
            .lock()
            .map(|tail| tail.trim().to_string())
            .unwrap_or_default();
        if tail.is_empty() {
            String::new()
        } else {
            format!(" (agent stderr: {tail})")
        }
    }

    fn wait_for_response(&mut self, id: u64, output: &mut String) -> Result<Value, String> {
        // One deadline for the whole operation; per-message resets let a
        // streaming agent run arbitrarily long.
        let deadline = Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let stderr = self.stderr_suffix();
                return Err(format!(
                    "ACP agent did not respond within {} seconds{stderr}",
                    self.timeout.as_secs()
                ));
            }
            let message = match self.messages.recv_timeout(remaining) {
                Ok(message) => message,
                // The channel closed: the agent process crashed or exited.
                Err(RecvTimeoutError::Disconnected) => {
                    let stderr = self.stderr_suffix();
                    return Err(format!("ACP agent exited unexpectedly{stderr}"));
                }
                Err(RecvTimeoutError::Timeout) => {
                    let stderr = self.stderr_suffix();
                    return Err(format!(
                        "ACP agent did not respond within {} seconds{stderr}",
                        self.timeout.as_secs()
                    ));
                }
            };
            let message = message?;
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
                } else if let Some(request_id) =
                    message.get("id").and_then(Value::as_u64).or_else(|| {
                        message
                            .get("id")
                            .and_then(Value::as_str)
                            .and_then(|value| value.parse().ok())
                    })
                {
                    self.send_error(
                        request_id,
                        -32601,
                        "Graf does not support ACP agent requests",
                    )?;
                }
                continue;
            }
            if message_id(&message) != Some(id) {
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
        // Reaching Drop means every reader is drained; join for tidiness.
        if let Some(reader) = self._stderr_reader.take() {
            let _ = reader.join();
        }
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

    #[test]
    fn crashed_agent_reports_crash_with_stderr_tail() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agent = temp.path().join("agent.py");
        // The agent prints a clue on stderr then dies instead of responding.
        std::fs::write(
            &agent,
            r#"import json, sys
for line in sys.stdin:
    print(json.dumps({"jsonrpc":"2.0","id":json.loads(line)["id"],"result":{"protocolVersion":1,"agentInfo":{"name":"crasher"}}}), flush=True)
    print("i blew up early", file=sys.stderr, flush=True)
    sys.exit(3)
"#,
        )
        .expect("write agent");
        let mut client = AcpClient::connect(
            Path::new("python3"),
            &[agent.display().to_string()],
            Duration::from_secs(10),
        )
        .expect("connect");

        assert!(client.initialize().is_ok());
        let error = client
            .complete(temp.path(), "s", "u")
            .expect_err("agent crashed");
        assert!(
            error.contains("exited"),
            "crash should not be mislabeled a timeout: {error}"
        );
        assert!(
            error.contains("i blew up early"),
            "stderr tail should be quoted: {error}"
        );
    }

    #[test]
    fn string_ids_match_responses() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agent = temp.path().join("agent.py");
        // This agent answers every request with a string-valued id.
        std::fs::write(
            &agent,
            r#"import json, sys
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    reply_id = str(request["id"])
    if method == "initialize":
        print(json.dumps({"jsonrpc":"2.0","id":reply_id,"result":{"protocolVersion":1,"agentInfo":{"name":"string-id"}}}), flush=True)
    elif method == "session/new":
        print(json.dumps({"jsonrpc":"2.0","id":reply_id,"result":{"sessionId":"s"}}), flush=True)
    elif method == "session/prompt":
        print(json.dumps({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"string-ids work"}}}}), flush=True)
        print(json.dumps({"jsonrpc":"2.0","id":reply_id,"result":{"stopReason":"end_turn"}}), flush=True)
"#,
        )
        .expect("write agent");

        let mut client = AcpClient::connect(
            Path::new("python3"),
            &[agent.display().to_string()],
            Duration::from_secs(5),
        )
        .expect("connect");

        assert_eq!(client.initialize().expect("initialize"), "string-id");
        let text = client
            .complete(temp.path(), "system", "user")
            .expect("string-id responses must match");
        assert_eq!(text, "string-ids work");
    }
}
