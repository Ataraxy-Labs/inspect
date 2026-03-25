#!/usr/bin/env python3
"""
End-to-end Martian code-review benchmark for inspect.

Runs agent/src/review-diff-v2.ts on all 47 PRs, then scores with the
Martian LLM judge. Single command, full results.

Usage:
    python3 benchmarks/run_benchmark.py                    # full run
    python3 benchmarks/run_benchmark.py --concurrency 10   # limit parallelism
    python3 benchmarks/run_benchmark.py --judge-only       # re-judge latest run
    python3 benchmarks/run_benchmark.py --pr 37429 8330    # specific PRs only
"""

import argparse
import subprocess
import sys
import os
from pathlib import Path

BENCHMARKS_DIR = Path(__file__).resolve().parent
ROOT_DIR = BENCHMARKS_DIR.parent


def main():
    parser = argparse.ArgumentParser(description="End-to-end inspect benchmark")
    parser.add_argument("--concurrency", type=int, default=47, help="Max parallel agents (default: 47)")
    parser.add_argument("--model", default="claude-opus-4-6", help="Model to use")
    parser.add_argument("--pr", nargs="+", help="Specific PR numbers (default: all 47)")
    parser.add_argument("--judge-only", action="store_true", help="Skip agent run, just re-judge latest")
    parser.add_argument("--summary", help="Judge a specific summary file")
    args = parser.parse_args()

    # Step 1: Run agent reviews
    if not args.judge_only:
        print("=" * 60)
        print("STEP 1: Running review-diff-v2.ts on PRs")
        print("=" * 60)

        bench_cmd = [
            sys.executable, str(BENCHMARKS_DIR / "run_diff_bench_v4.py"),
            "--concurrency", str(args.concurrency),
            "--model", args.model,
        ]
        if args.pr:
            bench_cmd += ["--pr"] + args.pr
        else:
            bench_cmd += ["--all"]

        result = subprocess.run(bench_cmd, cwd=str(ROOT_DIR))
        if result.returncode != 0:
            print(f"\nBenchmark failed with exit code {result.returncode}", file=sys.stderr)
            sys.exit(1)

    # Step 2: Run Martian judge
    print("\n" + "=" * 60)
    print("STEP 2: Running Martian LLM judge")
    print("=" * 60)

    judge_cmd = [sys.executable, str(BENCHMARKS_DIR / "run_martian_judge.py")]
    if args.summary:
        judge_cmd += ["--summary", args.summary]

    result = subprocess.run(judge_cmd, cwd=str(ROOT_DIR))
    if result.returncode != 0:
        print(f"\nJudge failed with exit code {result.returncode}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
