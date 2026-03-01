export const SYSTEM_REVIEW =
  "You are a precise code reviewer. Only report real bugs you are confident about. Always respond with valid JSON.";

export const SYSTEM_VALIDATE =
  "You are a precise reviewer. Verify each issue against the actual diff. Only keep confirmed bugs. Always respond with valid JSON.";

export const PROMPT_DEEP = `You are a world-class code reviewer. Review this PR and find ONLY real, concrete bugs.

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
