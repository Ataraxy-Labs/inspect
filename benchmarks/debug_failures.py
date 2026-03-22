#!/usr/bin/env python3
"""Analyze failures: compare our output vs golden comments for all recent runs.

Usage: python3 benchmarks/debug_failures.py [run_log.json ...]
  If no args, uses the latest run log(s).
"""
import json, sys, glob, os
from pathlib import Path

BENCHMARK_DATA = "/tmp/martian-eval/code-review-benchmark/offline/results/benchmark_data.json"

def load_golden():
    with open(BENCHMARK_DATA) as f:
        return json.load(f)

def analyze_run(log_path, benchmark):
    with open(log_path) as f:
        run = json.load(f)

    print(f"\n{'='*80}")
    print(f"Run: {os.path.basename(log_path)} | model={run.get('model')} | {run.get('processed')} PRs")
    print(f"{'='*80}")

    total_golden = 0
    total_matched = 0
    total_ours = 0

    for pr in run.get("prs", []):
        if pr.get("status") not in ("ok",):
            continue

        golden_url = pr.get("golden_url", "")
        if not golden_url or golden_url not in benchmark:
            continue

        entry = benchmark[golden_url]
        golden_comments = entry.get("golden_comments", [])
        candidates = pr.get("agent", {}).get("candidates", [])
        findings = pr.get("agent", {}).get("finding_details", [])
        slices = pr.get("agent", {}).get("slices", [])

        pr_num = pr.get("pr_number", "?")
        repo = pr.get("repo", "?")

        print(f"\n--- {repo} PR#{pr_num}: {pr.get('pr_title', '')[:60]} ---")
        print(f"    Golden: {len(golden_comments)} | Ours: {len(candidates)} | Findings: {len(findings)} | Slices: {len(slices)}")
        print(f"    Time: {pr.get('agent',{}).get('elapsed_s',0):.0f}s | Tools: {pr.get('agent',{}).get('tool_calls',0)}")

        if findings:
            print(f"    Detector findings:")
            for f in findings:
                print(f"      [{f.get('severity','?')}] {f.get('rule','?')}: {f.get('entity','?')} @ {f.get('file','?')}")

        print(f"\n    GOLDEN BUGS:")
        for i, g in enumerate(golden_comments):
            comment = g.get("comment", str(g)) if isinstance(g, dict) else str(g)
            print(f"      {i+1}. {comment[:200]}")

        print(f"\n    OUR OUTPUT:")
        if not candidates:
            print(f"      (none)")
        for i, c in enumerate(candidates):
            print(f"      {i+1}. {c[:200]}")

        # Simple text matching to estimate hits/misses
        matched = set()
        for gi, g in enumerate(golden_comments):
            gtext = (g.get("comment", str(g)) if isinstance(g, dict) else str(g)).lower()
            for ci, c in enumerate(candidates):
                ctext = c.lower()
                # Extract key terms from golden
                gwords = set(w for w in gtext.split() if len(w) > 5)
                cwords = set(w for w in ctext.split() if len(w) > 5)
                overlap = len(gwords & cwords)
                if overlap >= 3:
                    matched.add(gi)
                    break

        missed = [i for i in range(len(golden_comments)) if i not in matched]
        if missed:
            print(f"\n    ❌ MISSED ({len(missed)}/{len(golden_comments)}):")
            for i in missed:
                g = golden_comments[i]
                comment = g.get("comment", str(g)) if isinstance(g, dict) else str(g)
                print(f"      {i+1}. {comment[:200]}")

        total_golden += len(golden_comments)
        total_matched += len(matched)
        total_ours += len(candidates)

    print(f"\n{'='*80}")
    print(f"TOTALS: {total_matched}/{total_golden} golden matched ({100*total_matched/max(total_golden,1):.0f}% recall)")
    print(f"        {total_ours} total issues output")
    if total_ours > 0:
        print(f"        Precision (upper bound): {100*total_matched/total_ours:.0f}%")
    print(f"{'='*80}")


def main():
    benchmark = load_golden()

    if len(sys.argv) > 1:
        logs = sys.argv[1:]
    else:
        # Use latest run log
        all_logs = sorted(glob.glob(str(Path(__file__).parent / "results" / "run_log_*.json")))
        logs = all_logs[-1:] if all_logs else []

    if not logs:
        print("No run logs found")
        return

    for log in logs:
        analyze_run(log, benchmark)


if __name__ == "__main__":
    main()
