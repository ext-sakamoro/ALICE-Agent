//! hf-download — HuggingFace Hub からモデル・データセットをダウンロードするコマンド。
//!
//! ```bash
//! # モデルの全ファイルをダウンロード
//! hf-download --repo sakamoro/alice-train --dest ~/.cache/alice/hf/alice-train
//!
//! # 特定ファイルのみ
//! hf-download --repo sakamoro/alice-train --files config.json tokenizer.json
//!
//! # データセット
//! hf-download --repo sakamoro/alice-train-data --type dataset --dest ./data
//!
//! # ブランチ指定
//! hf-download --repo sakamoro/alice-train --revision test-results --files test_results.txt
//!
//! # テキスト取得（stdout 出力）
//! hf-download --repo sakamoro/alice-train --revision test-results --fetch test_results.txt
//!
//! # ファイル一覧表示
//! hf-download --repo sakamoro/alice-train --list
//! ```

use alice_agent::hf_download::{default_cache_dir, HfDownloader, HfRepo, RepoType};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "hf-download",
    about = "HuggingFace Hub からモデル・データセットをダウンロードする"
)]
struct Cli {
    /// リポジトリ ID (例: sakamoro/alice-train)
    #[arg(long, short = 'r')]
    repo: String,

    /// リポジトリ種別 (model / dataset)
    #[arg(long, short = 't', default_value = "model")]
    repo_type: String,

    /// ブランチまたはコミットハッシュ (デフォルト: main)
    #[arg(long, default_value = "main")]
    revision: String,

    /// ダウンロードするファイル名 (複数指定可、省略時は全ファイル)
    #[arg(long, short = 'f', num_args = 1..)]
    files: Option<Vec<String>>,

    /// 保存先ディレクトリ (デフォルト: ~/.cache/alice/hf/<repo_name>)
    #[arg(long, short = 'd')]
    dest: Option<String>,

    /// ファイル一覧を表示して終了
    #[arg(long, short = 'l')]
    list: bool,

    /// テキストファイルを取得して stdout に出力 (保存しない)
    #[arg(long)]
    fetch: Option<String>,

    /// HF トークン (省略時は HF_TOKEN 環境変数)
    #[arg(long)]
    token: Option<String>,

    /// リポジトリ情報を表示
    #[arg(long, short = 'i')]
    info: bool,
}

fn main() {
    let cli = Cli::parse();

    // トークン解決: CLI引数 > 環境変数
    let token = cli
        .token
        .or_else(|| std::env::var("HF_TOKEN").ok())
        .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok());

    let dl = match HfDownloader::new(token) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("エラー: {e}");
            std::process::exit(1);
        }
    };

    let repo_type = match cli.repo_type.as_str() {
        "dataset" | "datasets" => RepoType::Dataset,
        _ => RepoType::Model,
    };

    let repo = HfRepo::new(&cli.repo, repo_type).with_revision(&cli.revision);

    // --fetch: テキスト取得して stdout へ
    if let Some(filename) = &cli.fetch {
        match dl.fetch_text(&repo, filename) {
            Ok(text) => {
                print!("{text}");
            }
            Err(e) => {
                eprintln!("エラー: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // --info: リポジトリ情報表示
    if cli.info {
        match dl.repo_info(&repo) {
            Ok(info) => {
                println!("repo_id:       {}", info.repo_id);
                println!("private:       {}", info.private);
                if let Some(ts) = &info.last_modified {
                    println!("last_modified: {ts}");
                }
                println!("files:         {}", info.siblings.len());
            }
            Err(e) => {
                eprintln!("エラー: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // --list: ファイル一覧表示
    if cli.list {
        match dl.list_files(&repo) {
            Ok(files) => {
                for f in &files {
                    println!("{f}");
                }
                eprintln!("合計: {} ファイル", files.len());
            }
            Err(e) => {
                eprintln!("エラー: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // ダウンロード先
    let dest_dir = match &cli.dest {
        Some(d) => std::path::PathBuf::from(shellexpand::tilde(d).as_ref()),
        None => {
            let repo_name = cli
                .repo
                .split('/')
                .last()
                .unwrap_or(&cli.repo)
                .to_string();
            default_cache_dir().join(&repo_name)
        }
    };

    eprintln!("リポジトリ: {} ({})", cli.repo, cli.revision);
    eprintln!("保存先:     {}", dest_dir.display());

    let filenames: Vec<&str> = cli
        .files
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| s.as_str())
        .collect();

    let label = if filenames.is_empty() {
        "全ファイル".to_string()
    } else {
        format!("{} ファイル", filenames.len())
    };
    eprintln!("対象:       {label}");

    match dl.download_files(&repo, &filenames, &dest_dir) {
        Ok(summary) => {
            eprintln!(
                "完了: {}/{} ファイル ({:.1} MB)",
                summary.succeeded,
                summary.total_files,
                summary.total_bytes_mb(),
            );
            if !summary.failed.is_empty() {
                eprintln!("失敗:");
                for (fname, err) in &summary.failed {
                    eprintln!("  {fname}: {err}");
                }
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("エラー: {e}");
            std::process::exit(1);
        }
    }
}
