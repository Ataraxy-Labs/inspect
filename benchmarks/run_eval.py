#!/usr/bin/env python3
"""Run inspect on all Greptile benchmark PRs and produce eval CSV."""

import json
import subprocess
import csv
import os
import sys

INSPECT = os.path.expanduser("~/inspect/target/release/inspect")
REPOS_DIR = "/tmp/inspect-eval/repos"
GOLDEN_DIR = "/tmp/inspect-eval"

# repo name in golden comments -> github org/repo fork name -> PR numbers
REPOS = {
    "keycloak": {
        "fork": "ai-code-review-evaluation/keycloak-greptile",
        "golden_file": "keycloak.json",
    },
    "discourse": {
        "fork": "ai-code-review-evaluation/discourse-greptile",
        "golden_file": "discourse.json",
    },
    "grafana": {
        "fork": "ai-code-review-evaluation/grafana-greptile",
        "golden_file": "grafana.json",
    },
    "sentry": {
        "fork": "ai-code-review-evaluation/sentry-greptile",
        "golden_file": "sentry.json",
    },
    "cal_dot_com": {
        "fork": "ai-code-review-evaluation/cal.com-greptile",
        "golden_file": "cal_dot_com.json",
    },
}


def clone_repo(fork, name):
    """Shallow clone a repo if not already cloned."""
    repo_dir = os.path.join(REPOS_DIR, name)
    if os.path.exists(repo_dir):
        print(f"  {name} already cloned", file=sys.stderr)
        return repo_dir
    print(f"  cloning {fork}...", file=sys.stderr)
    subprocess.run(
        ["gh", "repo", "clone", fork, repo_dir, "--", "--depth=100"],
        capture_output=True,
    )
    # Fetch all PR refs
    subprocess.run(
        ["git", "fetch", "origin", "refs/pull/*/head:refs/remotes/origin/pr-head/*"],
        cwd=repo_dir,
        capture_output=True,
    )
    return repo_dir


def get_prs_for_repo(fork):
    """Get all PR numbers and titles from the fork."""
    result = subprocess.run(
        ["gh", "api", f"repos/{fork}/pulls?state=all&per_page=50",
         "--jq", '.[] | "\(.number)\t\(.title)\t\(.head.sha)"'],
        capture_output=True, text=True,
    )
    prs = []
    for line in result.stdout.strip().split("\n"):
        if not line:
            continue
        parts = line.split("\t", 2)
        if len(parts) == 3:
            prs.append({"number": int(parts[0]), "title": parts[1], "head_sha": parts[2]})
    return prs


def match_pr_to_golden(pr_title, golden_prs):
    """Match a PR title to golden comments entry."""
    for gpr in golden_prs:
        # Check if titles match (golden titles may be truncated or slightly different)
        gt = gpr["pr_title"].strip()
        pt = pr_title.strip()
        if gt == pt or gt.startswith(pt[:40]) or pt.startswith(gt[:40]):
            return gpr
    return None


def run_inspect(repo_dir, head_sha):
    """Run inspect diff on a commit and return JSON result."""
    # Make sure we have the commit
    subprocess.run(
        ["git", "fetch", "--depth=50", "origin", head_sha],
        cwd=repo_dir, capture_output=True,
    )
    result = subprocess.run(
        [INSPECT, "diff", head_sha, "--repo", repo_dir, "--format", "json"],
        capture_output=True, text=True, timeout=120,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def main():
    os.makedirs(REPOS_DIR, exist_ok=True)

    rows = []

    for repo_name, config in REPOS.items():
        print(f"\n=== {repo_name} ===", file=sys.stderr)

        # Load golden comments
        golden_path = os.path.join(GOLDEN_DIR, config["golden_file"])
        with open(golden_path) as f:
            golden_prs = json.load(f)

        # Clone repo
        repo_dir = clone_repo(config["fork"], repo_name)

        # Get PRs from fork
        prs = get_prs_for_repo(config["fork"])
        print(f"  found {len(prs)} PRs", file=sys.stderr)

        for pr in prs:
            # Skip dependabot/non-benchmark PRs
            golden = match_pr_to_golden(pr["title"], golden_prs)
            if golden is None:
                continue

            print(f"  PR #{pr['number']}: {pr['title'][:60]}...", file=sys.stderr)

            # Run inspect
            inspect_result = run_inspect(repo_dir, pr["head_sha"])
            if inspect_result is None:
                print(f"    inspect failed, skipping", file=sys.stderr)
                # Still add golden comments with no inspect data
                for comment in golden["comments"]:
                    rows.append({
                        "repo": repo_name,
                        "pr_number": pr["number"],
                        "pr_title": golden["pr_title"],
                        "golden_comment": comment["comment"],
                        "golden_severity": comment["severity"],
                        "inspect_entity_count": 0,
                        "inspect_hc_entities": "",
                        "inspect_hc_entity_files": "",
                        "inspect_hc_entity_content_snippet": "",
                        "inspect_all_entities_summary": "",
                    })
                continue

            # Get High/Critical entities
            entities = inspect_result.get("entity_reviews", [])
            hc_entities = [e for e in entities if e["risk_level"] in ("High", "Critical")]
            medium_entities = [e for e in entities if e["risk_level"] == "Medium"]

            # Build entity summaries
            hc_summary = []
            for e in hc_entities:
                content = e.get("after_content") or e.get("before_content") or ""
                # Truncate content to first 500 chars for CSV
                snippet = content[:500].replace("\n", "\\n") if content else ""
                hc_summary.append(f"{e['entity_type']}::{e['entity_name']} ({e['file_path']}) [{e['risk_level']}, score={e['risk_score']:.2f}]")

            all_summary = []
            for e in entities:
                all_summary.append(f"{e['entity_name']} ({e['risk_level']})")

            # For each golden comment in this PR
            for comment in golden["comments"]:
                # Build HC entity content for LLM matching
                hc_content_parts = []
                for e in hc_entities:
                    content = e.get("after_content") or e.get("before_content") or ""
                    snippet = content[:800]
                    hc_content_parts.append(
                        f"[{e['risk_level']}] {e['entity_type']} {e['entity_name']} in {e['file_path']}:\n{snippet}"
                    )

                medium_content_parts = []
                for e in medium_entities:
                    content = e.get("after_content") or e.get("before_content") or ""
                    snippet = content[:500]
                    medium_content_parts.append(
                        f"[Medium] {e['entity_type']} {e['entity_name']} in {e['file_path']}:\n{snippet}"
                    )

                rows.append({
                    "repo": repo_name,
                    "pr_number": pr["number"],
                    "pr_title": golden["pr_title"],
                    "golden_comment": comment["comment"],
                    "golden_severity": comment["severity"],
                    "inspect_entity_count": len(entities),
                    "inspect_hc_count": len(hc_entities),
                    "inspect_medium_count": len(medium_entities),
                    "inspect_hc_entities": " | ".join(hc_summary),
                    "inspect_hc_entity_content": "\n---\n".join(hc_content_parts),
                    "inspect_medium_entity_content": "\n---\n".join(medium_content_parts),
                    "inspect_all_entities_summary": ", ".join(all_summary),
                })

    # Write CSV
    output_path = "/tmp/inspect-eval/eval.csv"
    if rows:
        fieldnames = rows[0].keys()
        with open(output_path, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=fieldnames)
            writer.writeheader()
            writer.writerows(rows)
        print(f"\nWrote {len(rows)} rows to {output_path}", file=sys.stderr)
    else:
        print("No rows generated!", file=sys.stderr)


if __name__ == "__main__":
    main()
