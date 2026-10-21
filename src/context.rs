use std::path::Path;

const DEFAULT_SYSTEM_PROMPT: &str = "\
You are ALICE, a local coding agent. You help users with software engineering tasks \
by reading files, writing code, running commands, and solving problems. \
Be concise and direct. Always prefer editing existing files over creating new ones.";

/// システムプロンプトを構築する `context_files` の名前を working_dir で順に探し、
/// 最初に見つかった 1 つをプロジェクトの指示として読み込む (既定は `ALICE.md` だけ、
/// config.toml の `context_files` で変えられる)
pub fn build_context(working_dir: &str, context_files: &[String]) -> String {
    let mut prompt = DEFAULT_SYSTEM_PROMPT.to_string();

    for name in context_files {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// a fresh directory under the system temp dir, removed on drop
    struct Dir(std::path::PathBuf);
    impl Dir {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir_with(files: &[(&str, &str)]) -> Dir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "alice-agent-context-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        for (name, body) in files {
            fs::write(d.join(name), body).unwrap();
        }
        Dir(d)
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_list_reads_alice_md() {
        let d = dir_with(&[("ALICE.md", "use tabs"), ("NOTES.md", "other")]);
        let p = build_context(
            d.path().to_str().unwrap(),
            &crate::config::default_context_files(),
        );
        assert!(
            p.contains("# Project Instructions (from ALICE.md)\n\nuse tabs"),
            "{p}"
        );
        assert!(!p.contains("other"), "{p}");
    }

    #[test]
    fn configured_list_is_searched_in_order_and_the_first_found_is_read() {
        let d = dir_with(&[("SECOND.md", "second"), ("THIRD.md", "third")]);
        let p = build_context(
            d.path().to_str().unwrap(),
            &names(&["FIRST.md", "SECOND.md", "THIRD.md"]),
        );
        assert!(
            p.contains("# Project Instructions (from SECOND.md)\n\nsecond"),
            "{p}"
        );
        assert!(!p.contains("third"), "{p}");
    }

    #[test]
    fn a_file_outside_the_list_is_not_read() {
        let d = dir_with(&[("NOTES.md", "not listed")]);
        let p = build_context(
            d.path().to_str().unwrap(),
            &crate::config::default_context_files(),
        );
        assert!(!p.contains("Project Instructions"), "{p}");
        assert!(!p.contains("not listed"), "{p}");
    }

    #[test]
    fn missing_files_and_an_empty_list_give_the_base_prompt() {
        let d = dir_with(&[]);
        let base = build_context(d.path().to_str().unwrap(), &[]);
        assert_eq!(base, DEFAULT_SYSTEM_PROMPT);
        let p = build_context(
            d.path().to_str().unwrap(),
            &names(&["ALICE.md", "OTHER.md"]),
        );
        assert_eq!(p, DEFAULT_SYSTEM_PROMPT);
    }
}
