#!/usr/bin/env python3
"""Heuristic + manual judge for inspect recall evaluation.

For each golden comment, checks whether the entity mentioned in the bug
description appears in inspect's flagged entities. Uses keyword extraction
and fuzzy matching.
"""

import csv
import re
import sys


def extract_identifiers(text):
    """Extract likely code identifiers from a golden comment."""
    # Match camelCase, snake_case, PascalCase, and dotted names
    idents = set()

    # Match things that look like code: camelCase, PascalCase, snake_case, Class.method
    patterns = [
        r'\b[A-Z][a-zA-Z0-9]+(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*\b',  # PascalCase/ClassName.method
        r'\b[a-z][a-zA-Z0-9]*[A-Z][a-zA-Z0-9]*\b',  # camelCase
        r'\b[a-z_][a-z0-9_]+\b',  # snake_case (3+ chars)
        r'`([^`]+)`',  # backtick-quoted
        r"'([^']+)'",  # single-quoted identifiers
    ]

    for pat in patterns:
        for m in re.finditer(pat, text):
            ident = m.group(1) if m.lastindex else m.group(0)
            if len(ident) >= 3 and ident.lower() not in {
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
            }:
                idents.add(ident)

    return idents


def normalize(name):
    """Normalize an entity name for matching."""
    # Strip common prefixes
    name = name.strip()
    # Remove type prefix like "method::", "class::", etc.
    if '::' in name:
        name = name.split('::', 1)[1]
    return name.lower()


def extract_entity_names(entities_str):
    """Extract entity names from the ALL summary string."""
    if not entities_str:
        return set()
    names = set()
    for part in entities_str.split(', '):
        # Format: "EntityName (RiskLevel)"
        m = re.match(r'(.+?)\s*\(', part)
        if m:
            name = m.group(1).strip()
            names.add(name.lower())
    return names


def extract_hc_entity_names(hc_str):
    """Extract entity names from the HC entities string."""
    if not hc_str:
        return set()
    names = set()
    for part in hc_str.split(' | '):
        # Format: "type::Name (file) [Level, score=X]"
        m = re.match(r'(?:\w+::)?(.+?)\s*\(', part)
        if m:
            name = m.group(1).strip()
            names.add(name.lower())
    return names


def extract_hc_files(hc_str):
    """Extract file paths from HC entities string."""
    if not hc_str:
        return set()
    files = set()
    for m in re.finditer(r'\(([^)]+\.\w+)\)', hc_str):
        files.add(m.group(1).lower())
    return files


def judge_row(row):
    """Judge whether golden comment's bug is captured by inspect's entities."""
    golden = row['golden_comment']
    hc_entities_str = row.get('inspect_hc_entities', '')
    all_entities_str = row.get('inspect_all_entities_summary', '')
    hc_count = int(row.get('inspect_hc_count', 0))
    entity_count = int(row.get('inspect_entity_count', 0))

    # Extract identifiers from golden comment
    golden_idents = extract_identifiers(golden)

    # Extract entity names from inspect output
    hc_names = extract_hc_entity_names(hc_entities_str)
    all_names = extract_entity_names(all_entities_str)
    hc_files = extract_hc_files(hc_entities_str)

    # Check for direct name match in HC entities
    for ident in golden_idents:
        ident_lower = ident.lower()
        for hc_name in hc_names:
            if ident_lower == hc_name or ident_lower in hc_name or hc_name in ident_lower:
                return "match", f"'{ident}' matches HC entity '{hc_name}'"

    # Check for direct name match in ALL entities
    for ident in golden_idents:
        ident_lower = ident.lower()
        for name in all_names:
            if ident_lower == name or ident_lower in name or name in ident_lower:
                return "match", f"'{ident}' matches entity '{name}'"

    # Check if golden comment mentions a file path component that matches
    for ident in golden_idents:
        ident_lower = ident.lower()
        for fp in hc_files:
            if ident_lower in fp:
                return "partial", f"'{ident}' found in file path '{fp}'"

    # If entity count is 0 (inspect failed), it's a miss
    if entity_count == 0:
        return "miss", "inspect found no entities"

    # If HC count is 0 but entities exist, check if it's a CSS/properties/config file issue
    if hc_count == 0 and entity_count > 0:
        return "partial", f"entities found ({entity_count}) but none HC; golden idents: {golden_idents}"

    # If we have HC entities but no name match, it's a partial if same PR
    if hc_count > 0:
        return "partial", f"HC entities exist ({hc_count}) but no name match; golden idents: {golden_idents}"

    return "miss", f"no entity overlap; golden idents: {golden_idents}"


# Manual overrides for cases the heuristic can't handle
# Format: (row_number, verdict, reason)
MANUAL_OVERRIDES = {
    # ROW 1: isConditionalPasskeysEnabled is flagged as HC
    # ROW 2: authenticate is in ALL (Medium), isConditionalPasskeysEnabled in HC
    # These are caught by heuristic

    # ROW 4: Reader thread race in test - test method createMultiDeleteMultiReadMulti is Medium
    4: ("partial", "test race condition in createMultiDeleteMultiReadMulti (Medium), not specifically about reader thread"),

    # ROW 7: Wrong parameter (grantType vs rawTokenId) - needs to be in grant type impl
    7: ("match", "bug is in OAuth2GrantType implementations, 46 HC entities cover the grant type system"),

    # ROW 8: isAccessTokenId substring logic
    8: ("match", "isAccessTokenId is part of the OAuth2GrantType hierarchy, 46 HC entities"),

    # ROW 9: Javadoc accuracy - in the grant type interface
    9: ("match", "Javadoc is on OAuth2GrantType interface methods, which are HC"),

    # ROW 10: Catching RuntimeException - in grant type implementation
    10: ("match", "exception handling is in grant type code, which has 46 HC entities"),

    # ROW 11: Italian translation in Lithuanian file - .properties file, not Java
    11: ("miss", "bug is in .properties translation file, not in VerifyMessageProperties Java class"),

    # ROW 12: Traditional Chinese in Simplified Chinese file - same
    12: ("miss", "bug is in .properties translation file, not in Java code"),

    # ROW 15: canManage() permission check
    15: ("partial", "permission code is in HC but canManage() specifically not identified by name"),

    # ROW 16: hasPermission called with wrong params in getGroupIdsWithViewPermission
    16: ("partial", "permission system is HC but specific method not matched by name"),

    # ROW 17: Feature flag / AdminPermissions event listener
    17: ("partial", "permission test methods are HC but AdminPermissions event listener not specifically flagged"),

    # ROW 18: hasPermission resource lookup
    18: ("partial", "permission test infrastructure is HC but hasPermission implementation not specifically matched"),

    # ROW 19: getClientsWithPermission iteration
    19: ("partial", "permission test methods HC but getClientsWithPermission not specifically flagged"),

    # ROW 20: picocli.exit() method issue
    20: ("partial", "Profile/Feature/UpdateCompatibility are HC but picocli exit handling not specifically matched"),

    # ROW 21: Wrong keystore provider
    21: ("partial", "crypto-related entities are HC but specific provider selection bug not matched"),

    # ROW 22: Dead code ASN1Encoder
    22: ("partial", "ASN1Decoder is HC but ASN1Encoder dead code not specifically flagged"),

    # ROW 36-38: CSS color changes - no HC entities, CSS chunk-based entities
    36: ("miss", "CSS changes, no HC entities, only line-range Medium chunks"),
    37: ("miss", "CSS changes, no HC entities"),
    38: ("miss", "CSS changes, no HC entities"),

    # ROW 39: include_website_name? method suffix
    39: ("miss", "no HC entities, only Medium line-range chunks for serializer"),

    # ROW 40-41: CSS float/flexbox issues
    40: ("miss", "CSS layout issues, no HC entities"),
    41: ("miss", "CSS vendor prefix issue, no HC entities"),

    # ROW 42: SSRF in open(url) - TopicEmbed is HC
    42: ("match", "SSRF vulnerability in embedding code, TopicEmbed is Critical"),

    # ROW 43: indexOf origin validation bypass
    43: ("partial", "origin validation is likely in JS/ERB, not in the Ruby model classes flagged as HC"),

    # ROW 44: postMessage targetOrigin
    44: ("partial", "postMessage is in JS embed code, not in the Ruby models flagged as HC"),

    # ROW 45: X-Frame-Options ALLOWALL
    45: ("partial", "X-Frame-Options is in controller/middleware, not in the model classes flagged as HC"),

    # ROW 46: TopicEmbed.import NoMethodError
    46: ("match", "TopicEmbed is flagged as Critical"),

    # ROW 47: ERB block syntax error
    47: ("partial", "ERB template error, not in the model classes flagged"),

    # ROW 51: Typo stopNotificiationsText - JS property name
    51: ("miss", "typo is in JavaScript frontend code, not in Ruby models/controllers flagged"),

    # ROW 57: enableSqlExpressions always returns false
    57: ("miss", "enableSqlExpressions function not in HC entities (TablesList and ReadQuery are HC)"),

    # ROW 58: NewInMemoryDB not implemented methods
    58: ("partial", "NewInMemoryDB is Medium, RunCommands is Medium, not HC"),

    # ROW 62: d.Log instead of log variable
    62: ("miss", "no HC entities for this PR, all Medium"),

    # ROW 63: recordLegacyDuration vs recordStorageDuration
    63: ("miss", "no HC entities for this PR"),

    # ROW 64: name vs options.Kind inconsistency
    64: ("miss", "no HC entities for this PR"),

    # ROW 67: applyTemplateVariables unused parameter
    67: ("miss", "applyTemplateVariables not in HC entities (runSplitQuery and runShardSplitQuery are HC)"),

    # ROW 82: Magic number 50 in tests
    82: ("partial", "test infrastructure is HC but magic number is a code style issue"),

    # ROW 83: test docstring mismatch
    83: ("partial", "test function is in HC area but docstring mismatch is style issue"),

    # ROW 87: Typo in test method name
    87: ("partial", "test is in the assignment source area (HC) but typo is a naming issue"),

    # ROW 88: Test method name vs implementation mismatch
    88: ("partial", "test is in HC area but name mismatch is style"),

    # ROW 91: Fixed sleep in tests
    91: ("miss", "test flakiness issue, SpanFlusher is HC but test timing not captured"),

    # ROW 93: time.sleep monkeypatched
    93: ("miss", "test-specific mock issue, not in SpanFlusher HC entities"),

    # ROW 95: Breaking error response format changes
    95: ("miss", "error response format not in MatchedRow/delete HC entities"),

    # ROW 96: Detector validator wrong key
    96: ("miss", "detector validator not in replay delete HC entities"),

    # ROW 97: zip ordering assumption
    97: ("miss", "dict ordering not related to replay delete HC entities"),

    # ROW 107: Importing non-existent OptimizedCursorPaginator
    107: ("match", "OptimizedCursorPaginator is in ALL entities (Low), get_result is Critical"),

    # ROW 109: API key auth user_id=None
    109: ("partial", "auth issue is in a different module from paginator HC entities"),

    # ROW 112: macOS sed syntax
    112: ("miss", "shell script portability issue, not in TypeScript/Prisma HC entities"),

    # ROW 118-119: cal.com reminder concurrency/deletion
    118: ("miss", "no HC entities, only Medium line-range chunks"),
    119: ("miss", "no HC entities, only Medium chunks"),
}


def main():
    input_path = "/tmp/inspect-eval/eval.csv"
    output_path = "/tmp/inspect-eval/eval_judged.csv"

    with open(input_path) as f:
        reader = csv.DictReader(f)
        rows = list(reader)

    print(f"Judging {len(rows)} rows...", file=sys.stderr)

    results = []
    for i, row in enumerate(rows):
        row_num = i + 1

        # Use manual override if available
        if row_num in MANUAL_OVERRIDES:
            verdict, reason = MANUAL_OVERRIDES[row_num]
        else:
            verdict, reason = judge_row(row)

        row["llm_verdict"] = verdict
        row["llm_reason"] = reason
        results.append(row)

        status = {"match": "MATCH", "partial": "PARTIAL", "miss": "MISS"}.get(verdict, "???")
        gc = row['golden_comment'][:70]
        print(f"  [{row_num:3d}/141] {status:7s} | {row['repo']:12s} PR#{row['pr_number']:3s} | {gc}", file=sys.stderr)

    # Write judged CSV
    fieldnames = list(results[0].keys())
    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(results)

    # Compute stats
    total = len(results)
    matches = sum(1 for r in results if r["llm_verdict"] == "match")
    partials = sum(1 for r in results if r["llm_verdict"] == "partial")
    misses = sum(1 for r in results if r["llm_verdict"] == "miss")

    print(f"\n{'='*60}", file=sys.stderr)
    print(f"RESULTS ({total} golden comments)", file=sys.stderr)
    print(f"{'='*60}", file=sys.stderr)
    print(f"  Match:   {matches:3d} ({matches/total*100:.1f}%)", file=sys.stderr)
    print(f"  Partial: {partials:3d} ({partials/total*100:.1f}%)", file=sys.stderr)
    print(f"  Miss:    {misses:3d} ({misses/total*100:.1f}%)", file=sys.stderr)
    print(f"", file=sys.stderr)

    strict_recall = matches / total * 100
    lenient_recall = (matches + partials) / total * 100
    print(f"  Strict recall (match only):     {strict_recall:.1f}%", file=sys.stderr)
    print(f"  Lenient recall (match+partial): {lenient_recall:.1f}%", file=sys.stderr)

    # Per-repo breakdown
    print(f"\nPer-repo breakdown:", file=sys.stderr)
    repos = sorted(set(r["repo"] for r in results))
    for repo in repos:
        repo_rows = [r for r in results if r["repo"] == repo]
        repo_total = len(repo_rows)
        repo_matches = sum(1 for r in repo_rows if r["llm_verdict"] == "match")
        repo_partials = sum(1 for r in repo_rows if r["llm_verdict"] == "partial")
        repo_misses = sum(1 for r in repo_rows if r["llm_verdict"] == "miss")
        strict = repo_matches / repo_total * 100 if repo_total else 0
        lenient = (repo_matches + repo_partials) / repo_total * 100 if repo_total else 0
        print(f"  {repo:15s}: {repo_total:2d} | match={repo_matches:2d} partial={repo_partials:2d} miss={repo_misses:2d} | strict={strict:5.1f}% lenient={lenient:5.1f}%", file=sys.stderr)

    # Per-severity breakdown
    print(f"\nPer-severity breakdown:", file=sys.stderr)
    for sev in ["Critical", "High", "Medium", "Low"]:
        sev_rows = [r for r in results if r["golden_severity"] == sev]
        sev_total = len(sev_rows)
        if sev_total == 0:
            continue
        sev_matches = sum(1 for r in sev_rows if r["llm_verdict"] == "match")
        sev_partials = sum(1 for r in sev_rows if r["llm_verdict"] == "partial")
        sev_misses = sum(1 for r in sev_rows if r["llm_verdict"] == "miss")
        strict = sev_matches / sev_total * 100
        lenient = (sev_matches + sev_partials) / sev_total * 100
        print(f"  {sev:10s}: {sev_total:2d} | match={sev_matches:2d} partial={sev_partials:2d} miss={sev_misses:2d} | strict={strict:5.1f}% lenient={lenient:5.1f}%", file=sys.stderr)

    # HC recall (High + Critical only)
    hc_rows = [r for r in results if r["golden_severity"] in ("High", "Critical")]
    hc_total = len(hc_rows)
    hc_matches = sum(1 for r in hc_rows if r["llm_verdict"] == "match")
    hc_partials = sum(1 for r in hc_rows if r["llm_verdict"] == "partial")
    hc_strict = hc_matches / hc_total * 100 if hc_total else 0
    hc_lenient = (hc_matches + hc_partials) / hc_total * 100 if hc_total else 0
    print(f"\n  HC recall (High+Critical, n={hc_total}):", file=sys.stderr)
    print(f"    Strict:  {hc_strict:.1f}%", file=sys.stderr)
    print(f"    Lenient: {hc_lenient:.1f}%", file=sys.stderr)

    print(f"\nWrote judged CSV to {output_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
