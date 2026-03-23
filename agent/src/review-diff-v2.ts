#!/usr/bin/env node
/**
 * review-diff-v2: Same thorough review as v1, but with:
 *   - Entity triage context from inspect's semantic analysis (all entities)
 *   - Separate extraction call (fresh context, no agent self-summary)
 *
 * Input (stdin JSON):
 *   { repo_dir, diff, findings, entity_reviews, pr_title, provider?, model? }
 * Output (stdout JSON):
 *   { verdicts: [{ rule_id, entity_name, verdict, explanation }] }
 */
import { getModel, completeSimple } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";
import { Agent } from "@mariozechner/pi-agent-core";
import type { AgentTool } from "@mariozechner/pi-agent-core";
import { Type } from "@sinclair/typebox";
import Parallel from "parallel-web";
import { readFileSync } from "fs";
import { resolve } from "path";
import type { DetectorFinding, EntityReview, ValidateOutput } from "./types.js";
import type { Context } from "@mariozechner/pi-ai";

// ── Load .env ──
const envPath = resolve(import.meta.dirname, "../../.env");
try {
  for (const line of readFileSync(envPath, "utf-8").split("\n")) {
    const m = line.match(/^([A-Z_]+)=(.+)$/);
    if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
  }
} catch {}

// ── Read stdin ──
async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf-8");
}

// ── System prompt (v4 — follows Anthropic prompting best practices) ──
function buildSystemPrompt(repoDir: string): string {
  const today = new Date().toLocaleDateString("en-US", {
    weekday: "short", month: "short", day: "numeric", year: "numeric",
  });
  return `You are an expert senior engineer performing a thorough code review. Your goal is to find concrete correctness bugs that would cause wrong behavior in production.

<instructions>
1. Generate a high-level summary of the changes.
2. Review each changed file and hunk. For each, explain what changed and check for bugs, logic errors, race conditions, type mismatches, broken callers, and incorrect behavior.
3. For changed or removed public APIs, use Grep to search for callers repo-wide. Flag callers that weren't updated.
4. Use Read tool to inspect entity code not visible in a truncated diff. Review ALL high-risk entities from the triage section, not just the first few files.
5. Review test code for correctness bugs too: wrong assertions, wrong cleanup values, hardcoded strings that should be parameterized, and mismatched test descriptions.
</instructions>

<guidelines>
Report a bug whenever the code as written would produce wrong behavior — even if existing code has the same pattern or the framework might handle it. The goal is to catch bugs before production, so err on the side of flagging.

Do NOT dismiss bugs because the pattern is "pre-existing", "consistent with existing code", or "also present in other files." A bug is a bug regardless of whether old code has the same mistake. If this PR introduces or copies a buggy pattern, report it.

Verify guards and type checks rather than assuming they're correct. For example, isinstance() may not match the actual runtime type if the object was created by a factory or subprocess.

Unreachable code paths, dead branches, and always-true/always-false conditions are bugs worth reporting. They indicate logic errors, missing handling, or code that will silently break when assumptions change.

When you see code calling a method or API, verify it actually exists in the target version. Standard library APIs change between language versions — a method present in Python 3.13+ may not exist in 3.12.

When you identify a potential issue, state it clearly rather than dismissing it. Say "potential bug" if uncertain.
</guidelines>

<examples>
<example>
<review_excerpt>The forEach with async callback means promises are fire-and-forget. Errors are unhandled.</review_excerpt>
<classification>BUG — fire-and-forget async causes silent failures and unhandled rejections</classification>
</example>
<example>
<review_excerpt>The if-else chain has dead branches — config is always non-null so the fallback branch never executes.</review_excerpt>
<classification>BUG — unreachable code indicates missing handling for the fallback case</classification>
</example>
<example>
<review_excerpt>The type check uses the base class, but the factory creates a subclass that doesn't inherit from it on this platform.</review_excerpt>
<classification>BUG — type check always returns false, so the guarded code path never executes</classification>
</example>
<example>
<review_excerpt>Test teardown deletes a hardcoded key but the test creates entries with a dynamic key, so cleanup silently does nothing.</review_excerpt>
<classification>BUG — test cleanup uses wrong identifier, leaving stale test data behind</classification>
</example>
<example>
<review_excerpt>This pattern is the same as existing code in the file, so it's fine.</review_excerpt>
<classification>BUG — pre-existing bugs are still bugs; report them if this PR introduces or copies the pattern</classification>
</example>
<example>
<review_excerpt>Minor: the variable name could be more descriptive.</review_excerpt>
<classification>SKIP — naming preference, no functional impact</classification>
</example>
<example>
<review_excerpt>Consider adding input validation here for robustness.</review_excerpt>
<classification>SKIP — defensive suggestion, current code is correct</classification>
</example>
</examples>

Current working directory (cwd): ${repoDir}
Today's date: ${today}`;
}

// ── Smart triage: classify entities as MECHANICAL / NEW_LOGIC / BEHAVIORAL ──
type TriageCategory = "MECHANICAL" | "NEW_LOGIC" | "BEHAVIORAL";

function tokenize(s: string): Set<string> {
  return new Set(s.split(/\s+/).filter(t => t.length > 0));
}

function jaccardSimilarity(before: string, after: string): number {
  const a = tokenize(before);
  const b = tokenize(after);
  if (a.size === 0 && b.size === 0) return 1.0;
  const intersection = [...a].filter(t => b.has(t)).length;
  const union = new Set([...a, ...b]).size;
  return union === 0 ? 1.0 : intersection / union;
}

function categorizeEntity(e: EntityReview): TriageCategory {
  if (e.change_type === "Added" || e.change_type === "added") return "NEW_LOGIC";
  if (e.change_type === "Deleted" || e.change_type === "deleted") return "MECHANICAL";
  if (!e.before_content || !e.after_content) {
    return e.before_content ? "MECHANICAL" : "NEW_LOGIC";
  }
  const sim = jaccardSimilarity(e.before_content, e.after_content);
  if (sim > 0.8) return "MECHANICAL";
  if (sim < 0.5) return "NEW_LOGIC";
  return "BEHAVIORAL";
}

// ── Build entity triage: smart-categorized manifest of ALL changed entities ──
function buildEntityTriage(entities: EntityReview[]): string {
  if (!entities || entities.length === 0) return "";

  const meaningful = entities.filter(e => e.entity_type !== "chunk");
  if (meaningful.length === 0) return "";

  // Categorize each entity
  const categorized = meaningful.map(e => ({ entity: e, category: categorizeEntity(e) }));

  const mechanical = categorized.filter(c => c.category === "MECHANICAL");
  const newLogic = categorized.filter(c => c.category === "NEW_LOGIC");
  const behavioral = categorized.filter(c => c.category === "BEHAVIORAL");

  const lines: string[] = [];
  lines.push(`--- CHANGED ENTITIES (${meaningful.length} total, smart-triaged) ---`);
  lines.push("");

  // BEHAVIORAL — these are where bugs hide (medium similarity, value changes)
  if (behavioral.length > 0) {
    lines.push(`BEHAVIORAL (verify — ${behavioral.length} changes):`);
    for (const { entity: e } of behavioral.sort((a, b) => b.entity.risk_score - a.entity.risk_score)) {
      const pub = e.is_public_api ? " [PUBLIC]" : "";
      const deps = e.dependent_count > 0 ? ` | ${e.dependent_count} dependents` : "";
      lines.push(`  ∆ ${e.entity_name} (${e.entity_type}) in ${e.file_path} risk=${e.risk_score.toFixed(2)}${deps}${pub}`);
    }
    lines.push("");
  }

  // NEW LOGIC — pure additions, read these
  if (newLogic.length > 0) {
    lines.push(`NEW LOGIC (read — ${newLogic.length} changes):`);
    for (const { entity: e } of newLogic.sort((a, b) => b.entity.risk_score - a.entity.risk_score)) {
      const pub = e.is_public_api ? " [PUBLIC]" : "";
      const deps = e.dependent_count > 0 ? ` | ${e.dependent_count} dependents` : "";
      lines.push(`  ⊕ ${e.entity_name} (${e.entity_type}) in ${e.file_path}${deps}${pub}`);
    }
    lines.push("");
  }

  // MECHANICAL — skip these (high similarity, renames, reformats)
  if (mechanical.length > 0) {
    lines.push(`MECHANICAL (skip — ${mechanical.length} changes):`);
    // Just list count per file to save tokens
    const byFile: Record<string, number> = {};
    for (const { entity: e } of mechanical) {
      byFile[e.file_path] = (byFile[e.file_path] ?? 0) + 1;
    }
    for (const [fp, count] of Object.entries(byFile).sort()) {
      lines.push(`  ⊖ ${fp} (${count} entities)`);
    }
    lines.push("");
  }

  lines.push("Focus on BEHAVIORAL and NEW LOGIC entities. If the diff is truncated, use Read/Grep tools to inspect entities not visible in the patch.");
  lines.push("After reviewing, search repo-wide (Grep) for callers of any changed/removed public APIs to check if they were updated.");
  lines.push("--- END CHANGED ENTITIES ---");
  return lines.join("\n");
}

// ── Reorder diff: put high-risk files first for large diffs ──
function reorderDiff(diff: string, entities: EntityReview[]): string {
  if (!entities.length || diff.length < 80_000) return diff;

  // Get priority files from BEHAVIORAL/NEW_LOGIC high-risk entities
  const meaningful = entities.filter(e => e.entity_type !== "chunk");
  const priorityFiles = new Set<string>();
  for (const e of meaningful) {
    const cat = categorizeEntity(e);
    if (cat === "BEHAVIORAL" || (cat === "NEW_LOGIC" && e.risk_score >= 0.4)) {
      priorityFiles.add(e.file_path);
    }
  }
  if (priorityFiles.size === 0) return diff;

  // Split diff into per-file hunks
  const filePattern = /^diff --git a\/.+$/gm;
  const splits: { file: string; content: string; priority: boolean }[] = [];
  let match: RegExpExecArray | null;
  const indices: number[] = [];
  while ((match = filePattern.exec(diff)) !== null) {
    indices.push(match.index);
  }

  for (let i = 0; i < indices.length; i++) {
    const start = indices[i];
    const end = i + 1 < indices.length ? indices[i + 1] : diff.length;
    const chunk = diff.slice(start, end);
    // Extract filename from "diff --git a/path b/path"
    const fileMatch = chunk.match(/^diff --git a\/(.+?) b\//);
    const file = fileMatch ? fileMatch[1] : "";
    const isPriority = [...priorityFiles].some(pf => file.endsWith(pf) || pf.endsWith(file) || file.includes(pf) || pf.includes(file));
    splits.push({ file, content: chunk, priority: isPriority });
  }

  // If there's content before the first diff header (unlikely), preserve it
  if (indices.length > 0 && indices[0] > 0) {
    const prefix = diff.slice(0, indices[0]);
    if (prefix.trim()) {
      splits.unshift({ file: "", content: prefix, priority: false });
    }
  }

  // Sort: priority files first, then original order
  const prioritySplits = splits.filter(s => s.priority);
  const otherSplits = splits.filter(s => !s.priority);

  if (prioritySplits.length === 0) return diff;

  const reordered = [...prioritySplits, ...otherSplits].map(s => s.content).join("");
  return reordered;
}

// ── Build user message: entities + detector hints + diff ──
// Follows Anthropic guide: longform data at top, query at bottom, XML tags for structure
function buildUserMessage(
  prTitle: string,
  diff: string,
  findings: DetectorFinding[],
  entities: EntityReview[],
): string {
  const parts: string[] = [];

  // Reorder diff to prioritize high-risk files for large diffs
  const reorderedDiff = reorderDiff(diff, entities);

  // Longform data FIRST (Anthropic: "put longform data at the top, queries at the end")
  // Cap diff at 120k chars
  if (reorderedDiff.length > 120_000) {
    parts.push(`<diff>\n${reorderedDiff.slice(0, 120_000)}\n... (diff truncated — use Read tool to inspect remaining files listed in changed_entities above)\n</diff>`);
  } else {
    parts.push(`<diff>\n${reorderedDiff}\n</diff>`);
  }

  // Entity triage — structured context about what changed
  const triage = buildEntityTriage(entities);
  if (triage) {
    parts.push("");
    parts.push(`<changed_entities>\n${triage}\n</changed_entities>`);
  }

  // Detector findings as noisy hints
  if (findings.length > 0) {
    const findingsText = findings.map(f =>
      `[${f.severity.toUpperCase()}] ${f.rule_id}: ${f.entity_name} in ${f.file_path}:${f.start_line}\n  ${f.message}\n  Evidence: ${f.evidence}`
    ).join("\n");
    parts.push("");
    parts.push(`<detector_findings note="Static analysis hints. Many are false positives. Only report if you independently verify a concrete bug.">\n${findingsText}\n</detector_findings>`);
  }

  // Query LAST (Anthropic: "queries at the end can improve response quality by up to 30%")
  parts.push("");
  const isLargeDiff = diff.length > 80_000;
  if (isLargeDiff && entities.length > 0) {
    // Build a concrete priority reading list of top BEHAVIORAL/NEW_LOGIC files the agent MUST check
    const meaningful = entities.filter(e => e.entity_type !== "chunk");
    const priorityEntities = meaningful
      .filter(e => {
        const cat = categorizeEntity(e);
        return cat === "BEHAVIORAL" || (cat === "NEW_LOGIC" && e.risk_score >= 0.4);
      })
      .sort((a, b) => b.risk_score - a.risk_score)
      .slice(0, 8);
    const uniqueFiles = [...new Set(priorityEntities.map(e => e.file_path))];
    const priorityList = uniqueFiles.slice(0, 5).map(f => `  - ${f}`).join("\n");

    parts.push(`<query>
Review this diff: ${prTitle || "Unstaged changes"}

This is a large diff (${entities.length} entities) that may be truncated. Before writing your review, use Read tool to inspect these high-risk files that may not be fully visible in the diff:
${priorityList}

After reading those, review the full diff systematically. Budget attention across ALL distinct code areas, not just the first files you encounter.
</query>`);
  } else {
    parts.push(`<query>Review this diff: ${prTitle || "Unstaged changes"}</query>`);
  }

  return parts.join("\n");
}

// ── Extract JSON from text ──
function extractJson(text: string): string {
  const trimmed = text.trim();
  const fenceMatch = trimmed.match(/```json\s*([\s\S]*?)```/);
  if (fenceMatch) {
    const inner = fenceMatch[1].trim();
    if (inner.startsWith("{") || inner.startsWith("[")) return inner;
  }
  const anyFence = trimmed.match(/```\s*([\s\S]*?)```/);
  if (anyFence) {
    const inner = anyFence[1].trim();
    if (inner.startsWith("{") || inner.startsWith("[")) return inner;
  }
  const issuesIdx = trimmed.indexOf('{"issues"');
  if (issuesIdx !== -1) {
    const sub = trimmed.slice(issuesIdx);
    let depth = 0;
    for (let i = 0; i < sub.length; i++) {
      if (sub[i] === "{") depth++;
      else if (sub[i] === "}") { depth--; if (depth === 0) return sub.slice(0, i + 1); }
    }
  }
  const first = trimmed.indexOf("{");
  const last = trimmed.lastIndexOf("}");
  if (first !== -1 && last > first) return trimmed.slice(first, last + 1);
  return trimmed;
}

interface RawIssue {
  issue: string;
  evidence: string;
  severity: "critical" | "high" | "medium";
  file: string;
}

// ── Web tools (web_search + read_web_page via Parallel) ──
function createWebTools(): AgentTool<any>[] {
  const apiKey = process.env.PARALLEL_API_KEY;
  if (!apiKey) return [];

  const client = new Parallel({ apiKey });

  const webSearch: AgentTool<any> = {
    name: "web_search",
    description: "Search the web for documentation, API references, or to verify whether a CSS property, library method, or API exists. Use when you need to confirm behavior of external APIs or browser features.",
    parameters: Type.Object({
      objective: Type.String({ description: "What you want to find out" }),
      search_queries: Type.Array(Type.String(), { description: "Keyword queries", minItems: 1, maxItems: 5 }),
    }),
    async execute(_id, params) {
      try {
        const result = await client.beta.search({
          objective: params.objective,
          search_queries: params.search_queries,
          max_results: 3,
          max_chars_per_result: 5000,
        });
        const text = (result.results ?? [])
          .map((r: any, i: number) => `## ${i + 1}. ${r.title ?? ""}\n${r.url ?? ""}\n${(r.content ?? r.text ?? r.snippet ?? "").slice(0, 3000)}`)
          .join("\n\n---\n\n");
        return { content: [{ type: "text", text: text || "No results found." }], details: {} };
      } catch (e: any) {
        return { content: [{ type: "text", text: `Search failed: ${e.message}` }], details: {} };
      }
    },
  };

  const readWebPage: AgentTool<any> = {
    name: "read_web_page",
    description: "Fetch and read a web page. Use to read documentation, changelogs, or API references when you need to verify external API behavior.",
    parameters: Type.Object({
      url: Type.String({ description: "URL to fetch" }),
      objective: Type.Optional(Type.String({ description: "What to extract from the page" })),
    }),
    async execute(_id, params) {
      try {
        const result = await client.beta.search({
          objective: params.objective ?? `Read contents of: ${params.url}`,
          search_queries: [params.url],
          max_results: 1,
          max_chars_per_result: 20000,
        });
        const page = result.results?.[0];
        if (!page) return { content: [{ type: "text", text: `No content for ${params.url}` }], details: {} };
        const text = `# ${page.title ?? ""}\n${page.url ?? ""}\n\n${page.content ?? page.text ?? page.snippet ?? ""}`;
        return { content: [{ type: "text", text }], details: {} };
      } catch (e: any) {
        return { content: [{ type: "text", text: `Failed: ${e.message}` }], details: {} };
      }
    },
  };

  return [webSearch, readWebPage];
}

// ── Main ──
async function main() {
  const raw = await readStdin();
  const input = JSON.parse(raw);

  const repoDir: string = input.repo_dir;
  const diff: string = input.diff ?? "";
  const findings: DetectorFinding[] = input.findings ?? [];
  const entities: EntityReview[] = input.entity_reviews ?? [];
  const prTitle: string = input.pr_title ?? "";
  const provider = input.provider ?? "anthropic";
  const modelId = input.model ?? "claude-opus-4-6";

  process.stderr.write(`[review-v2] diff=${diff.length} chars, ${findings.length} findings, ${entities.length} entities\n`);

  if (!diff) {
    const output: ValidateOutput = { verdicts: [] };
    process.stdout.write(JSON.stringify(output) + "\n");
    return;
  }

  // Setup model + tools
  const model = getModel(provider as any, modelId);
  const tools = [
    createReadTool(repoDir),
    createGrepTool(repoDir),
    createFindTool(repoDir),
    createBashTool(repoDir, {
      spawnHook: ({ command }: { command: string }) => {
        const dangerous = ["rm ", "mv ", "cp ", "chmod", "chown", "kill", "mkfs", ">", ">>", "sudo"];
        if (dangerous.some((d) => command.includes(d))) {
          throw new Error(`Blocked dangerous command: ${command}`);
        }
      },
    }),
    ...createWebTools(),
  ];

  // Build prompts
  const systemPrompt = buildSystemPrompt(repoDir);
  const userMessage = buildUserMessage(prTitle, diff, findings, entities);

  process.stderr.write(`[review-v2] System prompt: ${systemPrompt.length} chars\n`);
  process.stderr.write(`[review-v2] User message: ${userMessage.length} chars\n`);

  // ── Turn 1: Thorough review with tools ──
  let toolCalls = 0;
  let finalText = "";

  const agent = new Agent({
    initialState: {
      systemPrompt,
      model,
      thinkingLevel: "high" as any,
      tools,
    },
    toolExecution: "parallel",
    beforeToolCall: async ({ toolCall, args }) => {
      toolCalls++;
      process.stderr.write(`[tool] #${toolCalls}: ${toolCall.name}(${JSON.stringify(args).slice(0, 120)})\n`);
      return undefined;
    },
  });

  agent.subscribe((event: any) => {
    if (event.type === "message_end" && event.message?.role === "assistant") {
      finalText = "";
      for (const part of event.message.content ?? []) {
        if (typeof part === "string") finalText += part;
        else if (part.type === "text") finalText += part.text;
      }
    }
  });

  const t0 = Date.now();
  await agent.prompt(userMessage);
  await agent.waitForIdle();

  const reviewText = finalText;
  process.stderr.write(`[review-v2] Review done: ${toolCalls} tool calls\n`);
  process.stderr.write(`[review-v2] === RAW RESPONSE ===\n${reviewText}\n=== END RAW RESPONSE ===\n`);

  // ── Turn 2: Separate extraction call on raw review text ──
  const extractModel = getModel(provider as any, modelId);
  const extractionPrompt = `<review>
${reviewText}
</review>

<task>
Extract every concrete bug from the code review above.

<include>
- Crashes, wrong behavior, logic errors, race conditions
- Broken callers, type mismatches, API incompatibilities
- Unreachable code, dead branches, always-true/always-false conditions
- Security holes, data corruption
- Wrong values in test code: wrong cleanup aliases, wrong assertions, mismatched descriptions
- Bugs the reviewer noted but then dismissed as "pre-existing", "consistent with existing code", or "not introduced by this PR" — extract these too, a bug is a bug
</include>

<exclude>
- Style preferences, naming nits
- Architectural opinions, refactoring suggestions
- Test coverage suggestions ("add more tests")
- Defensive suggestions where the reviewer confirms the code is currently correct
</exclude>
</task>

<examples>
<example>
<review_excerpt>Bug: the if-else chain has dead branches. config is always a non-null object, so the else-if and else fallback paths can never execute.</review_excerpt>
<extracted>{"issue": "if-else chain has two unreachable branches: config is always a non-null object, so else-if and else fallback paths never execute", "evidence": "config is always non-null (returned by getConfig()), so the else-if and else branches can never execute", "severity": "medium", "file": "src/settings.ts"}</extracted>
</example>
<example>
<review_excerpt>The forEach with async callback means promises are fire-and-forget. Errors from sendNotification are unhandled promise rejections.</review_excerpt>
<extracted>{"issue": "forEach with async callback causes fire-and-forget promises — errors from sendNotification are unhandled promise rejections that may crash the process", "evidence": "users.forEach(async (user) => { ... sendNotification ... })", "severity": "high", "file": "notifications.ts"}</extracted>
</example>
<example>
<review_excerpt>Test teardown deletes resource with hardcoded ID "default" but the test creates resources with "test-resource-" + i. This is the same pattern as other tests so it's fine.</review_excerpt>
<extracted>{"issue": "Test teardown uses wrong ID 'default' instead of 'test-resource-' + i — cleanup silently fails to delete test resources", "evidence": "cleanup.delete(getResource('default')) — but resources were created as 'test-resource-' + i", "severity": "medium", "file": "resource_test.go"}</extracted>
<note>Extracted despite reviewer dismissal — pre-existing bugs are still bugs</note>
</example>
<example>
<review_excerpt>Minor: the variable name could be more descriptive. Consider renaming x to userCount.</review_excerpt>
<extracted>SKIP — pure naming preference, no functional impact</extracted>
</example>
<example>
<review_excerpt>This is fine for now but consider adding input validation for robustness.</review_excerpt>
<extracted>SKIP — defensive suggestion, reviewer confirms code is currently correct</extracted>
</example>
</examples>

<format>
Respond with ONLY a JSON object. No text before or after.
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet or quote from review", "severity": "critical|high|medium", "file": "path/to/file"}]}

Severity guide:
- critical: crashes, data corruption, security vulnerabilities, complete feature breakage
- high: logic errors, wrong values, race conditions, silent failures, broken callers
- medium: edge cases, misleading behavior, unreachable code, dead branches, wrong test values

Return {"issues": []} if no concrete issues found.
</format>`;

  process.stderr.write(`[review-v2] Running extraction with ${provider}/${modelId} (separate call)...\n`);

  const extractCtx: Context = {
    systemPrompt: `You are a code review extraction assistant. You extract structured bug reports from code review text.

<guidelines>
When in doubt about whether something has functional impact, include it.
If the reviewer identified a bug but then dismissed it (e.g., "pre-existing pattern", "not a concern", "consistent with existing code"), extract it anyway. A bug is a bug regardless of whether the reviewer decided to downplay it.
Do not add issues the reviewer did not mention. Only extract what the review text describes.
</guidelines>`,
    messages: [
      {
        role: "user" as const,
        content: extractionPrompt,
        timestamp: Date.now(),
      },
    ],
  };

  let extractionText = "";
  try {
    const extractResult = await completeSimple(extractModel, extractCtx, {
      temperature: 0,
      maxTokens: 4096,
    });
    for (const part of extractResult.content) {
      if (part.type === "text") extractionText += part.text;
    }
  } catch (e) {
    process.stderr.write(`[review-v2] Extraction call failed: ${e}\n`);
  }

  const elapsed = Date.now() - t0;
  process.stderr.write(`[review-v2] Extraction done: ${(elapsed / 1000).toFixed(1)}s total\n`);
  process.stderr.write(`[review-v2] === EXTRACTION RESPONSE ===\n${extractionText}\n=== END EXTRACTION RESPONSE ===\n`);

  // Parse the structured JSON response
  let issues: RawIssue[] = [];
  try {
    const json = extractJson(extractionText);
    const parsed = JSON.parse(json);
    if (Array.isArray(parsed.issues)) issues = parsed.issues;
  } catch {
    process.stderr.write(`[review-v2] WARNING: JSON parse failed\n`);
  }
  process.stderr.write(`[review-v2] Parsed ${issues.length} issues\n`);

  // Build output
  const output: ValidateOutput = {
    verdicts: issues.map((issue) => ({
      rule_id: "review",
      entity_name: issue.file ?? "unknown",
      verdict: "true_positive" as const,
      explanation: `[${issue.severity}] ${issue.issue} | evidence: ${issue.evidence || "see review"} | file: ${issue.file ?? "unknown"}`,
    })),
  };

  process.stdout.write(JSON.stringify(output) + "\n");
}

main().catch((e) => {
  process.stderr.write(`[fatal] ${e}\n`);
  process.exit(1);
});
