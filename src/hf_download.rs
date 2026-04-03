//! Hugging Face Hub ダウンロードモジュール。
//!
//! モデル・データセットのファイルを HF Hub API 経由で取得する。
//! 認証は `HF_TOKEN` 環境変数から取得。
//!
//! # 使用例
//!
//! ```no_run
//! use alice_agent::hf_download::{HfDownloader, HfRepo, RepoType};
//!
//! let dl = HfDownloader::from_env().unwrap();
//! let repo = HfRepo::new("sakamoro/alice-train", RepoType::Model);
//! dl.download_file(&repo, "config.json", "/tmp/config.json").unwrap();
//! ```

use reqwest::blocking::Client;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const HF_BASE_URL: &str = "https://huggingface.co";
const HF_API_URL: &str = "https://huggingface.co/api";

/// リポジトリの種別。
#[derive(Debug, Clone, PartialEq)]
pub enum RepoType {
    Model,
    Dataset,
}

impl RepoType {
    fn api_segment(&self) -> &'static str {
        match self {
            RepoType::Model => "models",
            RepoType::Dataset => "datasets",
        }
    }

    fn resolve_segment(&self) -> &'static str {
        match self {
            RepoType::Model => "",
            RepoType::Dataset => "datasets/",
        }
    }
}

/// HF リポジトリ識別子。
#[derive(Debug, Clone)]
pub struct HfRepo {
    pub repo_id: String,
    pub repo_type: RepoType,
    pub revision: String,
}

impl HfRepo {
    pub fn new(repo_id: &str, repo_type: RepoType) -> Self {
        Self {
            repo_id: repo_id.to_string(),
            repo_type,
            revision: "main".to_string(),
        }
    }

    pub fn with_revision(mut self, revision: &str) -> Self {
        self.revision = revision.to_string();
        self
    }

    /// ファイル取得 URL を構築する。
    pub fn resolve_url(&self, filename: &str) -> String {
        format!(
            "{}/{}{}/resolve/{}/{}",
            HF_BASE_URL,
            self.repo_type.resolve_segment(),
            self.repo_id,
            self.revision,
            filename,
        )
    }

    /// メタ情報 API URL を構築する。
    pub fn api_url(&self) -> String {
        format!(
            "{}/{}/{}",
            HF_API_URL,
            self.repo_type.api_segment(),
            self.repo_id,
        )
    }
}

/// HF Hub レスポンス — ファイルエントリ。
#[derive(Debug, Deserialize)]
pub struct HfSibling {
    pub rfilename: String,
}

/// HF Hub レスポンス — モデル・データセットのメタ情報。
#[derive(Debug, Deserialize)]
pub struct HfRepoInfo {
    #[serde(rename = "id")]
    pub repo_id: String,
    #[serde(default)]
    pub siblings: Vec<HfSibling>,
    #[serde(rename = "lastModified", default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub private: bool,
}

/// ダウンロードの進捗情報。
pub struct DownloadProgress {
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

/// HF Hub ダウンローダー。
pub struct HfDownloader {
    client: Client,
    token: Option<String>,
}

impl HfDownloader {
    /// `HF_TOKEN` 環境変数からトークンを取得して構築する。
    pub fn from_env() -> Result<Self, String> {
        let token = std::env::var("HF_TOKEN")
            .or_else(|_| std::env::var("HUGGING_FACE_HUB_TOKEN"))
            .ok();

        Self::new(token)
    }

    /// トークンを明示して構築する。`None` で匿名アクセス。
    pub fn new(token: Option<String>) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        Ok(Self { client, token })
    }

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {t}"))
    }

    /// リポジトリのメタ情報を取得する。
    pub fn repo_info(&self, repo: &HfRepo) -> Result<HfRepoInfo, String> {
        let url = repo.api_url();
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().map_err(|e| format!("request error: {e}"))?;
        let status = resp.status();

        if status == 401 || status == 403 {
            return Err(format!(
                "認証エラー ({status}): HF_TOKEN を確認してください"
            ));
        }
        if status == 404 {
            return Err(format!("リポジトリが見つかりません: {}", repo.repo_id));
        }
        if !status.is_success() {
            return Err(format!("API エラー: {status}"));
        }

        resp.json::<HfRepoInfo>()
            .map_err(|e| format!("レスポンス解析エラー: {e}"))
    }

    /// リポジトリ内のファイル一覧を返す。
    pub fn list_files(&self, repo: &HfRepo) -> Result<Vec<String>, String> {
        let info = self.repo_info(repo)?;
        Ok(info.siblings.into_iter().map(|s| s.rfilename).collect())
    }

    /// 単一ファイルをダウンロードして `dest` に保存する。
    ///
    /// - 親ディレクトリが存在しない場合は自動作成する
    /// - 進捗は `progress_cb` コールバックで通知する（`None` で無効）
    pub fn download_file(
        &self,
        repo: &HfRepo,
        filename: &str,
        dest: &Path,
        progress_cb: Option<&mut dyn FnMut(&DownloadProgress)>,
    ) -> Result<u64, String> {
        let url = repo.resolve_url(filename);

        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().map_err(|e| format!("request error: {e}"))?;
        let status = resp.status();

        if status == 401 || status == 403 {
            return Err(format!(
                "認証エラー ({status}): HF_TOKEN を確認してください ({url})"
            ));
        }
        if status == 404 {
            return Err(format!("ファイルが見つかりません: {filename} (repo: {})", repo.repo_id));
        }
        if !status.is_success() {
            return Err(format!("ダウンロードエラー: {status} ({url})"));
        }

        let total_bytes = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        // 親ディレクトリ作成
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("ディレクトリ作成エラー: {e}"))?;
        }

        let mut file = fs::File::create(dest)
            .map_err(|e| format!("ファイル作成エラー ({}): {e}", dest.display()))?;

        let mut bytes_downloaded = 0u64;
        let body = resp.bytes().map_err(|e| format!("読み込みエラー: {e}"))?;

        // 進捗コールバック
        if let Some(cb) = progress_cb {
            bytes_downloaded += body.len() as u64;
            cb(&DownloadProgress {
                filename: filename.to_string(),
                bytes_downloaded,
                total_bytes,
            });
        } else {
            bytes_downloaded = body.len() as u64;
        }

        file.write_all(&body)
            .map_err(|e| format!("書き込みエラー: {e}"))?;

        Ok(bytes_downloaded)
    }

    /// リポジトリ内の複数ファイルを `dest_dir` 以下にダウンロードする。
    ///
    /// `filenames` が空の場合はリポジトリの全ファイルをダウンロードする。
    pub fn download_files(
        &self,
        repo: &HfRepo,
        filenames: &[&str],
        dest_dir: &Path,
    ) -> Result<DownloadSummary, String> {
        let targets: Vec<String> = if filenames.is_empty() {
            self.list_files(repo)?
        } else {
            filenames.iter().map(|s| s.to_string()).collect()
        };

        let mut summary = DownloadSummary {
            total_files: targets.len(),
            succeeded: 0,
            failed: Vec::new(),
            total_bytes: 0,
        };

        for filename in &targets {
            let dest = dest_dir.join(filename);
            match self.download_file(repo, filename, &dest, None) {
                Ok(bytes) => {
                    summary.succeeded += 1;
                    summary.total_bytes += bytes;
                }
                Err(e) => {
                    summary.failed.push((filename.clone(), e));
                }
            }
        }

        Ok(summary)
    }

    /// テキストファイルを文字列として取得する（保存しない）。
    pub fn fetch_text(&self, repo: &HfRepo, filename: &str) -> Result<String, String> {
        let url = repo.resolve_url(filename);

        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().map_err(|e| format!("request error: {e}"))?;
        let status = resp.status();

        if status == 401 || status == 403 {
            return Err(format!("認証エラー ({status})"));
        }
        if status == 404 {
            return Err(format!("ファイルが見つかりません: {filename}"));
        }
        if !status.is_success() {
            return Err(format!("エラー: {status}"));
        }

        resp.text().map_err(|e| format!("テキスト取得エラー: {e}"))
    }
}

/// `download_files` の結果サマリー。
pub struct DownloadSummary {
    pub total_files: usize,
    pub succeeded: usize,
    pub failed: Vec<(String, String)>,
    pub total_bytes: u64,
}

impl DownloadSummary {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn total_bytes_mb(&self) -> f64 {
        self.total_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// よく使うリポジトリのヘルパー。
pub mod repos {
    use super::{HfRepo, RepoType};

    pub fn alice_train() -> HfRepo {
        HfRepo::new("sakamoro/alice-train", RepoType::Model)
    }

    pub fn alice_ml() -> HfRepo {
        HfRepo::new("sakamoro/alice-ml", RepoType::Model)
    }

    pub fn alice_train_data() -> HfRepo {
        HfRepo::new("sakamoro/alice-train-data", RepoType::Dataset)
    }
}

/// デフォルトのダウンロード先ディレクトリ（`~/.cache/alice/hf/`）。
pub fn default_cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("alice")
        .join("hf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_resolve_url_model() {
        let repo = HfRepo::new("sakamoro/alice-train", RepoType::Model);
        let url = repo.resolve_url("config.json");
        assert_eq!(
            url,
            "https://huggingface.co/sakamoro/alice-train/resolve/main/config.json"
        );
    }

    #[test]
    fn test_repo_resolve_url_dataset() {
        let repo = HfRepo::new("sakamoro/alice-train-data", RepoType::Dataset);
        let url = repo.resolve_url("data/train.jsonl");
        assert_eq!(
            url,
            "https://huggingface.co/datasets/sakamoro/alice-train-data/resolve/main/data/train.jsonl"
        );
    }

    #[test]
    fn test_repo_resolve_url_with_revision() {
        let repo = HfRepo::new("sakamoro/alice-train", RepoType::Model)
            .with_revision("test-results");
        let url = repo.resolve_url("test_results.txt");
        assert_eq!(
            url,
            "https://huggingface.co/sakamoro/alice-train/resolve/test-results/test_results.txt"
        );
    }

    #[test]
    fn test_repo_api_url_model() {
        let repo = HfRepo::new("sakamoro/alice-train", RepoType::Model);
        assert_eq!(
            repo.api_url(),
            "https://huggingface.co/api/models/sakamoro/alice-train"
        );
    }

    #[test]
    fn test_repo_api_url_dataset() {
        let repo = HfRepo::new("sakamoro/alice-train-data", RepoType::Dataset);
        assert_eq!(
            repo.api_url(),
            "https://huggingface.co/api/datasets/sakamoro/alice-train-data"
        );
    }

    #[test]
    fn test_downloader_no_token() {
        // トークンなしで構築できることを確認
        let dl = HfDownloader::new(None).unwrap();
        assert!(dl.token.is_none());
        assert!(dl.auth_header().is_none());
    }

    #[test]
    fn test_downloader_with_token() {
        let dl = HfDownloader::new(Some("hf_test_token".to_string())).unwrap();
        assert_eq!(dl.auth_header(), Some("Bearer hf_test_token".to_string()));
    }

    #[test]
    fn test_repos_helpers() {
        let r = repos::alice_train();
        assert_eq!(r.repo_id, "sakamoro/alice-train");
        assert_eq!(r.repo_type, RepoType::Model);
        assert_eq!(r.revision, "main");

        let r = repos::alice_ml();
        assert_eq!(r.repo_id, "sakamoro/alice-ml");

        let r = repos::alice_train_data();
        assert_eq!(r.repo_type, RepoType::Dataset);
    }

    #[test]
    fn test_default_cache_dir() {
        let dir = default_cache_dir();
        assert!(dir.to_string_lossy().contains(".cache/alice/hf"));
    }

    #[test]
    fn test_download_summary() {
        let s = DownloadSummary {
            total_files: 3,
            succeeded: 3,
            failed: Vec::new(),
            total_bytes: 2 * 1024 * 1024,
        };
        assert!(s.is_success());
        assert!((s.total_bytes_mb() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_download_summary_with_failure() {
        let s = DownloadSummary {
            total_files: 3,
            succeeded: 2,
            failed: vec![("missing.bin".to_string(), "404 not found".to_string())],
            total_bytes: 1024,
        };
        assert!(!s.is_success());
    }
}
