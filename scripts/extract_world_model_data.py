#!/usr/bin/env python3
"""
WorldModel 学習データ抽出 — Git 履歴から (state, action, outcome) ペアを生成。

Git の各コミットを「行動」、差分を「状態遷移」として抽出し、
WorldModel (RSSM) の学習に使える JSONL を生成する。

使い方:
    python3 scripts/extract_world_model_data.py \
        --repos ~/ALICE-Train ~/ALICE-ML ~/ALICE-SDF ~/Project-ALICE \
        --output data/world_model_training.jsonl \
        --max-commits 500

    # 全 ALICE リポジトリを一括処理
    python3 scripts/extract_world_model_data.py \
        --scan-dir ~ --prefix ALICE- \
        --output data/world_model_training.jsonl
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple


def run_git(repo_path: str, args: List[str]) -> Optional[str]:
    """Git コマンドを実行して stdout を返す。"""
    try:
        result = subprocess.run(
            ["git", "-C", repo_path] + args,
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode == 0:
            return result.stdout.strip()
        return None
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return None


def get_commit_list(repo_path: str, max_commits: int) -> List[Dict]:
    """コミット一覧を取得 (新しい順)。"""
    log = run_git(
        repo_path,
        [
            "log",
            f"--max-count={max_commits}",
            "--format=%H|%s|%an|%aI|%P",
            "--no-merges",
        ],
    )
    if not log:
        return []

    commits = []
    for line in log.strip().split("\n"):
        if not line:
            continue
        parts = line.split("|", 4)
        if len(parts) < 4:
            continue
        commits.append(
            {
                "hash": parts[0],
                "message": parts[1],
                "author": parts[2],
                "date": parts[3],
                "parents": parts[4].split() if len(parts) > 4 else [],
            }
        )
    return commits


def get_diff_stats(repo_path: str, commit_hash: str) -> Optional[Dict]:
    """コミットの diff 統計を取得。"""
    # --numstat: additions deletions filename
    numstat = run_git(repo_path, ["diff", "--numstat", f"{commit_hash}~1", commit_hash])
    if numstat is None:
        # 初回コミットの場合
        numstat = run_git(
            repo_path, ["diff", "--numstat", "--root", commit_hash]
        )
    if not numstat:
        return None

    files_changed = []
    total_additions = 0
    total_deletions = 0

    for line in numstat.strip().split("\n"):
        if not line:
            continue
        parts = line.split("\t", 2)
        if len(parts) < 3:
            continue

        add_str, del_str, filename = parts
        # バイナリファイルは "-" が入る
        additions = int(add_str) if add_str != "-" else 0
        deletions = int(del_str) if del_str != "-" else 0

        ext = Path(filename).suffix.lower()
        files_changed.append(
            {
                "path": filename,
                "ext": ext,
                "additions": additions,
                "deletions": deletions,
            }
        )
        total_additions += additions
        total_deletions += deletions

    return {
        "files": files_changed,
        "total_files": len(files_changed),
        "total_additions": total_additions,
        "total_deletions": total_deletions,
    }


def classify_action(message: str, diff_stats: Dict) -> Dict:
    """コミットメッセージと diff から行動タイプを分類。"""
    msg_lower = message.lower()

    # 行動タイプ推定
    if any(kw in msg_lower for kw in ["fix", "修正", "bugfix", "hotfix"]):
        action_type = "fix"
    elif any(kw in msg_lower for kw in ["feat", "add", "追加", "実装", "新規"]):
        action_type = "add_feature"
    elif any(kw in msg_lower for kw in ["refactor", "リファクタ", "rename", "move"]):
        action_type = "refactor"
    elif any(kw in msg_lower for kw in ["test", "テスト"]):
        action_type = "add_test"
    elif any(kw in msg_lower for kw in ["doc", "readme", "ドキュメント", "comment"]):
        action_type = "documentation"
    elif any(kw in msg_lower for kw in ["delete", "remove", "削除", "clean"]):
        action_type = "remove"
    elif any(kw in msg_lower for kw in ["update", "更新", "bump", "upgrade"]):
        action_type = "update"
    elif any(kw in msg_lower for kw in ["perf", "optim", "最適化", "高速化"]):
        action_type = "optimize"
    else:
        action_type = "other"

    # 変更規模
    total_changes = diff_stats["total_additions"] + diff_stats["total_deletions"]
    if total_changes < 10:
        scale = "tiny"
    elif total_changes < 50:
        scale = "small"
    elif total_changes < 200:
        scale = "medium"
    elif total_changes < 1000:
        scale = "large"
    else:
        scale = "massive"

    # 主要言語
    ext_counts: Dict[str, int] = {}
    for f in diff_stats["files"]:
        ext = f["ext"]
        if ext:
            ext_counts[ext] = ext_counts.get(ext, 0) + f["additions"] + f["deletions"]
    primary_lang = max(ext_counts, key=ext_counts.get) if ext_counts else ""

    return {
        "type": action_type,
        "scale": scale,
        "primary_lang": primary_lang,
        "description": message[:200],
    }


def predict_outcome(
    message: str, diff_stats: Dict, next_commit: Optional[Dict]
) -> Dict:
    """コミットの結果を推定。"""
    msg_lower = message.lower()

    # コンパイル成功の推定
    # "fix" コミットの直前は失敗していた可能性が高い
    compile_success = True
    if any(kw in msg_lower for kw in ["wip", "broken", "todo", "temporary"]):
        compile_success = False

    # テスト通過の推定
    test_pass = True
    if any(kw in msg_lower for kw in ["fail", "broken", "skip test"]):
        test_pass = False

    # 連鎖修正の推定
    follow_up_needed = False
    if next_commit:
        next_msg = next_commit["message"].lower()
        # 次のコミットが fix → このコミットは問題を起こした可能性
        if any(kw in next_msg for kw in ["fix", "修正", "hotfix", "revert"]):
            follow_up_needed = True
            # 次のコミットが同じファイルを触っているか
            # (ここでは簡易推定のみ)

    # 影響範囲の推定
    affected_modules = set()
    for f in diff_stats["files"]:
        parts = f["path"].split("/")
        if len(parts) >= 2:
            affected_modules.add(parts[0])

    return {
        "compile_success": compile_success,
        "test_pass": test_pass,
        "follow_up_needed": follow_up_needed,
        "affected_modules": list(affected_modules),
        "risk_score": estimate_risk(diff_stats, follow_up_needed),
    }


def estimate_risk(diff_stats: Dict, follow_up_needed: bool) -> float:
    """変更のリスクスコアを推定 (0.0-1.0)。"""
    risk = 0.0

    # ファイル数が多いほどリスク高
    risk += min(0.3, diff_stats["total_files"] * 0.03)

    # 削除が多いほどリスク高
    if diff_stats["total_additions"] + diff_stats["total_deletions"] > 0:
        delete_ratio = diff_stats["total_deletions"] / (
            diff_stats["total_additions"] + diff_stats["total_deletions"]
        )
        risk += delete_ratio * 0.2

    # 変更量が多いほどリスク高
    total = diff_stats["total_additions"] + diff_stats["total_deletions"]
    risk += min(0.2, total * 0.0002)

    # 連鎖修正が必要だったらリスク高
    if follow_up_needed:
        risk += 0.3

    return min(1.0, risk)


def encode_state(diff_stats: Dict) -> Dict:
    """状態をベクトル化可能な形式にエンコード。"""
    # ファイル種別ごとの変更量
    ext_features: Dict[str, int] = {}
    for f in diff_stats["files"]:
        ext = f["ext"] if f["ext"] else ".other"
        key = f"ext_{ext.lstrip('.')}"
        ext_features[key] = ext_features.get(key, 0) + f["additions"] + f["deletions"]

    return {
        "total_files": diff_stats["total_files"],
        "total_additions": diff_stats["total_additions"],
        "total_deletions": diff_stats["total_deletions"],
        "change_ratio": (
            diff_stats["total_additions"]
            / max(1, diff_stats["total_additions"] + diff_stats["total_deletions"])
        ),
        "file_extensions": ext_features,
        "files_list": [f["path"] for f in diff_stats["files"][:20]],
    }


def process_repo(repo_path: str, max_commits: int) -> List[Dict]:
    """1つのリポジトリから学習データを抽出。"""
    repo_name = Path(repo_path).name
    print(f"  Processing {repo_name}...", end="", flush=True)

    commits = get_commit_list(repo_path, max_commits)
    if not commits:
        print(" (no commits)")
        return []

    samples = []
    for i, commit in enumerate(commits):
        diff = get_diff_stats(repo_path, commit["hash"])
        if diff is None or diff["total_files"] == 0:
            continue

        # 次のコミット (時系列では前のコミット)
        next_commit = commits[i - 1] if i > 0 else None

        state = encode_state(diff)
        action = classify_action(commit["message"], diff)
        outcome = predict_outcome(commit["message"], diff, next_commit)

        samples.append(
            {
                "repo": repo_name,
                "commit": commit["hash"][:12],
                "date": commit["date"],
                "state": state,
                "action": action,
                "outcome": outcome,
            }
        )

    print(f" {len(samples)} samples")
    return samples


def find_repos(scan_dir: str, prefix: str) -> List[str]:
    """ディレクトリをスキャンしてリポジトリを検出。"""
    repos = []
    for entry in sorted(os.listdir(scan_dir)):
        if not entry.startswith(prefix):
            continue
        path = os.path.join(scan_dir, entry)
        if os.path.isdir(path) and os.path.isdir(os.path.join(path, ".git")):
            repos.append(path)
    return repos


def main():
    parser = argparse.ArgumentParser(
        description="Git 履歴から WorldModel 学習データを抽出"
    )
    parser.add_argument(
        "--repos",
        nargs="+",
        help="対象リポジトリのパスリスト",
    )
    parser.add_argument(
        "--scan-dir",
        help="リポジトリをスキャンするディレクトリ",
    )
    parser.add_argument(
        "--prefix",
        default="ALICE-",
        help="スキャン対象のプレフィックス (default: ALICE-)",
    )
    parser.add_argument(
        "--output",
        default="data/world_model_training.jsonl",
        help="出力ファイル (default: data/world_model_training.jsonl)",
    )
    parser.add_argument(
        "--max-commits",
        type=int,
        default=200,
        help="リポジトリあたりの最大コミット数 (default: 200)",
    )
    args = parser.parse_args()

    # リポジトリ一覧
    repos = []
    if args.repos:
        repos = [os.path.expanduser(r) for r in args.repos]
    elif args.scan_dir:
        scan = os.path.expanduser(args.scan_dir)
        repos = find_repos(scan, args.prefix)
        # Project-ALICE も追加
        project_alice = os.path.join(scan, "Project-ALICE")
        if os.path.isdir(os.path.join(project_alice, ".git")):
            repos.append(project_alice)
    else:
        print("Error: --repos or --scan-dir required")
        sys.exit(1)

    print(f"Found {len(repos)} repositories")

    # 抽出
    all_samples = []
    for repo in repos:
        if not os.path.isdir(repo):
            print(f"  Skipping {repo} (not found)")
            continue
        samples = process_repo(repo, args.max_commits)
        all_samples.extend(samples)

    # 出力
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with open(output_path, "w") as f:
        for sample in all_samples:
            f.write(json.dumps(sample, ensure_ascii=False) + "\n")

    print(f"\nTotal: {len(all_samples)} samples → {output_path}")

    # 統計
    action_types: Dict[str, int] = {}
    risk_scores = []
    for s in all_samples:
        t = s["action"]["type"]
        action_types[t] = action_types.get(t, 0) + 1
        risk_scores.append(s["outcome"]["risk_score"])

    print("\nAction type distribution:")
    for t, count in sorted(action_types.items(), key=lambda x: -x[1]):
        print(f"  {t}: {count}")

    if risk_scores:
        avg_risk = sum(risk_scores) / len(risk_scores)
        follow_ups = sum(1 for s in all_samples if s["outcome"]["follow_up_needed"])
        print(f"\nAverage risk score: {avg_risk:.3f}")
        print(f"Follow-up needed: {follow_ups}/{len(all_samples)} ({100*follow_ups/len(all_samples):.1f}%)")


if __name__ == "__main__":
    main()
