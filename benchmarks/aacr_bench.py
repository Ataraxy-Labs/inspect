#!/usr/bin/env python3
"""AACR-Bench comparison: inspect vs Greptile vs CodeRabbit.

Runs all three AI code review tools on the same dataset (AACR-Bench: 196 PRs,
2145 golden comments, 10 languages) and computes recall/precision/F1 per tool.

Usage:
    python aacr_bench.py --tools inspect,greptile,coderabbit --limit 20 --output results.csv
    python aacr_bench.py --tools inspect --limit 5          # quick validation
    python aacr_bench.py --tools greptile --limit 5         # test Greptile API
"""

import argparse
import csv
import json
import os
import re
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

# --- Config ---

INSPECT_BIN = os.path.expanduser("~/inspect/target/release/inspect")
CACHE_DIR = "/tmp/aacr-bench"
REPOS_DIR = f"{CACHE_DIR}/repos"
DATASET_URL = "https://raw.githubusercontent.com/alibaba/aacr-bench/main/dataset/positive_samples.json"
DATASET_PATH = f"{CACHE_DIR}/positive_samples.json"

ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY", "")
GREPTILE_API_KEY = os.environ.get("GREPTILE_API_KEY", "")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN", "")

LINE_TOLERANCE = 10  # lines of tolerance for matching


# --- Dataset ---

def download_dataset():
    """Download AACR-Bench positive_samples.json if not cached."""
    os.makedirs(CACHE_DIR, exist_ok=True)
    if os.path.exists(DATASET_PATH):
        return
    print("Downloading AACR-Bench dataset...", file=sys.stderr)
    urllib.request.urlretrieve(DATASET_URL, DATASET_PATH)
    print(f"  saved to {DATASET_PATH}", file=sys.stderr)


def load_dataset(limit=None, diverse=False):
    """Load and return list of PR dicts from the dataset.

    If diverse=True, round-robin sample across repos for better coverage.
    """
    with open(DATASET_PATH) as f:
        prs = json.load(f)

    if diverse and limit:
        # Group by repo, then round-robin pick
        by_repo = {}
        for pr in prs:
            owner, repo, _ = parse_pr_url(pr.get("githubPrUrl", ""))
            key = f"{owner}/{repo}"
            by_repo.setdefault(key, []).append(pr)

        selected = []
        repo_keys = list(by_repo.keys())
        idx = 0
        while len(selected) < limit and repo_keys:
            key = repo_keys[idx % len(repo_keys)]
            if by_repo[key]:
                selected.append(by_repo[key].pop(0))
            else:
                repo_keys.remove(key)
                if not repo_keys:
                    break
                continue
            idx += 1
        return selected

    if limit:
        prs = prs[:limit]
    return prs


def parse_pr_url(url):
    """Extract owner, repo from githubPrUrl like https://github.com/owner/repo/pull/123."""
    m = re.match(r"https://github\.com/([^/]+)/([^/]+)/pull/(\d+)", url)
    if not m:
        return None, None, None
    return m.group(1), m.group(2), int(m.group(3))


# --- Repo cloning ---

def ensure_repo(owner, repo):
    """Shallow clone a repo if not cached. Returns repo dir path."""
    os.makedirs(REPOS_DIR, exist_ok=True)
    repo_dir = f"{REPOS_DIR}/{owner}__{repo}"
    if os.path.exists(repo_dir):
        return repo_dir
    print(f"  cloning {owner}/{repo}...", file=sys.stderr)
    clone_url = f"https://github.com/{owner}/{repo}.git"
    subprocess.run(
        ["git", "clone", "--depth=1", "--no-checkout", clone_url, repo_dir],
        capture_output=True, timeout=300,
    )
    return repo_dir


def fetch_commits(repo_dir, *shas):
    """Fetch specific commits and make them available for diff."""
    for sha in shas:
        subprocess.run(
            ["git", "fetch", "--depth=50", "origin", sha],
            cwd=repo_dir, capture_output=True, timeout=120,
        )
    # Force checkout the head commit so inspect can ls-files
    if shas:
        subprocess.run(
            ["git", "checkout", "-f", shas[-1]],
            cwd=repo_dir, capture_output=True, timeout=60,
        )
        subprocess.run(
            ["git", "clean", "-fd"],
            cwd=repo_dir, capture_output=True, timeout=30,
        )


def get_diff_text(repo_dir, base_sha, head_sha):
    """Get unified diff text between two commits."""
    result = subprocess.run(
        ["git", "diff", base_sha, head_sha],
        cwd=repo_dir, capture_output=True, text=True, timeout=60,
    )
    return result.stdout if result.returncode == 0 else ""


# --- Tool runners ---

def run_inspect(repo_dir, base_sha, head_sha):
    """Run inspect diff, filter HC entities, return findings list.

    Each finding: {file, line, description, entity_name, risk_level}
    """
    diff_ref = f"{base_sha}..{head_sha}"
    result = subprocess.run(
        [INSPECT_BIN, "diff", diff_ref, "--repo", repo_dir, "--format", "json"],
        capture_output=True, text=True, timeout=180,
    )
    if result.returncode != 0:
        return []

    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        return []

    findings = []
    for e in data.get("entity_reviews", []):
        if e.get("risk_level") not in ("High", "Critical"):
            continue

        content = e.get("after_content") or e.get("before_content") or ""
        findings.append({
            "file": e.get("file_path", ""),
            "line": e.get("start_line", 0),
            "end_line": e.get("end_line", 0),
            "description": f"[{e['risk_level']}] {e['entity_type']} {e['entity_name']}: "
                           f"score={e['risk_score']:.2f}, blast_radius={e.get('blast_radius', 0)}, "
                           f"dependents={e.get('dependent_count', 0)}",
            "entity_name": e.get("entity_name", ""),
            "risk_level": e.get("risk_level", ""),
        })

    return findings


def _get_default_branch(owner, repo):
    """Get the default branch for a GitHub repo."""
    result = subprocess.run(
        ["gh", "api", f"repos/{owner}/{repo}", "--jq", ".default_branch"],
        capture_output=True, text=True, timeout=15,
    )
    branch = result.stdout.strip()
    return branch if branch else "main"


# Cache default branches to avoid repeated API calls
_branch_cache = {}


def run_greptile(owner, repo, base_sha, head_sha, diff_text):
    """Call Greptile API with PR diff, return findings list."""
    if not GREPTILE_API_KEY or not GITHUB_TOKEN:
        print("    skipping Greptile (no API key)", file=sys.stderr)
        return []

    # Get the correct default branch
    repo_key = f"{owner}/{repo}"
    if repo_key not in _branch_cache:
        _branch_cache[repo_key] = _get_default_branch(owner, repo)
    branch = _branch_cache[repo_key]

    # Truncate diff to ~100k chars to stay within API limits
    truncated_diff = diff_text[:100_000]

    prompt = (
        "Review this code diff for bugs, security issues, performance problems, "
        "and maintainability concerns. For each issue found, respond in JSON format: "
        '[{"file": "path/to/file", "line": 42, "description": "issue description"}]. '
        "Only return the JSON array, no other text."
    )

    payload = json.dumps({
        "messages": [{"role": "user", "content": f"{prompt}\n\n```diff\n{truncated_diff}\n```"}],
        "repositories": [{"remote": "github", "repository": repo_key, "branch": branch}],
    }).encode()

    req = urllib.request.Request(
        "https://api.greptile.com/v2/query",
        data=payload,
        headers={
            "Authorization": f"Bearer {GREPTILE_API_KEY}",
            "X-GitHub-Token": GITHUB_TOKEN,
            "Content-Type": "application/json",
        },
    )

    try:
        with urllib.request.urlopen(req, timeout=180) as resp:
            body = json.loads(resp.read())
    except urllib.request.HTTPError as e:
        err_body = e.read().decode()[:300]
        if "has not been submitted" in err_body:
            # Auto-submit and retry once
            print(f" not indexed, submitting...", file=sys.stderr, end="", flush=True)
            _submit_repo(repo_key, branch)
            _wait_for_indexing(repo_key, branch)
            print(f" retrying...", file=sys.stderr, end="", flush=True)
            try:
                req2 = urllib.request.Request(
                    "https://api.greptile.com/v2/query",
                    data=payload,
                    headers={
                        "Authorization": f"Bearer {GREPTILE_API_KEY}",
                        "X-GitHub-Token": GITHUB_TOKEN,
                        "Content-Type": "application/json",
                    },
                )
                with urllib.request.urlopen(req2, timeout=180) as resp2:
                    body = json.loads(resp2.read())
            except Exception as e2:
                print(f" retry failed: {e2}", file=sys.stderr)
                return []
        else:
            print(f"    Greptile API error: {e} - {err_body}", file=sys.stderr)
            return []
    except Exception as e:
        print(f"    Greptile API error: {e}", file=sys.stderr)
        return []

    # Parse response: Greptile returns {message: "...", sources: [...]}
    message = body.get("message", "")
    findings = _parse_json_findings(message)
    return findings


def _submit_repo(repo_key, branch):
    """Submit a repo to Greptile for indexing."""
    payload = json.dumps({
        "remote": "github",
        "repository": repo_key,
        "branch": branch,
    }).encode()
    req = urllib.request.Request(
        "https://api.greptile.com/v2/repositories",
        data=payload,
        headers={
            "Authorization": f"Bearer {GREPTILE_API_KEY}",
            "X-GitHub-Token": GITHUB_TOKEN,
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            resp.read()
    except Exception:
        pass


def _wait_for_indexing(repo_key, branch, max_wait=600):
    """Poll until repo is indexed (up to max_wait seconds)."""
    encoded = repo_key.replace("/", "%2F")
    url = f"https://api.greptile.com/v2/repositories/github%3A{branch}%3A{encoded}"
    start = time.time()
    while time.time() - start < max_wait:
        time.sleep(30)
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {GREPTILE_API_KEY}",
            "X-GitHub-Token": GITHUB_TOKEN,
        })
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                body = json.loads(resp.read())
                status = body.get("status", "")
                if status == "COMPLETED":
                    return True
                print(f" {status}...", file=sys.stderr, end="", flush=True)
        except Exception:
            pass
    return False


def run_coderabbit(repo_dir, base_sha, head_sha):
    """Run CodeRabbit CLI, return findings list."""
    # CodeRabbit needs a working tree, not a bare repo
    worktree_dir = f"{repo_dir}__worktree"
    if not os.path.exists(worktree_dir):
        subprocess.run(
            ["git", "clone", "--shared", repo_dir, worktree_dir],
            capture_output=True, timeout=120,
        )

    # Checkout head commit
    subprocess.run(
        ["git", "checkout", head_sha],
        cwd=worktree_dir, capture_output=True, timeout=30,
    )

    result = subprocess.run(
        ["coderabbit", "--plain", "--type", "committed", "--base", base_sha],
        cwd=worktree_dir, capture_output=True, text=True, timeout=300,
    )

    if result.returncode != 0:
        print(f"    CodeRabbit failed: {result.stderr[:200]}", file=sys.stderr)
        return []

    return _parse_coderabbit_output(result.stdout)


def _parse_json_findings(text):
    """Extract JSON array of findings from LLM/API text output."""
    # Try to find JSON array in the response
    findings = []
    json_match = re.search(r'\[.*\]', text, re.DOTALL)
    if json_match:
        try:
            items = json.loads(json_match.group())
            for item in items:
                if isinstance(item, dict):
                    findings.append({
                        "file": item.get("file", item.get("path", "")),
                        "line": item.get("line", item.get("from_line", 0)),
                        "end_line": item.get("end_line", item.get("to_line", 0)),
                        "description": item.get("description", item.get("message", "")),
                    })
        except json.JSONDecodeError:
            pass
    return findings


def _parse_coderabbit_output(text):
    """Parse CodeRabbit plaintext output into findings list.

    CodeRabbit --plain output format:
      path/to/file.py (line 42-50):
      [category] description of issue...
    """
    findings = []
    current_file = None
    current_line = 0

    for line in text.split("\n"):
        # Match file header: "path/to/file.ext (line N-M):"
        file_match = re.match(r'^(\S+)\s+\(line\s+(\d+)(?:-(\d+))?\):', line)
        if file_match:
            current_file = file_match.group(1)
            current_line = int(file_match.group(2))
            continue

        # Match category + description
        desc_match = re.match(r'^\[(\w+)\]\s+(.+)', line.strip())
        if desc_match and current_file:
            findings.append({
                "file": current_file,
                "line": current_line,
                "end_line": current_line,
                "description": f"[{desc_match.group(1)}] {desc_match.group(2)}",
            })
            continue

        # Also try plain description lines after file header
        if current_file and line.strip() and not line.startswith(" "):
            # Reset file context on non-indented non-matching lines
            current_file = None

    return findings


# --- Judge ---

def extract_identifiers(text):
    """Extract likely code identifiers from text (reused from heuristic_judge.py)."""
    idents = set()
    patterns = [
        r'\b[A-Z][a-zA-Z0-9]+(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*\b',
        r'\b[a-z][a-zA-Z0-9]*[A-Z][a-zA-Z0-9]*\b',
        r'\b[a-z_][a-z0-9_]+\b',
        r'`([^`]+)`',
        r"'([^']+)'",
    ]
    stopwords = {
        'the', 'this', 'that', 'with', 'from', 'will', 'can', 'may',
        'should', 'could', 'would', 'not', 'but', 'and', 'for', 'has',
        'have', 'been', 'being', 'are', 'was', 'were', 'because',
        'when', 'then', 'than', 'also', 'which', 'where', 'here',
        'there', 'into', 'only', 'instead', 'without', 'between',
        'during', 'using', 'after', 'before', 'other', 'method',
        'function', 'class', 'variable', 'parameter', 'returns',
        'called', 'calling', 'always', 'never', 'still', 'just',
        'like', 'some', 'any', 'all', 'each', 'both', 'same',
        'different', 'new', 'old', 'first', 'second', 'last',
        'however', 'since', 'further', 'consider', 'original',
        'issue', 'bug', 'error', 'test', 'check', 'code', 'value',
        'data', 'type', 'name', 'key', 'result', 'string', 'list',
        'null', 'none', 'true', 'false', 'set', 'get', 'add',
        'remove', 'create', 'update', 'delete', 'find', 'make',
        'call', 'run', 'use', 'missing', 'incorrect', 'wrong',
        'logic', 'potential', 'existing', 'current', 'causes',
        'causing', 'rather', 'whether', 'these', 'those', 'such',
        'what', 'does', 'how', 'its', 'might', 'already', 'directly',
        'immediately', 'properly', 'correctly', 'actually', 'specific',
        'possible', 'required', 'expected', 'necessary', 'even',
        'line', 'file', 'path', 'note', 'category', 'context',
    }
    for pat in patterns:
        for m in re.finditer(pat, text):
            ident = m.group(1) if m.lastindex else m.group(0)
            if len(ident) >= 3 and ident.lower() not in stopwords:
                idents.add(ident)
    return idents


def judge_finding(golden, findings):
    """Judge whether any tool finding matches a golden comment.

    Returns: ("match", reason) | ("partial", reason) | ("miss", reason)
    """
    g_file = golden.get("path", "")
    g_from = golden.get("from_line", 0) or 0
    g_to = golden.get("to_line", 0) or 0
    g_note = golden.get("note", "")
    golden_idents = extract_identifiers(g_note)

    for f in findings:
        f_file = f.get("file", "")
        f_line = f.get("line", 0) or 0
        f_end = f.get("end_line", 0) or f_line
        f_desc = f.get("description", "")

        # Check file match (normalize paths)
        file_match = _paths_match(g_file, f_file)
        if not file_match:
            continue

        # Check line overlap (with tolerance)
        line_match = _lines_overlap(g_from, g_to, f_line, f_end, LINE_TOLERANCE)

        # Check semantic overlap via identifiers
        finding_idents = extract_identifiers(f_desc)
        entity_name = f.get("entity_name", "")
        if entity_name:
            finding_idents.add(entity_name)
            finding_idents.add(entity_name.lower())

        ident_overlap = golden_idents & {i.lower() for i in finding_idents}

        if file_match and line_match:
            return "match", f"file+line match: {f_file}:{f_line}"
        if file_match and ident_overlap:
            return "match", f"file+ident match: {f_file}, idents={ident_overlap}"
        if file_match:
            return "partial", f"file match only: {f_file}"

    return "miss", f"no findings match {g_file}:{g_from}-{g_to}"


def _paths_match(golden_path, finding_path):
    """Check if two file paths refer to the same file (suffix match)."""
    if not golden_path or not finding_path:
        return False
    # Normalize
    gp = golden_path.replace("\\", "/").lstrip("/")
    fp = finding_path.replace("\\", "/").lstrip("/")
    return gp == fp or gp.endswith(fp) or fp.endswith(gp)


def _lines_overlap(g_from, g_to, f_from, f_to, tolerance):
    """Check if line ranges overlap within tolerance."""
    if not g_from and not g_to:
        return False
    if not f_from and not f_to:
        return False
    g_start = max(1, (g_from or g_to) - tolerance)
    g_end = (g_to or g_from) + tolerance
    f_start = f_from or f_to
    f_end = f_to or f_from
    return g_start <= f_end and f_start <= g_end


# --- Main ---

def run_benchmark(tools, limit, output_path, diverse=False):
    """Run the full benchmark pipeline."""
    download_dataset()
    prs = load_dataset(limit, diverse=diverse)

    total_golden = sum(len(pr.get("comments", [])) for pr in prs)
    print(f"Loaded {len(prs)} PRs with {total_golden} golden comments", file=sys.stderr)
    print(f"Tools: {', '.join(tools)}", file=sys.stderr)
    print(f"Output: {output_path}", file=sys.stderr)
    print("", file=sys.stderr)

    rows = []
    tool_findings_all = {t: [] for t in tools}  # for precision

    for pr_idx, pr in enumerate(prs):
        url = pr.get("githubPrUrl", "")
        owner, repo, pr_num = parse_pr_url(url)
        if not owner:
            print(f"  [{pr_idx+1}/{len(prs)}] skipping invalid URL: {url}", file=sys.stderr)
            continue

        source_sha = pr.get("source_commit", "")
        target_sha = pr.get("target_commit", "")
        lang = pr.get("project_main_language", "")
        comments = pr.get("comments", [])

        print(f"  [{pr_idx+1}/{len(prs)}] {owner}/{repo} PR#{pr_num} ({lang}, {len(comments)} comments)", file=sys.stderr)

        # Clone and fetch
        try:
            repo_dir = ensure_repo(owner, repo)
            fetch_commits(repo_dir, source_sha, target_sha)
        except Exception as e:
            print(f"    clone/fetch failed: {e}", file=sys.stderr)
            for c in comments:
                row = _make_row(pr, c, owner, repo, pr_num)
                for t in tools:
                    row[f"{t}_verdict"] = "error"
                    row[f"{t}_reason"] = str(e)
                    row[f"{t}_finding_count"] = 0
                rows.append(row)
            continue

        # Run each tool
        findings_by_tool = {}
        diff_text = None

        for tool in tools:
            print(f"    running {tool}...", file=sys.stderr, end="", flush=True)
            t0 = time.time()
            try:
                if tool == "inspect":
                    findings = run_inspect(repo_dir, source_sha, target_sha)
                elif tool == "greptile":
                    if diff_text is None:
                        diff_text = get_diff_text(repo_dir, source_sha, target_sha)
                    findings = run_greptile(owner, repo, source_sha, target_sha, diff_text)
                elif tool == "coderabbit":
                    findings = run_coderabbit(repo_dir, source_sha, target_sha)
                else:
                    findings = []
            except Exception as e:
                print(f" error: {e}", file=sys.stderr)
                findings = []

            elapsed = time.time() - t0
            findings_by_tool[tool] = findings
            tool_findings_all[tool].extend(findings)
            print(f" {len(findings)} findings ({elapsed:.1f}s)", file=sys.stderr)

        # Judge each golden comment against each tool's findings
        for c in comments:
            row = _make_row(pr, c, owner, repo, pr_num)
            for tool in tools:
                verdict, reason = judge_finding(c, findings_by_tool.get(tool, []))
                row[f"{tool}_verdict"] = verdict
                row[f"{tool}_reason"] = reason
                row[f"{tool}_finding_count"] = len(findings_by_tool.get(tool, []))
            rows.append(row)

    # Write CSV
    if rows:
        with open(output_path, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=rows[0].keys())
            writer.writeheader()
            writer.writerows(rows)
        print(f"\nWrote {len(rows)} rows to {output_path}", file=sys.stderr)

    # Print summary
    print_summary(rows, tools, tool_findings_all)


def _make_row(pr, comment, owner, repo, pr_num):
    """Create a base CSV row for a golden comment."""
    return {
        "repo": f"{owner}/{repo}",
        "pr_number": pr_num,
        "language": pr.get("project_main_language", ""),
        "golden_path": comment.get("path", ""),
        "golden_from_line": comment.get("from_line", ""),
        "golden_to_line": comment.get("to_line", ""),
        "golden_category": comment.get("category", ""),
        "golden_context": comment.get("context", ""),
        "golden_note": comment.get("note", "")[:500],
    }


def print_summary(rows, tools, tool_findings_all):
    """Print recall/precision/F1 summary table."""
    print(f"\n{'='*80}", file=sys.stderr)
    print(f"AACR-BENCH RESULTS ({len(rows)} golden comments)", file=sys.stderr)
    print(f"{'='*80}", file=sys.stderr)

    # Header
    print(f"\n{'Metric':<30}", end="", file=sys.stderr)
    for t in tools:
        print(f"  {t:>12}", end="", file=sys.stderr)
    print("", file=sys.stderr)
    print("-" * (30 + 14 * len(tools)), file=sys.stderr)

    for tool in tools:
        verdicts = [r.get(f"{tool}_verdict", "miss") for r in rows]
        matches = verdicts.count("match")
        partials = verdicts.count("partial")
        misses = verdicts.count("miss")
        errors = verdicts.count("error")
        total = len(verdicts)
        total_findings = sum(r.get(f"{tool}_finding_count", 0) for r in rows)
        # Deduplicate findings per PR for precision
        unique_findings = len(tool_findings_all.get(tool, []))

        strict_recall = matches / total * 100 if total else 0
        lenient_recall = (matches + partials) / total * 100 if total else 0
        precision = matches / unique_findings * 100 if unique_findings else 0
        f1 = 2 * (precision * strict_recall) / (precision + strict_recall) if (precision + strict_recall) else 0

        if tool == tools[0]:
            print(f"{'Match':<30}", end="", file=sys.stderr)
        print(f"  {matches:>12}", end="", file=sys.stderr)

    print("", file=sys.stderr)

    for tool in tools:
        verdicts = [r.get(f"{tool}_verdict", "miss") for r in rows]
        partials = verdicts.count("partial")
        if tool == tools[0]:
            print(f"{'Partial':<30}", end="", file=sys.stderr)
        print(f"  {partials:>12}", end="", file=sys.stderr)
    print("", file=sys.stderr)

    for tool in tools:
        verdicts = [r.get(f"{tool}_verdict", "miss") for r in rows]
        misses = verdicts.count("miss")
        if tool == tools[0]:
            print(f"{'Miss':<30}", end="", file=sys.stderr)
        print(f"  {misses:>12}", end="", file=sys.stderr)
    print("", file=sys.stderr)

    print("-" * (30 + 14 * len(tools)), file=sys.stderr)

    # Metrics
    for metric_name, metric_fn in [
        ("Strict recall (%)", lambda t: _strict_recall(rows, t)),
        ("Lenient recall (%)", lambda t: _lenient_recall(rows, t)),
        ("Total findings", lambda t: len(tool_findings_all.get(t, []))),
        ("Precision (%)", lambda t: _precision(rows, t, tool_findings_all)),
        ("F1 (%)", lambda t: _f1(rows, t, tool_findings_all)),
    ]:
        print(f"{metric_name:<30}", end="", file=sys.stderr)
        for tool in tools:
            val = metric_fn(tool)
            if isinstance(val, float):
                print(f"  {val:>11.1f}%", end="", file=sys.stderr)
            else:
                print(f"  {val:>12}", end="", file=sys.stderr)
        print("", file=sys.stderr)

    # Per-category breakdown
    categories = sorted(set(r.get("golden_category", "") for r in rows if r.get("golden_category")))
    if categories:
        print(f"\nPer-category strict recall:", file=sys.stderr)
        print(f"{'Category':<25}", end="", file=sys.stderr)
        for t in tools:
            print(f"  {t:>12}", end="", file=sys.stderr)
        print("", file=sys.stderr)
        for cat in categories:
            cat_rows = [r for r in rows if r.get("golden_category") == cat]
            print(f"  {cat:<23}", end="", file=sys.stderr)
            for tool in tools:
                val = _strict_recall(cat_rows, tool)
                print(f"  {val:>11.1f}%", end="", file=sys.stderr)
            print(f"  (n={len(cat_rows)})", file=sys.stderr)

    # Per-language breakdown
    languages = sorted(set(r.get("language", "") for r in rows if r.get("language")))
    if languages:
        print(f"\nPer-language strict recall:", file=sys.stderr)
        print(f"{'Language':<25}", end="", file=sys.stderr)
        for t in tools:
            print(f"  {t:>12}", end="", file=sys.stderr)
        print("", file=sys.stderr)
        for lang in languages:
            lang_rows = [r for r in rows if r.get("language") == lang]
            print(f"  {lang:<23}", end="", file=sys.stderr)
            for tool in tools:
                val = _strict_recall(lang_rows, tool)
                print(f"  {val:>11.1f}%", end="", file=sys.stderr)
            print(f"  (n={len(lang_rows)})", file=sys.stderr)


def _strict_recall(rows, tool):
    verdicts = [r.get(f"{tool}_verdict", "miss") for r in rows]
    total = len(verdicts)
    return verdicts.count("match") / total * 100 if total else 0.0


def _lenient_recall(rows, tool):
    verdicts = [r.get(f"{tool}_verdict", "miss") for r in rows]
    total = len(verdicts)
    return (verdicts.count("match") + verdicts.count("partial")) / total * 100 if total else 0.0


def _precision(rows, tool, tool_findings_all):
    verdicts = [r.get(f"{tool}_verdict", "miss") for r in rows]
    matches = verdicts.count("match")
    unique_findings = len(tool_findings_all.get(tool, []))
    return matches / unique_findings * 100 if unique_findings else 0.0


def _f1(rows, tool, tool_findings_all):
    sr = _strict_recall(rows, tool)
    p = _precision(rows, tool, tool_findings_all)
    return 2 * p * sr / (p + sr) if (p + sr) else 0.0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="AACR-Bench: inspect vs Greptile vs CodeRabbit")
    parser.add_argument("--tools", default="inspect", help="Comma-separated tool list: inspect,greptile,coderabbit")
    parser.add_argument("--limit", type=int, default=None, help="Limit number of PRs to process")
    parser.add_argument("--diverse", action="store_true", help="Round-robin sample across repos for language/repo diversity")
    parser.add_argument("--output", default="results.csv", help="Output CSV path")
    args = parser.parse_args()

    tool_list = [t.strip() for t in args.tools.split(",")]
    valid_tools = {"inspect", "greptile", "coderabbit"}
    for t in tool_list:
        if t not in valid_tools:
            print(f"Unknown tool: {t}. Valid: {', '.join(valid_tools)}", file=sys.stderr)
            sys.exit(1)

    run_benchmark(tool_list, args.limit, args.output, diverse=args.diverse)
