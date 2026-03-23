#!/usr/bin/env python3
"""
Run review-diff-v2.ts (entity triage + separate extraction) on all 47 Martian benchmark PRs.

Differences from run_diff_bench.py:
  - Passes entity_reviews from inspect to the agent
  - Calls review-diff-v2.ts instead of review-diff.ts
  - Same logging format for apples-to-apples comparison

Usage:
    python3 benchmarks/run_diff_bench_v2.py [--concurrency 10] [--limit 5] [--repo discourse] [--model claude-opus-4-6]
"""

import argparse
import asyncio
import json
import os
import subprocess
import sys
import time
from pathlib import Path

# --- Config ---
INSPECT_BIN = os.path.expanduser("~/Documents/work/inspect/target/release/inspect")
AGENT_DIR = str(Path(__file__).resolve().parent.parent / "agent")
WORKTREES_DIR = "/tmp/martian-eval/worktrees"
BENCHMARK_DATA = "/tmp/martian-eval/code-review-benchmark/offline/results/benchmark_data.json"
OUR_RESULTS = str(Path(__file__).resolve().parent / "results" / "20260318_183258_ast.json")
LOG_DIR = str(Path(__file__).resolve().parent / "results" / "diff_bench_logs")


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


def find_worktree(pr):
    base_sha = pr.get("base_sha")
    head_sha = pr.get("head_sha")
    if not base_sha or not head_sha:
        return None, None, None
    for repo_dir in Path(WORKTREES_DIR).iterdir():
        wt_key = f"{base_sha[:12]}_{head_sha[:12]}"
        wt_path = repo_dir / wt_key
        if wt_path.exists():
            return str(wt_path), base_sha, head_sha
    return None, base_sha, head_sha


def match_golden_url(pr, benchmark_data):
    url = pr.get("url", "")
    if url in benchmark_data:
        return url
    pr_num = pr.get("pr_number")
    head_sha = pr.get("head_sha", "")
    for golden_url, entry in benchmark_data.items():
        orig = entry.get("original_url", "")
        if orig and f"/pull/{pr_num}" in orig:
            return golden_url
        if orig and head_sha and head_sha in orig:
            return golden_url
        if head_sha and head_sha in golden_url:
            return golden_url
    return None


def get_diff_text(wt_dir, base_sha, head_sha):
    # Exclude lock files, generated code, minified assets, vendor dirs, snapshots
    exclude_patterns = [
        # Lock files
        ":(exclude)*.lock",
        ":(exclude)*lock.json",
        ":(exclude)*-lock.yaml",
        # Minified / bundled
        ":(exclude)*.min.js",
        ":(exclude)*.min.css",
        ":(exclude)*.map",
        ":(exclude)*.bundle.js",
        # Binary / media
        ":(exclude)*.svg",
        ":(exclude)*.png",
        ":(exclude)*.jpg",
        ":(exclude)*.gif",
        ":(exclude)*.ico",
        ":(exclude)*.woff*",
        ":(exclude)*.ttf",
        ":(exclude)*.eot",
        # Vendor / dependencies
        ":(exclude)vendor/*",
        ":(exclude)*/vendor/*",
        # Generated code (Go, Java, OpenAPI)
        ":(exclude)**/zz_generated.*",
        ":(exclude)**/zz_openapi_gen.*",
        ":(exclude)**/*_gen.go",
        ":(exclude)**/*_generated.*",
        ":(exclude)*.generated.*",
        # Snapshots / test fixtures
        ":(exclude)*.snap",
        ":(exclude)**/__snapshots__/*",
        ":(exclude)**/*_snapshots/*",
        ":(exclude)**/openapi_snapshots/*",
        # Build artifacts
        ":(exclude)*.bat",
    ]
    result = subprocess.run(
        ["git", "diff", base_sha, head_sha, "--"] + exclude_patterns,
        cwd=wt_dir, capture_output=True, text=True, timeout=120,
    )
    return result.stdout if result.returncode == 0 else ""


def run_inspect(wt_dir, base_sha, head_sha):
    diff_ref = f"{base_sha}..{head_sha}"
    try:
        result = subprocess.run(
            [INSPECT_BIN, "diff", diff_ref, "--repo", wt_dir, "--format", "json"],
            capture_output=True, text=True, timeout=300,
        )
        if result.returncode != 0:
            return None
        return json.loads(result.stdout)
    except (subprocess.TimeoutExpired, json.JSONDecodeError):
        return None


async def run_one_pr(pr, benchmark_data, model, timestamp, semaphore):
    """Run review-diff-v2 on a single PR, return structured result."""
    async with semaphore:
        pr_label = f"{pr['repo']}_PR{pr['pr_number']}"
        safe_label = pr_label.replace("/", "_").replace(" ", "_")[:80]
        log_path = os.path.join(LOG_DIR, timestamp, f"{safe_label}.log")

        result = {
            "repo": pr["repo"],
            "pr_number": pr["pr_number"],
            "pr_title": pr.get("pr_title", ""),
            "status": "pending",
            "golden_comments": 0,
            "golden_texts": [],
            "num_candidates": 0,
            "candidates": [],
            "tool_calls": 0,
            "entity_count": 0,
            "finding_count": 0,
            "elapsed_s": 0,
            "log_file": log_path,
        }

        # Match golden
        golden_url = match_golden_url(pr, benchmark_data)
        if not golden_url:
            result["status"] = "no_match"
            print(f"  [{pr_label}] NO_MATCH", file=sys.stderr)
            return result

        golden_entry = benchmark_data.get(golden_url, {})
        golden_comments = golden_entry.get("golden_comments", [])
        result["golden_comments"] = len(golden_comments)
        result["golden_texts"] = [
            (g.get("comment", str(g)) if isinstance(g, dict) else str(g))[:500]
            for g in golden_comments
        ]

        # Find worktree
        wt_dir, base_sha, head_sha = find_worktree(pr)
        if not wt_dir:
            result["status"] = "no_worktree"
            print(f"  [{pr_label}] NO_WORKTREE", file=sys.stderr)
            return result

        # Get diff
        diff_text = get_diff_text(wt_dir, base_sha, head_sha)
        if not diff_text:
            result["status"] = "no_diff"
            print(f"  [{pr_label}] NO_DIFF", file=sys.stderr)
            return result

        # Run inspect for detector findings + entity reviews
        inspect_data = run_inspect(wt_dir, base_sha, head_sha)
        findings = inspect_data.get("findings", []) if inspect_data else []
        entity_reviews = inspect_data.get("entity_reviews", []) if inspect_data else []

        print(f"  [{pr_label}] diff={len(diff_text)} findings={len(findings)} entities={len(entity_reviews)} — running agent...",
              file=sys.stderr, flush=True)

        result["entity_count"] = len(entity_reviews)
        result["finding_count"] = len(findings)

        # Build agent input — v2 includes entity_reviews
        # Strip before_content/after_content to avoid blowing up stdin size
        # (the TS side uses them for Jaccard triage but they come through inspect's JSON)
        entity_reviews_slim = []
        for er in entity_reviews:
            slim = {k: v for k, v in er.items() if k not in ("before_content", "after_content")}
            # Keep content only for triage classification (Jaccard similarity)
            if er.get("before_content") and er.get("after_content"):
                slim["before_content"] = er["before_content"][:2000]
                slim["after_content"] = er["after_content"][:2000]
            elif er.get("after_content"):
                slim["after_content"] = er["after_content"][:2000]
            entity_reviews_slim.append(slim)

        agent_input = {
            "repo_dir": os.path.abspath(wt_dir),
            "diff": diff_text[:120_000],
            "findings": findings,
            "entity_reviews": entity_reviews_slim,
            "pr_title": pr.get("pr_title", ""),
            "provider": "anthropic",
            "model": model,
        }

        # Run review-diff-v2.ts
        t0 = time.time()
        try:
            proc = await asyncio.create_subprocess_exec(
                "node", "--import", "tsx/esm", "src/review-diff-v2.ts",
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=AGENT_DIR,
                env={**os.environ},
            )
            stdout_bytes, stderr_bytes = await asyncio.wait_for(
                proc.communicate(input=json.dumps(agent_input).encode()),
                timeout=900,
            )
            stdout_text = stdout_bytes.decode("utf-8", errors="replace")
            stderr_text = stderr_bytes.decode("utf-8", errors="replace")
        except asyncio.TimeoutError:
            result["status"] = "timeout"
            result["elapsed_s"] = round(time.time() - t0, 1)
            print(f"  [{pr_label}] TIMEOUT ({result['elapsed_s']}s)", file=sys.stderr)
            return result
        except Exception as e:
            result["status"] = "error"
            result["elapsed_s"] = round(time.time() - t0, 1)
            result["error"] = str(e)
            print(f"  [{pr_label}] ERROR: {e}", file=sys.stderr)
            return result

        elapsed = time.time() - t0
        result["elapsed_s"] = round(elapsed, 1)

        # Count tool calls from stderr
        tool_calls = stderr_text.count("[tool] #")
        result["tool_calls"] = tool_calls

        # Write per-PR log file (same format as v1 for comparison)
        os.makedirs(os.path.dirname(log_path), exist_ok=True)
        with open(log_path, "w") as f:
            f.write(f"{'='*80}\n")
            f.write(f"PR: {pr_label}\n")
            f.write(f"Title: {pr.get('pr_title', '')}\n")
            f.write(f"Model: {model}\n")
            f.write(f"Approach: v2 (entity-triage + separate-extraction)\n")
            f.write(f"Diff: {len(diff_text)} chars | Findings: {len(findings)} | Entities: {len(entity_reviews)}\n")
            f.write(f"Elapsed: {elapsed:.1f}s | Tool calls: {tool_calls}\n")
            f.write(f"{'='*80}\n\n")

            f.write("--- GOLDEN COMMENTS ---\n")
            for i, gt in enumerate(result["golden_texts"], 1):
                f.write(f"{i}. {gt}\n\n")
            f.write("\n")

            f.write("--- DETECTOR FINDINGS ---\n")
            for fi in findings:
                f.write(f"  [{fi['severity']}] {fi['rule_id']}: {fi['entity_name']} in {fi['file_path']}:{fi['start_line']}\n")
                f.write(f"    {fi['message']}\n")
            if not findings:
                f.write("  (none)\n")
            f.write("\n")

            f.write(f"--- ENTITY REVIEWS ({len(entity_reviews)} total) ---\n")
            for er in sorted(entity_reviews, key=lambda e: e.get("risk_score", 0), reverse=True)[:30]:
                pub = " [PUBLIC]" if er.get("is_public_api") else ""
                f.write(f"  [{er.get('risk_level','?').upper()}] {er.get('entity_name','')} ({er.get('entity_type','')}, {er.get('change_type','')}) risk={er.get('risk_score',0):.2f} deps={er.get('dependent_count',0)}{pub} in {er.get('file_path','')}\n")
            if len(entity_reviews) > 30:
                f.write(f"  ... and {len(entity_reviews) - 30} more\n")
            f.write("\n")

            f.write("--- AGENT STDERR (tool calls + raw review) ---\n")
            f.write(stderr_text)
            f.write("\n\n")

            f.write("--- AGENT STDOUT (parsed JSON) ---\n")
            f.write(stdout_text)
            f.write("\n")

        # Parse output
        candidates = []
        try:
            agent_out = json.loads(stdout_text.strip())
            for v in agent_out.get("verdicts", []):
                explanation = v.get("explanation", "")
                if explanation:
                    candidates.append(explanation)
        except json.JSONDecodeError:
            result["status"] = "json_error"
            print(f"  [{pr_label}] JSON_ERROR ({elapsed:.0f}s, {tool_calls}tc)",
                  file=sys.stderr)
            return result

        result["status"] = "ok"
        result["num_candidates"] = len(candidates)
        result["candidates"] = candidates

        print(f"  [{pr_label}] OK — {len(candidates)} issues, {tool_calls}tc, {elapsed:.0f}s",
              file=sys.stderr, flush=True)

        return result


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--concurrency", type=int, default=10, help="Max parallel agents")
    parser.add_argument("--limit", type=int, help="Limit PRs")
    parser.add_argument("--model", default="claude-opus-4-6", help="Model to use")
    parser.add_argument("--repo", help="Filter by repo (keycloak, discourse, etc.)")
    args = parser.parse_args()

    # Load PR data
    with open(OUR_RESULTS) as f:
        our_data = json.load(f)
    our_prs = our_data["prs"]
    our_prs = [p for p in our_prs if not p.get("skipped") and not p.get("error")]
    print(f"Loaded {len(our_prs)} PRs", file=sys.stderr)

    # Load golden data
    with open(BENCHMARK_DATA) as f:
        benchmark_data = json.load(f)
    print(f"Loaded {len(benchmark_data)} golden PRs", file=sys.stderr)

    # Filter
    if args.repo:
        our_prs = [p for p in our_prs if p.get("repo") == args.repo]
    if args.limit:
        our_prs = our_prs[:args.limit]

    print(f"Running {len(our_prs)} PRs with concurrency={args.concurrency}, model={args.model}",
          file=sys.stderr)

    timestamp = time.strftime("%Y%m%d_%H%M%S")
    semaphore = asyncio.Semaphore(args.concurrency)

    # Run all PRs in parallel
    t0 = time.time()
    tasks = [run_one_pr(pr, benchmark_data, args.model, timestamp, semaphore) for pr in our_prs]
    results = await asyncio.gather(*tasks)
    total_elapsed = time.time() - t0

    # Summary
    ok = [r for r in results if r["status"] == "ok"]
    total_candidates = sum(r["num_candidates"] for r in ok)
    total_golden = sum(r["golden_comments"] for r in results if r.get("golden_comments"))
    total_tools = sum(r["tool_calls"] for r in ok)

    print(f"\n{'='*60}", file=sys.stderr)
    print(f"DONE: {len(ok)}/{len(results)} PRs processed", file=sys.stderr)
    print(f"Total candidates: {total_candidates}", file=sys.stderr)
    print(f"Total golden: {total_golden}", file=sys.stderr)
    print(f"Total tool calls: {total_tools}", file=sys.stderr)
    print(f"Total time: {total_elapsed:.0f}s (wall clock)", file=sys.stderr)
    print(f"Avg per PR: {total_elapsed/max(len(ok),1):.0f}s", file=sys.stderr)
    print(f"Logs: {LOG_DIR}/{timestamp}/", file=sys.stderr)

    # Per-repo breakdown
    repos = {}
    for r in results:
        repo = r["repo"]
        if repo not in repos:
            repos[repo] = {"ok": 0, "total": 0, "candidates": 0, "golden": 0}
        repos[repo]["total"] += 1
        if r["status"] == "ok":
            repos[repo]["ok"] += 1
            repos[repo]["candidates"] += r["num_candidates"]
        repos[repo]["golden"] += r.get("golden_comments", 0)

    print(f"\nPer-repo:", file=sys.stderr)
    for repo, stats in sorted(repos.items()):
        print(f"  {repo}: {stats['ok']}/{stats['total']} PRs, "
              f"{stats['candidates']} candidates vs {stats['golden']} golden",
              file=sys.stderr)

    # Save summary
    summary_path = os.path.join(LOG_DIR, timestamp, "_summary.json")
    os.makedirs(os.path.dirname(summary_path), exist_ok=True)
    summary = {
        "timestamp": timestamp,
        "model": args.model,
        "approach": "v2-entity-triage-separate-extraction",
        "concurrency": args.concurrency,
        "total_prs": len(results),
        "processed": len(ok),
        "total_candidates": total_candidates,
        "total_golden": total_golden,
        "total_tool_calls": total_tools,
        "wall_clock_s": round(total_elapsed, 1),
        "prs": results,
    }
    with open(summary_path, "w") as f:
        json.dump(summary, f, indent=2)
    print(f"\nSummary: {summary_path}", file=sys.stderr)

    # Also inject into benchmark_data for official eval
    for r in ok:
        golden_url = match_golden_url(
            {"url": "", "pr_number": r["pr_number"], "head_sha": ""},
            benchmark_data,
        )
        matching_pr = next((p for p in our_prs if p["pr_number"] == r["pr_number"]), None)
        if matching_pr:
            golden_url = match_golden_url(matching_pr, benchmark_data)
        if golden_url and golden_url in benchmark_data:
            entry = benchmark_data[golden_url]
            entry["reviews"] = [rv for rv in entry.get("reviews", []) if rv["tool"] != "inspect-diff-v2"]
            review_comments = [
                {"path": None, "line": None, "body": c, "created_at": None}
                for c in r["candidates"]
            ]
            entry["reviews"].append({
                "tool": "inspect-diff-v2",
                "repo_name": f"inspect-diff-v2-{args.model}",
                "pr_url": golden_url,
                "review_comments": review_comments,
            })

    benchmark_out = os.path.join(LOG_DIR, timestamp, "_benchmark_data_v2.json")
    with open(benchmark_out, "w") as f:
        json.dump(benchmark_data, f, indent=2)
    print(f"Benchmark data: {benchmark_out}", file=sys.stderr)
    print(f"\nTo run official eval:", file=sys.stderr)
    print(f"  cp {benchmark_out} {BENCHMARK_DATA}", file=sys.stderr)
    print(f"  cd /tmp/martian-eval/code-review-benchmark/offline", file=sys.stderr)
    print(f"  uv run python -m code_review_benchmark.step2_extract_comments --tool inspect-diff-v2", file=sys.stderr)
    print(f"  uv run python -m code_review_benchmark.step3_judge_comments --tool inspect-diff-v2", file=sys.stderr)


if __name__ == "__main__":
    asyncio.run(main())
