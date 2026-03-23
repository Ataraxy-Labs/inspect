#!/usr/bin/env node
/**
 * Simple-diff review: ONE pi-agent call with Amp's code-review prompt + tools.
 *
 * Input (stdin JSON):
 *   { repo_dir, diff, findings, pr_title, provider?, model? }
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
import { Agent, type AgentTool } from "@mariozechner/pi-agent-core";
import { Type } from "@sinclair/typebox";
import Parallel from "parallel-web";
import { readFileSync } from "fs";
import { resolve } from "path";
import type { DetectorFinding, ValidateOutput } from "./types.js";
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

// ── Amp's code-review system prompt (verbatim from their config) ──
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

Current working directory (cwd): ${repoDir}
Today's date: ${today}`;
}

// ── Build user message: diff + detector hints ──
function buildUserMessage(
  prTitle: string,
  diff: string,
  findings: DetectorFinding[],
): string {
  const parts: string[] = [];

  parts.push(`Review the following diff: ${prTitle || "Unstaged changes"}`);

  // Inject detector findings as high-signal hints
  if (findings.length > 0) {
    parts.push("");
    parts.push(`--- DETECTOR FINDINGS (${findings.length}) ---`);
    parts.push("Static analysis flagged these potential issues. Investigate each one by reading the relevant code:");
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
    parts.push("\n... (diff truncated)");
  } else {
    parts.push(diff);
  }

  return parts.join("\n");
}

// ── Extract JSON from agent response ──
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

// ── Parse free-form Amp review into structured issues ──
interface RawIssue {
  issue: string;
  evidence: string;
  severity: "critical" | "high" | "medium";
  file: string;
}

function parseReviewIntoIssues(text: string): RawIssue[] {
  // First try: agent returned JSON with {"issues": [...]}
  try {
    const json = extractJson(text);
    const parsed = JSON.parse(json);
    if (Array.isArray(parsed.issues)) return parsed.issues;
  } catch {}

  // Second try: parse free-form Amp-style review into issues
  // Look for bug-like patterns in the text
  const issues: RawIssue[] = [];
  const bugPatterns = [
    /\*\*(?:Bug|Issue|Problem|Error|Warning)\*\*[:\s]*(.+?)(?:\n|$)/gi,
    /(?:^|\n)[-•]\s*\*\*(.+?)\*\*/gm,
  ];

  // Extract file references
  const fileRefs = text.match(/`([^`]+\.[a-z]{1,5})`/g)?.map(m => m.slice(1, -1)) ?? [];

  // Split by file sections (## filename pattern)
  const sections = text.split(/^##\s+/m).filter(Boolean);
  for (const section of sections) {
    const fileMatch = section.match(/^[`*]*([^\s`*]+\.[a-z]{1,5})[`*]*/);
    const file = fileMatch?.[1] ?? "";

    // Look for lines mentioning bugs/issues
    const lines = section.split("\n");
    for (const line of lines) {
      const lower = line.toLowerCase();
      if (
        (lower.includes("bug") || lower.includes("issue") || lower.includes("error") ||
         lower.includes("wrong") || lower.includes("missing") || lower.includes("broken") ||
         lower.includes("crash") || lower.includes("race") || lower.includes("leak") ||
         lower.includes("dead code") || lower.includes("unreachable")) &&
        line.trim().length > 20
      ) {
        const severity: "critical" | "high" | "medium" =
          lower.includes("critical") || lower.includes("crash") || lower.includes("security")
            ? "critical"
            : lower.includes("bug") || lower.includes("wrong") || lower.includes("broken")
              ? "high"
              : "medium";

        issues.push({
          issue: line.replace(/^[-•*#>\s]+/, "").trim(),
          evidence: "",
          severity,
          file: file || fileRefs[0] || "unknown",
        });
      }
    }
  }

  return issues.slice(0, 10);
}

// ── Main ──
async function main() {
  const raw = await readStdin();
  const input = JSON.parse(raw);

  const repoDir: string = input.repo_dir;
  const diff: string = input.diff ?? "";
  const findings: DetectorFinding[] = input.findings ?? [];
  const prTitle: string = input.pr_title ?? "";
  const provider = input.provider ?? "anthropic";
  const modelId = input.model ?? "claude-opus-4-6";

  process.stderr.write(`[review-diff] Single-call review: ${diff.length} chars diff, ${findings.length} findings\n`);

  if (!diff) {
    const output: ValidateOutput = { verdicts: [] };
    process.stdout.write(JSON.stringify(output) + "\n");
    return;
  }

  // Setup model
  const model = getModel(provider as any, modelId);

  // Setup tools — same as Amp's code review: Bash, Read, Grep, glob/Find
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
  ];

  // Build prompts
  const systemPrompt = buildSystemPrompt(repoDir);
  const userMessage = buildUserMessage(prTitle, diff, findings);

  process.stderr.write(`[review-diff] System prompt: ${systemPrompt.length} chars\n`);
  process.stderr.write(`[review-diff] User message: ${userMessage.length} chars\n`);

  // Single agent call with thinking=HIGH (matches Amp's thinkingConfig)
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
  process.stderr.write(`[review-diff] Review done: ${toolCalls} tool calls\n`);
  process.stderr.write(`[review-diff] === RAW RESPONSE ===\n${reviewText}\n=== END RAW RESPONSE ===\n`);

  // ── Turn 2: Separate extraction call on raw review text ──
  // Use a FRESH model call (not the agent) to parse the raw review into structured JSON.
  // This avoids the agent "forgetting" findings from its own review.
  const extractModel = getModel(provider as any, modelId);
  const extractionPrompt = `You are a code review extraction assistant. Below is a detailed code review written by a senior engineer. Your job is to extract ONLY concrete correctness bugs from the review text.

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

Return {"issues": []} if no concrete bugs were found.

--- BEGIN CODE REVIEW ---
${reviewText}
--- END CODE REVIEW ---`;

  process.stderr.write(`[review-diff] Running extraction with ${provider}/${modelId} (separate call)...\n`);

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
    process.stderr.write(`[review-diff] Extraction call failed: ${e}\n`);
  }

  const elapsed = Date.now() - t0;
  process.stderr.write(`[review-diff] Extraction done: ${(elapsed / 1000).toFixed(1)}s total\n`);
  process.stderr.write(`[review-diff] === EXTRACTION RESPONSE ===\n${extractionText}\n=== END EXTRACTION RESPONSE ===\n`);

  // Parse the structured JSON response
  let issues: RawIssue[] = [];
  try {
    const json = extractJson(extractionText);
    const parsed = JSON.parse(json);
    if (Array.isArray(parsed.issues)) issues = parsed.issues;
  } catch {
    process.stderr.write(`[review-diff] WARNING: JSON parse failed, falling back to heuristic parser\n`);
    issues = parseReviewIntoIssues(reviewText);
  }
  process.stderr.write(`[review-diff] Parsed ${issues.length} issues\n`);

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
