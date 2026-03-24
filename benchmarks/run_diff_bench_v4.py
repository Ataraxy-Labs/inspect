#!/usr/bin/env python3
"""
Run review-diff-v2.ts (v4 changes: anti-dismissal, diff-reorder, tighter extraction)
on a targeted subset of PRs that had high FN counts in v3.

By default runs only the 8 worst-performing PRs from v3.
Use --all to run the full 47-PR benchmark.

Usage:
    python3 benchmarks/run_diff_bench_v4.py                    # 8 target PRs
    python3 benchmarks/run_diff_bench_v4.py --all              # all 47 PRs
    python3 benchmarks/run_diff_bench_v4.py --pr 79265 93824   # specific PRs
    python3 benchmarks/run_diff_bench_v4.py --repo grafana     # specific repo
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

# V3 results for comparison
V3_SUMMARY = str(Path(__file__).resolve().parent / "results" / "diff_bench_logs" / "20260323_141514" / "_summary.json")

# PRs with high FN counts in v3 — these are the validation targets
TARGET_PRS = [
    ("grafana", 79265),       # 5 FN: attention dilution, 364 entities, diff truncated
    ("sentry", 93824),        # 4 FN: agent dismissed isinstance() check, test bugs
    ("keycloak", 41249),      # 2 FN: agent said "no bugs found"
    ("keycloak", 32918),      # 2 FN: missed recursive caching bug
    ("grafana", 106778),      # 2 FN: missed key prop + silence bugs
    ("sentry", 95633),        # 2 FN: missed queue.shutdown, magic number
    ("cal_dot_com", 22345),   # 1 FN: agent dismissed as "functionally equivalent"
    ("discourse", "267d8be1f556ed59639ced396c885bb44586da19"),  # 1 FN: dismissed "fine for Discourse"
]


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
    exclude_patterns = [
        ":(exclude)*.lock", ":(exclude)*lock.json", ":(exclude)*-lock.yaml",
        ":(exclude)*.min.js", ":(exclude)*.min.css", ":(exclude)*.map", ":(exclude)*.bundle.js",
        ":(exclude)*.svg", ":(exclude)*.png", ":(exclude)*.jpg", ":(exclude)*.gif",
        ":(exclude)*.ico", ":(exclude)*.woff*", ":(exclude)*.ttf", ":(exclude)*.eot",
        ":(exclude)vendor/*", ":(exclude)*/vendor/*",
        ":(exclude)**/zz_generated.*", ":(exclude)**/zz_openapi_gen.*",
        ":(exclude)**/*_gen.go", ":(exclude)**/*_generated.*", ":(exclude)*.generated.*",
        ":(exclude)*.snap", ":(exclude)**/__snapshots__/*", ":(exclude)**/*_snapshots/*",
        ":(exclude)**/openapi_snapshots/*",
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


def load_v3_results():
    """Load v3 results for comparison."""
    if not os.path.exists(V3_SUMMARY):
        return {}
    with open(V3_SUMMARY) as f:
        data = json.load(f)
    lookup = {}
    for pr in data.get("prs", []):
        key = f"{pr['repo']}_PR{pr['pr_number']}"
        lookup[key] = pr
    return lookup


async def run_one_pr(pr, benchmark_data, model, timestamp, semaphore):
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

        wt_dir, base_sha, head_sha = find_worktree(pr)
        if not wt_dir:
            result["status"] = "no_worktree"
            print(f"  [{pr_label}] NO_WORKTREE", file=sys.stderr)
            return result

        diff_text = get_diff_text(wt_dir, base_sha, head_sha)
        if not diff_text:
            result["status"] = "no_diff"
            print(f"  [{pr_label}] NO_DIFF", file=sys.stderr)
            return result

        inspect_data = run_inspect(wt_dir, base_sha, head_sha)
        findings = inspect_data.get("findings", []) if inspect_data else []
        entity_reviews = inspect_data.get("entity_reviews", []) if inspect_data else []

        print(f"  [{pr_label}] diff={len(diff_text)} findings={len(findings)} entities={len(entity_reviews)} — running agent...",
              file=sys.stderr, flush=True)

        result["entity_count"] = len(entity_reviews)
        result["finding_count"] = len(findings)

        entity_reviews_slim = []
        for er in entity_reviews:
            slim = {k: v for k, v in er.items() if k not in ("before_content", "after_content")}
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

        t0 = time.time()
        max_retries = 2
        for attempt in range(1, max_retries + 1):
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

                # Retry if raw response was empty (generation failure)
                raw_marker = "=== RAW RESPONSE ==="
                end_marker = "=== END RAW RESPONSE ==="
                raw_start = stderr_text.find(raw_marker)
                raw_end = stderr_text.find(end_marker)
                if raw_start != -1 and raw_end != -1:
                    raw_content = stderr_text[raw_start + len(raw_marker):raw_end].strip()
                    if len(raw_content) < 50 and attempt < max_retries:
                        print(f"  [{pr_label}] Empty raw response (attempt {attempt}), retrying...",
                              file=sys.stderr, flush=True)
                        continue

                # Retry if no tool calls and very fast (likely crash)
                tool_count = stderr_text.count("[tool] #")
                elapsed_so_far = time.time() - t0
                if tool_count == 0 and elapsed_so_far < 30 and attempt < max_retries:
                    print(f"  [{pr_label}] Zero tool calls in {elapsed_so_far:.0f}s (attempt {attempt}), retrying...",
                          file=sys.stderr, flush=True)
                    continue

                break  # Success or final attempt
            except asyncio.TimeoutError:
                if attempt < max_retries:
                    print(f"  [{pr_label}] TIMEOUT (attempt {attempt}), retrying...",
                          file=sys.stderr, flush=True)
                    continue
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

        tool_calls = stderr_text.count("[tool] #")
        result["tool_calls"] = tool_calls

        os.makedirs(os.path.dirname(log_path), exist_ok=True)
        with open(log_path, "w") as f:
            f.write(f"{'='*80}\n")
            f.write(f"PR: {pr_label}\n")
            f.write(f"Title: {pr.get('pr_title', '')}\n")
            f.write(f"Model: {model}\n")
            f.write(f"Approach: v7 (v3-breadth + inclusive-extraction + dedup + cap8, no structured-block, no validation-filter)\n")
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

        candidates = []
        try:
            agent_out = json.loads(stdout_text.strip())
            for v in agent_out.get("verdicts", []):
                explanation = v.get("explanation", "")
                if explanation:
                    candidates.append(explanation)
        except json.JSONDecodeError:
            result["status"] = "json_error"
            print(f"  [{pr_label}] JSON_ERROR ({elapsed:.0f}s, {tool_calls}tc)", file=sys.stderr)
            return result

        result["status"] = "ok"
        result["num_candidates"] = len(candidates)
        result["candidates"] = candidates

        print(f"  [{pr_label}] OK — {len(candidates)} issues, {tool_calls}tc, {elapsed:.0f}s",
              file=sys.stderr, flush=True)

        return result


async def main():
    parser = argparse.ArgumentParser(description="Run v4 benchmark (targeted or full)")
    parser.add_argument("--concurrency", type=int, default=5, help="Max parallel agents")
    parser.add_argument("--limit", type=int, help="Limit PRs")
    parser.add_argument("--model", default="claude-opus-4-6", help="Model to use")
    parser.add_argument("--repo", help="Filter by repo")
    parser.add_argument("--pr", nargs="+", help="Specific PR numbers to run")
    parser.add_argument("--all", action="store_true", help="Run all 47 PRs (full benchmark)")
    args = parser.parse_args()

    # Load PR data
    with open(OUR_RESULTS) as f:
        our_data = json.load(f)
    our_prs = our_data["prs"]
    our_prs = [p for p in our_prs if not p.get("skipped") and not p.get("error")]
    print(f"Loaded {len(our_prs)} PRs total", file=sys.stderr)

    with open(BENCHMARK_DATA) as f:
        benchmark_data = json.load(f)
    print(f"Loaded {len(benchmark_data)} golden PRs", file=sys.stderr)

    # Filter to target PRs
    if args.all:
        selected = our_prs
        mode = "FULL (all 47 PRs)"
    elif args.pr:
        pr_set = set(args.pr)
        selected = [p for p in our_prs if str(p["pr_number"]) in pr_set]
        mode = f"SPECIFIC ({len(selected)} PRs: {', '.join(args.pr)})"
    elif args.repo:
        selected = [p for p in our_prs if p.get("repo") == args.repo]
        mode = f"REPO ({args.repo}, {len(selected)} PRs)"
    else:
        # Default: only target PRs with high FN counts
        target_set = {(repo, pr_num) for repo, pr_num in TARGET_PRS}
        selected = [
            p for p in our_prs
            if (p["repo"], p["pr_number"]) in target_set
        ]
        mode = f"TARGETED ({len(selected)} worst-FN PRs from v3)"

    if args.limit:
        selected = selected[:args.limit]

    print(f"Mode: {mode}", file=sys.stderr)
    print(f"Running {len(selected)} PRs with concurrency={args.concurrency}, model={args.model}",
          file=sys.stderr)

    # Load v3 results for comparison
    v3_lookup = load_v3_results()

    timestamp = time.strftime("%Y%m%d_%H%M%S")
    semaphore = asyncio.Semaphore(args.concurrency)

    t0 = time.time()
    tasks = [run_one_pr(pr, benchmark_data, args.model, timestamp, semaphore) for pr in selected]
    results = await asyncio.gather(*tasks)
    total_elapsed = time.time() - t0

    ok = [r for r in results if r["status"] == "ok"]
    total_candidates = sum(r["num_candidates"] for r in ok)
    total_golden = sum(r["golden_comments"] for r in results if r.get("golden_comments"))
    total_tools = sum(r["tool_calls"] for r in ok)

    print(f"\n{'='*80}", file=sys.stderr)
    print(f"V4 RESULTS — {mode}", file=sys.stderr)
    print(f"{'='*80}", file=sys.stderr)
    print(f"Processed: {len(ok)}/{len(results)} PRs", file=sys.stderr)
    print(f"Total candidates: {total_candidates}", file=sys.stderr)
    print(f"Total golden: {total_golden}", file=sys.stderr)
    print(f"Total tool calls: {total_tools}", file=sys.stderr)
    print(f"Total time: {total_elapsed:.0f}s", file=sys.stderr)
    print(f"Logs: {LOG_DIR}/{timestamp}/", file=sys.stderr)

    # Per-PR comparison with v3
    print(f"\n{'='*80}", file=sys.stderr)
    print(f"{'PR':<45} {'v3 cands':>10} {'v4 cands':>10} {'golden':>8} {'v3 tools':>10} {'v4 tools':>10} {'v4 time':>8}", file=sys.stderr)
    print(f"{'-'*45} {'-'*10} {'-'*10} {'-'*8} {'-'*10} {'-'*10} {'-'*8}", file=sys.stderr)

    for r in results:
        if r["status"] != "ok":
            print(f"  {r['repo']}_PR{r['pr_number']:<30} {r['status']}", file=sys.stderr)
            continue
        pr_key = f"{r['repo']}_PR{r['pr_number']}"
        v3 = v3_lookup.get(pr_key, {})
        v3_cands = v3.get("num_candidates", "?")
        v3_tools = v3.get("tool_calls", "?")
        print(f"  {pr_key:<43} {str(v3_cands):>10} {r['num_candidates']:>10} {r['golden_comments']:>8} {str(v3_tools):>10} {r['tool_calls']:>10} {r['elapsed_s']:>7.0f}s",
              file=sys.stderr)

    print(f"{'='*80}", file=sys.stderr)

    # Print golden vs candidates for manual review
    print(f"\n{'='*80}", file=sys.stderr)
    print(f"DETAILED: Golden comments vs V4 candidates", file=sys.stderr)
    print(f"{'='*80}", file=sys.stderr)
    for r in ok:
        pr_key = f"{r['repo']}_PR{r['pr_number']}"
        print(f"\n--- {pr_key} (golden={r['golden_comments']}, v4_candidates={r['num_candidates']}) ---",
              file=sys.stderr)
        print(f"  GOLDEN:", file=sys.stderr)
        for i, gt in enumerate(r["golden_texts"], 1):
            print(f"    G{i}: {gt[:200]}", file=sys.stderr)
        print(f"  V4 CANDIDATES:", file=sys.stderr)
        for i, c in enumerate(r["candidates"], 1):
            print(f"    C{i}: {c[:200]}", file=sys.stderr)
        if not r["candidates"]:
            print(f"    (none)", file=sys.stderr)

    # Save summary
    summary_path = os.path.join(LOG_DIR, timestamp, "_summary_v7.json")
    os.makedirs(os.path.dirname(summary_path), exist_ok=True)
    summary = {
        "timestamp": timestamp,
        "model": args.model,
        "approach": "v7-inclusive-extraction-dedup-cap8-no-validation-filter",
        "mode": mode,
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
    print(f"\nNext steps:", file=sys.stderr)
    print(f"  1. Review logs in {LOG_DIR}/{timestamp}/ (check RAW RESPONSE for each PR)", file=sys.stderr)
    print(f"  2. Compare golden vs candidates printed above", file=sys.stderr)
    print(f"  3. If improved, run full benchmark: python3 benchmarks/run_diff_bench_v4.py --all", file=sys.stderr)


if __name__ == "__main__":
    asyncio.run(main())
