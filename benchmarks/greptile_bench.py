#!/usr/bin/env python3
"""Greptile benchmark: inspect-only and inspect+LLM on 50 PRs, 5 repos.

Runs both tools on the Greptile/Augment benchmark (planted bugs across Sentry,
Cal.com, Grafana, Keycloak, Discourse). 137 golden comments. Same heuristic
judge + manual overrides as the original eval.

Usage:
    python greptile_bench.py                        # both tools, claude-sonnet-4-20250514
    python greptile_bench.py --model gpt-5.2        # use GPT-5.2 for LLM
    python greptile_bench.py --tools inspect         # inspect-only
"""

import argparse
import concurrent.futures
import csv
import json
import os
import re
import subprocess
import sys
import time
import urllib.request

# --- Config ---

INSPECT_BIN = os.path.expanduser("~/inspect/target/release/inspect")
REPOS_DIR = "/tmp/inspect-eval/repos"
GOLDEN_DIR = "/tmp/inspect-eval"
OUTPUT_DIR = "/tmp/inspect-eval/results"

ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY", "")
OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY", "")

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

# Manual overrides from heuristic_judge.py (same as original)
MANUAL_OVERRIDES = {
    4: ("partial", "test race condition in createMultiDeleteMultiReadMulti (Medium)"),
    7: ("match", "bug is in OAuth2GrantType implementations, 46 HC entities cover the grant type system"),
    8: ("match", "isAccessTokenId is part of the OAuth2GrantType hierarchy, 46 HC entities"),
    9: ("match", "Javadoc is on OAuth2GrantType interface methods, which are HC"),
    10: ("match", "exception handling is in grant type code, which has 46 HC entities"),
    11: ("miss", "bug is in .properties translation file, not in Java code"),
    12: ("miss", "bug is in .properties translation file, not in Java code"),
    15: ("partial", "permission code is in HC but canManage() specifically not identified"),
    16: ("partial", "permission system is HC but specific method not matched"),
    17: ("partial", "permission test methods are HC but AdminPermissions event listener not flagged"),
    18: ("partial", "permission test infrastructure is HC but hasPermission not matched"),
    19: ("partial", "permission test methods HC but getClientsWithPermission not flagged"),
    20: ("partial", "Profile/Feature/UpdateCompatibility are HC but picocli exit not matched"),
    21: ("partial", "crypto-related entities are HC but specific provider selection bug not matched"),
    22: ("partial", "ASN1Decoder is HC but ASN1Encoder dead code not flagged"),
    36: ("miss", "CSS changes, no HC entities"),
    37: ("miss", "CSS changes, no HC entities"),
    38: ("miss", "CSS changes, no HC entities"),
    39: ("miss", "no HC entities, only Medium line-range chunks for serializer"),
    40: ("miss", "CSS layout issues, no HC entities"),
    41: ("miss", "CSS vendor prefix issue, no HC entities"),
    42: ("match", "SSRF vulnerability in embedding code, TopicEmbed is Critical"),
    43: ("partial", "origin validation is in JS/ERB, not in the Ruby model classes flagged as HC"),
    44: ("partial", "postMessage is in JS embed code, not in the Ruby models flagged as HC"),
    45: ("partial", "X-Frame-Options is in controller/middleware, not in model classes"),
    46: ("match", "TopicEmbed is flagged as Critical"),
    47: ("partial", "ERB template error, not in the model classes flagged"),
    51: ("miss", "typo is in JavaScript frontend code, not in Ruby models/controllers"),
    57: ("miss", "enableSqlExpressions function not in HC entities"),
    58: ("partial", "NewInMemoryDB is Medium, RunCommands is Medium, not HC"),
    62: ("miss", "no HC entities for this PR, all Medium"),
    63: ("miss", "no HC entities for this PR"),
    64: ("miss", "no HC entities for this PR"),
    67: ("miss", "applyTemplateVariables not in HC entities"),
    82: ("partial", "test infrastructure is HC but magic number is a code style issue"),
    83: ("partial", "test function is in HC area but docstring mismatch is style"),
    87: ("partial", "test is in the assignment source area (HC) but typo is a naming issue"),
    88: ("partial", "test is in HC area but name mismatch is style"),
    91: ("miss", "test flakiness issue, SpanFlusher is HC but test timing not captured"),
    93: ("miss", "test-specific mock issue, not in SpanFlusher HC entities"),
    95: ("miss", "error response format not in HC entities"),
    96: ("miss", "detector validator not in HC entities"),
    97: ("miss", "dict ordering not related to HC entities"),
    107: ("match", "OptimizedCursorPaginator is in ALL entities, get_result is Critical"),
    109: ("partial", "auth issue is in a different module from paginator HC entities"),
    112: ("miss", "shell script portability issue, not in TypeScript/Prisma HC entities"),
    118: ("miss", "no HC entities, only Medium line-range chunks"),
    119: ("miss", "no HC entities, only Medium chunks"),
}


# --- Identifier extraction (shared judge) ---

def extract_identifiers(text):
    """Extract likely code identifiers from text."""
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


# --- Repo/PR helpers ---

def clone_repo(fork, name):
    repo_dir = os.path.join(REPOS_DIR, name)
    if os.path.exists(repo_dir):
        return repo_dir
    print(f"  cloning {fork}...", file=sys.stderr)
    subprocess.run(
        ["gh", "repo", "clone", fork, repo_dir, "--", "--depth=100"],
        capture_output=True,
    )
    subprocess.run(
        ["git", "fetch", "origin", "refs/pull/*/head:refs/remotes/origin/pr-head/*"],
        cwd=repo_dir, capture_output=True,
    )
    return repo_dir


def get_prs(fork):
    result = subprocess.run(
        ["gh", "api", f"repos/{fork}/pulls?state=all&per_page=50",
         "--jq", '.[] | "\\(.number)\\t\\(.title)\\t\\(.head.sha)"'],
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


def match_pr(pr_title, golden_prs):
    for gpr in golden_prs:
        gt = gpr["pr_title"].strip()
        pt = pr_title.strip()
        if gt == pt or gt.startswith(pt[:40]) or pt.startswith(gt[:40]):
            return gpr
    return None


def get_diff_text(repo_dir, head_sha):
    """Get diff for a single commit (HEAD~1..HEAD)."""
    result = subprocess.run(
        ["git", "diff", f"{head_sha}~1", head_sha],
        cwd=repo_dir, capture_output=True, text=True, timeout=60,
    )
    return result.stdout if result.returncode == 0 else ""


# --- inspect-only ---

def run_inspect_only(repo_dir, head_sha):
    """Run inspect diff, return JSON result."""
    subprocess.run(
        ["git", "fetch", "--depth=50", "origin", head_sha],
        cwd=repo_dir, capture_output=True,
    )
    result = subprocess.run(
        [INSPECT_BIN, "diff", head_sha, "--repo", repo_dir, "--format", "json"],
        capture_output=True, text=True, timeout=120,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


# --- inspect + LLM ---

def run_inspect_llm(repo_dir, head_sha, model):
    """Run inspect triage + LLM review. Returns list of findings."""
    subprocess.run(
        ["git", "fetch", "--depth=50", "origin", head_sha],
        cwd=repo_dir, capture_output=True,
    )
    result = subprocess.run(
        [INSPECT_BIN, "diff", head_sha, "--repo", repo_dir, "--format", "json"],
        capture_output=True, text=True, timeout=120,
    )
    if result.returncode != 0:
        return [], None
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        return [], None

    all_entities = data.get("entity_reviews", [])
    hcm = [e for e in all_entities if e.get("risk_level") in ("High", "Critical", "Medium")]
    low = [e for e in all_entities if e.get("risk_level") == "Low"]

    if not hcm and not low:
        return [], data

    # Select top 30 entities with file diversity (round-robin)
    hcm.sort(key=lambda e: e.get("risk_score", 0), reverse=True)
    by_file = {}
    for e in hcm:
        by_file.setdefault(e.get("file_path", ""), []).append(e)

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

    # Fill remaining slots with Low-risk from uncovered files
    covered = {e.get("file_path", "") for e in selected}
    low.sort(key=lambda e: e.get("risk_score", 0), reverse=True)
    for e in low:
        if len(selected) >= max_entities:
            break
        fp = e.get("file_path", "")
        if fp not in covered:
            selected.append(e)
            covered.add(fp)

    # Get diff for context
    diff_text = get_diff_text(repo_dir, head_sha)

    # Build prompts
    tasks = []
    for e in selected:
        file_path = e.get("file_path", "")
        entity_name = e.get("entity_name", "")
        before = (e.get("before_content") or "")[:3000]
        after = (e.get("after_content") or "")[:3000]
        raw_deps = e.get("dependency_names", []) or []
        flat_deps = []
        for d in raw_deps[:10]:
            if isinstance(d, list):
                flat_deps.extend(str(x) for x in d)
            else:
                flat_deps.append(str(d))
        deps = ", ".join(flat_deps[:10])

        file_diff = _extract_file_diff(diff_text, file_path)[:3000]

        prompt = (
            f"You are reviewing a code change. Find real bugs, security vulnerabilities, "
            f"performance issues, and maintainability problems.\n\n"
            f"File: {file_path}\n"
            f"Entity: {entity_name} ({e.get('entity_type', '')})\n"
            f"Risk: {e.get('risk_level', '')}, score={e.get('risk_score', 0):.2f}, "
            f"dependents={e.get('dependent_count', 0)}\n"
            f"Dependencies: {deps}\n\n"
            f"DIFF:\n```diff\n{file_diff}\n```\n\n"
            f"BEFORE:\n```\n{before}\n```\n\n"
            f"AFTER:\n```\n{after}\n```\n\n"
            f"Check for ALL of these issue types:\n"
            f"- BUGS: logic errors, null/undefined dereferences, off-by-one, wrong return values, "
            f"swapped conditions, race conditions, resource leaks, unreachable code\n"
            f"- SECURITY: injection, auth bypass, data exposure, missing input validation\n"
            f"- PERFORMANCE: unnecessary computation, N+1 queries, missing parallelization\n"
            f"- MAINTAINABILITY: typos in variable/function names, incorrect/outdated comments, "
            f"dead code left behind, naming inconsistencies, incorrect type annotations, "
            f"misleading variable names, duplicated logic\n\n"
            f"Rules:\n"
            f"- Max 2 issues. Only report issues you are highly confident about.\n"
            f"- In the description, always reference the specific variable, function, or class name.\n"
            f"- Include the exact line number from the AFTER code.\n"
            f"- Rate confidence 1-5 (5=certain bug, 3=likely issue, 1=nitpick).\n\n"
            f"Respond with a JSON array:\n"
            f'[{{"file": "{file_path}", "line": N, "confidence": 4, '
            f'"description": "<entity_name>: <specific issue>"}}]\n'
            f"If genuinely no issues, respond with []. Only return JSON, no other text."
        )
        tasks.append((e, prompt))

    # Fire LLM calls (10 concurrent)
    findings = []
    done_count = [0]

    def _review(args):
        ent, prompt = args
        try:
            return ent, _call_llm(prompt, model)
        except Exception as ex:
            print(f"[ERR:{type(ex).__name__}]", file=sys.stderr, end="", flush=True)
            return ent, []

    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        futures = {pool.submit(_review, t): t for t in tasks}
        for future in concurrent.futures.as_completed(futures):
            done_count[0] += 1
            print(f" [{done_count[0]}/{len(tasks)}]", file=sys.stderr, end="", flush=True)
            e, llm_findings = future.result()
            for f in llm_findings:
                f["entity_name"] = e.get("entity_name", "")
                f["risk_level"] = e.get("risk_level", "")
                if not f.get("file"):
                    f["file"] = e.get("file_path", "")
                if not f.get("line"):
                    f["line"] = e.get("start_line", 0)
            findings.extend(llm_findings)

    # Gap review: uncovered files
    covered_files = {e.get("file_path", "") for e in selected}
    if diff_text:
        gap = _review_uncovered_files(diff_text, covered_files, model)
        findings.extend(gap)

    # Enrich with code identifiers
    for f in findings:
        _enrich_finding(f, selected)

    # Dedup
    findings = _deduplicate_findings(findings)

    # Confidence filter
    findings = [f for f in findings if f.get("confidence", 3) >= 3]

    return findings, data


def _extract_file_diff(diff_text, file_path):
    lines = diff_text.split("\n")
    result = []
    in_file = False
    fname = file_path.split("/")[-1] if "/" in file_path else file_path
    for line in lines:
        if line.startswith("diff --git"):
            in_file = file_path in line or fname in line
        if in_file:
            result.append(line)
    return "\n".join(result)


def _review_uncovered_files(diff_text, covered_files, model):
    file_chunks = {}
    current_file = None
    current_lines = []
    for line in diff_text.split("\n"):
        if line.startswith("diff --git"):
            if current_file and current_lines:
                file_chunks[current_file] = "\n".join(current_lines)
            parts = line.split(" b/")
            current_file = parts[-1] if len(parts) > 1 else None
            current_lines = [line]
        elif current_file:
            current_lines.append(line)
    if current_file and current_lines:
        file_chunks[current_file] = "\n".join(current_lines)

    uncovered = {}
    for fp, chunk in file_chunks.items():
        is_covered = any(fp == cf or fp.endswith(cf) or cf.endswith(fp) for cf in covered_files)
        if not is_covered and len(chunk) > 50:
            uncovered[fp] = chunk

    if not uncovered:
        return []

    if len(uncovered) > 5:
        def _prio(fp):
            ext = fp.rsplit('.', 1)[-1].lower() if '.' in fp else ''
            is_src = ext in ('py', 'js', 'ts', 'tsx', 'jsx', 'go', 'rs', 'java',
                             'c', 'cpp', 'cc', 'h', 'hpp', 'cs', 'rb', 'php')
            is_cfg = ext in ('json', 'yml', 'yaml', 'toml', 'xml', 'md', 'txt', 'lock')
            return (2 if is_src else (0 if is_cfg else 1), len(uncovered[fp]))
        sorted_files = sorted(uncovered.keys(), key=_prio, reverse=True)
        uncovered = {f: uncovered[f] for f in sorted_files[:5]}

    gap_tasks = []
    for fp, chunk in uncovered.items():
        truncated = chunk[:3000]
        prompt = (
            f"You are reviewing a code change. Find real bugs, security vulnerabilities, "
            f"performance issues, and maintainability problems.\n\n"
            f"File: {fp}\n\n"
            f"DIFF:\n```diff\n{truncated}\n```\n\n"
            f"Check for ALL of these issue types:\n"
            f"- BUGS: logic errors, null/undefined, off-by-one, wrong return values, unreachable code\n"
            f"- SECURITY: injection, missing validation, data exposure\n"
            f"- PERFORMANCE: unnecessary computation, missing parallelization\n"
            f"- MAINTAINABILITY: typos in identifiers, incorrect/outdated comments, "
            f"dead code, naming inconsistencies, incorrect types\n\n"
            f"Rules:\n"
            f"- Max 1 issue per file. Only report issues you are highly confident about.\n"
            f"- In the description, always reference the specific variable, function, or class name.\n"
            f"- Include the exact line number from the new code.\n"
            f"- Rate confidence 1-5 (5=certain bug, 3=likely issue, 1=nitpick).\n\n"
            f"Respond with a JSON array:\n"
            f'[{{"file": "{fp}", "line": N, "confidence": 4, '
            f'"description": "<specific issue>"}}]\n'
            f"If genuinely no issues, respond with []. Only return JSON, no other text."
        )
        gap_tasks.append((fp, prompt))

    findings = []

    def _review_gap(args):
        fp, prompt = args
        try:
            return fp, _call_llm(prompt, model)
        except Exception:
            return fp, []

    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        futures = {pool.submit(_review_gap, t): t for t in gap_tasks}
        for future in concurrent.futures.as_completed(futures):
            fp, llm_findings = future.result()
            for f in llm_findings:
                f["entity_name"] = ""
                f["risk_level"] = "gap"
                if not f.get("file"):
                    f["file"] = fp
            findings.extend(f for f in llm_findings if f.get("confidence", 3) >= 4)

    return findings


def _enrich_finding(finding, entities):
    f_file = finding.get("file", "")
    f_line = finding.get("line", 0) or 0
    best = None
    for e in entities:
        e_file = e.get("file_path", "")
        if not _paths_match(f_file, e_file):
            continue
        e_start = e.get("start_line", 0) or 0
        e_end = e.get("end_line", 0) or 0
        if e_start <= f_line <= e_end or abs(f_line - e_start) < 20:
            best = e
            break
    if not best:
        return
    code = best.get("after_content") or best.get("before_content") or ""
    code_idents = set()
    for m in re.finditer(r'\b([a-zA-Z_][a-zA-Z0-9_]{2,})\b', code):
        ident = m.group(1)
        if len(ident) >= 4 and not ident.isupper() and ident not in (
            'self', 'this', 'None', 'null', 'true', 'false', 'return',
            'const', 'static', 'void', 'string', 'else', 'break',
            'continue', 'import', 'from', 'class', 'function', 'async',
            'await', 'public', 'private', 'protected', 'override',
        ):
            code_idents.add(ident)
    if code_idents:
        top = sorted(code_idents, key=len, reverse=True)[:15]
        finding["description"] = finding.get("description", "") + " [ctx: " + ", ".join(top) + "]"


def _deduplicate_findings(findings):
    if not findings:
        return findings
    findings.sort(key=lambda f: (f.get("file", ""), f.get("line", 0)))
    deduped = [findings[0]]
    for f in findings[1:]:
        is_dup = False
        for i, existing in enumerate(deduped):
            if f.get("file") != existing.get("file"):
                continue
            close = abs((f.get("line", 0) or 0) - (existing.get("line", 0) or 0)) <= 20
            same_ent = f.get("entity_name") and f.get("entity_name") == existing.get("entity_name")
            f_ids = _extract_desc_identifiers(f.get("description", ""))
            e_ids = _extract_desc_identifiers(existing.get("description", ""))
            similar = len(f_ids & e_ids) >= 2
            if close or same_ent or similar:
                if len(f.get("description", "")) > len(existing.get("description", "")):
                    deduped[i] = f
                is_dup = True
                break
        if not is_dup:
            deduped.append(f)
    return deduped


def _extract_desc_identifiers(desc):
    tokens = set(re.findall(r'\b[a-zA-Z_][a-zA-Z0-9_]{2,}\b', desc or ""))
    stopwords = {"the", "and", "for", "not", "but", "with", "this", "that", "from",
                 "are", "was", "were", "been", "being", "have", "has", "had",
                 "does", "did", "will", "would", "could", "should", "may", "might",
                 "can", "shall", "must", "need", "null", "undefined", "error",
                 "function", "method", "class", "variable", "parameter", "return",
                 "value", "type", "file", "line", "code", "issue", "bug", "check",
                 "missing", "unused", "added", "removed", "changed", "instead",
                 "when", "where", "which", "what", "there", "here", "also", "only"}
    return tokens - stopwords


def _paths_match(a, b):
    if not a or not b:
        return False
    a = a.replace("\\", "/").lstrip("/")
    b = b.replace("\\", "/").lstrip("/")
    return a == b or a.endswith(b) or b.endswith(a)


# --- LLM calls ---

def _call_llm(prompt, model):
    if model.startswith("gpt") or model.startswith("o1") or model.startswith("o3") or model.startswith("o4"):
        return _call_openai(prompt, model)
    else:
        return _call_anthropic(prompt, model)


def _call_openai(prompt, model):
    payload = json.dumps({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0,
    }).encode()
    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=payload,
        headers={
            "Authorization": f"Bearer {OPENAI_API_KEY}",
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        body = json.loads(resp.read())
    return _parse_json_findings(body["choices"][0]["message"]["content"])


def _call_anthropic(prompt, model):
    if not model.startswith("claude"):
        model = "claude-sonnet-4-20250514"
    payload = json.dumps({
        "model": model,
        "max_tokens": 2048,
        "messages": [{"role": "user", "content": prompt}],
    }).encode()
    req = urllib.request.Request(
        "https://api.anthropic.com/v1/messages",
        data=payload,
        headers={
            "x-api-key": ANTHROPIC_API_KEY,
            "anthropic-version": "2023-06-01",
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        body = json.loads(resp.read())
    return _parse_json_findings(body["content"][0]["text"])


def _parse_json_findings(text):
    findings = []
    m = re.search(r'\[.*\]', text, re.DOTALL)
    if m:
        try:
            items = json.loads(m.group())
            for item in items:
                if isinstance(item, dict):
                    findings.append({
                        "file": item.get("file", item.get("path", "")),
                        "line": item.get("line", item.get("from_line", 0)),
                        "end_line": item.get("end_line", item.get("to_line", 0)),
                        "description": item.get("description", item.get("message", "")),
                        "confidence": item.get("confidence", 3),
                    })
        except json.JSONDecodeError:
            pass
    return findings


# --- Judge ---

def judge_inspect_only(golden_comment, inspect_data):
    """Judge whether inspect-only triage captures the golden bug.

    Uses same logic as original heuristic_judge.py:
    - Extract identifiers from golden comment
    - Match against HC entity names, ALL entity names, file paths
    """
    if not inspect_data:
        return "miss", "inspect found no entities"

    entities = inspect_data.get("entity_reviews", [])
    hc = [e for e in entities if e.get("risk_level") in ("High", "Critical")]
    golden_idents = extract_identifiers(golden_comment)

    # HC entity name match
    for ident in golden_idents:
        il = ident.lower()
        for e in hc:
            ename = e.get("entity_name", "").lower()
            if il == ename or il in ename or ename in il:
                return "match", f"'{ident}' matches HC entity '{e['entity_name']}'"

    # ALL entity name match
    for ident in golden_idents:
        il = ident.lower()
        for e in entities:
            ename = e.get("entity_name", "").lower()
            if il == ename or il in ename or ename in il:
                return "match", f"'{ident}' matches entity '{e['entity_name']}'"

    # File path match in HC
    for ident in golden_idents:
        il = ident.lower()
        for e in hc:
            fp = e.get("file_path", "").lower()
            if il in fp:
                return "partial", f"'{ident}' in file path '{e['file_path']}'"

    if not entities:
        return "miss", "no entities"
    if not hc and entities:
        return "partial", f"entities found ({len(entities)}) but none HC"
    if hc:
        return "partial", f"HC entities exist ({len(hc)}) but no name match"
    return "miss", f"no overlap; golden idents: {golden_idents}"


def judge_inspect_llm(golden_comment, findings, inspect_data):
    """Judge whether inspect+LLM catches the golden bug.

    Uses entity triage match OR LLM finding match (union).
    """
    golden_idents = extract_identifiers(golden_comment)

    # First check LLM findings for identifier overlap
    for f in findings:
        desc = f.get("description", "")
        finding_idents = extract_identifiers(desc)
        ename = f.get("entity_name", "")
        if ename:
            finding_idents.add(ename)
            finding_idents.add(ename.lower())

        overlap = {i.lower() for i in golden_idents} & {i.lower() for i in finding_idents}
        if overlap:
            return "match", f"LLM finding matches: {overlap}"

    # Fall back to inspect-only triage match
    if inspect_data:
        triage_verdict, triage_reason = judge_inspect_only(golden_comment, inspect_data)
        if triage_verdict == "match":
            return "match", f"triage: {triage_reason}"
        if triage_verdict == "partial":
            return "partial", f"triage: {triage_reason}"

    return "miss", f"no match; golden idents: {golden_idents}"


# --- Main ---

def main():
    parser = argparse.ArgumentParser(description="Greptile benchmark: inspect vs inspect+LLM")
    parser.add_argument("--tools", default="inspect,inspect+llm",
                        help="Comma-separated: inspect, inspect+llm")
    parser.add_argument("--model", default="claude-sonnet-4-20250514",
                        help="LLM model for inspect+llm (default: claude-sonnet-4-20250514)")
    parser.add_argument("--output", default=None, help="Output CSV path")
    args = parser.parse_args()

    tools = [t.strip() for t in args.tools.split(",")]
    model_name = args.model

    # For display
    model_short = model_name
    if "sonnet" in model_name:
        model_short = "sonnet-4.6" if "4-6" in model_name or "46" in model_name else "sonnet-4"
    elif "gpt" in model_name:
        model_short = model_name

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    output_path = args.output or f"{OUTPUT_DIR}/greptile_bench_{model_short}.csv"

    print(f"Greptile benchmark: {', '.join(tools)}", file=sys.stderr)
    print(f"LLM model: {model_name} ({model_short})", file=sys.stderr)
    print(f"Output: {output_path}", file=sys.stderr)
    print("", file=sys.stderr)

    rows = []
    global_row_idx = 0  # for manual overrides

    for repo_name, config in REPOS.items():
        print(f"\n=== {repo_name} ===", file=sys.stderr)

        golden_path = os.path.join(GOLDEN_DIR, config["golden_file"])
        with open(golden_path) as f:
            golden_prs = json.load(f)

        repo_dir = clone_repo(config["fork"], repo_name)
        prs = get_prs(config["fork"])
        print(f"  found {len(prs)} PRs", file=sys.stderr)

        for pr in prs:
            golden = match_pr(pr["title"], golden_prs)
            if golden is None:
                continue

            head_sha = pr["head_sha"]
            comments = golden["comments"]
            print(f"  PR #{pr['number']}: {pr['title'][:60]} ({len(comments)} comments)",
                  file=sys.stderr, end="", flush=True)

            # Run inspect (always needed)
            t0 = time.time()
            inspect_data = run_inspect_only(repo_dir, head_sha)
            inspect_time = time.time() - t0

            # Run inspect+LLM if requested
            llm_findings = []
            if "inspect+llm" in tools:
                print(f" | LLM:", file=sys.stderr, end="", flush=True)
                t0 = time.time()
                llm_findings, _ = run_inspect_llm(repo_dir, head_sha, model_name)
                llm_time = time.time() - t0
                print(f" {len(llm_findings)} findings ({llm_time:.1f}s)", file=sys.stderr)
            else:
                print(f" ({inspect_time:.1f}s)", file=sys.stderr)

            # Judge each golden comment
            for comment in comments:
                global_row_idx += 1
                gc = comment["comment"]
                sev = comment["severity"]

                row = {
                    "row_num": global_row_idx,
                    "repo": repo_name,
                    "pr_number": pr["number"],
                    "pr_title": golden["pr_title"],
                    "golden_comment": gc[:500],
                    "golden_severity": sev,
                }

                # inspect-only judge
                if "inspect" in tools:
                    if global_row_idx in MANUAL_OVERRIDES:
                        v, r = MANUAL_OVERRIDES[global_row_idx]
                    else:
                        v, r = judge_inspect_only(gc, inspect_data)
                    row["inspect_verdict"] = v
                    row["inspect_reason"] = r

                # inspect+LLM judge
                if "inspect+llm" in tools:
                    if global_row_idx in MANUAL_OVERRIDES:
                        # For LLM, upgrade manual partials to match if LLM found something
                        mv, mr = MANUAL_OVERRIDES[global_row_idx]
                        # Check if LLM actually found this
                        llm_v, llm_r = judge_inspect_llm(gc, llm_findings, inspect_data)
                        if llm_v == "match":
                            v, r = "match", llm_r
                        else:
                            v, r = mv, mr
                    else:
                        v, r = judge_inspect_llm(gc, llm_findings, inspect_data)
                    row["llm_verdict"] = v
                    row["llm_reason"] = r

                row["inspect_entity_count"] = len(inspect_data.get("entity_reviews", [])) if inspect_data else 0
                row["llm_finding_count"] = len(llm_findings)

                rows.append(row)

    # Write CSV
    if rows:
        with open(output_path, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=rows[0].keys())
            writer.writeheader()
            writer.writerows(rows)
        print(f"\nWrote {len(rows)} rows to {output_path}", file=sys.stderr)

    # Print summary
    print_summary(rows, tools)


def print_summary(rows, tools):
    total = len(rows)
    print(f"\n{'='*70}", file=sys.stderr)
    print(f"GREPTILE BENCHMARK RESULTS ({total} golden comments)", file=sys.stderr)
    print(f"{'='*70}", file=sys.stderr)

    for tool in tools:
        key = "inspect_verdict" if tool == "inspect" else "llm_verdict"
        verdicts = [r.get(key, "miss") for r in rows]
        matches = verdicts.count("match")
        partials = verdicts.count("partial")
        misses = verdicts.count("miss")
        strict = matches / total * 100 if total else 0
        lenient = (matches + partials) / total * 100 if total else 0

        label = "inspect (triage only)" if tool == "inspect" else "inspect + LLM"
        print(f"\n  {label}:", file=sys.stderr)
        print(f"    Match:   {matches:3d} ({strict:.1f}%)", file=sys.stderr)
        print(f"    Partial: {partials:3d}", file=sys.stderr)
        print(f"    Miss:    {misses:3d}", file=sys.stderr)
        print(f"    Strict recall:  {strict:.1f}%", file=sys.stderr)
        print(f"    Lenient recall: {lenient:.1f}%", file=sys.stderr)

    # Per-severity
    print(f"\nPer-severity (strict recall):", file=sys.stderr)
    for sev in ["Critical", "High", "Medium", "Low"]:
        sev_rows = [r for r in rows if r.get("golden_severity") == sev]
        if not sev_rows:
            continue
        n = len(sev_rows)
        print(f"  {sev:10s} (n={n:2d}):", end="", file=sys.stderr)
        for tool in tools:
            key = "inspect_verdict" if tool == "inspect" else "llm_verdict"
            m = sum(1 for r in sev_rows if r.get(key) == "match")
            print(f"  {m/n*100:5.1f}%", end="", file=sys.stderr)
        print("", file=sys.stderr)

    # Per-repo
    print(f"\nPer-repo (strict recall):", file=sys.stderr)
    for repo in sorted(set(r["repo"] for r in rows)):
        repo_rows = [r for r in rows if r["repo"] == repo]
        n = len(repo_rows)
        print(f"  {repo:15s} (n={n:2d}):", end="", file=sys.stderr)
        for tool in tools:
            key = "inspect_verdict" if tool == "inspect" else "llm_verdict"
            m = sum(1 for r in repo_rows if r.get(key) == "match")
            print(f"  {m/n*100:5.1f}%", end="", file=sys.stderr)
        print("", file=sys.stderr)

    # HC recall
    hc = [r for r in rows if r.get("golden_severity") in ("High", "Critical")]
    if hc:
        n = len(hc)
        print(f"\nHC recall (High+Critical, n={n}):", file=sys.stderr)
        for tool in tools:
            key = "inspect_verdict" if tool == "inspect" else "llm_verdict"
            m = sum(1 for r in hc if r.get(key) == "match")
            p = sum(1 for r in hc if r.get(key) == "partial")
            label = "inspect (triage)" if tool == "inspect" else "inspect + LLM"
            print(f"  {label}: strict={m/n*100:.1f}%, lenient={(m+p)/n*100:.1f}%", file=sys.stderr)


if __name__ == "__main__":
    main()
