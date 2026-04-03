pub mod transport;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use transport::StdioTransport;

/// MCP サーバーから取得したツール定義。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// MCP クライアント。1つの MCP サーバーとの接続を管理。
pub struct McpClient {
    transport: StdioTransport,
    server_name: String,
    tools: Vec<McpTool>,
}

impl McpClient {
    /// MCP サーバーを起動して接続。
    pub fn connect(
        server_name: &str,
        command: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<Self, String> {
        let transport = StdioTransport::spawn(command, args, env)?;
        let mut client = Self {
            transport,
            server_name: server_name.to_string(),
            tools: Vec::new(),
        };
        client.initialize()?;
        Ok(client)
    }

    fn initialize(&mut self) -> Result<(), String> {
        // initialize
        let _init_result = self.transport.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "alice-agent",
                    "version": "0.1.0"
                }
            }),
        )?;

        // initialized notification
        self.transport
            .notify("notifications/initialized", serde_json::json!({}))?;

        // tools/list
        let tools_result = self
            .transport
            .request("tools/list", serde_json::json!({}))?;

        if let Some(tools_arr) = tools_result.get("tools").and_then(|t| t.as_array()) {
            for tool in tools_arr {
                if let Ok(mcp_tool) = serde_json::from_value::<McpTool>(tool.clone()) {
                    self.tools.push(mcp_tool);
                }
            }
        }

        eprintln!(
            "[MCP] {} connected ({} tools)",
            self.server_name,
            self.tools.len()
        );

        Ok(())
    }

    /// ツール一覧。
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// サーバー名。
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// ツール実行。
    pub fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<String, String> {
        let result = self.transport.request(
            "tools/call",
            serde_json::json!({
                "name": name,
                "arguments": arguments,
            }),
        )?;

        // content 配列からテキストを抽出
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let texts: Vec<&str> = content
                .iter()
                .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                .collect();
            Ok(texts.join("\n"))
        } else {
            Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
    }
}
