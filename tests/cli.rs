//! `alice` binary を一時 HOME (config.toml 入り) と一時作業ディレクトリで実行し、
//! config と環境変数が provider まで届くことを確かめる
//!
//! provider の送信先は `OPENAI_BASE_URL` で loopback の 1 回きりの HTTP 応答器に向け、
//! 受け取った request body (system prompt を含む) を検査する
#![cfg(feature = "api")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// a fresh directory under the system temp dir, removed on drop
struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(tag: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("alice-agent-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d)
}

/// HOME/.alice-agent/config.toml を `config` で、作業ディレクトリを `files` で作る
fn layout(tag: &str, config: &str, files: &[(&str, &str)]) -> (Dir, PathBuf, PathBuf) {
    let root = temp_dir(tag);
    let home = root.0.join("home");
    let work = root.0.join("work");
    std::fs::create_dir_all(home.join(".alice-agent")).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(home.join(".alice-agent/config.toml"), config).unwrap();
    for (name, body) in files {
        std::fs::write(work.join(name), body).unwrap();
    }
    (root, home, work)
}

/// 環境を空にして `alice` を実行する (HOME と `env` だけを渡す)
fn alice(home: &Path, work: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_alice"));
    cmd.env_clear()
        .env("HOME", home)
        .arg("-w")
        .arg(work)
        .args(args)
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("run alice")
}

/// 1 回だけ request を受けて `reply` を chat completion として返す 受けた body を送る
fn one_shot_server(reply: &str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let body = serde_json::json!({ "choices": [{ "message": { "content": reply } }] }).to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream);
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        let mut req = vec![0u8; len];
        let _ = reader.read_exact(&mut req);
        let mut stream = reader.into_inner();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = tx.send(String::from_utf8_lossy(&req).into_owned());
    });
    (url, rx)
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn configured_context_file_and_env_reach_the_provider() {
    let (_root, home, work) = layout(
        "context",
        "context_files = [\"TEAM.md\"]\n",
        &[
            ("ALICE.md", "default instructions"),
            ("TEAM.md", "configured instructions"),
        ],
    );
    let (url, rx) = one_shot_server("stub reply");
    let out = alice(
        &home,
        &work,
        &[
            ("OPENAI_API_KEY", "k"),
            ("OPENAI_BASE_URL", &url),
            ("OPENAI_MODEL", "stub-model"),
        ],
        &["--provider", "openai", "-p", "hello"],
    );
    let stderr = text(&out.stderr);
    assert!(out.status.success(), "status {:?}\n{stderr}", out.status);
    assert!(text(&out.stdout).contains("stub reply"), "{stderr}");

    let req = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the provider sent no request");
    let req: serde_json::Value = serde_json::from_str(&req).unwrap();
    assert_eq!(req["model"], "stub-model");
    let system = req["messages"][0]["content"].as_str().unwrap();
    assert!(
        system.contains("# Project Instructions (from TEAM.md)\n\nconfigured instructions"),
        "{system}"
    );
    assert!(!system.contains("default instructions"), "{system}");
    assert!(req["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "hello"));
}

#[test]
fn a_key_without_a_model_is_the_required_model_error() {
    let (_root, home, work) = layout("model", "", &[]);
    let out = alice(
        &home,
        &work,
        &[("ANTHROPIC_API_KEY", "k")],
        &["--provider", "anthropic", "-p", "hello"],
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("ANTHROPIC_MODEL not set"), "{stderr}");
}
