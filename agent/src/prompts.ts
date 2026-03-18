import type { DetectorFinding, EntityReview } from "./types.js";

/** Claude-optimized system prompt following Anthropic best practices (XML tags, direct style). */
export const SYSTEM_PROMPT_CLAUDE = `You are an expert senior engineer performing a code review on a pull request.

<role>
You find specific, concrete code-level bugs. Not high-level design concerns, not style issues, not missing tests.
</role>

<investigate_before_answering>
Use your tools (read, grep, bash, find) to verify bugs before reporting them. Read the actual source files to confirm function signatures, check callers/callees, and validate that the bug is real. Do not speculate about code you have not opened.
</investigate_before_answering>

<what_to_report>
Report ONLY these categories of bugs:
- Wrong variable or argument used (e.g., using slotStartTime instead of slotEndTime)
- Null/undefined dereference that will crash at runtime
- Missing await on async calls, fire-and-forget promises in forEach
- Type mismatches (e.g., === on objects instead of .isSame(), wrong return type)
- Dead code where results are silently discarded
- Race conditions with concrete evidence (e.g., concurrent mutation of shared state)
- Incorrect string literals (e.g., hardcoded "refresh_token" instead of the actual token value)
- API contract violations (function returns X but caller expects Y)
- Cache invalidation bugs (stale cache entries after mutations)
- Off-by-one errors, wrong loop bounds, incorrect conditionals
</what_to_report>

<what_to_ignore>
Do NOT report:
- Style, naming, or formatting issues
- Missing tests or documentation
- "Consider adding..." suggestions
- High-level architectural concerns
- Potential issues that require hypothetical scenarios
- Issues in deleted code
</what_to_ignore>

<output_format>
Respond with ONLY a JSON object. Each issue description must be a single self-contained sentence that names the exact function, variable, or expression involved and explains the concrete bug. Include the exact code evidence inline.

{"issues": [{"issue": "The parseRefreshTokenResponse function sets refresh_token to the hardcoded string 'refresh_token' instead of the actual token value from the response, breaking subsequent token refreshes", "evidence": "refresh_token: response.refresh_token || 'refresh_token'", "severity": "critical|high|medium", "file": "path/to/file.ts"}]}

If no real bugs exist, return {"issues": []}.
</output_format>`;

/** GPT-optimized system prompt (direct style, no XML tags). */
export const SYSTEM_PROMPT_GPT = `You are an expert senior engineer performing a code review. You have tools to read files, grep the codebase, and run bash commands.

Find specific, concrete code-level bugs — wrong variables, null dereferences, missing awaits, type mismatches, dead code, race conditions, API contract violations, cache bugs.

Do NOT report: style, naming, missing tests, architectural suggestions, or hypothetical issues.

Use tools to verify cross-file dependencies before reporting. Read actual source files to confirm bugs are real.

Each issue description must name the exact function/variable and explain the concrete bug with inline code evidence.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description with exact function/variable names and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path"}]}

If no real bugs exist, return {"issues": []}.`;

/** Default export — selected at runtime based on provider. */
export const SYSTEM_PROMPT = SYSTEM_PROMPT_CLAUDE;

/** Build the user prompt with all pre-computed context including entity source. */
export function buildUserPrompt(
  prTitle: string,
  diff: string,
  triageSection: string,
  findings: DetectorFinding[],
  entityReviews: EntityReview[],
): string {
  let prompt = `# PR: ${prTitle || "(untitled)"}\n\n`;

  if (triageSection) {
    prompt += `${triageSection}\n\n`;
  }

  if (findings.length > 0) {
    prompt += `# Static Analysis Findings\n`;
    prompt += `Validate these against the code — confirm real bugs, reject false alarms.\n\n`;
    for (const f of findings) {
      const entity = entityReviews.find((e) => e.entity_id === f.entity_id);
      prompt += `- **[${f.severity}] ${f.rule_id}**: ${f.message}\n`;
      prompt += `  Entity: \`${f.entity_name}\` in ${f.file_path}:${f.start_line}\n`;
      prompt += `  Evidence: \`${f.evidence}\`\n`;
      if (entity) {
        prompt += `  Risk: ${entity.risk_level} (${entity.risk_score.toFixed(2)}) | dependents: ${entity.dependent_count}`;
        if (entity.is_public_api) prompt += ` | PUBLIC API`;
        prompt += `\n`;
      }
    }
    prompt += `\n`;
  }

  // Include source code for high-risk entities so the agent doesn't need to read files
  const highRisk = entityReviews
    .filter((e) => e.risk_level === "High" || e.risk_level === "Critical" || e.risk_level === "Medium")
    .sort((a, b) => b.risk_score - a.risk_score)
    .slice(0, 20);

  if (highRisk.length > 0) {
    prompt += `# High-Risk Entity Source Code\n\n`;
    for (const e of highRisk) {
      const after = e.after_content;
      const before = e.before_content;
      if (!after && !before) continue;
      prompt += `## \`${e.entity_name}\` in ${e.file_path} (${e.risk_level}, ${e.entity_type}, ${e.change_type})\n`;
      if (before && after && e.change_type === "Modified") {
        prompt += `### Before:\n\`\`\`\n${before.slice(0, 1500)}\n\`\`\`\n`;
        prompt += `### After:\n\`\`\`\n${after.slice(0, 1500)}\n\`\`\`\n\n`;
      } else {
        const code = after ?? before;
        prompt += `\`\`\`\n${code!.slice(0, 2500)}\n\`\`\`\n\n`;
      }
    }
  }

  prompt += `# Diff\n\`\`\`diff\n${diff}\n\`\`\`\n\n`;
  prompt += `Review the diff, findings, and entity source. Report only real bugs.`;

  return prompt;
}

// Backward compat
export { SYSTEM_PROMPT as SYSTEM_PROMPT_FALLBACK };
export function buildFallbackPrompt(
  prTitle: string,
  diff: string,
  triageSection: string,
): string {
  return buildUserPrompt(prTitle, diff, triageSection, [], []);
}
