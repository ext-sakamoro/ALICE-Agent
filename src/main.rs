use alice_agent::config::{sessions_dir, AgentConfig};
use alice_agent::context::build_context;
use alice_agent::conversation::session::Session;
use alice_agent::conversation::ConversationRuntime;
use alice_agent::permission::{PermissionLevel, PermissionPolicy};
use alice_agent::tools::StandardTools;
use alice_agent::tui::repl::run_repl;
use clap::Parser;

#[derive(Parser)]
#[command(name = "alice", about = "ALICE — Local-first coding agent")]
struct Cli {
    /// One-shot prompt (skip REPL)
    #[arg(short, long)]
    prompt: Option<String>,

    /// Path to .alice model file
    #[arg(long)]
    model: Option<String>,

    /// Path to tokenizer.json
    #[arg(long)]
    tokenizer: Option<String>,

    /// Working directory
    #[arg(short = 'w', long, default_value = ".")]
    working_dir: String,

    /// Permission level: read-only, workspace-write, full-access
    #[arg(long, default_value = "workspace-write")]
    permission: String,

    /// Skip all permission checks
    #[arg(long)]
    dangerously_skip_permissions: bool,

    /// Use API provider: openai, anthropic
    #[arg(long)]
    provider: Option<String>,

    /// Resume latest session
    #[arg(long)]
    resume: bool,
}

fn main() {
    let cli = Cli::parse();

    // 作業ディレクトリ解決
    let working_dir = std::fs::canonicalize(&cli.working_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&cli.working_dir));
    let working_dir_str = working_dir.to_string_lossy().to_string();

    // config
    let config = AgentConfig::load();

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
    let tools = StandardTools::new(&working_dir_str);

    // コンテキスト (ALICE.md / CLAUDE.md)
    let system_prompt = build_context(&working_dir_str);

    // プロバイダ
    let provider: Box<dyn alice_agent::provider::AgentProvider> =
        create_provider(&cli, &config);

    // ランタイム構築
    let mut runtime = ConversationRuntime::new(
        provider,
        Box::new(tools),
        system_prompt,
        permission_policy,
    );

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
    if let Some(prompt) = &cli.prompt {
        match runtime.run_turn(prompt) {
            Ok(response) => println!("{response}"),
            Err(e) => {
                eprintln!("[error] {e}");
                std::process::exit(1);
            }
        }
        // セッション保存
        save_session(&runtime, &working_dir_str);
    } else {
        eprintln!("[ALICE] ワークスペース: {working_dir_str}");
        eprintln!("[ALICE] /help でヘルプ、/exit で終了");
        eprintln!();
        run_repl(&mut runtime);
        // REPL 終了時にセッション保存
        save_session(&runtime, &working_dir_str);
    }
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

fn create_provider(
    cli: &Cli,
    config: &AgentConfig,
) -> Box<dyn alice_agent::provider::AgentProvider> {
    // --provider 指定時は API を優先
    #[cfg(feature = "api")]
    if let Some(provider_name) = &cli.provider {
        match provider_name.as_str() {
            "openai" | "anthropic" => {
                match alice_agent::provider::openai::OpenAiProvider::from_env() {
                    Ok(p) => return Box::new(p),
                    Err(e) => {
                        eprintln!("[ALICE] API provider error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            _ => {
                eprintln!("[ALICE] unknown provider: {provider_name}");
                std::process::exit(1);
            }
        }
    }

    #[cfg(feature = "local")]
    {
        let model_path = cli.model.as_deref().unwrap_or(&config.model_path);
        let tokenizer_path = cli.tokenizer.as_deref().unwrap_or(&config.tokenizer_path);

        if std::path::Path::new(model_path).exists()
            && std::path::Path::new(tokenizer_path).exists()
        {
            match alice_agent::provider::local::LocalProvider::load(model_path, tokenizer_path) {
                Ok(p) => return Box::new(p),
                Err(e) => eprintln!("[ALICE] local model load failed: {e}"),
            }
        }
    }

    // API フォールバック (環境変数があれば)
    #[cfg(feature = "api")]
    {
        if let Ok(p) = alice_agent::provider::openai::OpenAiProvider::from_env() {
            return Box::new(p);
        }
    }

    eprintln!("[ALICE] モデルもAPIキーも見つかりません");
    eprintln!("[ALICE] echo モードで起動 (ツールテスト用)");
    Box::new(EchoProvider)
}

struct EchoProvider;

impl alice_agent::provider::AgentProvider for EchoProvider {
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
