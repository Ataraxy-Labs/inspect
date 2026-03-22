/**
 * Parallel slice-based code review using pi-agent-core subagents.
 *
 * Architecture (inspired by mi-pi's subagent pattern):
 * 1. Rust pipeline builds change-impact slices (caller→callee, override→interface)
 * 2. This module spawns one Agent per slice, running in parallel
 * 3. Each agent reasons about 2-5 bug hypotheses with minimal tool use
 * 4. Results are merged and deduped
 *
 * Key difference from mi-pi's piSpawn: we use Agent directly (in-process)
 * instead of spawning pi CLI subprocesses. This avoids process overhead
 * and lets us share tools/model config.
 */
import { Agent } from "@mariozechner/pi-agent-core";
import type { Model } from "@mariozechner/pi-ai";
import type { ToolDefinition } from "@mariozechner/pi-coding-agent";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ReviewSlice {
  /** Unique ID for this slice */
  id: string;
  /** Human-readable title (e.g., "Auth flow: TagDevice → CreateOrUpdateDevice") */
  title: string;
  /** The user prompt containing code snippets, call chains, and hypotheses */
  prompt: string;
  /** If true, all code is inline — skip tools entirely for speed */
  noTools?: boolean;
}

export interface SliceIssue {
  issue: string;
  evidence: string;
  severity: "critical" | "high" | "medium";
  file: string;
  /** Which slice produced this issue */
  slice_id: string;
}

export interface SliceResult {
  slice_id: string;
  issues: SliceIssue[];
  tool_calls: number;
  elapsed_ms: number;
  error?: string;
}

export interface ParallelReviewResult {
  slice_results: SliceResult[];
  merged_issues: SliceIssue[];
  total_tool_calls: number;
  total_elapsed_ms: number;
}

// ---------------------------------------------------------------------------
// System prompt — optimized for slice-based review
// ---------------------------------------------------------------------------

const SLICE_SYSTEM_PROMPT = `You are an expert code reviewer analyzing a change-impact slice — a focused set of related code changes connected by caller→callee relationships.

WORKFLOW:
1. FIRST: Read the slice carefully. Check each detector finding and hypothesis — these are high-signal leads from static analysis.
2. Use tools to verify when needed (read source files, grep for callers). Be thorough — check return types, null paths, and all branches.
3. Report ALL real bugs you find, including medium-severity ones like dead code, misleading messages, and naming issues. Don't only report critical bugs.

IMPORTANT: When detector findings flag something (e.g., unreachable-branch, case-insensitive-compare), investigate them seriously. If the evidence confirms the finding, REPORT IT even if the severity is medium or low. These signals exist because similar patterns caused real bugs.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if genuinely no bugs found after investigating all findings.`;

// ---------------------------------------------------------------------------
// Concurrent execution helper (like mi-pi's mapConcurrent)
// ---------------------------------------------------------------------------

async function mapConcurrent<T, R>(
  items: T[],
  limit: number,
  fn: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let nextIndex = 0;

  async function worker() {
    while (nextIndex < items.length) {
      const i = nextIndex++;
      results[i] = await fn(items[i], i);
    }
  }

  const workers = Array.from({ length: Math.min(limit, items.length) }, () =>
    worker(),
  );
  await Promise.all(workers);
  return results;
}

// ---------------------------------------------------------------------------
// Single slice reviewer
// ---------------------------------------------------------------------------

async function runAgent(
  slice: ReviewSlice,
  model: Model,
  tools: ToolDefinition[],
  thinkingLevel: string,
  onLog?: (msg: string) => void,
): Promise<{ text: string; toolCalls: number }> {
  let toolCalls = 0;
  let finalText = "";

  const agent = new Agent({
    initialState: {
      systemPrompt: SLICE_SYSTEM_PROMPT,
      model,
      thinkingLevel: thinkingLevel as any,
      tools,
    },
    toolExecution: "parallel",
    beforeToolCall: async ({ toolCall, args }) => {
      toolCalls++;
      onLog?.(
        `  [${slice.id}] tool #${toolCalls}: ${toolCall.name}(${JSON.stringify(args).slice(0, 100)})`,
      );
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

  await agent.prompt(slice.prompt);
  await agent.waitForIdle();
  return { text: finalText, toolCalls };
}

const MAX_RETRIES = 2;

async function reviewSlice(
  slice: ReviewSlice,
  model: Model,
  tools: ToolDefinition[],
  thinkingLevel: string,
  onLog?: (msg: string) => void,
): Promise<SliceResult> {
  const start = Date.now();
  let totalToolCalls = 0;

  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      const { text, toolCalls } = await runAgent(slice, model, tools, thinkingLevel, onLog);
      totalToolCalls += toolCalls;

      const jsonStr = extractJson(text);
      const parsed = JSON.parse(jsonStr);
      const issues: SliceIssue[] = (parsed.issues ?? []).map((issue: any) => ({
        ...issue,
        slice_id: slice.id,
      }));

      const elapsed = Date.now() - start;
      onLog?.(
        `  [${slice.id}] done: ${issues.length} issues, ${totalToolCalls} tools, ${(elapsed / 1000).toFixed(1)}s`,
      );
      return { slice_id: slice.id, issues, tool_calls: totalToolCalls, elapsed_ms: elapsed };
    } catch (e: any) {
      if (attempt < MAX_RETRIES) {
        onLog?.(`  [${slice.id}] JSON parse failed (attempt ${attempt + 1}), retrying...`);
      } else {
        const elapsed = Date.now() - start;
        onLog?.(`  [${slice.id}] ERROR after ${attempt + 1} attempts: ${e.message}`);
        return {
          slice_id: slice.id,
          issues: [],
          tool_calls: totalToolCalls,
          elapsed_ms: elapsed,
          error: e.message,
        };
      }
    }
  }

  // unreachable but TS needs it
  return { slice_id: slice.id, issues: [], tool_calls: totalToolCalls, elapsed_ms: Date.now() - start };
}

// ---------------------------------------------------------------------------
// Parallel review orchestrator
// ---------------------------------------------------------------------------

export async function reviewSlicesParallel(
  slices: ReviewSlice[],
  model: Model,
  tools: ToolDefinition[],
  opts: {
    concurrency?: number;
    thinkingLevel?: string;
    onLog?: (msg: string) => void;
  } = {},
): Promise<ParallelReviewResult> {
  const concurrency = opts.concurrency ?? 4;
  const thinkingLevel = opts.thinkingLevel ?? "high";
  const onLog = opts.onLog ?? ((msg: string) => process.stderr.write(msg + "\n"));

  onLog(`[review] Starting parallel review: ${slices.length} slices, concurrency=${concurrency}`);

  const totalStart = Date.now();

  const sliceResults = await mapConcurrent(
    slices,
    concurrency,
    async (slice, i) => {
      onLog(`[review] Starting slice ${i + 1}/${slices.length}: ${slice.title}`);
      return reviewSlice(slice, model, tools, thinkingLevel, onLog);
    },
  );

  // Merge and deduplicate issues
  const allIssues = sliceResults.flatMap((r) => r.issues);
  const merged = deduplicateIssues(allIssues);

  const totalElapsed = Date.now() - totalStart;
  const totalTools = sliceResults.reduce((sum, r) => sum + r.tool_calls, 0);

  onLog(
    `[review] Done: ${merged.length} unique issues from ${allIssues.length} raw, ` +
    `${totalTools} total tool calls, ${(totalElapsed / 1000).toFixed(1)}s`,
  );

  return {
    slice_results: sliceResults,
    merged_issues: merged,
    total_tool_calls: totalTools,
    total_elapsed_ms: totalElapsed,
  };
}

// ---------------------------------------------------------------------------
// Deduplication
// ---------------------------------------------------------------------------

function deduplicateIssues(issues: SliceIssue[]): SliceIssue[] {
  const seen = new Map<string, SliceIssue>();
  for (const issue of issues) {
    // Key by file + first 80 chars of issue description (rough dedup)
    const key = `${issue.file}::${issue.issue.slice(0, 80).toLowerCase()}`;
    if (!seen.has(key)) {
      seen.set(key, issue);
    } else {
      // Keep the higher severity one
      const existing = seen.get(key)!;
      const severityOrder = { critical: 3, high: 2, medium: 1 };
      if (
        (severityOrder[issue.severity] ?? 0) >
        (severityOrder[existing.severity] ?? 0)
      ) {
        seen.set(key, issue);
      }
    }
  }
  return [...seen.values()];
}

// ---------------------------------------------------------------------------
// JSON extraction helper
// ---------------------------------------------------------------------------

function extractJson(text: string): string {
  const trimmed = text.trim();

  // 1. Try JSON inside ```json fences
  const fenceMatch = trimmed.match(/```json\s*([\s\S]*?)```/);
  if (fenceMatch) {
    const inner = fenceMatch[1].trim();
    if (inner.startsWith("{")) return inner;
  }

  // 2. Try any ``` fences
  const anyFence = trimmed.match(/```\s*([\s\S]*?)```/);
  if (anyFence) {
    const inner = anyFence[1].trim();
    if (inner.startsWith("{")) return inner;
  }

  // 3. Find {"issues" specifically — avoids matching Go/Java code braces
  const issuesIdx = trimmed.indexOf('{"issues"');
  if (issuesIdx !== -1) {
    const sub = trimmed.slice(issuesIdx);
    // Find the matching closing brace by counting depth
    let depth = 0;
    for (let i = 0; i < sub.length; i++) {
      if (sub[i] === "{") depth++;
      else if (sub[i] === "}") {
        depth--;
        if (depth === 0) return sub.slice(0, i + 1);
      }
    }
  }

  // 4. Last resort: first { to last }
  const first = trimmed.indexOf("{");
  const last = trimmed.lastIndexOf("}");
  if (first !== -1 && last > first) return trimmed.slice(first, last + 1);

  return trimmed;
}
