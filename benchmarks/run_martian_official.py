#!/usr/bin/env python3
"""Run inspect agent on the official Martian benchmark PRs.

Reads cached worktrees/SHAs from our benchmark runs, runs inspect+agent,
injects results as tool="inspect" into the official benchmark_data.json,
then runs their step3 judge.

Usage:
    python3 benchmarks/run_martian_official.py [--limit N] [--model gpt-5.4] [--skip-agent]
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

# --- Config ---
INSPECT_BIN = os.path.expanduser("~/Documents/work/inspect/target/release/inspect")
AGENT_DIR = str(Path(__file__).resolve().parent.parent / "agent")
CACHE_DIR = "/tmp/martian-eval"
WORKTREES_DIR = f"{CACHE_DIR}/worktrees"
BENCHMARK_DIR = "/tmp/martian-eval/code-review-benchmark/offline"
BENCHMARK_DATA = f"{BENCHMARK_DIR}/results/benchmark_data.json"
OUR_RESULTS = str(Path(__file__).resolve().parent / "results" / "20260318_183258_ast.json")

TOOL_NAME = "inspect"


def load_dotenv():
    for p in [Path(__file__).resolve().parent.parent / ".env", Path.home() / ".env"]:
        if p.exists():
            with open(p) as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith("#") and "=" in line:
                        key, _, value = line.partition("=")
                        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))
            break


load_dotenv()
ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY", "")
OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY", "")


def run_inspect(wt_dir, base_sha, head_sha):
    """Run inspect diff, return parsed JSON."""
    diff_ref = f"{base_sha}..{head_sha}"
    result = subprocess.run(
        [INSPECT_BIN, "diff", diff_ref, "--repo", wt_dir, "--format", "json"],
        capture_output=True, text=True, timeout=300)
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def get_diff_text(wt_dir, base_sha, head_sha):
    result = subprocess.run(["git", "diff", base_sha, head_sha],
                            cwd=wt_dir, capture_output=True, text=True, timeout=120)
    return result.stdout if result.returncode == 0 else ""


def build_triage_section(entities):
    meaningful = [e for e in entities
                  if e.get("change_type") in ("Modified", "Added") and e.get("entity_type") != "chunk"]
    meaningful.sort(key=lambda e: e.get("risk_score", 0), reverse=True)
    top = meaningful[:20]
    if not top:
        return ""
    by_file = {}
    for e in top:
        by_file.setdefault(e.get("file_path", ""), []).append(e)
    file_entries = sorted(by_file.items(), key=lambda kv: max(e.get("risk_score", 0) for e in kv[1]), reverse=True)
    lines = ["## Entity-level triage (highest-risk changes)"]
    for fp, ents in file_entries:
        lines.append(f"\n### {fp}")
        for e in ents:
            public = " [PUBLIC API]" if e.get("is_public_api") else ""
            lines.append(f"- `{e.get('entity_name', '')}` ({e.get('entity_type', '')}, {e.get('change_type', '')}) | "
                         f"risk={e.get('risk_level', '')} ({e.get('risk_score', 0):.2f}) | "
                         f"dependents={e.get('dependent_count', 0)}{public}")
    return "\n".join(lines)


def run_agent(wt_dir, base_sha, head_sha, inspect_data, pr_title="", model=None):
    """Run agent, return list of issue strings (candidates for the judge)."""
    if not inspect_data:
        return []

    all_entities = inspect_data.get("entity_reviews", [])
    detector_findings = inspect_data.get("findings", [])
    diff_text = get_diff_text(wt_dir, base_sha, head_sha)
    if not diff_text:
        return []

    agent_input = {
        "pr_title": pr_title,
        "diff": diff_text[:80_000],
        "triage_section": build_triage_section(all_entities),
        "findings": detector_findings,
        "entity_reviews": all_entities,
        "repo_dir": os.path.abspath(wt_dir),
    }
    if model:
        if model.startswith("gpt") or model.startswith("o"):
            agent_input["provider"] = "openai"
        else:
            agent_input["provider"] = "anthropic"
        agent_input["model"] = model

    t0 = time.time()
    try:
        proc = subprocess.run(
            ["node", "--import", "tsx/esm", "src/validate.ts"],
            cwd=AGENT_DIR,
            input=json.dumps(agent_input),
            capture_output=True, text=True, timeout=300,
            env={**os.environ, "ANTHROPIC_API_KEY": ANTHROPIC_API_KEY, "OPENAI_API_KEY": OPENAI_API_KEY},
        )
    except subprocess.TimeoutExpired:
        print(f" TIMEOUT", file=sys.stderr, end="")
        return []

    elapsed = time.time() - t0
    tool_calls = proc.stderr.count("[tool]") if proc.stderr else 0
    print(f" {elapsed:.0f}s/{tool_calls}tc", file=sys.stderr, end="", flush=True)

    if proc.returncode != 0:
        print(f" FAIL", file=sys.stderr, end="")
        return []

    stdout = proc.stdout.strip()
    if not stdout:
        return []

    try:
        agent_out = json.loads(stdout)
    except json.JSONDecodeError:
        return []

    # Extract issue descriptions as candidate strings
    candidates = []
    for v in agent_out.get("verdicts", []):
        explanation = v.get("explanation", "")
        if explanation:
            candidates.append(explanation)

    return candidates


def find_worktree(our_pr):
    """Find the cached worktree for a PR from our AST results."""
    base_sha = our_pr.get("base_sha")
    head_sha = our_pr.get("head_sha")
    if not base_sha or not head_sha:
        return None, None, None

    # Search all worktree dirs
    for repo_dir in Path(WORKTREES_DIR).iterdir():
        wt_key = f"{base_sha[:12]}_{head_sha[:12]}"
        wt_path = repo_dir / wt_key
        if wt_path.exists():
            return str(wt_path), base_sha, head_sha

    return None, base_sha, head_sha


def match_golden_url(our_pr, benchmark_data):
    """Match our PR record to a golden URL in benchmark_data."""
    url = our_pr.get("url", "")
    # Direct match
    if url in benchmark_data:
        return url
    # Match via original_url (forked repos use original_url to point to real repo)
    pr_num = our_pr.get("pr_number")
    head_sha = our_pr.get("head_sha", "")
    for golden_url, entry in benchmark_data.items():
        orig = entry.get("original_url", "")
        # Match by PR number in original_url
        if orig and f"/pull/{pr_num}" in orig:
            return golden_url
        # Match by commit SHA in original_url (discourse uses commit URLs)
        if orig and head_sha and head_sha in orig:
            return golden_url
        # Match by commit SHA in the golden URL itself
        if head_sha and head_sha in golden_url:
            return golden_url
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, help="Limit PRs")
    parser.add_argument("--model", default="gpt-5.4", help="Agent model")
    parser.add_argument("--skip-agent", action="store_true", help="Skip agent, just show what we'd run")
    parser.add_argument("--repo", help="Filter by repo (keycloak, sentry, etc.)")
    args = parser.parse_args()

    # Load our cached AST results (has base_sha/head_sha for all 50 PRs)
    with open(OUR_RESULTS) as f:
        our_data = json.load(f)
    our_prs = our_data["prs"]
    print(f"Loaded {len(our_prs)} PRs from our AST cache", file=sys.stderr)

    # Load official benchmark data
    with open(BENCHMARK_DATA) as f:
        benchmark_data = json.load(f)
    print(f"Loaded {len(benchmark_data)} PRs from official benchmark", file=sys.stderr)

    # Filter
    if args.repo:
        our_prs = [p for p in our_prs if p.get("repo") == args.repo]
    if args.limit:
        our_prs = our_prs[:args.limit]

    # Skip already-skipped/errored PRs
    our_prs = [p for p in our_prs if not p.get("skipped") and not p.get("error")]
    print(f"Processing {len(our_prs)} PRs", file=sys.stderr)

    total_candidates = 0
    processed = 0
    start = time.time()

    for i, pr in enumerate(our_prs):
        pr_label = f"{pr['repo']} PR#{pr['pr_number']}"
        print(f"\n[{i+1}/{len(our_prs)}] {pr_label}: {pr['pr_title'][:50]}", file=sys.stderr, end="", flush=True)

        golden_url = match_golden_url(pr, benchmark_data)
        if not golden_url:
            print(f" NO_MATCH", file=sys.stderr)
            continue

        wt_dir, base_sha, head_sha = find_worktree(pr)
        if not wt_dir:
            print(f" NO_WORKTREE", file=sys.stderr)
            continue

        if args.skip_agent:
            print(f" SKIP (dry run)", file=sys.stderr)
            continue

        # Run inspect
        print(f" inspect...", file=sys.stderr, end="", flush=True)
        inspect_data = run_inspect(wt_dir, base_sha, head_sha)
        if not inspect_data:
            print(f" INSPECT_FAIL", file=sys.stderr)
            continue

        entities = inspect_data.get("entity_reviews", [])
        findings = inspect_data.get("findings", [])
        print(f" {len(entities)}e/{len(findings)}f", file=sys.stderr, end="", flush=True)

        # Run agent
        print(f" agent...", file=sys.stderr, end="", flush=True)
        candidates = run_agent(wt_dir, base_sha, head_sha, inspect_data,
                               pr_title=pr.get("pr_title", ""), model=args.model)
        print(f" → {len(candidates)} issues", file=sys.stderr, end="", flush=True)

        # Inject into benchmark_data as a new tool review
        entry = benchmark_data[golden_url]
        # Remove any existing inspect review
        entry["reviews"] = [r for r in entry.get("reviews", []) if r["tool"] != TOOL_NAME]
        # Add our review — candidates go as review_comments body
        review_comments = [{"path": None, "line": None, "body": c, "created_at": None} for c in candidates]
        entry["reviews"].append({
            "tool": TOOL_NAME,
            "repo_name": f"inspect-agent-{args.model}",
            "pr_url": golden_url,
            "review_comments": review_comments,
        })

        total_candidates += len(candidates)
        processed += 1

    elapsed = time.time() - start
    print(f"\n\nDone: {processed} PRs, {total_candidates} total candidates, {elapsed:.0f}s", file=sys.stderr)

    if not args.skip_agent:
        # Save updated benchmark_data
        with open(BENCHMARK_DATA, "w") as f:
            json.dump(benchmark_data, f, indent=2)
        print(f"Saved to {BENCHMARK_DATA}", file=sys.stderr)
        print(f"\nNext steps:", file=sys.stderr)
        print(f"  cd {BENCHMARK_DIR}", file=sys.stderr)
        print(f"  uv run python -m code_review_benchmark.step2_extract_comments --tool {TOOL_NAME}", file=sys.stderr)
        print(f"  uv run python -m code_review_benchmark.step3_judge_comments --tool {TOOL_NAME}", file=sys.stderr)


if __name__ == "__main__":
    main()
