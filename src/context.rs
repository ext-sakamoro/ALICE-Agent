use std::path::Path;

const DEFAULT_SYSTEM_PROMPT: &str = "\
You are ALICE, a local coding agent. You help users with software engineering tasks \
by reading files, writing code, running commands, and solving problems. \
Be concise and direct. Always prefer editing existing files over creating new ones.";

/// プロジェクトコンテキスト (ALICE.md / CLAUDE.md) を読み込んでシステムプロンプトを構築。
pub fn build_context(working_dir: &str) -> String {
    let mut prompt = DEFAULT_SYSTEM_PROMPT.to_string();

    // ALICE.md → CLAUDE.md の順で探す
    let context_files = ["ALICE.md", "CLAUDE.md"];
    for name in &context_files {
        let path = Path::new(working_dir).join(name);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                // 大きすぎる場合は切り詰め
                let trimmed = if content.len() > 20_000 {
                    format!("{}...(truncated)", &content[..20_000])
                } else {
                    content
                };
                prompt.push_str(&format!(
                    "\n\n# Project Instructions (from {name})\n\n{trimmed}"
                ));
                break; // 最初に見つかったもののみ
            }
        }
    }

    // .gitignore があればプロジェクトの種類を推測
    let gitignore = Path::new(working_dir).join(".gitignore");
    if gitignore.exists() {
        prompt.push_str("\n\nThis is a git repository.");
    }

    prompt
}
