// System messages for each lens
export const SYSTEM_PRECISE =
  "You are a precise code reviewer. Only report real bugs you are confident about. Always respond with valid JSON.";

export const SYSTEM_DATA =
  "You are a data correctness reviewer. Always respond with valid JSON.";

export const SYSTEM_CONCURRENCY =
  "You are a concurrency/state bug reviewer. Always respond with valid JSON.";

export const SYSTEM_CONTRACTS =
  "You are an API contracts reviewer. Always respond with valid JSON.";

export const SYSTEM_SECURITY =
  "You are a security reviewer. Always respond with valid JSON.";

export const SYSTEM_TYPOS =
  "You are a character-level detail reviewer. Always respond with valid JSON.";

export const SYSTEM_RUNTIME =
  "You are a runtime failure analyst. Always respond with valid JSON.";

export const SYSTEM_VALIDATE =
  "You are a precise reviewer. Verify each issue against the actual diff. Only keep confirmed bugs. Always respond with valid JSON.";

// Specialized lens prompts

export const PROMPT_DATA = `You are a code reviewer specializing in DATA CORRECTNESS issues.

PR Title: {pr_title}

PR Diff:
{diff}

Focus ONLY on: wrong translations, wrong constants/mappings/enum values, copy-paste errors, wrong key/field references, case sensitivity in comparisons, incorrect regex.
Rules: ONLY concrete data issues. Be specific. Max 5 issues.

For each issue, provide a JSON object with "issue" (description), "evidence" (the specific code), "severity" (critical/high/medium/low), and "file" (file path).
Respond with ONLY: {{"issues": [{{"issue": "desc", "evidence": "code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_CONCURRENCY = `You are a code reviewer specializing in CONCURRENCY and STATE bugs.

PR Title: {pr_title}

PR Diff:
{diff}

Focus ONLY on: race conditions, missing locks/transactions, stale reads, process lifecycle bugs, cache inconsistency, feature flag inconsistency.
Rules: ONLY issues with evidence in the diff. Be specific. Max 5 issues.

For each issue, provide a JSON object with "issue" (description), "evidence" (the specific code), "severity" (critical/high/medium/low), and "file" (file path).
Respond with ONLY: {{"issues": [{{"issue": "desc", "evidence": "code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_CONTRACTS = `You are a code reviewer specializing in API CONTRACT violations.

PR Title: {pr_title}

PR Diff:
{diff}

Focus ONLY on: missing abstract method implementations, wrong signatures/types, API breaking changes, wrong parameter order, key mismatches, missing React keys, import errors, method name typos breaking interfaces.
Rules: ONLY verifiable issues. Be specific. Max 5 issues.

For each issue, provide a JSON object with "issue" (description), "evidence" (the specific code), "severity" (critical/high/medium/low), and "file" (file path).
Respond with ONLY: {{"issues": [{{"issue": "desc", "evidence": "code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_SECURITY = `You are a security-focused code reviewer.

PR Title: {pr_title}

PR Diff:
{diff}

Focus ONLY on: SSRF, XSS, injection, auth bypass, origin/referrer bypass, case sensitivity bypass in security comparisons, frame options misconfig, hardcoded secrets.
Rules: ONLY real exploitable vulnerabilities. Be specific. Max 5 issues.

For each issue, provide a JSON object with "issue" (description), "evidence" (the specific code), "severity" (critical/high/medium/low), and "file" (file path).
Respond with ONLY: {{"issues": [{{"issue": "desc", "evidence": "code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_TYPOS = `You are a code reviewer with exceptional attention to character-level detail.

PR Title: {pr_title}

PR Diff:
{diff}

Focus ONLY on:
- Method/function/variable name TYPOS causing runtime errors
- Wrong language in locale/translation files
- Missing required method suffixes (Rails '?', etc.)
- Case sensitivity bugs in comparisons
- Wrong vendor prefixes
- Property/key name mismatches

Rules: Character-level precision. Only if it causes runtime failure. Max 5 issues.

For each issue, provide a JSON object with "issue" (description), "evidence" (the specific code), "severity" (critical/high/medium/low), and "file" (file path).
Respond with ONLY: {{"issues": [{{"issue": "desc", "evidence": "code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_RUNTIME = `You are a code reviewer focused on RUNTIME FAILURES.

PR Title: {pr_title}

PR Diff:
{diff}

For each changed function/class, ask: "What would happen if I ran this code?"

Focus ONLY on:
- Null/nil/undefined dereference
- Missing abstract method implementations causing TypeError
- Unreachable code branches
- Infinite recursion without termination
- Wrong error messages
- Panic on nil in Go
- Missing React keys

Rules: RUNTIME behavior only. Only actual failures. Max 5 issues.

For each issue, provide a JSON object with "issue" (description), "evidence" (the specific code), "severity" (critical/high/medium/low), and "file" (file path).
Respond with ONLY: {{"issues": [{{"issue": "desc", "evidence": "code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_GENERAL = `You are a world-class code reviewer. Review this PR and find ONLY real, concrete bugs.

PR Title: {pr_title}

PR Diff:
{diff}

Look specifically for these categories of issues:
1. Logic errors: wrong conditions, off-by-one, incorrect algorithms, broken control flow, inverted booleans
2. Concurrency bugs: race conditions, missing locks, unsafe shared state, deadlocks, unhandled async promises
3. Null/undefined safety: missing null checks, possible NPE, Optional.get() without isPresent(), uninitialized variables
4. Error handling: swallowed exceptions, missing error propagation, wrong error types
5. Data correctness: wrong translations, wrong constants, incorrect mappings, copy-paste errors, stale cache data
6. Security: SSRF, XSS, injection, auth bypass, exposed secrets, unsafe deserialization, origin validation bypass
7. Type mismatches: wrong return types, incompatible casts, API contract violations, schema errors
8. Breaking changes: removed public APIs without migration, changed behavior silently
9. State consistency: asymmetric cache trust, orphaned data, inconsistent updates across related fields
10. Naming/contract bugs: method name typos that break interfaces, property names that don't match expected contracts

Rules:
- ONLY report issues you are highly confident about (>90% sure)
- Be specific: name the file, function/variable, and exactly what's wrong
- Naming typos ARE bugs if they would cause a runtime error or break an API contract
- Do NOT report: style preferences, missing tests, docs, "could be improved"
- Do NOT report issues about code that was only deleted/removed
- Maximum 10 issues. Quality over quantity.

For each issue, provide it as a JSON object with "issue" (description), "evidence" (quote the specific code lines), "severity" (critical/high/medium/low), and "file" (file path).

Respond with ONLY a JSON object:
{{"issues": [{{"issue": "description", "evidence": "the specific code", "severity": "high", "file": "path/to/file"}}]}}`;

export const PROMPT_VALIDATE = `You are a senior code reviewer doing final validation. You have the PR diff and candidate issues.

PR Title: {pr_title}

PR Diff (for verification):
{diff}

Candidate Issues:
{candidates}

For each candidate, verify against the actual diff:
1. Can you find the specific code that's buggy? If yes, keep it.
2. Is this a real bug that would cause incorrect behavior in production? If yes, keep it.
3. Is this about deleted/removed code being replaced? If so, DROP it.
4. Is this speculative or theoretical ("could potentially...")? If so, DROP it.
5. Is this about style, naming conventions, or missing tests? If so, DROP it.

Return ONLY the issues that are verified real bugs with evidence in the diff.

Respond with ONLY a JSON object:
{{"issues": [{{"issue": "description", "evidence": "the specific code", "severity": "high", "file": "path/to/file"}}]}}`;

/** Smart diff truncation that deprioritizes tests, docs, configs. */
export function truncateDiff(diff: string, maxChars: number = 80000): string {
  if (diff.length <= maxChars) return diff;

  const parts = diff.split("diff --git ");
  if (!parts.length) return diff.slice(0, maxChars);

  const scored: [number, string][] = [];
  for (const part of parts) {
    if (!part.trim()) continue;

    const adds = (part.match(/\n\+/g) || []).length - (part.match(/\n\+\+\+/g) || []).length;
    const dels = (part.match(/\n-/g) || []).length - (part.match(/\n---/g) || []).length;
    const modBonus = Math.min(adds, dels) * 2;
    let score = adds + dels + modBonus;

    const firstLine = (part.split("\n")[0] || "").toLowerCase();

    if (["test", "spec", "mock", "__test__", "fixture"].some((kw) => firstLine.includes(kw)))
      score *= 0.3;
    if ([".md", ".adoc", ".txt", ".rst", "changelog", "readme"].some((kw) => firstLine.includes(kw)))
      score *= 0.2;
    if ([".snap", ".lock", "package-lock", "yarn.lock"].some((kw) => firstLine.includes(kw)))
      score *= 0.1;
    if ([".json", ".yaml", ".yml", ".toml", ".xml"].some((kw) => firstLine.includes(kw)))
      score *= 0.5;

    scored.push([score, part]);
  }

  scored.sort((a, b) => b[0] - a[0]);

  let result = "";
  for (const [, part] of scored) {
    const candidate = "diff --git " + part;
    if (result.length + candidate.length > maxChars) break;
    result += candidate;
  }

  return result || diff.slice(0, maxChars);
}
