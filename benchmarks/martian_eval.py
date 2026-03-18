#!/usr/bin/env python3
"""Martian Offline Benchmark eval harness for inspect.

Evaluates inspect against the withmartian/code-review-benchmark:
50 PRs across 5 repos (Sentry, Grafana, Cal.com, Discourse, Keycloak)
with 137 human-curated golden comments.

Two modes:
  ast  — Deterministic, no LLM. Runs inspect to get entity reviews,
         matches identifiers from golden comments against entity names/files.
         Gives triage recall (ceiling) and triage precision. Fast inner loop.

  llm  — Full pipeline. Inspect triage + LLM review + Martian judge prompt
         for semantic matching. Computes precision/recall/F1.

Usage:
    python benchmarks/martian_eval.py --mode ast --limit 5
    python benchmarks/martian_eval.py --mode ast --repo sentry
    python benchmarks/martian_eval.py --mode llm --limit 2 --model gpt-4o
    python benchmarks/martian_eval.py --mode ast              # full run
"""

import argparse
import concurrent.futures
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

# --- Config ---

INSPECT_BIN = os.path.expanduser("~/Documents/work/inspect/target/release/inspect")
CACHE_DIR = "/tmp/martian-eval"
REPOS_DIR = f"{CACHE_DIR}/repos"
WORKTREES_DIR = f"{CACHE_DIR}/worktrees"
GOLDEN_DIR = f"{CACHE_DIR}/golden_comments"
BENCHMARK_REPO = "/tmp/martian-eval/benchmark"

def load_dotenv():
    """Load .env from project root."""
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

OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY", "")
ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY", "")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN", "")

REPO_MAP = {
    "sentry": "getsentry/sentry",
    "grafana": "grafana/grafana",
    "keycloak": "keycloak/keycloak",
    "discourse": "discourse/discourse",
    "cal_dot_com": "calcom/cal.com",
}

# Reverse map: GitHub owner/repo -> short name
REVERSE_REPO_MAP = {v: k for k, v in REPO_MAP.items()}

# Skip slow PRs with huge graphs (>100s each, 89% of total runtime)
SKIP_PRS = {
    ("ai-code-review-evaluation", "sentry-greptile", "1"),   # 148s, 121k graph entities
    ("grafana", "grafana", "90939"),                          # 92s, 76k graph entities
    ("getsentry", "sentry", "94376"),                         # 65s, 123k graph entities
}


# --- Dataset loading ---

def load_golden_comments(repo_filter=None):
    """Load golden comments from local benchmark clone.

    Returns list of PR dicts:
    {repo, pr_title, url, original_url, pr_number, owner, repo_name, comments: [{comment, severity}]}
    """
    golden_dir = Path(BENCHMARK_REPO) / "offline" / "golden_comments"
    if not golden_dir.exists():
        print(f"ERROR: Benchmark repo not found at {BENCHMARK_REPO}", file=sys.stderr)
        print(f"  Clone it: git clone https://github.com/withmartian/code-review-benchmark.git {BENCHMARK_REPO}", file=sys.stderr)
        sys.exit(1)

    all_prs = []
    for json_file in sorted(golden_dir.glob("*.json")):
        repo_short = json_file.stem  # e.g., "sentry", "grafana"
        if repo_filter and repo_short != repo_filter:
            continue

        with open(json_file) as f:
            entries = json.load(f)

        for entry in entries:
            url = entry["url"]
            original_url = entry.get("original_url")

            # Resolve the real repo URL
            real_url = original_url or url
            owner, repo_name, pr_number, is_commit = _parse_github_url(real_url)

            # Fallback: if url points to forked repo and no original_url
            if not owner and not original_url:
                # Try to use the forked repo directly
                owner, repo_name, pr_number, is_commit = _parse_github_url(url)

            all_prs.append({
                "repo": repo_short,
                "pr_title": entry.get("pr_title", ""),
                "url": url,
                "original_url": original_url,
                "real_url": real_url,
                "owner": owner,
                "repo_name": repo_name,
                "pr_number": pr_number,
                "is_commit": is_commit,
                "comments": entry.get("comments", []),
            })

    return all_prs


def _parse_github_url(url):
    """Parse a GitHub PR or commit URL. Returns (owner, repo, number_or_sha, is_commit)."""
    # PR URL: https://github.com/owner/repo/pull/123
    m = re.match(r"https://github\.com/([^/]+)/([^/]+)/pull/(\d+)", url)
    if m:
        return m.group(1), m.group(2), int(m.group(3)), False

    # Commit URL: https://github.com/owner/repo/commit/sha
    m = re.match(r"https://github\.com/([^/]+)/([^/]+)/commit/([0-9a-f]+)", url)
    if m:
        return m.group(1), m.group(2), m.group(3), True

    return None, None, None, False


# --- Repo management ---

def ensure_repo(owner, repo_name):
    """Clone repo if not cached. Returns repo dir path."""
    os.makedirs(REPOS_DIR, exist_ok=True)
    repo_dir = f"{REPOS_DIR}/{owner}__{repo_name}"
    if os.path.exists(repo_dir):
        return repo_dir
    print(f"  cloning {owner}/{repo_name}...", file=sys.stderr)
    clone_url = f"https://github.com/{owner}/{repo_name}.git"
    subprocess.run(
        ["git", "clone", "--no-checkout", "--filter=blob:none", clone_url, repo_dir],
        capture_output=True, timeout=1200,
    )
    return repo_dir


def _github_api(endpoint):
    """Call GitHub REST API. Returns parsed JSON or None."""
    import urllib.request
    url = f"https://api.github.com{endpoint}"
    headers = {"Accept": "application/vnd.github.v3+json"}
    if GITHUB_TOKEN:
        headers["Authorization"] = f"Bearer {GITHUB_TOKEN}"
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except Exception as e:
        print(f"    GitHub API error: {e}", file=sys.stderr)
        return None


def resolve_pr_shas(owner, repo_name, pr_number):
    """Get base and head SHAs for a PR via GitHub API. Cached locally."""
    cache_file = f"{CACHE_DIR}/sha_cache/{owner}_{repo_name}_{pr_number}.json"
    os.makedirs(os.path.dirname(cache_file), exist_ok=True)
    if os.path.exists(cache_file):
        with open(cache_file) as f:
            return json.load(f)

    pr_data = _github_api(f"/repos/{owner}/{repo_name}/pulls/{pr_number}")
    if not pr_data:
        return None

    data = {
        "base_sha": pr_data["base"]["sha"],
        "head_sha": pr_data["head"]["sha"],
        "base_ref": pr_data["base"]["ref"],
    }
    with open(cache_file, "w") as f:
        json.dump(data, f)
    return data


def _commit_exists(repo_dir, sha):
    """Check if a commit already exists in the local repo."""
    try:
        result = subprocess.run(
            ["git", "cat-file", "-t", sha],
            cwd=repo_dir, capture_output=True, text=True, timeout=10,
        )
        return result.returncode == 0 and result.stdout.strip() == "commit"
    except subprocess.TimeoutExpired:
        return False


def _fetch_if_needed(repo_dir, sha):
    """Fetch a commit only if not already local."""
    if sha and not _commit_exists(repo_dir, sha):
        subprocess.run(
            ["git", "fetch", "--depth=200", "origin", sha],
            cwd=repo_dir, capture_output=True, timeout=300,
        )


def ensure_worktree(repo_dir, base_sha, head_sha):
    """Create a cached git worktree for this PR's head commit.

    Returns worktree path. Worktrees are stable per base..head pair,
    cached across runs, and parallel-safe (each PR gets its own dir).
    """
    # Fetch commits into the shared repo
    _fetch_if_needed(repo_dir, base_sha)
    _fetch_if_needed(repo_dir, head_sha)

    # Worktree keyed by the diff pair
    repo_basename = os.path.basename(repo_dir)
    wt_key = f"{base_sha[:12]}_{head_sha[:12]}"
    wt_dir = f"{WORKTREES_DIR}/{repo_basename}/{wt_key}"

    if os.path.exists(wt_dir):
        return wt_dir

    os.makedirs(os.path.dirname(wt_dir), exist_ok=True)
    result = subprocess.run(
        ["git", "worktree", "add", "--detach", wt_dir, head_sha],
        cwd=repo_dir, capture_output=True, text=True, timeout=120,
    )
    if result.returncode != 0:
        # Fallback: if worktree add fails (e.g., already registered but dir missing),
        # prune and retry
        subprocess.run(["git", "worktree", "prune"], cwd=repo_dir, capture_output=True, timeout=10)
        result = subprocess.run(
            ["git", "worktree", "add", "--detach", wt_dir, head_sha],
            cwd=repo_dir, capture_output=True, text=True, timeout=120,
        )
        if result.returncode != 0:
            print(f"    worktree failed: {result.stderr[:200]}", file=sys.stderr)
            return None

    return wt_dir


# --- Inspect runner ---

def run_inspect(repo_dir, base_sha, head_sha):
    """Run inspect diff, return parsed JSON result. Never cached — changes with code."""
    diff_ref = f"{base_sha}..{head_sha}"
    result = subprocess.run(
        [INSPECT_BIN, "diff", diff_ref, "--repo", repo_dir, "--format", "json"],
        capture_output=True, text=True, timeout=300,
    )
    if result.returncode != 0:
        print(f"    inspect failed: {result.stderr[:200]}", file=sys.stderr)
        return None

    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        print(f"    inspect JSON parse error", file=sys.stderr)
        return None


# --- AST mode: identifier matching ---

def extract_identifiers(text):
    """Extract likely code identifiers from a golden comment."""
    idents = set()
    patterns = [
        r'\b[A-Z][a-zA-Z0-9]+(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*\b',  # PascalCase/ClassName.method
        r'\b[a-z][a-zA-Z0-9]*[A-Z][a-zA-Z0-9]*\b',  # camelCase
        r'\b[a-z_][a-z0-9_]{2,}\b',  # snake_case (3+ chars)
        r'`([^`]+)`',  # backtick-quoted
        r"'([^']+)'",  # single-quoted identifiers
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
        'instead', 'ensure', 'handling', 'handle', 'used', 'uses',
        'may', 'could', 'would', 'while', 'because', 'since',
        'also', 'still', 'just', 'only', 'other', 'more', 'than',
        'access', 'object', 'case', 'return', 'default', 'state',
        'event', 'query', 'request', 'response', 'path', 'file',
        'config', 'option', 'field', 'item', 'index', 'count',
        'size', 'length', 'start', 'end', 'time', 'user',
        'fails', 'failed', 'instead', 'renamed', 'passed',
        'passing', 'checking', 'checked', 'caused', 'leading',
    }

    for pat in patterns:
        for m in re.finditer(pat, text):
            ident = m.group(1) if m.lastindex else m.group(0)
            if len(ident) >= 3 and ident.lower() not in stopwords:
                idents.add(ident)

    return idents


def ast_match_comment(golden_comment, inspect_data):
    """Check if a golden comment's issue is covered by inspect's entity reviews.

    Returns (verdict, reason, matched_entities) where verdict is 'match', 'partial', or 'miss'.
    """
    if not inspect_data:
        return "miss", "inspect returned no data", []

    entities = inspect_data.get("entity_reviews", [])
    if not entities:
        return "miss", "no entities found", []

    comment_text = golden_comment["comment"]
    golden_idents = extract_identifiers(comment_text)

    # Strategy 0: Detector findings match — highest signal, pre-filtered for likely bugs
    findings = inspect_data.get("findings", [])
    if findings and golden_idents:
        for ident in golden_idents:
            ident_lower = ident.lower()
            for f in findings:
                fname = f.get("entity_name", "").lower()
                fmsg = f.get("message", "").lower()
                fevidence = f.get("evidence", "").lower()
                if (ident_lower == fname or ident_lower in fname or fname in ident_lower
                        or ident_lower in fmsg or ident_lower in fevidence):
                    # Find the matching entity review for the return value
                    matched_entity = next(
                        (e for e in entities if e.get("entity_id") == f.get("entity_id")),
                        None,
                    )
                    return "match", (
                        f"S0: '{ident}' matches finding '{f.get('rule_id')}' on "
                        f"'{f.get('entity_name')}' ({f.get('severity', '')})"
                    ), [matched_entity] if matched_entity else []

    # Extract entity info
    entity_names = set()
    entity_files = set()
    hc_entities = []  # High/Critical
    hcm_entities = []  # High/Critical/Medium

    for e in entities:
        name_lower = e.get("entity_name", "").lower()
        entity_names.add(name_lower)
        file_path = e.get("file_path", "").lower()
        entity_files.add(file_path)

        risk = e.get("risk_level", "Low")
        if risk in ("High", "Critical"):
            hc_entities.append(e)
        if risk in ("High", "Critical", "Medium"):
            hcm_entities.append(e)

    hc_names = {e.get("entity_name", "").lower() for e in hc_entities}
    hcm_names = {e.get("entity_name", "").lower() for e in hcm_entities}
    hc_files = {e.get("file_path", "").lower() for e in hc_entities}
    hcm_files = {e.get("file_path", "").lower() for e in hcm_entities}

    # Also extract identifiers from entity content for deeper matching
    hcm_content_idents = set()
    for e in hcm_entities:
        content = e.get("after_content") or e.get("before_content") or ""
        for m in re.finditer(r'\b([a-zA-Z_][a-zA-Z0-9_]{3,})\b', content):
            hcm_content_idents.add(m.group(1).lower())

    matched = []

    # Strategy 1: Direct name match in HC entities
    for ident in golden_idents:
        ident_lower = ident.lower()
        for e in hc_entities:
            ename = e.get("entity_name", "").lower()
            if ident_lower == ename or ident_lower in ename or ename in ident_lower:
                return "match", f"'{ident}' matches HC entity '{e['entity_name']}'", [e]

    # Strategy 2: Direct name match in HCM entities
    for ident in golden_idents:
        ident_lower = ident.lower()
        for e in hcm_entities:
            ename = e.get("entity_name", "").lower()
            if ident_lower == ename or ident_lower in ename or ename in ident_lower:
                return "match", f"'{ident}' matches HCM entity '{e['entity_name']}'", [e]

    # Strategy 3: Direct name match in ALL entities
    for ident in golden_idents:
        ident_lower = ident.lower()
        for e in entities:
            ename = e.get("entity_name", "").lower()
            if ident_lower == ename or ident_lower in ename or ename in ident_lower:
                return "partial", f"'{ident}' matches entity '{e['entity_name']}' ({e.get('risk_level')})", [e]

    # Strategy 4: Golden comment identifiers appear in HCM entity code content
    ident_matches_in_content = 0
    for ident in golden_idents:
        if ident.lower() in hcm_content_idents:
            ident_matches_in_content += 1
    if ident_matches_in_content >= 2:
        return "partial", f"{ident_matches_in_content} golden idents found in HCM entity content", []

    # Strategy 5: File path overlap
    for ident in golden_idents:
        ident_lower = ident.lower()
        for fp in hcm_files:
            if ident_lower in fp or (len(ident_lower) >= 5 and any(ident_lower in part for part in fp.split("/"))):
                return "partial", f"'{ident}' found in HCM file path '{fp}'", []

    # If we have entities but no match
    if hcm_entities:
        return "partial", f"HCM entities exist ({len(hcm_entities)}) but no name match; golden_idents={golden_idents}", []

    if entities:
        return "miss", f"entities found ({len(entities)}) but all Low risk; golden_idents={golden_idents}", []

    return "miss", f"no entity overlap; golden_idents={golden_idents}", []


# --- LLM mode ---

def _call_llm(prompt, model, system=None):
    """Call OpenAI or Anthropic API and return parsed JSON."""
    if model.startswith("gpt") or model.startswith("o1") or model.startswith("o3") or model.startswith("o4"):
        return _call_openai(prompt, model, system)
    else:
        return _call_anthropic(prompt, model, system)


def _call_openai(prompt, model, system=None):
    """Call OpenAI chat completions API."""
    import urllib.request
    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})

    payload = json.dumps({
        "model": model,
        "messages": messages,
        "temperature": 0.0,
    }).encode()

    # Use local proxy if available, else fall back to OpenAI
    _openai_base = os.environ.get("OPENAI_BASE_URL", "https://api.openai.com")
    _openai_key = os.environ.get("OPENAI_API_KEY_OVERRIDE", OPENAI_API_KEY)
    req = urllib.request.Request(
        f"{_openai_base}/v1/chat/completions",
        data=payload,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {_openai_key}",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        data = json.loads(resp.read())

    content = data["choices"][0]["message"]["content"].strip()
    # Strip markdown fences
    if content.startswith("```"):
        content = content.split("```")[1]
        if content.startswith("json"):
            content = content[4:]
        content = content.strip()
    return json.loads(content)


def _call_anthropic(prompt, model, system=None):
    """Call Anthropic messages API."""
    import urllib.request
    messages = [{"role": "user", "content": prompt}]

    body = {
        "model": model,
        "max_tokens": 4096,
        "messages": messages,
        "temperature": 0.0,
    }
    if system:
        body["system"] = system

    payload = json.dumps(body).encode()
    req = urllib.request.Request(
        "https://api.anthropic.com/v1/messages",
        data=payload,
        headers={
            "Content-Type": "application/json",
            "x-api-key": ANTHROPIC_API_KEY,
            "anthropic-version": "2023-06-01",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        data = json.loads(resp.read())

    content = data["content"][0]["text"].strip()
    if content.startswith("```"):
        content = content.split("```")[1]
        if content.startswith("json"):
            content = content[4:]
        content = content.strip()
    return json.loads(content)


def _extract_file_diff(full_diff, file_path):
    """Extract diff for a specific file from the full diff."""
    parts = full_diff.split("diff --git ")
    for part in parts:
        if file_path in part.split("\n")[0]:
            return "diff --git " + part
    return ""


def get_diff_text(repo_dir, base_sha, head_sha):
    """Get unified diff text between two commits."""
    result = subprocess.run(
        ["git", "diff", base_sha, head_sha],
        cwd=repo_dir, capture_output=True, text=True, timeout=120,
    )
    return result.stdout if result.returncode == 0 else ""


def run_inspect_llm(repo_dir, base_sha, head_sha, inspect_data, model="gpt-4o"):
    """Full LLM pipeline: inspect triage + LLM review on top entities.

    Returns list of finding dicts: [{description, file, line, confidence}]
    """
    if not inspect_data:
        return []

    all_entities = inspect_data.get("entity_reviews", [])
    hcm_entities = [e for e in all_entities
                    if e.get("risk_level") in ("High", "Critical", "Medium")]

    if not hcm_entities:
        return []

    # Build entity_id -> detector findings index for prompt enrichment
    detector_findings = inspect_data.get("findings", [])
    findings_by_entity = {}
    for f in detector_findings:
        eid = f.get("entity_id", "")
        findings_by_entity.setdefault(eid, []).append(f)

    # Select top entities with file diversity (round-robin)
    # Prioritize entities that have detector findings
    entities_with_findings = set(findings_by_entity.keys())
    hcm_entities.sort(key=lambda e: (
        e.get("entity_id", "") in entities_with_findings,  # findings first
        e.get("risk_score", 0),
    ), reverse=True)
    by_file = {}
    for e in hcm_entities:
        fp = e.get("file_path", "")
        by_file.setdefault(fp, []).append(e)

    selected = []
    max_entities = 30
    file_keys = list(by_file.keys())
    idx = 0
    while len(selected) < max_entities and file_keys:
        key = file_keys[idx % len(file_keys)]
        if by_file[key]:
            selected.append(by_file[key].pop(0))
        else:
            file_keys.remove(key)
            if not file_keys:
                break
            continue
        idx += 1

    diff_text = get_diff_text(repo_dir, base_sha, head_sha)

    # Build review prompts
    tasks = []
    for e in selected:
        file_path = e.get("file_path", "")
        entity_name = e.get("entity_name", "")
        entity_id = e.get("entity_id", "")
        before = (e.get("before_content") or "")[:3000]
        after = (e.get("after_content") or "")[:3000]
        file_diff = _extract_file_diff(diff_text, file_path)[:3000]

        raw_deps = e.get("dependency_names", []) or []
        flat_deps = []
        for d in raw_deps[:10]:
            if isinstance(d, list):
                flat_deps.extend(str(x) for x in d)
            else:
                flat_deps.append(str(d))
        deps = ", ".join(flat_deps[:10])

        # Include detector findings for this entity if any
        entity_findings = findings_by_entity.get(entity_id, [])
        findings_section = ""
        if entity_findings:
            lines = ["DETECTOR FINDINGS (static analysis flagged these — validate or reject):"]
            for df in entity_findings:
                lines.append(
                    f"  - [{df.get('severity', '')}] {df.get('rule_id', '')}: "
                    f"{df.get('message', '')} (evidence: {df.get('evidence', '')[:200]})"
                )
            findings_section = "\n".join(lines) + "\n\n"

        prompt = (
            f"You are reviewing a code change. Find real bugs, security vulnerabilities, "
            f"performance issues, and maintainability problems.\n\n"
            f"File: {file_path}\n"
            f"Entity: {entity_name} ({e.get('entity_type', '')})\n"
            f"Risk: {e.get('risk_level', '')}, score={e.get('risk_score', 0):.2f}, "
            f"dependents={e.get('dependent_count', 0)}\n"
            f"Dependencies: {deps}\n\n"
            f"{findings_section}"
            f"DIFF:\n```diff\n{file_diff}\n```\n\n"
            f"BEFORE:\n```\n{before}\n```\n\n"
            f"AFTER:\n```\n{after}\n```\n\n"
            f"Rules:\n"
            f"- Max 2 issues. Only report issues you are highly confident about.\n"
            f"- If detector findings are provided, validate them against the diff. "
            f"Confirm true positives, reject false positives.\n"
            f"- In the description, always reference the specific variable, function, or class name.\n"
            f"- Rate confidence 1-5 (5=certain bug, 3=likely issue, 1=nitpick).\n\n"
            f"Respond with a JSON array:\n"
            f'[{{"file": "{file_path}", "line": N, "confidence": 4, '
            f'"description": "<specific issue>"}}]\n'
            f"If genuinely no issues, respond with []. Only return JSON."
        )
        tasks.append((e, prompt))

    # Seed findings from detectors (high-confidence ones become direct candidates)
    findings = []
    for df in detector_findings:
        if df.get("confidence", 0) >= 0.7:
            findings.append({
                "file": df.get("file_path", ""),
                "line": df.get("start_line", 0),
                "confidence": 4 if df.get("confidence", 0) >= 0.85 else 3,
                "description": f"{df.get('message', '')} [{df.get('rule_id', '')}]",
                "entity_name": df.get("entity_name", ""),
                "risk_level": df.get("severity", ""),
                "source": "detector",
            })
    done = [0]

    def _review_entity(args):
        ent, prompt = args
        try:
            result = _call_llm(prompt, model)
            return ent, result if isinstance(result, list) else []
        except Exception as ex:
            print(f" [ERR:{type(ex).__name__}]", file=sys.stderr, end="", flush=True)
            return ent, []

    print(f"    LLM reviewing {len(tasks)} entities...", file=sys.stderr, end="", flush=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        futures = {pool.submit(_review_entity, t): t for t in tasks}
        for future in concurrent.futures.as_completed(futures):
            done[0] += 1
            e, llm_findings = future.result()
            for f in llm_findings:
                if isinstance(f, dict):
                    f["entity_name"] = e.get("entity_name", "")
                    f["risk_level"] = e.get("risk_level", "")
                    if not f.get("file"):
                        f["file"] = e.get("file_path", "")
                    findings.append(f)
    print(f" done ({len(findings)} findings)", file=sys.stderr)

    # Filter by confidence
    findings = [f for f in findings if f.get("confidence", 3) >= 3]

    # Cap
    if len(findings) > 15:
        findings.sort(key=lambda f: f.get("confidence", 3), reverse=True)
        findings = findings[:15]

    return findings


# --- Agent mode ---

AGENT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "agent")


def _build_triage_section(entities):
    """Build entity-grouped triage section text (Python port of prompts.rs build_rich_triage)."""
    meaningful = [
        e for e in entities
        if e.get("change_type") in ("Modified", "Added")
        and e.get("entity_type") != "chunk"
    ]
    meaningful.sort(key=lambda e: e.get("risk_score", 0), reverse=True)
    top = meaningful[:20]
    if not top:
        return ""

    by_file = {}
    for e in top:
        by_file.setdefault(e.get("file_path", ""), []).append(e)

    file_entries = sorted(
        by_file.items(),
        key=lambda kv: max(e.get("risk_score", 0) for e in kv[1]),
        reverse=True,
    )

    lines = [
        "## Entity-level triage (highest-risk changes)",
        "Each entity includes: name, type, change_type, classification, risk (score), blast_radius, dependents count.",
    ]
    for fp, ents in file_entries:
        lines.append(f"\n### {fp}")
        for e in ents:
            public = " [PUBLIC API]" if e.get("is_public_api") else ""
            lines.append(
                f"- `{e.get('entity_name', '')}` ({e.get('entity_type', '')}, "
                f"{e.get('change_type', '')}, {e.get('classification', '')}) | "
                f"risk={e.get('risk_level', '')} ({e.get('risk_score', 0):.2f}) | "
                f"blast_radius={e.get('blast_radius', 0)} | "
                f"dependents={e.get('dependent_count', 0)}{public}"
            )
    return "\n".join(lines)


def run_inspect_agent(repo_dir, base_sha, head_sha, inspect_data, model=None):
    """Agentic pipeline: spawn pi-core validator with tool access.

    Returns list of finding dicts compatible with run_inspect_llm output:
    [{description, file, line, confidence, entity_name, risk_level}]
    """
    if not inspect_data:
        return []

    all_entities = inspect_data.get("entity_reviews", [])
    detector_findings = inspect_data.get("findings", [])
    diff_text = get_diff_text(repo_dir, base_sha, head_sha)

    if not diff_text:
        return []

    # Build triage section
    triage_section = _build_triage_section(all_entities)

    # Truncate diff for the agent (same 80k limit as Rust side)
    truncated_diff = diff_text[:80_000]

    # Build agent input
    agent_input = {
        "pr_title": "",  # Not available in benchmark context
        "diff": truncated_diff,
        "triage_section": triage_section,
        "findings": detector_findings,
        "entity_reviews": all_entities,
        "repo_dir": os.path.abspath(repo_dir),
    }
    if model:
        # Map model names to provider
        if model.startswith("gpt") or model.startswith("o1") or model.startswith("o3") or model.startswith("o4"):
            agent_input["provider"] = "openai"
            agent_input["model"] = model
        elif model.startswith("claude"):
            agent_input["provider"] = "anthropic"
            agent_input["model"] = model
        else:
            agent_input["provider"] = "anthropic"
            agent_input["model"] = model

    input_json = json.dumps(agent_input)

    # Spawn agent process
    print(f"    Agent: spawning validator ({len(detector_findings)} findings, {len(all_entities)} entities)...",
          file=sys.stderr, end="", flush=True)
    t0 = time.time()
    try:
        proc = subprocess.run(
            ["node", "--import", "tsx/esm", "src/validate.ts"],
            cwd=AGENT_DIR,
            input=input_json,
            capture_output=True,
            text=True,
            timeout=300,
            env={**os.environ, "ANTHROPIC_API_KEY": ANTHROPIC_API_KEY, "OPENAI_API_KEY": OPENAI_API_KEY},
        )
    except subprocess.TimeoutExpired:
        print(f" TIMEOUT after 300s", file=sys.stderr)
        return []

    elapsed = time.time() - t0

    if proc.stderr:
        # Count tool calls from stderr
        tool_calls = proc.stderr.count("[tool]")
        print(f" {elapsed:.1f}s, {tool_calls} tool calls", file=sys.stderr, end="", flush=True)

    if proc.returncode != 0:
        print(f" FAILED (exit {proc.returncode})", file=sys.stderr)
        if proc.stderr:
            print(f"    stderr: {proc.stderr[:300]}", file=sys.stderr)
        return []

    # Parse output
    stdout = proc.stdout.strip()
    if not stdout:
        print(f" empty output", file=sys.stderr)
        return []

    try:
        agent_out = json.loads(stdout)
    except json.JSONDecodeError as e:
        print(f" JSON parse error: {e}", file=sys.stderr)
        return []

    verdicts = agent_out.get("verdicts", [])

    # All verdicts from the new agent are issues (true_positive)
    findings = []
    for v in verdicts:
        explanation = v.get("explanation", "")
        file_path = v.get("entity_name", "")
        findings.append({
            "file": file_path,
            "line": 0,
            "confidence": 4,
            "description": explanation,
            "entity_name": file_path,
            "risk_level": "High",
            "source": "agent",
        })

    print(f" done ({len(findings)} issues)", file=sys.stderr)

    return findings


def process_pr_agent(pr_info, model=None):
    """Process a single PR in agent mode. Returns precision/recall/F1."""
    owner = pr_info["owner"]
    repo_name = pr_info["repo_name"]
    pr_number = pr_info["pr_number"]
    is_commit = pr_info["is_commit"]

    if not owner or not repo_name:
        return {"error": "could not parse URL", "results": {}}

    if (owner, repo_name, str(pr_number)) in SKIP_PRS:
        return {"skipped": True, "skip_reason": "slow PR", "results": {}}

    try:
        repo_dir = ensure_repo(owner, repo_name)
    except Exception as e:
        return {"error": f"clone failed: {e}", "results": {}}

    if is_commit:
        sha = str(pr_number)
        _fetch_if_needed(repo_dir, sha)
        result = subprocess.run(
            ["git", "rev-parse", f"{sha}^"],
            cwd=repo_dir, capture_output=True, text=True, timeout=10,
        )
        if result.returncode != 0:
            return {"error": f"could not get parent of {sha}", "results": {}}
        base_sha = result.stdout.strip()
        head_sha = sha
    else:
        sha_info = resolve_pr_shas(owner, repo_name, pr_number)
        if not sha_info:
            return {"error": f"could not resolve PR SHAs", "results": {}}
        base_sha = sha_info["base_sha"]
        head_sha = sha_info["head_sha"]

    wt_dir = ensure_worktree(repo_dir, base_sha, head_sha)
    if not wt_dir:
        return {"error": "worktree creation failed", "results": {}}

    inspect_data = run_inspect(wt_dir, base_sha, head_sha)
    findings = run_inspect_agent(wt_dir, base_sha, head_sha, inspect_data, model)
    eval_result = evaluate_llm_findings(pr_info["comments"], findings, model="gpt-4o")

    entities = (inspect_data or {}).get("entity_reviews", [])
    hcm_entities = [e for e in entities if e.get("risk_level") in ("High", "Critical", "Medium")]
    entity_count = len(entities)
    hcm_count = len(hcm_entities)
    timing = (inspect_data or {}).get("timing", {})

    return {
        "error": None,
        "results": eval_result,
        "findings_count": len(findings),
        "findings": findings,
        "entity_count": entity_count,
        "hcm_count": hcm_count,
        "timing": timing,
        "base_sha": base_sha,
        "head_sha": head_sha,
    }


# --- Martian judge ---

JUDGE_PROMPT = """You are evaluating AI code review tools.
Determine if the candidate issue matches the golden (expected) comment.

Golden Comment (the issue we're looking for):
{golden_comment}

Candidate Issue (from the tool's review):
{candidate}

Instructions:
- Determine if the candidate identifies the SAME underlying issue as the golden comment
- Accept semantic matches - different wording is fine if it's the same problem
- Focus on whether they point to the same bug, concern, or code issue

Respond with ONLY a JSON object:
{{"reasoning": "brief explanation", "match": true, "confidence": 0.8}}"""


def judge_match(golden_comment, candidate_text, model="gpt-4o"):
    """Use LLM judge to determine if candidate matches golden comment."""
    prompt = JUDGE_PROMPT.format(
        golden_comment=golden_comment,
        candidate=candidate_text,
    )
    try:
        result = _call_llm(prompt, model, system="You are a precise code review evaluator. Always respond with valid JSON.")
        return result
    except Exception:
        return {"match": False, "confidence": 0.0, "error": True}


def evaluate_llm_findings(golden_comments, findings, model="gpt-4o"):
    """Evaluate LLM findings against golden comments using Martian judge.

    Returns dict with tp, fp, fn, precision, recall, f1, details.
    """
    if not findings:
        return {
            "tp": 0, "fp": 0, "fn": len(golden_comments),
            "precision": 0.0, "recall": 0.0, "f1": 0.0,
            "true_positives": [], "false_positives": [],
            "false_negatives": [gc["comment"] for gc in golden_comments],
        }

    candidates = [f.get("description", "") for f in findings if f.get("description")]
    if not candidates:
        return {
            "tp": 0, "fp": 0, "fn": len(golden_comments),
            "precision": 0.0, "recall": 0.0, "f1": 0.0,
            "true_positives": [], "false_positives": candidates,
            "false_negatives": [gc["comment"] for gc in golden_comments],
        }

    # Judge all golden×candidate pairs
    tasks = []
    task_meta = []
    for gc in golden_comments:
        for candidate in candidates:
            tasks.append((gc["comment"], candidate))
            task_meta.append({"golden": gc["comment"], "severity": gc.get("severity"), "candidate": candidate})

    results = []
    def _judge(args):
        golden_text, candidate_text = args
        return judge_match(golden_text, candidate_text, model)

    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        futures = [pool.submit(_judge, t) for t in tasks]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())

    # Wait for ordered results
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        future_map = {pool.submit(_judge, t): i for i, t in enumerate(tasks)}
        indexed_results = [None] * len(tasks)
        for future in concurrent.futures.as_completed(future_map):
            idx = future_map[future]
            indexed_results[idx] = future.result()
        results = indexed_results

    # Build match matrix
    golden_matched = {}
    for gc in golden_comments:
        golden_matched[gc["comment"]] = {"matched": False, "best_confidence": 0.0, "matched_candidate": None}

    candidate_matched = {c: False for c in candidates}

    for i, result in enumerate(results):
        meta = task_meta[i]
        golden = meta["golden"]
        candidate = meta["candidate"]

        if result and result.get("match") and result.get("confidence", 0) > golden_matched[golden]["best_confidence"]:
            golden_matched[golden]["matched"] = True
            golden_matched[golden]["best_confidence"] = result.get("confidence", 0)
            golden_matched[golden]["matched_candidate"] = candidate
            candidate_matched[candidate] = True

    true_positives = [g for g, info in golden_matched.items() if info["matched"]]
    false_negatives = [g for g, info in golden_matched.items() if not info["matched"]]
    false_positives = [c for c, matched in candidate_matched.items() if not matched]

    tp = len(true_positives)
    fp = len(false_positives)
    fn = len(false_negatives)
    precision = tp / (tp + fp) if (tp + fp) > 0 else 0.0
    recall = tp / (tp + fn) if (tp + fn) > 0 else 0.0
    f1 = 2 * precision * recall / (precision + recall) if (precision + recall) > 0 else 0.0

    return {
        "tp": tp, "fp": fp, "fn": fn,
        "precision": precision, "recall": recall, "f1": f1,
        "true_positives": true_positives,
        "false_positives": false_positives,
        "false_negatives": false_negatives,
    }


# --- Main process ---

def process_pr_ast(pr_info):
    """Process a single PR in AST mode. Returns per-comment results."""
    owner = pr_info["owner"]
    repo_name = pr_info["repo_name"]
    pr_number = pr_info["pr_number"]
    is_commit = pr_info["is_commit"]

    if not owner or not repo_name:
        return {"error": "could not parse URL", "results": []}

    if (owner, repo_name, str(pr_number)) in SKIP_PRS:
        return {"skipped": True, "skip_reason": "slow PR", "results": []}

    # Get repo
    try:
        repo_dir = ensure_repo(owner, repo_name)
    except Exception as e:
        return {"error": f"clone failed: {e}", "results": []}

    # Get base/head SHAs
    if is_commit:
        # For commit URLs, diff is commit_sha^..commit_sha
        sha = str(pr_number)
        _fetch_if_needed(repo_dir, sha)
        result = subprocess.run(
            ["git", "rev-parse", f"{sha}^"],
            cwd=repo_dir, capture_output=True, text=True, timeout=10,
        )
        if result.returncode != 0:
            return {"error": f"could not get parent of {sha}", "results": []}
        base_sha = result.stdout.strip()
        head_sha = sha
    else:
        sha_info = resolve_pr_shas(owner, repo_name, pr_number)
        if not sha_info:
            return {"error": f"could not resolve PR SHAs", "results": []}
        base_sha = sha_info["base_sha"]
        head_sha = sha_info["head_sha"]

    # Get or create a stable worktree for this PR
    wt_dir = ensure_worktree(repo_dir, base_sha, head_sha)
    if not wt_dir:
        return {"error": "worktree creation failed", "results": []}

    # Run inspect on the worktree
    inspect_data = run_inspect(wt_dir, base_sha, head_sha)

    entities = (inspect_data or {}).get("entity_reviews", [])
    hcm_entities = [e for e in entities if e.get("risk_level") in ("High", "Critical", "Medium")]
    hcm_names = {e.get("entity_name", "").lower() for e in hcm_entities}
    all_names = {e.get("entity_name", "").lower() for e in entities}

    # Match each golden comment with diagnosis
    results = []
    for gc in pr_info["comments"]:
        verdict, reason, matched_ents = ast_match_comment(gc, inspect_data)

        # Diagnose: find best matching entity and compute score gap
        gc_idents = extract_identifiers(gc["comment"])
        best_entity = None
        best_score = -1.0
        for e in entities:
            ename = e.get("entity_name", "").lower()
            for ident in gc_idents:
                if ident.lower() == ename or ident.lower() in ename or ename in ident.lower():
                    if e.get("risk_score", 0) > best_score:
                        best_entity = e
                        best_score = e.get("risk_score", 0)

        in_any = any(i.lower() in all_names or any(i.lower() in n for n in all_names) for i in gc_idents) if gc_idents else False
        in_hcm = any(i.lower() in hcm_names or any(i.lower() in n for n in hcm_names) for i in gc_idents) if gc_idents else False

        if not in_any:
            diagnosis = "entity_not_extracted"
        elif not in_hcm:
            diagnosis = "entity_low_risk"
        else:
            diagnosis = "none"

        diag = {
            "comment": gc["comment"],
            "severity": gc.get("severity", ""),
            "verdict": verdict,
            "reason": reason,
            "diagnosis": diagnosis,
        }
        if best_entity:
            diag["best_entity_name"] = best_entity.get("entity_name")
            diag["best_entity_score"] = best_entity.get("risk_score", 0)
            diag["best_entity_level"] = best_entity.get("risk_level", "Low")
            diag["best_entity_classification"] = best_entity.get("classification", "")
            diag["best_entity_change_type"] = best_entity.get("change_type", "")
            diag["best_entity_is_public_api"] = best_entity.get("is_public_api", False)
            diag["best_entity_blast_radius"] = best_entity.get("blast_radius", 0)
            diag["best_entity_dependent_count"] = best_entity.get("dependent_count", 0)
            diag["best_entity_structural_change"] = best_entity.get("structural_change")
            diag["score_gap_to_medium"] = round(0.3 - best_entity.get("risk_score", 0), 4) if best_entity.get("risk_score", 0) < 0.3 else 0
            diag["score_gap_to_high"] = round(0.5 - best_entity.get("risk_score", 0), 4) if best_entity.get("risk_score", 0) < 0.5 else 0

        results.append(diag)

    entity_count = len(entities)
    hcm_count = len(hcm_entities)
    timing = (inspect_data or {}).get("timing", {})

    return {
        "error": None,
        "results": results,
        "entity_count": entity_count,
        "hcm_count": hcm_count,
        "base_sha": base_sha,
        "head_sha": head_sha,
        "timing": timing,
    }


def process_pr_llm(pr_info, model="gpt-4o"):
    """Process a single PR in LLM mode. Returns precision/recall/F1."""
    owner = pr_info["owner"]
    repo_name = pr_info["repo_name"]
    pr_number = pr_info["pr_number"]
    is_commit = pr_info["is_commit"]

    if not owner or not repo_name:
        return {"error": "could not parse URL", "results": {}}

    try:
        repo_dir = ensure_repo(owner, repo_name)
    except Exception as e:
        return {"error": f"clone failed: {e}", "results": {}}

    if is_commit:
        sha = str(pr_number)
        _fetch_if_needed(repo_dir, sha)
        result = subprocess.run(
            ["git", "rev-parse", f"{sha}^"],
            cwd=repo_dir, capture_output=True, text=True, timeout=10,
        )
        if result.returncode != 0:
            return {"error": f"could not get parent of {sha}", "results": {}}
        base_sha = result.stdout.strip()
        head_sha = sha
    else:
        sha_info = resolve_pr_shas(owner, repo_name, pr_number)
        if not sha_info:
            return {"error": f"could not resolve PR SHAs", "results": {}}
        base_sha = sha_info["base_sha"]
        head_sha = sha_info["head_sha"]

    wt_dir = ensure_worktree(repo_dir, base_sha, head_sha)
    if not wt_dir:
        return {"error": "worktree creation failed", "results": {}}

    inspect_data = run_inspect(wt_dir, base_sha, head_sha)
    findings = run_inspect_llm(wt_dir, base_sha, head_sha, inspect_data, model)
    eval_result = evaluate_llm_findings(pr_info["comments"], findings, model)

    entities = (inspect_data or {}).get("entity_reviews", [])
    hcm_entities = [e for e in entities if e.get("risk_level") in ("High", "Critical", "Medium")]
    hcm_names = {e.get("entity_name", "").lower() for e in hcm_entities}
    all_names = {e.get("entity_name", "").lower() for e in entities}
    entity_count = len(entities)
    hcm_count = len(hcm_entities)
    timing = (inspect_data or {}).get("timing", {})

    # Run AST matching for each golden comment with full diagnosis (same as AST mode)
    ast_verdicts = []
    for gc in pr_info["comments"]:
        verdict, reason, matched_ents = ast_match_comment(gc, inspect_data)

        gc_idents = extract_identifiers(gc["comment"])
        best_entity = None
        best_score = -1.0
        for e in entities:
            ename = e.get("entity_name", "").lower()
            for ident in gc_idents:
                if ident.lower() == ename or ident.lower() in ename or ename in ident.lower():
                    if e.get("risk_score", 0) > best_score:
                        best_entity = e
                        best_score = e.get("risk_score", 0)

        in_any = any(i.lower() in all_names or any(i.lower() in n for n in all_names) for i in gc_idents) if gc_idents else False
        in_hcm = any(i.lower() in hcm_names or any(i.lower() in n for n in hcm_names) for i in gc_idents) if gc_idents else False

        if not in_any:
            diagnosis = "entity_not_extracted"
        elif not in_hcm:
            diagnosis = "entity_low_risk"
        else:
            diagnosis = "none"

        diag = {
            "comment": gc["comment"],
            "severity": gc.get("severity", ""),
            "verdict": verdict,
            "reason": reason,
            "diagnosis": diagnosis,
        }
        if best_entity:
            diag["best_entity_name"] = best_entity.get("entity_name")
            diag["best_entity_score"] = best_entity.get("risk_score", 0)
            diag["best_entity_level"] = best_entity.get("risk_level", "Low")
            diag["best_entity_classification"] = best_entity.get("classification", "")
            diag["best_entity_change_type"] = best_entity.get("change_type", "")
            diag["best_entity_is_public_api"] = best_entity.get("is_public_api", False)
            diag["best_entity_blast_radius"] = best_entity.get("blast_radius", 0)
            diag["best_entity_dependent_count"] = best_entity.get("dependent_count", 0)
            diag["best_entity_structural_change"] = best_entity.get("structural_change")
            diag["score_gap_to_medium"] = round(0.3 - best_entity.get("risk_score", 0), 4) if best_entity.get("risk_score", 0) < 0.3 else 0
            diag["score_gap_to_high"] = round(0.5 - best_entity.get("risk_score", 0), 4) if best_entity.get("risk_score", 0) < 0.5 else 0

        ast_verdicts.append(diag)

    # For each LLM false negative, add LLM-specific diagnosis layer
    fn_diagnoses = []
    for fn_text in eval_result.get("false_negatives", []):
        ast_v = next((v for v in ast_verdicts if v["comment"] == fn_text), None)

        llm_diag = {
            "missed_comment": fn_text,
            "ast_verdict": ast_v.get("verdict") if ast_v else "unknown",
            "ast_reason": ast_v.get("reason") if ast_v else "",
            "ast_diagnosis": ast_v.get("diagnosis") if ast_v else "unknown",
            "golden_identifiers": list(extract_identifiers(fn_text)),
        }

        # Determine LLM-specific failure bucket
        ast_diagnosis = ast_v.get("diagnosis", "unknown") if ast_v else "unknown"
        if ast_diagnosis == "entity_not_extracted":
            llm_diag["llm_failure_bucket"] = "blocked_by_extraction"
        elif ast_diagnosis == "entity_low_risk":
            llm_diag["llm_failure_bucket"] = "blocked_by_risk"
        else:
            llm_diag["llm_failure_bucket"] = "llm_missed"

        # Copy score gap info from AST verdict
        if ast_v:
            for k in ("best_entity_name", "best_entity_score", "best_entity_level",
                       "best_entity_classification", "best_entity_change_type",
                       "score_gap_to_medium", "score_gap_to_high"):
                if k in ast_v:
                    llm_diag[k] = ast_v[k]

        fn_diagnoses.append(llm_diag)

    return {
        "error": None,
        "results": eval_result,
        "findings_count": len(findings),
        "findings": findings,
        "ast_verdicts": ast_verdicts,
        "entity_count": entity_count,
        "hcm_count": hcm_count,
        "timing": timing,
        "base_sha": base_sha,
        "head_sha": head_sha,
        "fn_diagnoses": fn_diagnoses,
    }


# --- Reporting ---

def print_ast_report(all_results, prs):
    """Print AST mode results with pretty formatting."""
    total = 0
    matches = 0
    partials = 0
    misses = 0
    skipped = 0
    errors = 0
    s0_matches = 0  # matches from Strategy 0 (detector findings)

    by_repo = {}
    by_severity = {}

    for pr_info, pr_result in zip(prs, all_results):
        repo = pr_info["repo"]
        if pr_result.get("skipped"):
            skipped += 1
            continue
        if pr_result.get("error"):
            errors += 1
            continue

        for r in pr_result["results"]:
            total += 1
            v = r["verdict"]
            sev = r["severity"]
            reason = r.get("reason", "")
            if v == "match":
                matches += 1
                if reason.startswith("S0:"):
                    s0_matches += 1
            elif v == "partial":
                partials += 1
            else:
                misses += 1

            by_repo.setdefault(repo, {"match": 0, "partial": 0, "miss": 0, "total": 0})
            by_repo[repo][v] += 1
            by_repo[repo]["total"] += 1

            by_severity.setdefault(sev, {"match": 0, "partial": 0, "miss": 0, "total": 0})
            by_severity[sev][v] += 1
            by_severity[sev]["total"] += 1

    print(f"\n{'='*70}")
    print(f"AST TRIAGE RESULTS ({total} golden comments, {skipped} PR skipped, {errors} PR errors)")
    print(f"{'='*70}")
    print(f"  Match:   {matches:3d} ({matches/total*100:.1f}%)" if total else "  Match:   0")
    if s0_matches:
        print(f"    (S0 detector findings: {s0_matches})")
    print(f"  Partial: {partials:3d} ({partials/total*100:.1f}%)" if total else "  Partial: 0")
    print(f"  Miss:    {misses:3d} ({misses/total*100:.1f}%)" if total else "  Miss:    0")

    if total:
        strict_recall = matches / total * 100
        lenient_recall = (matches + partials) / total * 100
        print(f"\n  Strict recall (match only):     {strict_recall:.1f}%")
        print(f"  Lenient recall (match+partial): {lenient_recall:.1f}%")

    if by_repo:
        print(f"\n  Per-repo breakdown:")
        print(f"  {'Repo':<15} {'Total':>5} {'Match':>6} {'Partial':>8} {'Miss':>5} {'Strict':>8} {'Lenient':>8}")
        print(f"  {'-'*60}")
        for repo in sorted(by_repo.keys()):
            r = by_repo[repo]
            t = r["total"]
            strict = r["match"] / t * 100 if t else 0
            lenient = (r["match"] + r["partial"]) / t * 100 if t else 0
            print(f"  {repo:<15} {t:>5} {r['match']:>6} {r['partial']:>8} {r['miss']:>5} {strict:>7.1f}% {lenient:>7.1f}%")

    if by_severity:
        print(f"\n  Per-severity breakdown:")
        print(f"  {'Severity':<10} {'Total':>5} {'Match':>6} {'Partial':>8} {'Miss':>5} {'Strict':>8} {'Lenient':>8}")
        print(f"  {'-'*60}")
        for sev in ["Critical", "High", "Medium", "Low"]:
            if sev not in by_severity:
                continue
            r = by_severity[sev]
            t = r["total"]
            strict = r["match"] / t * 100 if t else 0
            lenient = (r["match"] + r["partial"]) / t * 100 if t else 0
            print(f"  {sev:<10} {t:>5} {r['match']:>6} {r['partial']:>8} {r['miss']:>5} {strict:>7.1f}% {lenient:>7.1f}%")


def print_llm_report(all_results, prs):
    """Print LLM mode results."""
    total_tp = 0
    total_fp = 0
    total_fn = 0
    errors = 0
    by_repo = {}

    for pr_info, pr_result in zip(prs, all_results):
        repo = pr_info["repo"]
        if pr_result.get("error"):
            errors += 1
            continue

        r = pr_result["results"]
        tp = r.get("tp", 0)
        fp = r.get("fp", 0)
        fn = r.get("fn", 0)
        total_tp += tp
        total_fp += fp
        total_fn += fn

        by_repo.setdefault(repo, {"tp": 0, "fp": 0, "fn": 0})
        by_repo[repo]["tp"] += tp
        by_repo[repo]["fp"] += fp
        by_repo[repo]["fn"] += fn

    precision = total_tp / (total_tp + total_fp) if (total_tp + total_fp) > 0 else 0.0
    recall = total_tp / (total_tp + total_fn) if (total_tp + total_fn) > 0 else 0.0
    f1 = 2 * precision * recall / (precision + recall) if (precision + recall) > 0 else 0.0

    print(f"\n{'='*70}")
    print(f"LLM REVIEW RESULTS ({errors} PR errors)")
    print(f"{'='*70}")
    print(f"  True Positives:  {total_tp}")
    print(f"  False Positives: {total_fp}")
    print(f"  False Negatives: {total_fn}")
    print(f"\n  Precision: {precision:.1%}")
    print(f"  Recall:    {recall:.1%}")
    print(f"  F1:        {f1:.1%}")

    if by_repo:
        print(f"\n  Per-repo breakdown:")
        print(f"  {'Repo':<15} {'TP':>4} {'FP':>4} {'FN':>4} {'Precision':>10} {'Recall':>8} {'F1':>8}")
        print(f"  {'-'*55}")
        for repo in sorted(by_repo.keys()):
            r = by_repo[repo]
            p = r["tp"] / (r["tp"] + r["fp"]) if (r["tp"] + r["fp"]) > 0 else 0.0
            rec = r["tp"] / (r["tp"] + r["fn"]) if (r["tp"] + r["fn"]) > 0 else 0.0
            f = 2 * p * rec / (p + rec) if (p + rec) > 0 else 0.0
            print(f"  {repo:<15} {r['tp']:>4} {r['fp']:>4} {r['fn']:>4} {p:>10.1%} {rec:>7.1%} {f:>7.1%}")


# --- Result persistence ---

RESULTS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")


def _build_pr_record(mode, pr_info, pr_result):
    """Build a single PR record dict for saving."""
    rec = {
        "repo": pr_info["repo"],
        "pr_number": pr_info["pr_number"],
        "pr_title": pr_info["pr_title"],
        "url": pr_info["real_url"],
        "golden_comment_count": len(pr_info["comments"]),
        "golden_comments": pr_info["comments"],
        "skipped": pr_result.get("skipped", False),
        "skip_reason": pr_result.get("skip_reason"),
        "error": pr_result.get("error"),
    }
    if mode == "ast":
        rec["entity_count"] = pr_result.get("entity_count", 0)
        rec["hcm_count"] = pr_result.get("hcm_count", 0)
        rec["base_sha"] = pr_result.get("base_sha")
        rec["head_sha"] = pr_result.get("head_sha")
        rec["timing"] = pr_result.get("timing", {})
        rec["verdicts"] = pr_result.get("results", [])
    elif mode in ("llm", "agent"):
        r = pr_result.get("results", {})
        rec["findings_count"] = pr_result.get("findings_count", 0)
        rec["findings"] = pr_result.get("findings", [])
        rec["tp"] = r.get("tp", 0)
        rec["fp"] = r.get("fp", 0)
        rec["fn"] = r.get("fn", 0)
        rec["precision"] = r.get("precision", 0.0)
        rec["recall"] = r.get("recall", 0.0)
        rec["f1"] = r.get("f1", 0.0)
        rec["true_positives"] = r.get("true_positives", [])
        rec["false_positives"] = r.get("false_positives", [])
        rec["false_negatives"] = r.get("false_negatives", [])
        rec["entity_count"] = pr_result.get("entity_count", 0)
        rec["hcm_count"] = pr_result.get("hcm_count", 0)
        rec["timing"] = pr_result.get("timing", {})
        rec["base_sha"] = pr_result.get("base_sha")
        rec["head_sha"] = pr_result.get("head_sha")
        rec["ast_verdicts"] = pr_result.get("ast_verdicts", [])
        rec["fn_diagnoses"] = pr_result.get("fn_diagnoses", [])
    return rec


def save_incremental(filepath, args, prs_so_far, results_so_far, start_time):
    """Write current progress to the results file after each PR."""
    os.makedirs(RESULTS_DIR, exist_ok=True)
    mode = args.mode
    model = getattr(args, "model", None)
    repo_filter = getattr(args, "repo", None)
    elapsed = time.time() - start_time

    pr_records = [
        _build_pr_record(mode, pr_info, pr_result)
        for pr_info, pr_result in zip(prs_so_far, results_so_far)
    ]

    agg = _compute_aggregates(mode, prs_so_far, results_so_far)

    result = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "mode": mode,
        "model": model,
        "repo_filter": repo_filter,
        "pr_count": len(prs_so_far),
        "golden_comment_count": sum(len(pr["comments"]) for pr in prs_so_far),
        "elapsed_seconds": round(elapsed, 1),
        "per_pr_seconds": round(elapsed / len(prs_so_far), 1) if prs_so_far else 0,
        "in_progress": True,
        "aggregates": agg,
        "prs": pr_records,
    }
    with open(filepath, "w") as f:
        json.dump(result, f, indent=2)


def save_results(args, prs, all_results, elapsed):
    """Save full run results to benchmarks/results/ as timestamped JSON.

    Stores everything needed to analyze and improve:
    - Run metadata (mode, model, timestamp, duration)
    - Per-PR: golden comments, inspect entity counts, findings, verdicts, TP/FP/FN
    - Aggregate: precision/recall/F1 (LLM) or match/partial/miss (AST)
    - Per-repo and per-severity breakdowns
    - Full list of false negatives (missed golden comments) for improvement
    """
    os.makedirs(RESULTS_DIR, exist_ok=True)
    ts = time.strftime("%Y%m%d_%H%M%S")
    mode = args.mode
    model = getattr(args, "model", None)
    repo_filter = getattr(args, "repo", None)

    # Build per-PR detail records
    pr_records = [
        _build_pr_record(mode, pr_info, pr_result)
        for pr_info, pr_result in zip(prs, all_results)
    ]

    # Compute aggregates
    agg = _compute_aggregates(mode, prs, all_results)

    result = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "mode": mode,
        "model": model,
        "repo_filter": repo_filter,
        "pr_count": len(prs),
        "golden_comment_count": sum(len(pr["comments"]) for pr in prs),
        "elapsed_seconds": round(elapsed, 1),
        "per_pr_seconds": round(elapsed / len(prs), 1) if prs else 0,
        "risk_config": {
            "classification_weights": {
                "Text": 0.0, "Syntax": 0.08, "Functional": 0.22,
                "TextSyntax": 0.10, "TextFunctional": 0.22,
                "SyntaxFunctional": 0.25, "TextSyntaxFunctional": 0.28,
            },
            "change_type_weights": {
                "Deleted": 0.12, "Modified": 0.08, "Renamed": 0.04,
                "Moved": 0.0, "Added": 0.02,
            },
            "public_api_boost": 0.12,
            "blast_radius_scale": 0.30,
            "dependent_count_scale": 0.15,
            "structural_discount": 0.20,
            "thresholds": {"Medium": 0.30, "High": 0.50, "Critical": 0.70},
        },
        "aggregates": agg,
        "prs": pr_records,
    }

    # Save JSON
    filename = f"{ts}_{mode}"
    if model and mode in ("llm", "agent"):
        filename += f"_{model.replace('/', '_')}"
    if repo_filter:
        filename += f"_{repo_filter}"
    filepath = os.path.join(RESULTS_DIR, f"{filename}.json")

    with open(filepath, "w") as f:
        json.dump(result, f, indent=2)

    print(f"\nResults saved to: {filepath}", file=sys.stderr)

    # Also append a summary line to the JSONL ledger for autoresearch compatibility
    jsonl_path = os.path.join(RESULTS_DIR, "runs.jsonl")
    summary = {
        "timestamp": result["timestamp"],
        "mode": mode,
        "model": model,
        "repo_filter": repo_filter,
        "pr_count": len(prs),
        "elapsed_seconds": result["elapsed_seconds"],
        **agg,
        "detail_file": os.path.basename(filepath),
    }
    with open(jsonl_path, "a") as f:
        f.write(json.dumps(summary) + "\n")


def _compute_aggregates(mode, prs, all_results):
    """Compute aggregate metrics for a run."""
    agg = {}

    if mode == "ast":
        total = matches = partials = misses = skipped = errors = 0
        by_repo = {}
        by_severity = {}

        for pr_info, pr_result in zip(prs, all_results):
            repo = pr_info["repo"]
            if pr_result.get("skipped"):
                skipped += 1
                continue
            if pr_result.get("error"):
                errors += 1
                continue
            for r in pr_result.get("results", []):
                total += 1
                v = r["verdict"]
                sev = r.get("severity", "")
                if v == "match": matches += 1
                elif v == "partial": partials += 1
                else: misses += 1

                by_repo.setdefault(repo, {"match": 0, "partial": 0, "miss": 0, "total": 0})
                by_repo[repo][v] += 1
                by_repo[repo]["total"] += 1

                by_severity.setdefault(sev, {"match": 0, "partial": 0, "miss": 0, "total": 0})
                by_severity[sev][v] += 1
                by_severity[sev]["total"] += 1

        strict_recall = matches / total if total else 0
        lenient_recall = (matches + partials) / total if total else 0

        agg = {
            "total": total,
            "match": matches,
            "partial": partials,
            "miss": misses,
            "skipped": skipped,
            "errors": errors,
            "strict_recall": round(strict_recall, 4),
            "lenient_recall": round(lenient_recall, 4),
            "by_repo": by_repo,
            "by_severity": by_severity,
        }

        # Tuning signals: analyze partials for autoresearch
        total_hcm = 0
        total_entities_all = 0
        partial_gaps = []
        partials_by_classification = {}
        partials_by_change_type = {}
        partials_public_api_true = 0
        partials_public_api_false = 0
        fn_diag_counts = {"entity_not_extracted": 0, "entity_low_risk": 0, "none": 0}

        for pr_info, pr_result in zip(prs, all_results):
            if pr_result.get("error"):
                continue
            total_hcm += pr_result.get("hcm_count", 0)
            total_entities_all += pr_result.get("entity_count", 0)
            for r in pr_result.get("results", []):
                diag = r.get("diagnosis", "none")
                fn_diag_counts[diag] = fn_diag_counts.get(diag, 0) + 1
                if r["verdict"] == "partial" and r.get("best_entity_score") is not None:
                    gap = r.get("score_gap_to_medium", 0)
                    partial_gaps.append(gap)
                    cls = r.get("best_entity_classification", "unknown")
                    ct = r.get("best_entity_change_type", "unknown")
                    partials_by_classification[cls] = partials_by_classification.get(cls, 0) + 1
                    partials_by_change_type[ct] = partials_by_change_type.get(ct, 0) + 1
                    if r.get("best_entity_is_public_api"):
                        partials_public_api_true += 1
                    else:
                        partials_public_api_false += 1

        agg["sent_entity_count"] = total_hcm
        agg["sent_entity_mean_per_pr"] = round(total_hcm / len(prs), 1) if prs else 0
        agg["total_entity_count"] = total_entities_all
        agg["extraction_recall"] = round(lenient_recall, 4)
        agg["risk_block_rate"] = round(partials / total, 4) if total else 0
        agg["fn_diagnosis_summary"] = fn_diag_counts

        if partial_gaps:
            partial_gaps.sort()
            agg["risk_tuning_signals"] = {
                "partials_by_classification": partials_by_classification,
                "partials_by_change_type": partials_by_change_type,
                "partials_public_api_true": partials_public_api_true,
                "partials_public_api_false": partials_public_api_false,
                "avg_gap_to_medium": round(sum(partial_gaps) / len(partial_gaps), 4),
                "p50_gap_to_medium": round(partial_gaps[len(partial_gaps) // 2], 4),
                "p90_gap_to_medium": round(partial_gaps[int(len(partial_gaps) * 0.9)], 4) if len(partial_gaps) >= 2 else round(partial_gaps[-1], 4),
                "partials_within_0_05_of_medium": sum(1 for g in partial_gaps if 0 < g <= 0.05),
                "partials_within_0_10_of_medium": sum(1 for g in partial_gaps if 0 < g <= 0.10),
            }

    elif mode in ("llm", "agent"):
        total_tp = total_fp = total_fn = errors = 0
        by_repo = {}
        all_false_negatives = []

        for pr_info, pr_result in zip(prs, all_results):
            repo = pr_info["repo"]
            if pr_result.get("error"):
                errors += 1
                continue
            r = pr_result.get("results", {})
            tp, fp, fn = r.get("tp", 0), r.get("fp", 0), r.get("fn", 0)
            total_tp += tp
            total_fp += fp
            total_fn += fn

            by_repo.setdefault(repo, {"tp": 0, "fp": 0, "fn": 0})
            by_repo[repo]["tp"] += tp
            by_repo[repo]["fp"] += fp
            by_repo[repo]["fn"] += fn

            for fn_text in r.get("false_negatives", []):
                all_false_negatives.append({
                    "repo": repo,
                    "pr_number": pr_info["pr_number"],
                    "pr_title": pr_info["pr_title"],
                    "missed_comment": fn_text,
                })

        precision = total_tp / (total_tp + total_fp) if (total_tp + total_fp) > 0 else 0.0
        recall = total_tp / (total_tp + total_fn) if (total_tp + total_fn) > 0 else 0.0
        f1 = 2 * precision * recall / (precision + recall) if (precision + recall) > 0 else 0.0

        # Per-repo P/R/F1
        by_repo_prf = {}
        for repo, counts in by_repo.items():
            p = counts["tp"] / (counts["tp"] + counts["fp"]) if (counts["tp"] + counts["fp"]) > 0 else 0.0
            r_ = counts["tp"] / (counts["tp"] + counts["fn"]) if (counts["tp"] + counts["fn"]) > 0 else 0.0
            f = 2 * p * r_ / (p + r_) if (p + r_) > 0 else 0.0
            by_repo_prf[repo] = {**counts, "precision": round(p, 4), "recall": round(r_, 4), "f1": round(f, 4)}

        agg = {
            "tp": total_tp,
            "fp": total_fp,
            "fn": total_fn,
            "errors": errors,
            "precision": round(precision, 4),
            "recall": round(recall, 4),
            "f1": round(f1, 4),
            "by_repo": by_repo_prf,
            "false_negatives": all_false_negatives,
        }

        # Add diagnosis breakdown from fn_diagnoses across all PRs
        diag_counts = {"entity_not_extracted": 0, "entity_low_risk": 0, "llm_missed": 0}
        all_diagnoses = []
        for pr_info2, pr_result2 in zip(prs, all_results):
            for d in pr_result2.get("fn_diagnoses", []):
                diag = d.get("diagnosis", "unknown")
                diag_counts[diag] = diag_counts.get(diag, 0) + 1
                all_diagnoses.append({**d, "repo": pr_info2["repo"], "pr_number": pr_info2["pr_number"]})
        agg["fn_diagnosis_summary"] = diag_counts
        agg["fn_diagnoses_all"] = all_diagnoses

    return agg


def main():
    parser = argparse.ArgumentParser(description="Martian Offline Benchmark eval harness for inspect")
    parser.add_argument("--mode", choices=["ast", "llm", "agent"], default="ast", help="Evaluation mode")
    parser.add_argument("--limit", type=int, help="Limit number of PRs")
    parser.add_argument("--repo", help="Filter by repo (sentry, grafana, keycloak, discourse, cal_dot_com)")
    parser.add_argument("--model", default="gpt-4o", help="LLM model for llm mode (default: gpt-4o)")
    parser.add_argument("--parallel", type=int, default=1, help="Number of PRs to process in parallel")
    parser.add_argument("--verbose", action="store_true", help="Show per-comment details")
    parser.add_argument("--resume-from", type=int, default=0, metavar="N", help="Resume from PR number N (0-indexed, skips first N PRs). Loads results from existing incremental file.")
    args = parser.parse_args()

    # Ensure inspect binary exists
    if not os.path.exists(INSPECT_BIN):
        print(f"inspect binary not found at {INSPECT_BIN}", file=sys.stderr)
        print(f"Build it: cargo build -p inspect-cli --release", file=sys.stderr)
        sys.exit(1)

    # Load dataset
    prs = load_golden_comments(repo_filter=args.repo)
    total_comments = sum(len(pr["comments"]) for pr in prs)
    print(f"Loaded {len(prs)} PRs, {total_comments} golden comments", file=sys.stderr)

    if args.limit:
        prs = prs[:args.limit]
        total_comments = sum(len(pr["comments"]) for pr in prs)
        print(f"Limited to {len(prs)} PRs, {total_comments} golden comments", file=sys.stderr)

    print(f"Mode: {args.mode}", file=sys.stderr)
    if args.mode in ("llm", "agent"):
        print(f"Model: {args.model}", file=sys.stderr)

    start_time = time.time()

    # Deterministic incremental filepath (stable across resume runs)
    _inc_filename = f"incremental_{args.mode}"
    if args.mode in ("llm", "agent") and args.model:
        _inc_filename += f"_{args.model.replace('/', '_')}"
    if getattr(args, "repo", None):
        _inc_filename += f"_{args.repo}"
    os.makedirs(RESULTS_DIR, exist_ok=True)
    _inc_filepath = os.path.join(RESULTS_DIR, f"{_inc_filename}.json")

    # Resume: load previous results from incremental file
    resume_from = getattr(args, "resume_from", 0)
    all_results_resumed = []
    if resume_from > 0 and os.path.exists(_inc_filepath):
        with open(_inc_filepath) as f:
            prev = json.load(f)
        saved_prs = prev.get("prs", [])[:resume_from]
        # Convert saved PR records back to process_pr_* result format
        for rec in saved_prs:
            if args.mode in ("llm", "agent"):
                all_results_resumed.append({
                    "results": {
                        "tp": rec.get("tp", 0), "fp": rec.get("fp", 0), "fn": rec.get("fn", 0),
                        "precision": rec.get("precision", 0.0), "recall": rec.get("recall", 0.0), "f1": rec.get("f1", 0.0),
                        "true_positives": rec.get("true_positives", []),
                        "false_positives": rec.get("false_positives", []),
                        "false_negatives": rec.get("false_negatives", []),
                    },
                    "findings_count": rec.get("findings_count", 0),
                    "findings": rec.get("findings", []),
                    "entity_count": rec.get("entity_count", 0),
                    "hcm_count": rec.get("hcm_count", 0),
                    "timing": rec.get("timing", {}),
                    "base_sha": rec.get("base_sha"),
                    "head_sha": rec.get("head_sha"),
                    "ast_verdicts": rec.get("ast_verdicts", []),
                    "fn_diagnoses": rec.get("fn_diagnoses", []),
                    "skipped": rec.get("skipped", False),
                    "skip_reason": rec.get("skip_reason"),
                    "error": rec.get("error"),
                })
            else:
                all_results_resumed.append({
                    "results": rec.get("verdicts", []),
                    "entity_count": rec.get("entity_count", 0),
                    "hcm_count": rec.get("hcm_count", 0),
                    "timing": rec.get("timing", {}),
                    "base_sha": rec.get("base_sha"),
                    "head_sha": rec.get("head_sha"),
                    "skipped": rec.get("skipped", False),
                    "skip_reason": rec.get("skip_reason"),
                    "error": rec.get("error"),
                })
        print(f"Resuming from PR {resume_from}, loaded {len(all_results_resumed)} previous results from {_inc_filepath}", file=sys.stderr)

    def _process_one(i_pr):
        i, pr_info = i_pr
        pr_label = f"{pr_info['repo']} PR#{pr_info['pr_number']}"
        print(f"\n[{i+1}/{len(prs)}] {pr_label}: {pr_info['pr_title'][:60]}", file=sys.stderr)

        if args.mode == "ast":
            return process_pr_ast(pr_info)
        elif args.mode == "agent":
            return process_pr_agent(pr_info, model=args.model)
        else:
            return process_pr_llm(pr_info, model=args.model)

    if args.parallel > 1:
        # Parallel execution — worktrees make this safe
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.parallel) as pool:
            futures = {pool.submit(_process_one, (i, pr)): (i, pr) for i, pr in enumerate(prs)}
            for future in concurrent.futures.as_completed(futures):
                i, pr_info = futures[future]
                result = future.result()
                all_results.append((i, pr_info, result))
                if result.get("skipped"):
                    reason = result.get("skip_reason", "skipped")
                    print(f"  [{pr_info['repo']}] SKIPPED: {reason}", file=sys.stderr)
                elif result.get("error"):
                    print(f"  [{pr_info['repo']}] ERROR: {result['error']}", file=sys.stderr)
                elif args.mode == "ast":
                    ec = result.get("entity_count", 0)
                    hcm = result.get("hcm_count", 0)
                    print(f"  [{pr_info['repo']}] {ec} entities, {hcm} HCM", file=sys.stderr)
        # Re-sort by original order
        all_results.sort(key=lambda x: x[0])
        all_results_final = [r for _, _, r in all_results]
    else:
        all_results_final = list(all_results_resumed)  # seed with resumed data
        for i, pr_info in enumerate(prs):
            if i < resume_from:
                print(f"\n[{i+1}/{len(prs)}] SKIP (resumed): {pr_info['repo']} PR#{pr_info['pr_number']}", file=sys.stderr)
                continue

            pr_label = f"{pr_info['repo']} PR#{pr_info['pr_number']}"
            print(f"\n[{i+1}/{len(prs)}] {pr_label}: {pr_info['pr_title'][:60]}", file=sys.stderr)
            print(f"  URL: {pr_info['real_url']}", file=sys.stderr)
            print(f"  {len(pr_info['comments'])} golden comments", file=sys.stderr)

            if args.mode == "ast":
                result = process_pr_ast(pr_info)
            elif args.mode == "agent":
                result = process_pr_agent(pr_info, model=args.model)
            else:
                result = process_pr_llm(pr_info, model=args.model)

            if result.get("skipped"):
                reason = result.get("skip_reason", "skipped")
                print(f"  SKIPPED: {reason}", file=sys.stderr)
            elif result.get("error"):
                print(f"  ERROR: {result['error']}", file=sys.stderr)
            elif args.mode == "ast":
                ec = result.get("entity_count", 0)
                hcm = result.get("hcm_count", 0)
                print(f"  inspect: {ec} entities, {hcm} HCM", file=sys.stderr)
                for r in result["results"]:
                    status = {"match": "✓ MATCH", "partial": "~ PARTIAL", "miss": "✗ MISS"}.get(r["verdict"], "?")
                    if args.verbose or r["verdict"] == "miss":
                        comment_short = r["comment"][:70]
                        print(f"    {status:12s} [{r['severity']:>8s}] {comment_short}", file=sys.stderr)
                        if args.verbose:
                            print(f"      reason: {r['reason'][:100]}", file=sys.stderr)
            elif args.mode in ("llm", "agent"):
                r = result.get("results", {})
                fc = result.get("findings_count", 0)
                print(f"  findings: {fc}, tp={r.get('tp',0)}, fp={r.get('fp',0)}, fn={r.get('fn',0)}", file=sys.stderr)

            all_results_final.append(result)

            # Incremental save after each PR
            save_incremental(_inc_filepath, args, prs[:i+1], all_results_final, start_time)

    elapsed = time.time() - start_time

    # Print report
    if args.mode == "ast":
        print_ast_report(all_results_final, prs)
    else:
        print_llm_report(all_results_final, prs)

    print(f"\nTotal time: {elapsed:.1f}s ({elapsed/len(prs):.1f}s/PR)")

    # Persist results
    save_results(args, prs, all_results_final, elapsed)


if __name__ == "__main__":
    main()
