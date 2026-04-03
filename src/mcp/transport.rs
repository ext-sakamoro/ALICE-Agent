use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// JSON-RPC 2.0 over stdio トランスポート。
pub struct StdioTransport {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl StdioTransport {
    /// MCP サーバープロセスを起動。
    pub fn spawn(command: &str, args: &[&str], env: &[(&str, &str)]) -> Result<Self, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for &(key, value) in env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| format!("failed to spawn MCP server: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or("failed to capture MCP server stdout")?;
        let reader = BufReader::new(stdout);

        Ok(Self { child, reader })
    }

    /// JSON-RPC request を送信し、response を受信。
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.send(&request)?;
        self.receive_response(id)
    }

    /// JSON-RPC notification を送信 (レスポンスなし)。
    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send(&notification)
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let json = serde_json::to_string(message).map_err(|e| format!("JSON serialize: {e}"))?;

        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or("MCP server stdin unavailable")?;

        // Content-Length ヘッダ + JSON ボディ
        write!(stdin, "Content-Length: {}\r\n\r\n{}", json.len(), json)
            .map_err(|e| format!("write to MCP server: {e}"))?;
        stdin
            .flush()
            .map_err(|e| format!("flush MCP server: {e}"))?;

        Ok(())
    }

    fn receive_response(&mut self, expected_id: u64) -> Result<Value, String> {
        loop {
            // Content-Length ヘッダを読む
            let mut header = String::new();
            loop {
                header.clear();
                self.reader
                    .read_line(&mut header)
                    .map_err(|e| format!("read MCP header: {e}"))?;
                let trimmed = header.trim();
                if trimmed.is_empty() {
                    break;
                }
            }

            // ボディを読む (1行)
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .map_err(|e| format!("read MCP body: {e}"))?;

            if line.trim().is_empty() {
                continue;
            }

            let msg: Value =
                serde_json::from_str(line.trim()).map_err(|e| format!("parse MCP response: {e}"))?;

            // notification はスキップ
            if msg.get("id").is_none() {
                continue;
            }

            let id = msg
                .get("id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if id != expected_id {
                continue;
            }

            // エラーチェック
            if let Some(error) = msg.get("error") {
                let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
                let message = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("MCP error {code}: {message}"));
            }

            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // サーバープロセスを終了
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
