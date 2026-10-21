use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default = "default_tokenizer_path")]
    pub tokenizer_path: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// working directory で探すプロジェクトの指示 file の名前 (順に探し、最初の 1 つを読む)
    #[serde(default = "default_context_files")]
    pub context_files: Vec<String>,
}

fn default_model_path() -> String {
    config_dir()
        .join("models/model.alice")
        .to_string_lossy()
        .to_string()
}

fn default_tokenizer_path() -> String {
    config_dir()
        .join("models/tokenizer.json")
        .to_string_lossy()
        .to_string()
}

fn default_max_tokens() -> usize {
    4096
}

fn default_temperature() -> f32 {
    0.3
}

/// 既定で読むプロジェクトの指示 file
pub(crate) fn default_context_files() -> Vec<String> {
    vec!["ALICE.md".to_string()]
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model_path: default_model_path(),
            tokenizer_path: default_tokenizer_path(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            context_files: default_context_files(),
        }
    }
}

impl AgentConfig {
    /// config.toml から読み込み。ファイルがなければデフォルト。
    pub fn load() -> Self {
        let path = config_dir().join("config.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}

/// ~/.alice-agent/
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".alice-agent")
}

/// ~/.alice-agent/sessions/
pub fn sessions_dir() -> PathBuf {
    config_dir().join("sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_context_files_is_alice_md_only() {
        assert_eq!(
            AgentConfig::default().context_files,
            vec!["ALICE.md".to_string()]
        );
    }

    #[test]
    fn context_files_is_read_from_config_toml_and_defaults_when_absent() {
        let c: AgentConfig =
            toml::from_str("context_files = [\"AGENTS.md\", \"ALICE.md\"]").unwrap();
        assert_eq!(
            c.context_files,
            vec!["AGENTS.md".to_string(), "ALICE.md".to_string()]
        );
        let c: AgentConfig = toml::from_str("max_tokens = 10").unwrap();
        assert_eq!(c.context_files, vec!["ALICE.md".to_string()]);
    }
}
