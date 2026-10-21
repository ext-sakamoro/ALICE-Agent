//! `alice` コマンドの組み立て (引数 → config / 環境変数 / 作業ディレクトリ → runtime)
//!
//! binary の `main` は [`run`] を 1 度呼ぶだけにする 環境変数の読取と config は
//! 引数で受け取るので、試験は差し替えた表と一時ディレクトリで同じ経路を通せる

use crate::config::{sessions_dir, AgentConfig};
use crate::context::system_prompt;
use crate::conversation::session::Session;
use crate::conversation::ConversationRuntime;
use crate::permission::{PermissionLevel, PermissionPolicy};
use crate::provider::AgentProvider;
use crate::tools::StandardTools;
use crate::tui::repl::run_repl;
use clap::Parser;

#[derive(Parser)]
#[command(name = "alice", about = "ALICE — Local-first coding agent")]
pub struct Cli {
    /// One-shot prompt (skip REPL)
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Path to .alice model file
    #[arg(long)]
    pub model: Option<String>,

    /// Path to tokenizer.json
    #[arg(long)]
    pub tokenizer: Option<String>,

    /// Working directory
    #[arg(short = 'w', long, default_value = ".")]
    pub working_dir: String,

    /// Permission level: read-only, workspace-write, full-access
    #[arg(long, default_value = "workspace-write")]
    pub permission: String,

    /// Skip all permission checks
    #[arg(long)]
    pub dangerously_skip_permissions: bool,

    /// Use API provider: openai, anthropic
    #[arg(long)]
    pub provider: Option<String>,

    /// Resume latest session
    #[arg(long)]
    pub resume: bool,
}

/// `alice` を実行して終了コードを返す `env` は環境変数の読取 (binary は `std::env::var`)
pub fn run(cli: &Cli, env: impl Fn(&str) -> Option<String>, config: &AgentConfig) -> i32 {
    // 作業ディレクトリ解決
    let working_dir = std::fs::canonicalize(&cli.working_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&cli.working_dir));
    let working_dir_str = working_dir.to_string_lossy().to_string();

    let mut runtime = match build_runtime(cli, &env, config, &working_dir_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[ALICE] {e}");
            return 1;
        }
    };

    // セッション復元
    if cli.resume {
        match Session::load_latest(&sessions_dir()) {
            Ok(Some(session)) => {
                eprintln!(
                    "[ALICE] セッション復元: {} ({} messages)",
                    session.id,
                    session.messages.len()
                );
                runtime.restore_messages(session.messages);
            }
            Ok(None) => eprintln!("[ALICE] 復元可能なセッションなし"),
            Err(e) => eprintln!("[ALICE] セッション復元エラー: {e}"),
        }
    }

    // ワンショット or REPL
    let code = if let Some(prompt) = &cli.prompt {
        match runtime.run_turn(prompt) {
            Ok(response) => {
                println!("{response}");
                0
            }
            Err(e) => {
                eprintln!("[error] {e}");
                1
            }
        }
    } else {
        eprintln!("[ALICE] ワークスペース: {working_dir_str}");
        eprintln!("[ALICE] /help でヘルプ、/exit で終了");
        eprintln!();
        run_repl(&mut runtime);
        0
    };
    if code == 0 {
        save_session(&runtime, &working_dir_str);
    }
    code
}

/// provider / tools / system prompt / permission を組んだ runtime (session の復元はしない)
pub fn build_runtime(
    cli: &Cli,
    env: &dyn Fn(&str) -> Option<String>,
    config: &AgentConfig,
    working_dir: &str,
) -> Result<ConversationRuntime, String> {
    // パーミッション
    let permission_level = if cli.dangerously_skip_permissions {
        PermissionLevel::FullAccess
    } else {
        match cli.permission.as_str() {
            "read-only" => PermissionLevel::ReadOnly,
            "full-access" => PermissionLevel::FullAccess,
            _ => PermissionLevel::WorkspaceWrite,
        }
    };
    let permission_policy = PermissionPolicy::new(permission_level);

    // ツール
    let tools = StandardTools::new(working_dir);

    // コンテキスト (config の context_files、既定は ALICE.md)
    let system_prompt = system_prompt(config, working_dir);

    // プロバイダ
    let provider = create_provider(cli, env, config)?;

    Ok(ConversationRuntime::new(
        provider,
        Box::new(tools),
        system_prompt,
        permission_policy,
    ))
}

fn save_session(runtime: &ConversationRuntime, working_dir: &str) {
    let messages = runtime.messages();
    if messages.is_empty() {
        return;
    }

    let mut session = Session::new(working_dir, "alice");
    session.messages = messages.to_vec();

    match session.save(&sessions_dir()) {
        Ok(()) => eprintln!("[ALICE] セッション保存: {}", session.id),
        Err(e) => eprintln!("[ALICE] セッション保存エラー: {e}"),
    }
}

/// `--provider` 指定時は API (失敗は Err) / なければ local model → API (key があれば、失敗は Err) → echo の順
pub fn create_provider(
    cli: &Cli,
    env: &dyn Fn(&str) -> Option<String>,
    config: &AgentConfig,
) -> Result<Box<dyn AgentProvider>, String> {
    // --provider 指定時は API を優先
    #[cfg(feature = "api")]
    if let Some(provider_name) = &cli.provider {
        return match provider_name.as_str() {
            "openai" | "anthropic" => crate::provider::openai::OpenAiProvider::from_env_with(env)
                .map(|p| Box::new(p) as Box<dyn AgentProvider>)
                .map_err(|e| format!("API provider error: {e}")),
            _ => Err(format!("unknown provider: {provider_name}")),
        };
    }

    // the model paths in config are read only by the local provider
    #[cfg(not(feature = "local"))]
    let _ = config;
    #[cfg(not(feature = "api"))]
    let _ = env;
    #[cfg(not(any(feature = "api", feature = "local")))]
    let _ = cli;

    #[cfg(feature = "local")]
    {
        let model_path = cli.model.as_deref().unwrap_or(&config.model_path);
        let tokenizer_path = cli.tokenizer.as_deref().unwrap_or(&config.tokenizer_path);

        if std::path::Path::new(model_path).exists()
            && std::path::Path::new(tokenizer_path).exists()
        {
            match crate::provider::local::LocalProvider::load(model_path, tokenizer_path) {
                Ok(p) => return Ok(Box::new(p)),
                Err(e) => eprintln!("[ALICE] local model load failed: {e}"),
            }
        }
    }

    // API フォールバック: key があれば API を使い、設定の誤り (model 未設定等) は Err で止める
    // (echo に落とすのは key が 1 つも無い時だけ)
    #[cfg(feature = "api")]
    if crate::provider::openai::OpenAiProvider::key_is_set(env) {
        return crate::provider::openai::OpenAiProvider::from_env_with(env)
            .map(|p| Box::new(p) as Box<dyn AgentProvider>)
            .map_err(|e| format!("API provider error: {e}"));
    }

    eprintln!("[ALICE] モデルもAPIキーも見つかりません");
    eprintln!("[ALICE] echo モードで起動 (ツールテスト用)");
    Ok(Box::new(EchoProvider))
}

struct EchoProvider;

impl AgentProvider for EchoProvider {
    fn name(&self) -> &str {
        "echo"
    }

    fn generate(&self, messages: &[(&str, &str)]) -> Result<String, String> {
        if let Some((_, content)) = messages.last() {
            Ok(format!("[echo] {content}"))
        } else {
            Ok("[echo] (no messages)".to_string())
        }
    }
}

#[cfg(all(test, feature = "api"))]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("alice").chain(args.iter().copied()))
    }

    fn table(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn provider_flag_with_a_key_and_no_model_is_the_required_model_error() {
        let e = create_provider(
            &cli(&["--provider", "anthropic"]),
            &table(&[("ANTHROPIC_API_KEY", "k")]),
            &AgentConfig::default(),
        )
        .err()
        .expect("a key with no model must not build a provider");
        assert!(e.contains("ANTHROPIC_MODEL not set"), "{e}");
    }

    #[test]
    fn provider_flag_reads_the_given_table() {
        let p = create_provider(
            &cli(&["--provider", "openai"]),
            &table(&[("OPENAI_API_KEY", "k")]),
            &AgentConfig::default(),
        )
        .unwrap();
        assert_eq!(p.name(), "openai");
    }

    #[test]
    fn an_unknown_provider_is_an_error() {
        let e = create_provider(
            &cli(&["--provider", "other"]),
            &table(&[]),
            &AgentConfig::default(),
        )
        .err()
        .unwrap();
        assert!(e.contains("unknown provider: other"), "{e}");
    }
}
