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

// ── System prompt (v8 — v3 breadth + anti-dismissal + stricter extraction + lenient validation) ──
function buildSystemPrompt(repoDir: string): string {
  const today = new Date().toLocaleDateString("en-US", {
    weekday: "short", month: "short", day: "numeric", year: "numeric",
  });
  return `You are an expert senior engineer with deep knowledge of software engineering best practices, security, performance, and maintainability.

Your task is to perform a thorough code review of the provided diff description. The diff description might be a git or bash command that generates the diff or a description of the diff which can then be used to generate the git or bash command to generate the full diff.

After reading the diff, do the following:
1. Generate a high-level summary of the changes in the diff.
2. Go file-by-file and review each changed hunk.
3. Comment on what changed in that hunk (including the line range) and how it relates to other changed hunks and code, reading any other relevant files. Also call out bugs, hackiness, unnecessary code, naming inconsistencies, or too much shared mutable state.
4. For any changed or removed public APIs, use Grep to search for callers repo-wide. Check if callers were updated to match the new signature/behavior. Flag broken callers.
5. Use Read tool to inspect entity code not visible in a truncated diff. Review ALL high-risk entities from the triage section, not just the first few files.
6. Review test code for correctness bugs too: wrong assertions, wrong cleanup values, hardcoded strings that should be parameterized, mismatched test descriptions, and typos.

Do NOT dismiss bugs because the pattern is "pre-existing", "consistent with existing code", or "also present in other files." A bug is a bug regardless of whether old code has the same mistake. If this PR introduces or copies a buggy pattern, report it.

Verify guards and type checks rather than assuming they're correct. For example, isinstance() may not match the actual runtime type if the object was created by a factory or subprocess.

When you identify a potential issue, state it clearly rather than dismissing it. Say "potential bug" if uncertain.

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

  // ── Turn 2: Extract candidates from FULL review text ──
  const extractSource = reviewText;
  process.stderr.write(`[review-v2] Extraction source: full review text (${extractSource.length} chars)\n`);

  const extractModel = getModel(provider as any, modelId);
  const extractionPrompt = `You are a code review extraction assistant. Below is a code review. Your job is to extract ONLY concrete correctness bugs the reviewer identified.

RULES — EXTRACT only issues that meet ALL of these criteria:
1. The reviewer explicitly identifies a CONCRETE functional problem (crash, wrong value, data loss, security hole, broken caller, race condition, incorrect logic, unreachable code)
2. The issue has a SPECIFIC code reference (file + function/variable/line)
3. The reviewer explains WHY it's wrong (not just "consider" or "could be improved")

DO NOT EXTRACT:
- Suggestions phrased as "consider...", "might want to...", "could be improved by..." UNLESS the reviewer also states the current code is BROKEN
- Missing features: "no rate limiting", "no validation", "no upper bound" — these are hardening suggestions, not bugs
- Theoretical edge cases the reviewer hedges about: "if X were to happen...", "in theory..."
- Style/naming issues with no functional impact (e.g., "misleading name" without a concrete lookup failure)
- Redundant/duplicate reports of the same underlying bug in different files — pick the ONE best instance
- Architectural opinions: "should use X pattern instead of Y"
- Missing error handling for unlikely scenarios

Classify each bug:
- "critical": crashes (NPE, TypeError, NoMethodError), data corruption, security vulnerabilities with concrete exploit path, complete feature breakage
- "high": logic errors producing wrong values, race conditions with concrete trigger scenario, silent failures where operations are skipped, broken callers due to API changes
- "medium": test correctness bugs (wrong assertion, mismatched description), dead code (unreachable branches, always-false conditions), wrong error messages that mislead debugging

For each issue, provide a short canonical_key (e.g., "null-deref-mainHostDestinationCalendar", "wrong-operator-and-vs-or").

Respond with ONLY this JSON, no other text:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet or quote from the review", "severity": "critical|high|medium", "file": "path/to/file", "canonical_key": "short-unique-bug-id"}]}

Return {"issues": []} if no concrete issues were found.

--- BEGIN CODE REVIEW ---
${extractSource}
--- END CODE REVIEW ---`;

  process.stderr.write(`[review-v2] Running extraction with ${provider}/${modelId} (separate call)...\n`);

  const extractCtx: Context = {
    systemPrompt: "You extract structured bug reports from code review text. Extract ONLY concrete correctness bugs: crashes, wrong behavior, broken callers, wrong values, logic errors, race conditions with concrete triggers, dead/unreachable code, test correctness bugs. DO NOT extract: style preferences, hardening suggestions (missing validation, missing rate limiting), theoretical edge cases, architectural opinions, or duplicate instances of the same bug. Be selective — quality over quantity.",
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

  process.stderr.write(`[review-v2] === EXTRACTION RESPONSE ===\n${extractionText}\n=== END EXTRACTION RESPONSE ===\n`);

  // Parse extracted candidates
  let candidates: (RawIssue & { canonical_key?: string })[] = [];
  try {
    const json = extractJson(extractionText);
    const parsed = JSON.parse(json);
    if (Array.isArray(parsed.issues)) candidates = parsed.issues;
  } catch {
    process.stderr.write(`[review-v2] WARNING: Extraction JSON parse failed\n`);
  }
  process.stderr.write(`[review-v2] Extracted ${candidates.length} candidates\n`);

  // ── Deduplicate by canonical_key ──
  const seen = new Set<string>();
  const deduped: typeof candidates = [];
  for (const c of candidates) {
    const key = c.canonical_key?.toLowerCase().replace(/[^a-z0-9]/g, "") ?? "";
    if (key && seen.has(key)) {
      process.stderr.write(`[review-v2] Dedup: skipping duplicate key "${c.canonical_key}"\n`);
      continue;
    }
    if (key) seen.add(key);
    deduped.push(c);
  }
  process.stderr.write(`[review-v2] After dedup: ${deduped.length} candidates (removed ${candidates.length - deduped.length})\n`);

  // ── Lenient validation filter (v8): drop only obvious non-bugs ──
  const validateModel = getModel(provider as any, modelId);
  const validated: typeof deduped = [];

  if (deduped.length > 0) {
    const issueList = deduped.map((c, i) =>
      `[${i}] [${c.severity}] ${c.issue} | file: ${c.file} | evidence: ${(c.evidence || "").slice(0, 200)}`
    ).join("\n");

    const validatePrompt = `You are a code review quality filter. Below are candidate bugs extracted from a code review. For each candidate, decide: KEEP or DROP.

KEEP if the candidate describes a CONCRETE functional problem:
- Crashes, null dereferences, type errors at runtime
- Logic errors: wrong operator, inverted condition, wrong variable, wrong return value
- Broken callers: API changed but callers not updated
- Race conditions with a concrete trigger scenario
- Silent failures: operations skipped due to fire-and-forget, wrong branch taken
- Security vulnerabilities with a concrete exploit path
- Dead code: unreachable branches, always-true/false conditions
- Test bugs: wrong assertion value, wrong HTTP method, mismatched test name
- Wrong error messages that actively mislead (e.g., says "login" in a "disable" endpoint)
- Typos that cause functional failures (e.g., misspelled method name causing lookup miss)

DROP if the candidate is:
- A hardening suggestion: "add rate limiting", "add validation", "add upper bound", "add error handling"
- A theoretical concern: "if someone were to...", "in a future scenario..."
- A style/naming preference with no runtime impact
- An architectural opinion: "should use X pattern"
- A duplicate of another candidate (same root cause, different file)
- Missing feature rather than broken feature

Respond with ONLY a JSON array of indices to KEEP, e.g.: [0, 1, 3, 5]
If all should be kept: return all indices. If none: return [].

CANDIDATES:
${issueList}`;

    const validateCtx: Context = {
      systemPrompt: "You filter code review candidates. Keep concrete bugs. Drop hardening suggestions, style nits, theoretical concerns, and duplicates. When uncertain, KEEP.",
      messages: [{ role: "user" as const, content: validatePrompt, timestamp: Date.now() }],
    };

    let validateText = "";
    try {
      const validateResult = await completeSimple(validateModel, validateCtx, {
        temperature: 0,
        maxTokens: 1024,
      });
      for (const part of validateResult.content) {
        if (part.type === "text") validateText += part.text;
      }
    } catch (e) {
      process.stderr.write(`[review-v2] Validation call failed: ${e}\n`);
      // On failure, keep all candidates
      validated.push(...deduped);
    }

    if (validated.length === 0) {
      // Parse the keep indices
      try {
        const match = validateText.match(/\[[\d\s,]*\]/);
        if (match) {
          const keepIndices: number[] = JSON.parse(match[0]);
          for (const idx of keepIndices) {
            if (idx >= 0 && idx < deduped.length) {
              validated.push(deduped[idx]);
            }
          }
          process.stderr.write(`[review-v2] Validation: kept ${validated.length}/${deduped.length} (dropped ${deduped.length - validated.length})\n`);
          // Log what was dropped
          for (let i = 0; i < deduped.length; i++) {
            if (!keepIndices.includes(i)) {
              process.stderr.write(`[review-v2] Validation dropped [${i}]: ${deduped[i].canonical_key} — ${deduped[i].issue.slice(0, 80)}\n`);
            }
          }
        } else {
          process.stderr.write(`[review-v2] Validation parse failed, keeping all\n`);
          validated.push(...deduped);
        }
      } catch {
        process.stderr.write(`[review-v2] Validation JSON parse failed, keeping all\n`);
        validated.push(...deduped);
      }
    }
  }

  process.stderr.write(`[review-v2] After validation: ${validated.length} candidates\n`);

  // Sort by severity, cap at 6
  let issues = validated;
  const severityOrder: Record<string, number> = { critical: 0, high: 1, medium: 2 };
  issues.sort((a, b) => (severityOrder[a.severity] ?? 3) - (severityOrder[b.severity] ?? 3));
  if (issues.length > 6) {
    issues = issues.slice(0, 6);
    process.stderr.write(`[review-v2] Capped to 6 issues\n`);
  }

  const elapsed = Date.now() - t0;
  process.stderr.write(`[review-v2] Total: ${(elapsed / 1000).toFixed(1)}s, ${toolCalls} tool calls, ${issues.length} final issues\n`);

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
