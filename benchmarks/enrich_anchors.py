#!/usr/bin/env python3
"""Enrich low-confidence anchors by matching golden comment keywords to changed files and diff content."""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

WORKTREES_DIR = "/tmp/martian-eval/worktrees"
AST_CACHE = str(Path(__file__).resolve().parent / "results" / "20260318_183258_ast.json")
ANCHORS = str(Path(__file__).resolve().parent / "golden_anchors_auto.json")


def find_worktree(pr_data):
    base, head = pr_data.get("base_sha", ""), pr_data.get("head_sha", "")
    if not base or not head:
        return None
    for repo_dir in Path(WORKTREES_DIR).iterdir():
        wt = repo_dir / f"{base[:12]}_{head[:12]}"
        if wt.exists():
            return str(wt)
    return None


def get_changed_files(wt, base, head):
    """Get list of files changed in the diff."""
    try:
        r = subprocess.run(
            ["git", "diff", "--name-only", base, head],
            cwd=wt, capture_output=True, text=True, timeout=30,
        )
        if r.returncode == 0:
            return [f for f in r.stdout.strip().split("\n") if f]
    except Exception:
        pass
    return []


def get_diff_text(wt, base, head):
    """Get the full diff text."""
    try:
        r = subprocess.run(
            ["git", "diff", "-U3", base, head],
            cwd=wt, capture_output=True, text=True, timeout=60,
        )
        if r.returncode == 0:
            return r.stdout
    except Exception:
        pass
    return ""


def extract_keywords(comment):
    """Extract meaningful keywords from a golden comment for file matching."""
    words = set()
    # Extract quoted strings
    words.update(re.findall(r"'([^']+)'", comment))
    words.update(re.findall(r'"([^"]+)"', comment))
    # CamelCase and camelCase
    words.update(re.findall(r'\b([A-Z][a-z]+(?:[A-Z][a-z0-9]+)+)\b', comment))
    words.update(re.findall(r'\b([a-z]+(?:[A-Z][a-z0-9]+)+)\b', comment))
    # snake_case identifiers
    words.update(re.findall(r'\b([a-z][a-z0-9]*(?:_[a-z0-9]+)+)\b', comment))
    # Function calls
    words.update(re.findall(r'\b([A-Za-z_]\w{3,})\s*\(', comment))
    # Dotted paths
    words.update(re.findall(r'\.([a-zA-Z_]\w{3,})', comment))
    return {w for w in words if len(w) >= 4}


def score_file_match(filepath, keywords, diff_chunk):
    """Score how well a file matches the golden comment keywords."""
    fname = filepath.split("/")[-1].lower()
    score = 0
    matched_kw = []
    for kw in keywords:
        kw_lower = kw.lower()
        # File name contains keyword
        if kw_lower in fname:
            score += 3
            matched_kw.append(kw)
        # Keyword appears in the diff chunk for this file
        elif diff_chunk and kw_lower in diff_chunk.lower():
            score += 1
            matched_kw.append(kw)
    return score, matched_kw


def main():
    with open(AST_CACHE) as f:
        ast_data = json.load(f)
    with open(ANCHORS) as f:
        anchors = json.load(f)

    # Build PR lookup
    pr_lookup = {}
    for pr in ast_data["prs"]:
        if not pr.get("skipped") and not pr.get("error"):
            key = (pr["repo"], pr["pr_number"])
            pr_lookup[key] = pr

    enriched = 0
    for anchor in anchors:
        if anchor["confidence"] != "low":
            continue

        key = (anchor["repo"], anchor["pr_number"])
        pr = pr_lookup.get(key)
        if not pr:
            continue

        wt = find_worktree(pr)
        if not wt:
            continue

        base, head = pr["base_sha"], pr["head_sha"]
        changed_files = get_changed_files(wt, base, head)
        if not changed_files:
            continue

        keywords = extract_keywords(anchor["comment"])
        if not keywords:
            # Fall back: if comment mentions specific concepts, try to match by file extension
            # For bugs with no extractable keywords, anchor to the most-changed file
            diff = get_diff_text(wt, base, head)
            # Find the file with most changes
            file_changes = {}
            current_file = None
            for line in diff.split("\n"):
                if line.startswith("+++ b/"):
                    current_file = line[6:]
                elif current_file and (line.startswith("+") or line.startswith("-")):
                    file_changes[current_file] = file_changes.get(current_file, 0) + 1

            if file_changes:
                # Pick the most-changed non-test file
                prod_files = {f: c for f, c in file_changes.items()
                              if not any(p in f for p in ["/test/", "/tests/", "test_", "_test.", ".test.", ".spec."])}
                if prod_files:
                    best = max(prod_files, key=prod_files.get)
                    anchor["anchor_file"] = best
                    anchor["confidence"] = "medium"
                    anchor["anchor_method"] = "most-changed-file"
                    enriched += 1
            continue

        # Get diff per file
        diff = get_diff_text(wt, base, head)
        file_diffs = {}
        current_file = None
        current_chunk = []
        for line in diff.split("\n"):
            if line.startswith("+++ b/"):
                if current_file:
                    file_diffs[current_file] = "\n".join(current_chunk)
                current_file = line[6:]
                current_chunk = []
            elif current_file:
                current_chunk.append(line)
        if current_file:
            file_diffs[current_file] = "\n".join(current_chunk)

        # Score each changed file
        best_score = 0
        best_file = None
        best_kw = []
        for f in changed_files:
            chunk = file_diffs.get(f, "")
            score, matched = score_file_match(f, keywords, chunk)
            if score > best_score:
                best_score = score
                best_file = f
                best_kw = matched

        if best_file and best_score >= 1:
            anchor["anchor_file"] = best_file
            anchor["confidence"] = "medium" if best_score >= 2 else "low-enriched"
            anchor["anchor_method"] = "keyword-file-match"
            anchor["matched_keywords"] = best_kw[:5]
            enriched += 1

    # Save enriched anchors
    with open(ANCHORS, "w") as f:
        json.dump(anchors, f, indent=2)

    # Stats
    from collections import Counter
    conf = Counter(a["confidence"] for a in anchors)
    print(f"Enriched {enriched} anchors", file=sys.stderr)
    print(f"  high:          {conf['high']}", file=sys.stderr)
    print(f"  medium:        {conf['medium']}", file=sys.stderr)
    print(f"  low-enriched:  {conf.get('low-enriched', 0)}", file=sys.stderr)
    print(f"  low:           {conf['low']}", file=sys.stderr)
    print(f"Coverage: {(conf['high']+conf['medium']+conf.get('low-enriched',0))/len(anchors)*100:.0f}%", file=sys.stderr)

    # Show remaining lows
    remaining_low = [a for a in anchors if a["confidence"] == "low"]
    if remaining_low:
        print(f"\n--- STILL LOW ({len(remaining_low)}) ---", file=sys.stderr)
        for a in remaining_low:
            print(f"  [{a['severity']}] {a['repo']} PR#{a['pr_number']} bug#{a['bug_index']}: {a['comment'][:100]}", file=sys.stderr)


if __name__ == "__main__":
    main()
