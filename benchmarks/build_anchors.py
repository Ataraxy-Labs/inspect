#!/usr/bin/env python3
"""Auto-generate anchor file for 137 golden bugs by matching golden comments to inspect entities."""
import json
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

INSPECT_BIN = os.path.expanduser("~/Documents/work/inspect/target/release/inspect")
AST_CACHE = str(Path(__file__).resolve().parent / "results" / "20260318_183258_ast.json")
BENCHMARK_DATA = "/tmp/martian-eval/code-review-benchmark/offline/results/benchmark_data.json"
WORKTREES_DIR = "/tmp/martian-eval/worktrees"
OUTPUT = str(Path(__file__).resolve().parent / "golden_anchors_auto.json")


def find_worktree(pr):
    base, head = pr.get("base_sha", ""), pr.get("head_sha", "")
    if not base or not head:
        return None
    for repo_dir in Path(WORKTREES_DIR).iterdir():
        wt = repo_dir / f"{base[:12]}_{head[:12]}"
        if wt.exists():
            return str(wt)
    return None


def match_golden_url(pr, bd):
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


def run_inspect(wt, base, head):
    try:
        r = subprocess.run(
            [INSPECT_BIN, "diff", f"{base}..{head}", "--repo", wt, "--format", "json"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=120,
        )
        if r.returncode == 0:
            return json.loads(r.stdout)
    except Exception:
        pass
    return None


def extract_ids(text):
    """Extract likely code identifiers from golden comment text."""
    ids = set()
    # Backtick-quoted code
    for m in re.findall(r"`([^`]+)`", text):
        # Could be multi-word, split on non-identifier chars
        for part in re.findall(r"[A-Za-z_]\w+", m):
            if len(part) >= 3:
                ids.add(part)
    # CamelCase: ConditionalPasskeysEnabled, TopicUser
    ids.update(re.findall(r"\b([A-Z][a-z]+(?:[A-Z][a-z0-9]+)+)\b", text))
    # camelCase: getBouncyCastleProvider, isConditionalPasskeysEnabled
    ids.update(re.findall(r"\b([a-z]+(?:[A-Z][a-z0-9]+)+)\b", text))
    # Function/method calls: someName(
    ids.update(re.findall(r"\b([A-Za-z_]\w{2,})\s*\(", text))
    # Dotted access: obj.method
    ids.update(re.findall(r"\.([a-zA-Z_]\w{2,})", text))
    # ALL_CAPS constants
    ids.update(re.findall(r"\b([A-Z][A-Z0-9_]{2,})\b", text))

    noise = {
        "the", "for", "and", "but", "not", "with", "this", "that", "from",
        "will", "can", "may", "should", "could", "would", "instead", "which",
        "when", "where", "what", "than", "also", "method", "function", "class",
        "file", "code", "value", "check", "return", "returns", "null", "true",
        "false", "error", "string", "result", "parameter", "missing", "wrong",
        "calling", "called", "checking", "causing", "leading", "causes",
        "use", "using", "used", "new", "old", "same", "different", "bug",
        "issue", "problem", "because", "since", "after", "before", "during",
        "between", "into", "upon", "does", "still", "been", "being", "have",
        "has", "had", "are", "was", "were", "any", "all", "each", "every",
        "some", "other", "such", "like", "case", "type", "name", "set",
        "get", "run", "let", "var", "val", "key", "map", "list", "add",
        "Medium", "High", "Low", "Critical",
    }
    return {i for i in ids if len(i) >= 3 and i not in noise and i.lower() not in noise}


def main():
    with open(AST_CACHE) as f:
        ast_data = json.load(f)
    with open(BENCHMARK_DATA) as f:
        bd = json.load(f)

    prs = [p for p in ast_data["prs"] if not p.get("skipped") and not p.get("error")]

    all_anchors = []
    processed = 0

    for pr in prs:
        gurl = match_golden_url(pr, bd)
        if not gurl:
            continue
        wt = find_worktree(pr)
        if not wt:
            continue

        golden = bd[gurl].get("golden_comments", [])
        data = run_inspect(wt, pr["base_sha"], pr["head_sha"])
        if not data:
            print(f"  SKIP {pr['repo']} PR#{pr['pr_number']}: inspect failed", file=sys.stderr)
            continue

        entities = data.get("entity_reviews", [])
        processed += 1
        print(f"  {pr['repo']} PR#{pr['pr_number']}: {len(golden)} bugs, {len(entities)} entities", file=sys.stderr)

        for gi, gc in enumerate(golden):
            comment = gc["comment"]
            severity = gc["severity"]
            ids = extract_ids(comment)

            # Try to match identifiers to entities
            matched_entities = []
            matched_files = set()

            for eid in ids:
                eid_lower = eid.lower()
                for e in entities:
                    ename = e.get("entity_name", "")
                    epath = e.get("file_path", "")
                    fname = epath.split("/")[-1]

                    # Exact entity name match
                    if eid == ename or eid_lower == ename.lower():
                        matched_entities.append({"entity_name": ename, "file_path": epath})
                        matched_files.add(epath)
                    # Substring match on entity name (for long identifiers >= 8 chars)
                    elif len(eid) >= 8 and eid_lower in ename.lower():
                        matched_entities.append({"entity_name": ename, "file_path": epath})
                        matched_files.add(epath)
                    # File name stem match (e.g., "TopicUser" matches "topic_user.rb")
                    elif len(eid) >= 6 and eid_lower in fname.lower().replace(".", "").replace("_", ""):
                        matched_files.add(epath)

            # Deduplicate matched entities
            seen = set()
            deduped = []
            for m in matched_entities:
                key = (m["entity_name"], m["file_path"])
                if key not in seen:
                    seen.add(key)
                    deduped.append(m)

            anchor = {
                "pr_url": gurl,
                "repo": pr["repo"],
                "pr_number": pr["pr_number"],
                "bug_index": gi,
                "severity": severity,
                "comment": comment[:300],
                "extracted_ids": sorted(ids)[:20],
                "matched_entities": deduped[:5],
                "matched_files": sorted(matched_files)[:5],
                "anchor_file": deduped[0]["file_path"] if deduped else (sorted(matched_files)[0] if matched_files else None),
                "anchor_entity": deduped[0]["entity_name"] if deduped else None,
                "confidence": "high" if deduped else ("medium" if matched_files else "low"),
            }
            all_anchors.append(anchor)

    # Stats
    conf = Counter(a["confidence"] for a in all_anchors)
    sev = Counter(a["severity"] for a in all_anchors)
    print(f"\nTotal anchors: {len(all_anchors)} (from {processed} PRs)", file=sys.stderr)
    print(f"  high (entity match):  {conf['high']}", file=sys.stderr)
    print(f"  medium (file match):  {conf['medium']}", file=sys.stderr)
    print(f"  low (no match):       {conf['low']}", file=sys.stderr)
    print(f"  by severity: {dict(sev)}", file=sys.stderr)

    # Show the low-confidence ones for manual review
    low = [a for a in all_anchors if a["confidence"] == "low"]
    if low:
        print(f"\n--- LOW CONFIDENCE (need manual anchoring) ---", file=sys.stderr)
        for a in low:
            print(f"  [{a['severity']}] {a['repo']} PR#{a['pr_number']} bug#{a['bug_index']}: {a['comment'][:120]}", file=sys.stderr)
            print(f"    extracted_ids: {a['extracted_ids'][:10]}", file=sys.stderr)

    with open(OUTPUT, "w") as f:
        json.dump(all_anchors, f, indent=2)
    print(f"\nSaved to {OUTPUT}", file=sys.stderr)


if __name__ == "__main__":
    main()
