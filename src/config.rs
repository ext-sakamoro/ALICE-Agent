use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// `~/.alice-agent/config.toml` から読み込む file が無ければ既定値、
    /// 読めない・TOML として解釈できない時は path と理由を含む Err (既定値で黙って続けない)
    pub fn load() -> Result<Self, String> {
        Self::load_from(&config_dir().join("config.toml"))
    }

    /// `path` の config を読む (無ければ既定値、読めない・解釈できなければ Err)
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("config {}: {e}", path.display())),
        };
        toml::from_str(&content).map_err(|e| format!("config {}: {e}", path.display()))
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

    fn temp_file(name: &str, body: Option<&str>) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("alice-agent-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        if let Some(b) = body {
            std::fs::write(&p, b).unwrap();
        }
        p
    }

    #[test]
    fn load_from_a_missing_file_gives_the_defaults() {
        let p = temp_file("missing.toml", None);
        assert_eq!(
            AgentConfig::load_from(&p).unwrap().context_files,
            AgentConfig::default().context_files
        );
    }

    #[test]
    fn load_from_a_malformed_file_is_an_error_naming_the_path() {
        let p = temp_file("malformed.toml", Some("context_files = [\n"));
        let e = AgentConfig::load_from(&p).unwrap_err();
        let _ = std::fs::remove_file(&p);
        assert!(e.contains(&p.display().to_string()), "{e}");
        assert!(e.contains("TOML parse error"), "{e}");
    }

    #[test]
    fn load_from_an_unreadable_path_is_an_error() {
        // a directory exists but cannot be read as a file
        let p = std::env::temp_dir();
        let e = AgentConfig::load_from(&p).unwrap_err();
        assert!(e.contains(&p.display().to_string()), "{e}");
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
