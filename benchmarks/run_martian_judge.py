#!/usr/bin/env python3
"""
Run the Martian code-review-benchmark judge on inspect's benchmark results.

Takes the output of run_diff_bench_v4.py (_summary_v7.json) and:
1. Maps our PR results to golden URLs in benchmark_data.json
2. Builds candidates.json with 1 raw review blob per PR
3. Runs step3_judge_comments.py to score TP/FP/FN
4. Prints results and leaderboard comparison

Usage:
    # Use latest summary (auto-detects most recent run)
    python3 benchmarks/run_martian_judge.py

    # Use specific summary file
    python3 benchmarks/run_martian_judge.py --summary benchmarks/results/diff_bench_logs/20260325_004352/_summary_v7.json

    # Skip judge run, just show results from existing evaluations
    python3 benchmarks/run_martian_judge.py --results-only
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

# --- Config ---
MARTIAN_DIR = "/tmp/martian-eval/code-review-benchmark-full/offline"
BENCHMARK_DATA = os.path.join(MARTIAN_DIR, "results/benchmark_data.json")
LOG_DIR = str(Path(__file__).resolve().parent / "results" / "diff_bench_logs")

# Leaderboard evaluations from other tools
LEADERBOARD_EVALS = "/tmp/martian-eval/code-review-benchmark/offline/results/openai_gpt-5.2/evaluations.json"


def find_latest_summary():
    """Find the most recent _summary_v7.json in the logs directory."""
    log_path = Path(LOG_DIR)
    if not log_path.exists():
        return None
    summaries = []
    for d in sorted(log_path.iterdir(), reverse=True):
        if d.is_dir():
            s = d / "_summary_v7.json"
            if s.exists():
                summaries.append(s)
    return str(summaries[0]) if summaries else None


def load_golden_url_map(benchmark_data):
    """Build mapping from (repo, pr_number) -> golden_url.

    Handles:
    - Direct PR URLs (cal.com, grafana, keycloak, sentry)
    - Discourse graphite forks (commit SHA in original_url)
    - Keycloak/sentry greptile forks (PR number in original_url)
    """
    url_map = {}  # (repo_prefix, pr_number_str) -> golden_url

    for golden_url, entry in benchmark_data.items():
        original = entry.get("original_url") or ""
        parts = golden_url.rstrip("/").split("/")

        if "/pull/" in golden_url:
            pr_num = parts[-1]

            # Direct repos: map GitHub org → our repo name
            org_to_repo = {
                "calcom": "cal_dot_com",
                "grafana": "grafana",
                "keycloak": "keycloak",
                "getsentry": "sentry",
            }
            # parts = ['https:', '', 'github.com', org, repo, 'pull', num]
            repo_org = parts[3] if len(parts) >= 7 else parts[-3]
            if repo_org in org_to_repo:
                url_map[(org_to_repo[repo_org], pr_num)] = golden_url

            # Greptile/graphite forks: use original_url to get real PR number
            if ("greptile" in golden_url or "graphite" in golden_url) and original:
                if "/pull/" in original:
                    orig_pr = original.rstrip("/").split("/")[-1]
                    if "keycloak" in golden_url:
                        url_map[("keycloak", orig_pr)] = golden_url
                    elif "sentry" in golden_url:
                        url_map[("sentry", orig_pr)] = golden_url
                elif "/commit/" in original:
                    # Discourse: original_url has commit SHA
                    sha = original.rstrip("/").split("/")[-1]
                    url_map[("discourse", sha)] = golden_url

    return url_map


def build_candidates(summary, benchmark_data):
    """Build candidates.json with 1 raw review blob per PR."""
    url_map = load_golden_url_map(benchmark_data)

    candidates = {}
    matched = 0
    missed = []

    for pr in summary.get("prs", []):
        if pr.get("status") != "ok":
            continue
        raw_review = pr.get("raw_review", "")
        if not raw_review or len(raw_review) < 100:
            missed.append((pr["repo"], pr["pr_number"], "no raw_review"))
            continue

        pr_num = str(pr["pr_number"])
        repo = pr["repo"]

        golden_url = url_map.get((repo, pr_num))
        if not golden_url:
            missed.append((repo, pr_num, "no golden URL match"))
            continue

        candidates[golden_url] = {
            "inspect": [
                {"text": raw_review, "path": "", "line": 0, "source": "raw_review_blob"}
            ]
        }
        matched += 1

    return candidates, matched, missed


def run_judge(candidates_path, evaluations_path):
    """Run step3_judge_comments.py."""
    env = {**os.environ}
    # Load .env from martian dir
    env_file = Path(MARTIAN_DIR) / ".env"
    if env_file.exists():
        with open(env_file) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    key, _, value = line.partition("=")
                    env[key.strip()] = value.strip().strip("'\"")

    cmd = [
        "python3",
        "code_review_benchmark/step3_judge_comments.py",
        "--tool", "inspect",
        "--force",
        "--evaluations-file", evaluations_path,
    ]
    print(f"\nRunning judge: {' '.join(cmd)}")
    result = subprocess.run(cmd, cwd=MARTIAN_DIR, env=env, capture_output=True, text=True, timeout=600)
    print(result.stderr)
    if result.returncode != 0:
        print(f"Judge failed with exit code {result.returncode}", file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        return False
    return True


def print_results(evaluations_path):
    """Print TP/FP/FN results and leaderboard comparison."""
    with open(evaluations_path) as f:
        data = json.load(f)

    tp = fp = fn = evaluated = 0
    for url, entry in data.items():
        e = entry.get("inspect", {})
        if e.get("skipped"):
            continue
        evaluated += 1
        tp += e.get("tp", 0)
        fp += e.get("fp", 0)
        fn += e.get("fn", 0)

    p = tp / (tp + fp) * 100 if tp + fp else 0
    r = tp / (tp + fn) * 100 if tp + fn else 0
    f1 = 2 * p * r / (p + r) if p + r else 0

    print(f"\n{'='*60}")
    print(f"INSPECT RESULTS (blob mode)")
    print(f"{'='*60}")
    print(f"Evaluated: {evaluated} PRs")
    print(f"TP={tp}, FP={fp}, FN={fn}")
    print(f"Precision={p:.1f}%, Recall={r:.1f}%, F1={f1:.1f}%")

    # Leaderboard comparison
    if os.path.exists(LEADERBOARD_EVALS):
        with open(LEADERBOARD_EVALS) as f:
            lb = json.load(f)

        from collections import defaultdict
        tools = defaultdict(lambda: {"tp": 0, "fp": 0, "fn": 0})
        for url, entry in lb.items():
            for tool, ev in entry.items():
                if not isinstance(ev, dict) or ev.get("skipped"):
                    continue
                tools[tool]["tp"] += ev.get("tp", 0)
                tools[tool]["fp"] += ev.get("fp", 0)
                tools[tool]["fn"] += ev.get("fn", 0)

        rows = []
        for name, v in tools.items():
            t, f, n = v["tp"], v["fp"], v["fn"]
            pp = t / (t + f) * 100 if t + f else 0
            rr = t / (t + n) * 100 if t + n else 0
            ff = 2 * pp * rr / (pp + rr) if pp + rr else 0
            rows.append((ff, name, pp, rr))

        rows.append((f1, ">>> inspect", p, r))
        rows.sort(reverse=True)

        print(f"\n{'='*60}")
        print(f"LEADERBOARD")
        print(f"{'='*60}")
        print(f"{'#':<4} {'Tool':<30} {'F1':>7} {'P':>7} {'R':>7}")
        print("-" * 60)
        for i, (ff, name, pp, rr) in enumerate(rows[:15], 1):
            marker = " <<<" if "inspect" in name else ""
            print(f"{i:<4} {name:<30} {ff:>6.1f}% {pp:>6.1f}% {rr:>6.1f}%{marker}")

    return f1


def main():
    parser = argparse.ArgumentParser(description="Run Martian judge on inspect benchmark results")
    parser.add_argument("--summary", help="Path to _summary_v7.json (default: latest)")
    parser.add_argument("--results-only", action="store_true", help="Skip judge run, just show results")
    parser.add_argument("--evaluations", help="Path to evaluations output file",
                        default=os.path.join(MARTIAN_DIR, "results/inspect_v10/evaluations_blob.json"))
    args = parser.parse_args()

    if args.results_only:
        if os.path.exists(args.evaluations):
            print_results(args.evaluations)
        else:
            print(f"No evaluations file found: {args.evaluations}", file=sys.stderr)
        return

    # Find summary
    summary_path = args.summary or find_latest_summary()
    if not summary_path or not os.path.exists(summary_path):
        print("No summary file found. Run run_diff_bench_v4.py --all first.", file=sys.stderr)
        sys.exit(1)

    print(f"Summary: {summary_path}")

    with open(summary_path) as f:
        summary = json.load(f)

    with open(BENCHMARK_DATA) as f:
        benchmark_data = json.load(f)

    # Build candidates
    candidates, matched, missed = build_candidates(summary, benchmark_data)
    print(f"Matched {matched} PRs to golden URLs")
    if missed:
        print(f"Missed {len(missed)} PRs:")
        for repo, pr_num, reason in missed:
            print(f"  {repo}/PR{pr_num}: {reason}")

    # Write candidates.json
    model_dir = Path(MARTIAN_DIR) / "results" / "gpt-5.2"
    model_dir.mkdir(parents=True, exist_ok=True)
    candidates_path = str(model_dir / "candidates.json")
    with open(candidates_path, "w") as f:
        json.dump(candidates, f, indent=2)
    print(f"Wrote {candidates_path} ({len(candidates)} PRs)")

    # Run judge
    ok = run_judge(candidates_path, args.evaluations)
    if not ok:
        sys.exit(1)

    # Print results
    print_results(args.evaluations)


if __name__ == "__main__":
    main()
