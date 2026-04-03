//! WorldModel 学習データ抽出 — Git 履歴から (state, action, outcome) ペアを生成。
//!
//! Git の各コミットを「行動」、差分を「状態遷移」として抽出し、
//! WorldModel (RSSM) の学習に使える JSONL を生成する。
//!
//! ```bash
//! cargo run --release --bin extract-world-model-data -- \
//!     --scan-dir ~ --prefix ALICE- \
//!     --output data/world_model_training.jsonl
//! ```

use clap::Parser;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "extract-world-model-data")]
struct Cli {
    /// 対象リポジトリのパス (複数指定可)
    #[arg(long, num_args = 1..)]
    repos: Option<Vec<String>>,

    /// リポジトリをスキャンするディレクトリ
    #[arg(long)]
    scan_dir: Option<String>,

    /// スキャン対象のプレフィックス
    #[arg(long, default_value = "ALICE-")]
    prefix: String,

    /// 出力ファイル
    #[arg(long, default_value = "data/world_model_training.jsonl")]
    output: String,

    /// リポジトリあたりの最大コミット数
    #[arg(long, default_value = "200")]
    max_commits: usize,
}

#[derive(Serialize)]
struct Sample {
    repo: String,
    commit: String,
    date: String,
    state: State,
    action: Action,
    outcome: Outcome,
}

#[derive(Serialize)]
struct State {
    total_files: usize,
    total_additions: usize,
    total_deletions: usize,
    change_ratio: f64,
    file_extensions: HashMap<String, usize>,
    files_list: Vec<String>,
}

#[derive(Serialize)]
struct Action {
    #[serde(rename = "type")]
    action_type: String,
    scale: String,
    primary_lang: String,
    description: String,
}

#[derive(Serialize)]
struct Outcome {
    compile_success: bool,
    test_pass: bool,
    follow_up_needed: bool,
    affected_modules: Vec<String>,
    risk_score: f64,
}

struct FileDiff {
    path: String,
    ext: String,
    additions: usize,
    deletions: usize,
}

struct CommitInfo {
    hash: String,
    message: String,
    date: String,
}

struct DiffStats {
    files: Vec<FileDiff>,
    total_files: usize,
    total_additions: usize,
    total_deletions: usize,
}

fn run_git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn get_commits(repo: &Path, max: usize) -> Vec<CommitInfo> {
    let log = match run_git(
        repo,
        &[
            "log",
            &format!("--max-count={max}"),
            "--format=%H|%s|%aI",
            "--no-merges",
        ],
    ) {
        Some(s) => s,
        None => return Vec::new(),
    };

    log.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let hash = parts.next()?.to_string();
            let message = parts.next()?.to_string();
            let date = parts.next()?.to_string();
            Some(CommitInfo { hash, message, date })
        })
        .collect()
}

fn get_diff_stats(repo: &Path, hash: &str) -> Option<DiffStats> {
    let numstat = run_git(repo, &["diff", "--numstat", &format!("{hash}~1"), hash])
        .or_else(|| run_git(repo, &["diff", "--numstat", "--root", hash]))?;

    if numstat.is_empty() {
        return None;
    }

    let mut files = Vec::new();
    let mut total_add = 0usize;
    let mut total_del = 0usize;

    for line in numstat.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let additions = parts[0].parse::<usize>().unwrap_or(0);
        let deletions = parts[1].parse::<usize>().unwrap_or(0);
        let path = parts[2].to_string();
        let ext = Path::new(&path)
            .extension()
            .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));

        total_add += additions;
        total_del += deletions;
        files.push(FileDiff {
            path,
            ext,
            additions,
            deletions,
        });
    }

    if files.is_empty() {
        return None;
    }

    Some(DiffStats {
        total_files: files.len(),
        total_additions: total_add,
        total_deletions: total_del,
        files,
    })
}

fn classify_action(message: &str, diff: &DiffStats) -> Action {
    let msg = message.to_lowercase();

    let action_type = if contains_any(&msg, &["fix", "修正", "bugfix", "hotfix"]) {
        "fix"
    } else if contains_any(&msg, &["feat", "add", "追加", "実装", "新規"]) {
        "add_feature"
    } else if contains_any(&msg, &["refactor", "リファクタ", "rename", "move"]) {
        "refactor"
    } else if contains_any(&msg, &["test", "テスト"]) {
        "add_test"
    } else if contains_any(&msg, &["doc", "readme", "ドキュメント"]) {
        "documentation"
    } else if contains_any(&msg, &["delete", "remove", "削除", "clean"]) {
        "remove"
    } else if contains_any(&msg, &["update", "更新", "bump", "upgrade"]) {
        "update"
    } else if contains_any(&msg, &["perf", "optim", "最適化", "高速化"]) {
        "optimize"
    } else {
        "other"
    };

    let total = diff.total_additions + diff.total_deletions;
    let scale = match total {
        0..=9 => "tiny",
        10..=49 => "small",
        50..=199 => "medium",
        200..=999 => "large",
        _ => "massive",
    };

    // 主要言語
    let mut ext_counts: HashMap<&str, usize> = HashMap::new();
    for f in &diff.files {
        if !f.ext.is_empty() {
            *ext_counts.entry(&f.ext).or_default() += f.additions + f.deletions;
        }
    }
    let primary_lang = ext_counts
        .iter()
        .max_by_key(|(_, &v)| v)
        .map_or("", |(k, _)| k)
        .to_string();

    let desc_len = message.len().min(200);
    Action {
        action_type: action_type.to_string(),
        scale: scale.to_string(),
        primary_lang,
        description: message[..desc_len].to_string(),
    }
}

fn predict_outcome(message: &str, diff: &DiffStats, next: Option<&CommitInfo>) -> Outcome {
    let msg = message.to_lowercase();

    let compile_success = !contains_any(&msg, &["wip", "broken", "todo", "temporary"]);
    let test_pass = !contains_any(&msg, &["fail", "broken", "skip test"]);

    let follow_up_needed = next
        .map(|n| {
            let nm = n.message.to_lowercase();
            contains_any(&nm, &["fix", "修正", "hotfix", "revert"])
        })
        .unwrap_or(false);

    let mut modules = Vec::new();
    for f in &diff.files {
        if let Some(first) = f.path.split('/').next() {
            let m = first.to_string();
            if !modules.contains(&m) {
                modules.push(m);
            }
        }
    }

    let risk = estimate_risk(diff, follow_up_needed);

    Outcome {
        compile_success,
        test_pass,
        follow_up_needed,
        affected_modules: modules,
        risk_score: risk,
    }
}

fn estimate_risk(diff: &DiffStats, follow_up: bool) -> f64 {
    let mut risk = 0.0f64;

    // ファイル数
    risk += (diff.total_files as f64 * 0.03).min(0.3);

    // 削除比率
    let total = (diff.total_additions + diff.total_deletions) as f64;
    if total > 0.0 {
        risk += (diff.total_deletions as f64 / total) * 0.2;
    }

    // 変更量
    risk += (total * 0.0002).min(0.2);

    // 連鎖修正
    if follow_up {
        risk += 0.3;
    }

    risk.min(1.0)
}

fn encode_state(diff: &DiffStats) -> State {
    let mut ext_features: HashMap<String, usize> = HashMap::new();
    for f in &diff.files {
        let key = if f.ext.is_empty() {
            "other".to_string()
        } else {
            f.ext.trim_start_matches('.').to_string()
        };
        *ext_features.entry(key).or_default() += f.additions + f.deletions;
    }

    let total = (diff.total_additions + diff.total_deletions).max(1) as f64;
    let files_list: Vec<String> = diff.files.iter().take(20).map(|f| f.path.clone()).collect();

    State {
        total_files: diff.total_files,
        total_additions: diff.total_additions,
        total_deletions: diff.total_deletions,
        change_ratio: diff.total_additions as f64 / total,
        file_extensions: ext_features,
        files_list,
    }
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

fn find_repos(scan_dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let Ok(entries) = std::fs::read_dir(scan_dir) else {
        return repos;
    };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(prefix) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() && path.join(".git").is_dir() {
            repos.push(path);
        }
    }

    // Project-ALICE も追加
    let project_alice = scan_dir.join("Project-ALICE");
    if project_alice.join(".git").is_dir() {
        repos.push(project_alice);
    }

    repos
}

fn process_repo(repo: &Path, max_commits: usize) -> Vec<Sample> {
    let repo_name = repo
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    eprint!("  {repo_name}...");

    let commits = get_commits(repo, max_commits);
    if commits.is_empty() {
        eprintln!(" (no commits)");
        return Vec::new();
    }

    let mut samples = Vec::new();
    for (i, commit) in commits.iter().enumerate() {
        let Some(diff) = get_diff_stats(repo, &commit.hash) else {
            continue;
        };
        if diff.total_files == 0 {
            continue;
        }

        let next = if i > 0 { Some(&commits[i - 1]) } else { None };

        let state = encode_state(&diff);
        let action = classify_action(&commit.message, &diff);
        let outcome = predict_outcome(&commit.message, &diff, next);

        samples.push(Sample {
            repo: repo_name.clone(),
            commit: commit.hash[..12].to_string(),
            date: commit.date.clone(),
            state,
            action,
            outcome,
        });
    }

    eprintln!(" {} samples", samples.len());
    samples
}

fn main() {
    let cli = Cli::parse();

    let repos: Vec<PathBuf> = if let Some(paths) = &cli.repos {
        paths.iter().map(|p| PathBuf::from(shellexpand::tilde(p).as_ref())).collect()
    } else if let Some(dir) = &cli.scan_dir {
        let scan = PathBuf::from(shellexpand::tilde(dir).as_ref());
        find_repos(&scan, &cli.prefix)
    } else {
        eprintln!("Error: --repos or --scan-dir required");
        std::process::exit(1);
    };

    eprintln!("Found {} repositories", repos.len());

    let mut all_samples = Vec::new();
    for repo in &repos {
        if !repo.is_dir() {
            eprintln!("  Skipping {} (not found)", repo.display());
            continue;
        }
        let samples = process_repo(repo, cli.max_commits);
        all_samples.extend(samples);
    }

    // 出力
    let output_path = PathBuf::from(&cli.output);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let mut file = std::fs::File::create(&output_path).expect("failed to create output file");
    for sample in &all_samples {
        let json = serde_json::to_string(sample).expect("serialize error");
        writeln!(file, "{json}").expect("write error");
    }

    eprintln!("\nTotal: {} samples → {}", all_samples.len(), cli.output);

    // 統計
    let mut action_types: HashMap<String, usize> = HashMap::new();
    let mut risk_sum = 0.0f64;
    let mut follow_ups = 0usize;

    for s in &all_samples {
        *action_types.entry(s.action.action_type.clone()).or_default() += 1;
        risk_sum += s.outcome.risk_score;
        if s.outcome.follow_up_needed {
            follow_ups += 1;
        }
    }

    eprintln!("\nAction type distribution:");
    let mut sorted: Vec<_> = action_types.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (t, count) in &sorted {
        eprintln!("  {t}: {count}");
    }

    if !all_samples.is_empty() {
        let n = all_samples.len();
        eprintln!(
            "\nAverage risk score: {:.3}",
            risk_sum / n as f64
        );
        eprintln!(
            "Follow-up needed: {follow_ups}/{n} ({:.1}%)",
            100.0 * follow_ups as f64 / n as f64
        );
    }
}
