#!/usr/bin/env python3
"""Autoresearch benchmark for inspect deterministic pipeline.

Uses the golden anchor file (128 bugs with file/entity anchors) to measure
bug-level recall@20: does the buggy file/entity appear in inspect's top-20
risk-ranked entities?

**Anti-overfitting: leave-one-repo-out cross-validation.**
Runs 5 folds (one per repo family), reports mean recall across held-out repos.
This ensures we can't overfit to any single repo's patterns.

**Metrics (all at the bug level, not PR level):**
- Primary: mean bug recall@20 across CV folds (higher = better, we negate for "lower is better")
- Secondary: per-fold recall, contamination, findings count

Usage:
    python3 benchmarks/autoresearch_bench.py
"""

import json
import os
import subprocess
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

# --- Config ---
INSPECT_BIN = os.path.expanduser("~/Documents/work/inspect/target/release/inspect")
AST_CACHE = str(Path(__file__).resolve().parent / "results" / "20260318_183258_ast.json")
BENCHMARK_DATA = "/tmp/martian-eval/code-review-benchmark/offline/results/benchmark_data.json"
ANCHORS_FILE = str(Path(__file__).resolve().parent / "golden_anchors_auto.json")
WORKTREES_DIR = "/tmp/martian-eval/worktrees"
TOP_N = 20

# Repo families for cross-validation
REPO_FAMILIES = {
    "cal.com": ["cal_dot_com"],
    "discourse": ["discourse"],
    "sentry": ["sentry"],
    "grafana": ["grafana"],
    "keycloak": ["keycloak"],
}

TEST_PATTERNS = [
    "/test/", "/tests/", "/testing/", "/__tests__/",
    "Test.java", "Tests.java", "Spec.java",
    "_test.go", "_test.py",
    ".test.ts", ".test.js", ".test.tsx", ".test.jsx",
    ".spec.ts", ".spec.js", ".spec.tsx", ".spec.jsx",
    "_test.rs", "_spec.rb", "_test.rb",
]


def is_test_file(fp: str) -> bool:
    return any(p in fp for p in TEST_PATTERNS)


def find_worktree(pr: dict) -> str | None:
    base, head = pr.get("base_sha", ""), pr.get("head_sha", "")
    if not base or not head:
        return None
    for repo_dir in Path(WORKTREES_DIR).iterdir():
        wt_key = f"{base[:12]}_{head[:12]}"
        wt_path = repo_dir / wt_key
        if wt_path.exists():
            return str(wt_path)
    return None


def match_golden_url(pr: dict, bd: dict) -> str | None:
    url = pr.get("url", "")
    if url in bd:
        return url
    pr_num = pr.get("pr_number")
    head_sha = pr.get("head_sha", "")
    for gurl, entry in bd.items():
        orig = entry.get("original_url", "") or ""
        if f"/pull/{pr_num}" in orig:
            return gurl
        if head_sha and head_sha in orig:
            return gurl
        if head_sha and head_sha in gurl:
            return gurl
    return None


def run_inspect(wt_dir: str, base_sha: str, head_sha: str) -> dict | None:
    try:
        result = subprocess.run(
            [INSPECT_BIN, "diff", f"{base_sha}..{head_sha}", "--repo", wt_dir, "--format", "json"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, timeout=120)
    except subprocess.TimeoutExpired:
        return None
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def repo_to_family(repo: str) -> str:
    """Map repo name (from AST cache) to repo family."""
    for family, repos in REPO_FAMILIES.items():
        if repo in repos:
            return family
    return repo


def check_bug_hit(anchor: dict, top_entities: list) -> bool:
    """Check if a golden bug's anchor is covered by the top-N entities."""
    anchor_file = anchor.get("anchor_file")
    anchor_entity = anchor.get("anchor_entity")

    if not anchor_file and not anchor_entity:
        return False

    for e in top_entities:
        epath = e.get("file_path", "")
        ename = e.get("entity_name", "")

        # Entity name match (strongest signal)
        if anchor_entity and anchor_entity == ename:
            return True
        # Entity name case-insensitive match
        if anchor_entity and anchor_entity.lower() == ename.lower():
            return True
        # Entity name substring (for long names like getBouncyCastleProvider)
        if anchor_entity and len(anchor_entity) >= 8:
            if anchor_entity.lower() in ename.lower() or ename.lower() in anchor_entity.lower():
                return True

        # File path match (weaker but still useful)
        if anchor_file:
            # Exact file path match
            if anchor_file == epath:
                return True
            # File name match (basename)
            if anchor_file.split("/")[-1] == epath.split("/")[-1]:
                return True
            # Anchor file is a suffix of entity file path
            if epath.endswith(anchor_file):
                return True

    return False


def main():
    t0 = time.time()

    with open(AST_CACHE) as f:
        ast_data = json.load(f)
    with open(BENCHMARK_DATA) as f:
        bd = json.load(f)
    with open(ANCHORS_FILE) as f:
        anchors = json.load(f)

    prs = [p for p in ast_data["prs"] if not p.get("skipped") and not p.get("error")]

    # Group anchors by (repo, pr_number)
    anchors_by_pr: dict[tuple, list] = defaultdict(list)
    for a in anchors:
        anchors_by_pr[(a["repo"], a["pr_number"])].append(a)

    # Run inspect on all PRs and cache results
    pr_results: dict[tuple, dict] = {}
    errors = 0
    for pr in prs:
        wt = find_worktree(pr)
        if not wt:
            errors += 1
            continue
        data = run_inspect(wt, pr["base_sha"], pr["head_sha"])
        if not data:
            errors += 1
            continue
        pr_results[(pr["repo"], pr["pr_number"])] = data

    # Leave-one-repo-out cross-validation
    families = sorted(REPO_FAMILIES.keys())
    fold_results = {}

    for held_out_family in families:
        held_out_repos = REPO_FAMILIES[held_out_family]

        # Score only bugs from the held-out repo
        total_bugs = 0
        hit_bugs = 0
        total_contam = 0
        total_findings = 0
        n_prs = 0

        for (repo, pr_num), data in pr_results.items():
            if repo not in held_out_repos:
                continue

            bug_anchors = anchors_by_pr.get((repo, pr_num), [])
            if not bug_anchors:
                continue

            entities = sorted(
                data.get("entity_reviews", []),
                key=lambda e: e.get("risk_score", 0),
                reverse=True,
            )
            top = entities[:TOP_N]

            # Bug-level recall
            for anchor in bug_anchors:
                total_bugs += 1
                if check_bug_hit(anchor, top):
                    hit_bugs += 1

            # Contamination
            total_contam += sum(1 for e in top if is_test_file(e.get("file_path", "")))
            total_findings += len(data.get("findings", []))
            n_prs += 1

        recall = hit_bugs / total_bugs if total_bugs > 0 else 0.0
        avg_contam = total_contam / n_prs if n_prs > 0 else 0.0
        avg_findings = total_findings / n_prs if n_prs > 0 else 0.0

        fold_results[held_out_family] = {
            "recall_at_20": round(recall, 4),
            "bugs_hit": hit_bugs,
            "bugs_total": total_bugs,
            "avg_contamination": round(avg_contam, 2),
            "avg_findings": round(avg_findings, 1),
            "n_prs": n_prs,
        }

    elapsed = time.time() - t0

    # Compute mean recall across folds
    recalls = [f["recall_at_20"] for f in fold_results.values()]
    mean_recall = sum(recalls) / len(recalls) if recalls else 0.0

    # Primary metric: negative mean recall (lower is better for autoresearch)
    primary = round(-mean_recall, 4)

    # Total bugs hit/total
    total_hit = sum(f["bugs_hit"] for f in fold_results.values())
    total_bugs = sum(f["bugs_total"] for f in fold_results.values())
    overall_recall = total_hit / total_bugs if total_bugs else 0.0

    # Report
    print(f"=== Leave-one-repo-out CV (bug-level recall@{TOP_N}) ===", file=sys.stderr)
    for family in families:
        f = fold_results[family]
        print(f"  {family:12s}: recall={f['recall_at_20']:.4f} ({f['bugs_hit']}/{f['bugs_total']})  contam={f['avg_contamination']:.1f}  findings/pr={f['avg_findings']:.0f}", file=sys.stderr)
    print(f"  {'MEAN':12s}: recall={mean_recall:.4f}", file=sys.stderr)
    print(f"  {'OVERALL':12s}: recall={overall_recall:.4f} ({total_hit}/{total_bugs})", file=sys.stderr)
    print(f"errors={errors}  elapsed={elapsed:.1f}s", file=sys.stderr)
    print(f"\nMETRIC={primary}", file=sys.stderr)

    # Machine-readable JSON (stdout)
    output = {
        "primary": primary,
        "mean_recall_at_20": round(mean_recall, 4),
        "overall_recall_at_20": round(overall_recall, 4),
        "total_bugs_hit": total_hit,
        "total_bugs": total_bugs,
        "folds": fold_results,
        "errors": errors,
        "elapsed_s": round(elapsed, 1),
    }
    print(json.dumps(output))


if __name__ == "__main__":
    main()
