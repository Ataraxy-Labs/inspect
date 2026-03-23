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

// ── System prompt (same as v1 — thorough code review) ──
function buildSystemPrompt(repoDir: string): string {
  const today = new Date().toLocaleDateString("en-US", {
    weekday: "short", month: "short", day: "numeric", year: "numeric",
  });
  return `You are an expert senior engineer with deep knowledge of software engineering best practices, security, performance, and maintainability.

Your task is to perform a thorough code review of the provided diff description. The diff description might be a git or bash command that generates the diff or a description of the diff which can then be used to generate the git or bash command to generate the full diff.

After reading the diff, do the following:
1. Generate a high-level summary of the changes in the diff.
2. Go file-by-file and review each changed hunk.
3. Comment on what changed in that hunk (including the line range) and how it relates to other changed hunks and code, reading any other relevant files. Also call out bugs, hackiness, unnecessary code, or too much shared mutable state.
4. For any changed or removed public APIs, use Grep to search for callers repo-wide. Check if callers were updated to match the new signature/behavior. Flag broken callers.

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

// ── Build user message: entities + detector hints + diff ──
function buildUserMessage(
  prTitle: string,
  diff: string,
  findings: DetectorFinding[],
  entities: EntityReview[],
): string {
  const parts: string[] = [];

  parts.push(`Review the following diff: ${prTitle || "Unstaged changes"}`);

  // For large diffs, add explicit priority instruction
  const isLargeDiff = diff.length > 80_000;
  if (isLargeDiff && entities.length > 0) {
    parts.push("");
    parts.push("⚠️ LARGE DIFF — This PR has a large diff that may be truncated below. To ensure full coverage:");
    parts.push("1. Start by reviewing ALL BEHAVIORAL and high-risk entities listed below — use Read tool if their code is not in the visible diff");
    parts.push("2. For each BEHAVIORAL entity, Read the full function and check for correctness bugs");
    parts.push("3. Do NOT spend all your time on the first few files — budget attention across all high-risk entities");
    parts.push("4. Use web_search to verify unfamiliar APIs, CSS properties, or library methods if unsure whether something is correct");
  }

  // Entity triage — full map of what changed
  const triage = buildEntityTriage(entities);
  if (triage) {
    parts.push("");
    parts.push(triage);
    parts.push("");
  }

  // Detector findings as noisy hints
  if (findings.length > 0) {
    parts.push("");
    parts.push(`--- DETECTOR FINDINGS (${findings.length}) ---`);
    parts.push("Static analysis flagged these potential issues. Many may be false positives. Use them as hints for where to look, but only report an issue if you can independently verify a concrete bug:");
    parts.push("");
    for (const f of findings) {
      parts.push(`⚠️ [${f.severity.toUpperCase()}] ${f.rule_id}: \`${f.entity_name}\` in ${f.file_path}:${f.start_line}`);
      parts.push(`   ${f.message}`);
      parts.push(`   Evidence: ${f.evidence}`);
    }
    parts.push("--- END DETECTOR FINDINGS ---");
    parts.push("");
  }

  // Cap diff at 120k chars
  if (diff.length > 120_000) {
    parts.push(diff.slice(0, 120_000));
    parts.push("\n... (diff truncated — use Read tool to inspect remaining files listed in CHANGED ENTITIES above)");
  } else {
    parts.push(diff);
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
  const extractionPrompt = `You are a code review extraction assistant. Below is a detailed code review written by a senior engineer. Your job is to extract concrete correctness issues from the review text.

RULES:
- Extract issues where the reviewer identified a concrete functional impact (crashes, wrong behavior, data loss, security holes, race conditions, silent failures, incorrect values)
- Include issues phrased as suggestions IF they describe a real functional problem (e.g., "consider using atomic increment" because the current code has a race condition)
- Each issue must have a specific code reference (file, function, variable, or line)
- DO NOT include: pure style/formatting preferences, naming-only nits, general "code smell" observations without functional impact, performance observations without correctness impact, missing test coverage notes, or architectural opinions
- DO NOT invent issues not discussed in the review
- When in doubt about whether something has functional impact, INCLUDE it

Classify each bug:
- "critical": crashes, data corruption, security vulnerabilities, complete feature breakage
- "high": logic errors, wrong values, race conditions, silent failures
- "medium": edge cases, misleading behavior, dead code with consequences

Respond with ONLY this JSON, no other text:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet or quote from the review", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no concrete issues were found.

--- BEGIN CODE REVIEW ---
${reviewText}
--- END CODE REVIEW ---`;

  process.stderr.write(`[review-v2] Running extraction with ${provider}/${modelId} (separate call)...\n`);

  const extractCtx: Context = {
    systemPrompt: "You extract structured bug reports from code review text. Extract issues with concrete functional impact. Include suggestions that describe real functional problems. Exclude pure style nits, naming preferences, and architectural opinions without correctness impact.",
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
