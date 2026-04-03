use crate::provider::AgentProvider;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::io::{self, Write};

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    is_anthropic: bool,
}

impl OpenAiProvider {
    /// OpenAI 互換プロバイダを構築。
    ///
    /// `base_url` が anthropic.com を含む場合は Anthropic API モードに切り替え。
    pub fn new(api_key: &str, base_url: &str, model: &str) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        let is_anthropic = base_url.contains("anthropic.com");

        eprintln!(
            "[ALICE] {} プロバイダ起動 (model: {model})",
            if is_anthropic { "Anthropic" } else { "OpenAI" }
        );

        Ok(Self {
            client,
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            is_anthropic,
        })
    }

    /// 環境変数から自動構築。
    pub fn from_env() -> Result<Self, String> {
        // Anthropic 優先
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            let model = std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
            return Self::new(&key, "https://api.anthropic.com", &model);
        }

        // OpenAI
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            let base = std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            let model =
                std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
            return Self::new(&key, &base, &model);
        }

        Err("ANTHROPIC_API_KEY or OPENAI_API_KEY not set".to_string())
    }

    fn generate_anthropic(&self, messages: &[(&str, &str)]) -> Result<String, String> {
        let mut system_text = String::new();
        let mut api_messages: Vec<Value> = Vec::new();

        for &(role, content) in messages {
            if role == "system" {
                system_text.push_str(content);
                system_text.push('\n');
            } else {
                let api_role = match role {
                    "tool" => "user",
                    _ => role,
                };
                api_messages.push(json!({
                    "role": api_role,
                    "content": content,
                }));
            }
        }

        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": api_messages,
        });
        if !system_text.is_empty() {
            body["system"] = json!(system_text.trim());
        }

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| format!("Anthropic API error: {e}"))?;

        let status = resp.status();
        let resp_body: Value = resp
            .json()
            .map_err(|e| format!("response parse error: {e}"))?;

        if !status.is_success() {
            let err_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(format!("Anthropic API {status}: {err_msg}"));
        }

        let content = resp_body["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|block| block["text"].as_str())
            .unwrap_or("");

        Ok(content.to_string())
    }

    fn generate_openai(&self, messages: &[(&str, &str)]) -> Result<String, String> {
        let api_messages: Vec<Value> = messages
            .iter()
            .map(|&(role, content)| {
                let api_role = match role {
                    "tool" => "user",
                    _ => role,
                };
                json!({
                    "role": api_role,
                    "content": content,
                })
            })
            .collect();

        let body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": api_messages,
        });

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| format!("OpenAI API error: {e}"))?;

        let status = resp.status();
        let resp_body: Value = resp
            .json()
            .map_err(|e| format!("response parse error: {e}"))?;

        if !status.is_success() {
            let err_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(format!("OpenAI API {status}: {err_msg}"));
        }

        let content = resp_body["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|choice| choice["message"]["content"].as_str())
            .unwrap_or("");

        Ok(content.to_string())
    }
}

impl AgentProvider for OpenAiProvider {
    fn name(&self) -> &str {
        if self.is_anthropic {
            "anthropic"
        } else {
            "openai"
        }
    }

    fn generate(&self, messages: &[(&str, &str)]) -> Result<String, String> {
        eprint!("[thinking...] ");
        io::stderr().flush().ok();

        let result = if self.is_anthropic {
            self.generate_anthropic(messages)
        } else {
            self.generate_openai(messages)
        };

        eprintln!();
        result
    }
}
