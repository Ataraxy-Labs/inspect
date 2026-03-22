#!/usr/bin/env node
/**
 * Benchmark entry point: reads JSON from stdin, runs slice-parallel review, outputs JSON to stdout.
 *
 * Input (stdin JSON):
 *   { repo_dir, entity_reviews, findings, pr_title?, diff?, provider?, model? }
 *
 * Output (stdout JSON):
 *   { verdicts: [{ rule_id, entity_name, verdict, explanation }] }
 *
 * Called by benchmarks/run_martian_official.py
 */
import { getModel } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";
import { readFileSync } from "fs";
import { resolve } from "path";
import { reviewSlicesParallel, type ReviewSlice } from "./review-parallel.js";
import type { EntityReview, DetectorFinding, ValidateOutput } from "./types.js";

// ── Load .env ──
const envPath = resolve(import.meta.dirname, "../../.env");
try {
  for (const line of readFileSync(envPath, "utf-8").split("\n")) {
    const m = line.match(/^([A-Z_]+)=(.+)$/);
    if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
  }
} catch {}

// Override API key for localhost proxy
// API key loaded from .env

// ── Read stdin ──
async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf-8");
}

// ── Test file filter ──
function isTest(path: string): boolean {
  return /test|spec|Test\.java|_test\.|\.test\./i.test(path);
}

// ── Slice cluster types ──
interface SliceCluster {
  anchor: EntityReview;
  related: EntityReview[];
  findings: DetectorFinding[];
}

// ── Build clusters from entity graph ──
function buildClusters(
  entityReviews: EntityReview[],
  findings: DetectorFinding[],
  entityMap: Map<string, EntityReview>,
): SliceCluster[] {
  const topEntities = entityReviews
    .slice(0, 30);

  const topEntityIds = new Set(topEntities.map((e) => e.entity_id));
  const findingEntityIds = new Set(findings.map((f) => f.entity_id));

  for (const e of entityReviews) {
    if (
      findingEntityIds.has(e.entity_id) &&
      !topEntityIds.has(e.entity_id)
    ) {
      topEntities.push(e);
      topEntityIds.add(e.entity_id);
    }
  }

  // Small-PR mode: include all entities if few total
  if (entityReviews.length <= 15) {
    for (const e of entityReviews) {
      if (!topEntityIds.has(e.entity_id)) {
        topEntities.push(e);
        topEntityIds.add(e.entity_id);
      }
    }
  }

  const used = new Set<string>();
  const clusters: SliceCluster[] = [];

  function collectReachable(
    start: EntityReview,
    clusterIds: Set<string>,
    maxHops: number,
  ): EntityReview[] {
    const collected: EntityReview[] = [];
    let frontier = [start];

    for (let hop = 0; hop < maxHops && frontier.length > 0; hop++) {
      const nextFrontier: EntityReview[] = [];
      for (const entity of frontier) {
        for (const [name, file] of entity.dependency_names) {
          if (collected.length >= 12) break;
          const dep = entityMap.get(`${name}::${file}`);
          if (dep && !clusterIds.has(dep.entity_id)) {
            collected.push(dep);
            clusterIds.add(dep.entity_id);
            nextFrontier.push(dep);
          }
        }
        for (const [name, file] of entity.dependent_names) {
          if (collected.length >= 12) break;
          const dep = entityMap.get(`${name}::${file}`);
          if (dep && !clusterIds.has(dep.entity_id)) {
            collected.push(dep);
            clusterIds.add(dep.entity_id);
            nextFrontier.push(dep);
          }
        }
      }
      if (collected.length >= 12) break;
      frontier = nextFrontier;
    }
    return collected;
  }

  for (const anchor of topEntities) {
    if (used.has(anchor.entity_id)) continue;
    if (anchor.entity_type === "file") continue;

    const clusterIds = new Set([anchor.entity_id]);
    const related = collectReachable(anchor, clusterIds, 3);
    const clusterFindings = findings.filter((f) => clusterIds.has(f.entity_id));

    if (related.length > 0 || clusterFindings.length > 0) {
      used.add(anchor.entity_id);
      for (const r of related) used.add(r.entity_id);
      clusters.push({ anchor, related, findings: clusterFindings });
    }
  }

  function getPackageDir(filePath: string): string {
    const parts = filePath.split("/");
    return parts.slice(0, -1).join("/");
  }

  function mergeClusters(input: SliceCluster[]): SliceCluster[] {
    const merged: SliceCluster[] = [];
    const consumed = new Set<number>();

    for (let i = 0; i < input.length; i++) {
      if (consumed.has(i)) continue;
      const cluster = { ...input[i], related: [...input[i].related], findings: [...input[i].findings] };
      const clusterDirs = new Set<string>();
      clusterDirs.add(getPackageDir(cluster.anchor.file_path));
      for (const r of cluster.related) clusterDirs.add(getPackageDir(r.file_path));

      for (let j = i + 1; j < input.length; j++) {
        if (consumed.has(j)) continue;
        const other = input[j];
        const otherDirs = new Set<string>();
        otherDirs.add(getPackageDir(other.anchor.file_path));
        for (const r of other.related) otherDirs.add(getPackageDir(r.file_path));

        const sharedDirs = [...clusterDirs].filter((d) => otherDirs.has(d));
        const combinedSize = 1 + cluster.related.length + 1 + other.related.length;
        if (sharedDirs.length > 0 && combinedSize <= 8) {
          const existingIds = new Set([cluster.anchor.entity_id, ...cluster.related.map((r) => r.entity_id)]);
          if (!existingIds.has(other.anchor.entity_id)) {
            cluster.related.push(other.anchor);
          }
          for (const r of other.related) {
            if (!existingIds.has(r.entity_id)) {
              cluster.related.push(r);
              existingIds.add(r.entity_id);
            }
          }
          for (const f of other.findings) {
            if (!cluster.findings.some((cf) => cf.entity_id === f.entity_id && cf.rule_id === f.rule_id)) {
              cluster.findings.push(f);
            }
          }
          for (const d of otherDirs) clusterDirs.add(d);
          consumed.add(j);
          if (other.anchor.risk_score > cluster.anchor.risk_score) {
            cluster.related.push(cluster.anchor);
            cluster.related = cluster.related.filter((r) => r.entity_id !== other.anchor.entity_id);
            cluster.anchor = other.anchor;
          }
        }
      }
      merged.push(cluster);
    }
    return merged;
  }

  const merged = mergeClusters(clusters);

  const claimedIds = new Set<string>();
  for (const c of merged) {
    claimedIds.add(c.anchor.entity_id);
    for (const r of c.related) claimedIds.add(r.entity_id);
  }
  for (const e of topEntities) {
    if (claimedIds.has(e.entity_id)) continue;
    if (e.entity_type === "file") continue;
    if (!e.after_content && !e.before_content) continue;
    // Skip entities with empty or near-empty content — they waste slice slots
    const content = e.after_content ?? e.before_content ?? "";
    if (content.trim().length < 20) continue;
    // Skip standalone one-liner variables/properties without findings — no review value
    if (content.split("\n").length <= 2 && !findings.some((f) => f.entity_id === e.entity_id)) continue;
    if (e.entity_type === "chunk" && content.split("\n").length < 8 && !findings.some((f) => f.entity_id === e.entity_id)) continue;
    const entityFindings = findings.filter((f) => f.entity_id === e.entity_id);
    merged.push({ anchor: e, related: [], findings: entityFindings });
  }

  merged.sort((a, b) => {
    const aHasFindings = a.findings.length > 0 ? 1 : 0;
    const bHasFindings = b.findings.length > 0 ? 1 : 0;
    if (bHasFindings !== aHasFindings) return bHasFindings - aHasFindings;
    const aMaxRisk = Math.max(a.anchor.risk_score, ...a.related.map((r) => r.risk_score));
    const bMaxRisk = Math.max(b.anchor.risk_score, ...b.related.map((r) => r.risk_score));
    return bMaxRisk - aMaxRisk;
  });

  const selected: SliceCluster[] = [];
  const coveredIds = new Set<string>();
  for (const c of merged) {
    const allIds = [c.anchor.entity_id, ...c.related.map((r) => r.entity_id)];
    const overlapCount = allIds.filter((id) => coveredIds.has(id)).length;
    if (allIds.length > 1 && overlapCount / allIds.length > 0.5) continue;
    selected.push(c);
    for (const id of allIds) coveredIds.add(id);
  }

  return selected.slice(0, 10);
}

// ── Extract diff hunks for specific files ──
function extractDiffForFiles(diff: string, filePaths: string[]): string {
  if (!diff || filePaths.length === 0) return "";
  
  const pathSet = new Set(filePaths);
  const diffLines = diff.split("\n");
  const relevantChunks: string[] = [];
  let currentFile = "";
  let inRelevantFile = false;
  let currentChunk: string[] = [];

  for (const line of diffLines) {
    const fileMatch = line.match(/^diff --git a\/(.+?) b\/(.+?)$/);
    if (fileMatch) {
      // Flush previous chunk
      if (inRelevantFile && currentChunk.length > 0) {
        relevantChunks.push(currentChunk.join("\n"));
      }
      currentFile = fileMatch[2];
      inRelevantFile = pathSet.has(currentFile);
      currentChunk = inRelevantFile ? [line] : [];
      continue;
    }
    if (inRelevantFile) {
      currentChunk.push(line);
    }
  }
  // Flush last chunk
  if (inRelevantFile && currentChunk.length > 0) {
    relevantChunks.push(currentChunk.join("\n"));
  }

  const result = relevantChunks.join("\n");
  // Cap at 8k chars to avoid blowing up the prompt
  if (result.length > 8000) {
    return result.slice(0, 8000) + "\n... (diff truncated)";
  }
  return result;
}

// ── Generate entity-specific questions from code patterns ──
function generateEntityQuestions(entity: EntityReview, _allEntities: EntityReview[]): string[] {
  const questions: string[] = [];
  const content = entity.after_content ?? "";
  if (!content) return questions;

  const fileName = entity.file_path.split("/").pop() ?? "";
  const baseName = fileName.replace(/\.\w+$/, "").toLowerCase();

  // Naming: export name vs filename mismatch
  if (entity.change_type === "Added" && entity.entity_type === "function") {
    const nameLower = entity.entity_name.toLowerCase();
    if (baseName && !baseName.includes(nameLower) && !nameLower.includes(baseName) && baseName !== "index") {
      questions.push(`\`${entity.entity_name}\` is exported from \`${fileName}\`. Does the name match the file's purpose?`);
    }
  }

  // Error messages: check if throw/Error strings reference wrong operations
  const errorStrings = content.match(/(?:throw\s+new\s+\w*Error|Error\(|error:\s*)[("'`]([^"'`]{10,80})[)"'`]/gi);
  if (errorStrings && errorStrings.length > 0) {
    questions.push(`\`${entity.entity_name}\` in \`${fileName}\` contains error messages. Do they match the endpoint/function's actual purpose?`);
  }

  // String comparisons without case normalization
  if (content.includes("indexOf(") || content.includes("includes(") || content.includes(".find(")) {
    if (!content.includes("toLowerCase") && !content.includes("toUpperCase") && !content.includes("localeCompare")) {
      if (content.includes("code") || content.includes("token") || content.includes("key") || content.includes("hex") || content.includes("hash")) {
        questions.push(`\`${entity.entity_name}\` uses indexOf/includes/find on what appears to be user input (codes/tokens). Is the comparison case-sensitive intentionally?`);
      }
    }
  }

  // Read-modify-write without transaction
  if ((content.includes("findFirst") || content.includes("findUnique") || content.includes("findOne")) &&
      (content.includes(".update(") || content.includes(".delete(")) &&
      !content.includes("transaction") && !content.includes("$transaction")) {
    questions.push(`\`${entity.entity_name}\` reads then writes DB records without a transaction. Can concurrent requests cause a race condition?`);
  }

  return questions;
}

// ── Convert cluster to review slice ──
function clusterToSlice(
  cluster: SliceCluster,
  idx: number,
  entityMap: Map<string, EntityReview>,
  diff: string,
): ReviewSlice {
  const { anchor, related, findings } = cluster;
  const allEntities = [anchor, ...related];

  const lines: string[] = [];
  lines.push(`You are reviewing a focused code change slice. Find correctness bugs — real problems that cause wrong behavior, crashes, or security issues.

RULES:
- Use the read tool to check the actual source file when you need surrounding context. Snippets may be incomplete.
- Use grep to find callers/callees when a signature or behavior changed.
- Report bugs you can explain with a concrete code path. "X calls Y which does Z" — not vague concerns.
- Do NOT report: style issues, missing tests, documentation, suggestions, or purely theoretical concerns.
- Do NOT report the same bug twice even if visible from multiple entities.
- Maximum 5 issues per slice. Prioritize high-severity bugs but include medium/low if real.

WHAT COUNTS AS A BUG:
- Wrong variable/function in a condition or expression
- Swapped or missing arguments in a function call
- Inverted logic (== vs !=, && vs ||, wrong polarity)
- Missing null/error check that will crash or produce wrong results
- forEach+async without await (fire-and-forget)
- Interface/API contract violation (method signature doesn't match interface, wrong arg types)
- Case-sensitive comparison where case-insensitive is needed (user input, tokens, emails, hex codes)
- Error/log message that references wrong operation (copy-paste from different endpoint)
- Exported name that contradicts the filename (confusing imports)
- Compilation/type error (wrong number of args, wrong types passed)
- Misleading error returns (returning wrong error code for the actual failure)
- Removed guard/assertion that protected against a real scenario
- Dead code branches that indicate a logic error (unreachable else)

WHAT IS NOT A BUG:
- Redundant code or unnecessary checks (harmless)
- Missing error handling for unlikely scenarios
- Code that "could be better" but isn't wrong
- Style or naming preferences with no functional impact`);
  lines.push("");
  lines.push(`# Slice ${idx + 1}: ${anchor.entity_name} (${anchor.entity_type}, ${anchor.change_type})`);
  lines.push(`## Anchor: \`${anchor.entity_name}\` in ${anchor.file_path}:${anchor.start_line}-${anchor.end_line}`);
  lines.push(`risk=${anchor.risk_level}(${anchor.risk_score.toFixed(2)}) blast=${anchor.blast_radius} deps=${anchor.dependent_count}`);
  lines.push("");

  if (findings.length > 0) {
    lines.push("## Detector Findings (investigate each — these are signals from static analysis)");
    for (const f of findings) {
      lines.push(`- **[${f.severity.toUpperCase()}] ${f.rule_id}**: \`${f.entity_name}\` — ${f.message}`);
      lines.push(`  Evidence: \`${f.evidence}\``);
    }
    lines.push("");
  }

  lines.push("## Code");
  // Tell agent which files to read for full context
  const uniqueFiles = [...new Set(allEntities.map((e) => e.file_path))];
  lines.push(`**Files to read for full context:** ${uniqueFiles.join(", ")}`);
  lines.push("");
  for (const e of allEntities) {
    const content = e.after_content ?? e.before_content ?? "";
    if (!content) continue;

    lines.push(`### \`${e.entity_name}\` (${e.entity_type}, ${e.change_type}) in ${e.file_path}:${e.start_line}-${e.end_line}`);

    const callers = e.dependent_names
      .slice(0, 3)
      .map(([n, f]) => `${n} (${f.split("/").pop()})`)
      .join(", ");
    const callees = e.dependency_names
      .filter(([n, f]) => entityMap.has(`${n}::${f}`))
      .slice(0, 3)
      .map(([n]) => n)
      .join(", ");
    if (callers) lines.push(`Called by: ${callers}`);
    if (callees) lines.push(`Calls: ${callees}`);

    const display = content.length > 2000 ? content.slice(0, 2000) + "\n... (truncated)" : content;
    lines.push("```");
    lines.push(display);
    lines.push("```");
    lines.push("");
  }

  // Add raw diff hunks for context
  const sliceFiles = [...new Set(allEntities.map((e) => e.file_path))];
  const diffHunks = extractDiffForFiles(diff, sliceFiles);
  if (diffHunks) {
    lines.push("## Raw Diff (what actually changed — look for wrong strings, copy-paste errors, case-sensitivity issues)");
    lines.push("```diff");
    lines.push(diffHunks);
    lines.push("```");
    lines.push("");
  }

  // Only finding-backed hypotheses (prune speculative ones that cause FPs)
  lines.push("## Targeted questions to investigate");
  lines.push(`For EACH entity above, systematically ask yourself these questions. If any answer reveals a bug, report it:

1. **Naming/context match**: Do error messages, log strings, and exported names match the file/endpoint/module they're in? (e.g., an error saying "login failed" in a "disable" endpoint is wrong)
2. **Branch reachability**: For every if/else-if/else chain, can ALL branches actually execute? If a variable is assigned from a function that always returns a value, the "else" branch is dead code.
3. **Guard completeness**: For every \`if (x.length > 0)\` or \`if (x != null)\` guard, what valid scenarios does it skip? Are there inputs where the guard is false but the operation should still run?
4. **Comparison correctness**: Are string comparisons case-sensitive when they shouldn't be? Is \`indexOf\`/\`includes\`/\`==\` used on user-facing input without normalization?
5. **Atomicity**: Is any read-then-modify-then-write sequence on shared/DB state wrapped in a transaction? If not, concurrent requests can both read the old value.`);
  lines.push("");
  const hyps: string[] = [];

  // Generate entity-specific questions from code patterns
  // (meta-questions above cover the general cases; entity-specific ones add signal for specific patterns)

  for (const f of findings) {
    if (f.rule_id === "removed-guard")
      hyps.push(`Guard removed: \`${f.evidence}\`. What did it protect? Is the scenario still handled?`);
    if (f.rule_id === "signature-change-with-callers")
      hyps.push(`\`${f.entity_name}\` signature changed with callers. Check all call sites.`);
    if (f.rule_id === "type-change-propagation")
      hyps.push(`Type \`${f.entity_name}\` changed but dependents not updated. Check usages.`);
    if (f.rule_id === "arity-change-with-callers")
      hyps.push(`\`${f.entity_name}\` parameter count changed. Grep for all callers — do they pass the right number of arguments?`);
    if (f.rule_id === "null-return-introduced")
      hyps.push(`\`${f.entity_name}\` now returns null. Do all callers handle null?`);
    if (f.rule_id === "callee-swap")
      hyps.push(`\`${f.entity_name}\` calls a different function now. Verify the replacement has compatible behavior.`);
    if (f.rule_id === "argument-order-swap")
      hyps.push(`Arguments may have been swapped in \`${f.entity_name}\`. Verify the order matches the function signature.`);
    if (f.rule_id === "logic-gate-swap")
      hyps.push(`AND/OR logic changed in \`${f.entity_name}\`. Verify the new logic matches the intended behavior.`);
    if (f.rule_id === "duplicate-method-def")
      hyps.push(`Ruby has no method overloading — the second definition of \`${f.entity_name}\` replaces the first. Check all callers.`);
    if (f.rule_id === "reduce-init-mismatch")
      hyps.push(`Verify __reduce__'s return tuple matches __init__'s parameter order exactly for \`${f.entity_name}\`.`);
    if (f.rule_id === "case-insensitive-compare-needed")
      hyps.push(`\`${f.entity_name}\` uses indexOf/includes/find without case normalization. If comparing user input (codes, tokens, emails, hex), this will reject valid mixed-case input.`);
    if (f.rule_id === "export-filename-mismatch")
      hyps.push(`\`${f.entity_name}\` is exported from a file with a different name. Does the export name match the file's purpose?`);
    if (f.rule_id === "error-message-context-mismatch")
      hyps.push(`Error message in \`${f.entity_name}\` references a different operation than the function/endpoint name. Was this copy-pasted from another handler?`);
    if (f.rule_id === "unreachable-branch")
      hyps.push(`**HIGH PRIORITY**: \`${f.entity_name}\` has else-if/else branches that may be unreachable because the variable is assigned from a function that always returns a value. Trace the function — does it have ANY code path that returns null/undefined/falsy? If not, the else branches are dead code.`);
    if (f.rule_id === "arity-change-with-callers")
      hyps.push(`\`${f.entity_name}\` parameter count changed. Check if all call sites pass the correct number/types of arguments — this can be a compilation error.`);
  }

  const uniqueHyps = [...new Set(hyps)];
  for (const h of uniqueHyps.slice(0, 8)) {
    lines.push(`- ${h}`);
  }
  if (uniqueHyps.length === 0) {
    lines.push("- Look for logic errors, null safety issues, argument mismatches, and broken control flow in the changed code.");
  }
  lines.push("");

  return {
    id: `slice-${idx + 1}-${anchor.entity_name.slice(0, 30)}`,
    title: `${anchor.entity_name} (${anchor.entity_type})`,
    prompt: lines.join("\n"),
  };
}

// ── Non-code file reviewer ──
// Catches bugs in .properties, .css, .json, .yaml, .xml that the entity slicer misses
function buildNonCodeSlice(diff: string): ReviewSlice | null {
  if (!diff) return null;

  // Extract changed non-code files from diff
  const nonCodeExts = /\.(properties|css|scss|less|json|yaml|yml|xml|po|pot|svg|html|txt|md|toml|cfg|ini|conf)$/i;
  const fileHeaders = diff.match(/^diff --git a\/(.+?) b\/(.+?)$/gm) ?? [];
  const nonCodeFiles: string[] = [];
  for (const header of fileHeaders) {
    const m = header.match(/^diff --git a\/(.+?) b\/(.+?)$/);
    if (m && nonCodeExts.test(m[2])) {
      nonCodeFiles.push(m[2]);
    }
  }
  if (nonCodeFiles.length === 0) return null;

  // Extract relevant diff hunks for non-code files
  const diffLines = diff.split("\n");
  let nonCodeDiff = "";
  let inNonCodeFile = false;
  let currentFile = "";

  for (const line of diffLines) {
    const fileMatch = line.match(/^diff --git a\/(.+?) b\/(.+?)$/);
    if (fileMatch) {
      currentFile = fileMatch[2];
      inNonCodeFile = nonCodeExts.test(currentFile);
      if (inNonCodeFile) {
        nonCodeDiff += line + "\n";
      }
      continue;
    }
    if (inNonCodeFile) {
      nonCodeDiff += line + "\n";
    }
  }

  if (nonCodeDiff.length < 50) return null;

  // Truncate to reasonable size
  if (nonCodeDiff.length > 15000) {
    nonCodeDiff = nonCodeDiff.slice(0, 15000) + "\n... (truncated)";
  }

  const prompt = `Review the following changes to non-code files (properties, CSS, config, translations, etc.) for correctness bugs ONLY.

IMPORTANT: Only report concrete, factual errors such as:
- Wrong language/locale text in translation files (e.g., Italian text in a Lithuanian file)
- Traditional vs Simplified Chinese character mismatches
- Incorrect CSS values (wrong colors, wrong units, wrong percentages)
- Typos in identifiers/keys that would break functionality
- Invalid config values or syntax errors
- Wrong file references or broken paths

Do NOT report: style preferences, formatting, missing translations, documentation issues, or subjective concerns.

Changed non-code files: ${nonCodeFiles.join(", ")}

\`\`\`diff
${nonCodeDiff}
\`\`\`

Respond with ONLY a JSON object:
{"issues": [{"issue": "description", "evidence": "exact text from diff", "severity": "critical|high|medium", "file": "path/to/file"}]}
Return {"issues": []} if no bugs.`;

  return {
    id: "slice-noncode",
    title: "Non-code file review",
    prompt,
  };
}



function extractJson(text: string): string {
  const trimmed = text.trim();
  const fenceMatch = trimmed.match(/```json\s*([\s\S]*?)```/);
  if (fenceMatch) {
    const inner = fenceMatch[1].trim();
    if (inner.startsWith("{")) return inner;
  }
  const anyFence = trimmed.match(/```\s*([\s\S]*?)```/);
  if (anyFence) {
    const inner = anyFence[1].trim();
    if (inner.startsWith("{")) return inner;
  }
  const issuesIdx = trimmed.indexOf('{"issues"');
  if (issuesIdx !== -1) {
    const sub = trimmed.slice(issuesIdx);
    let depth = 0;
    for (let i = 0; i < sub.length; i++) {
      if (sub[i] === "{") depth++;
      else if (sub[i] === "}") {
        depth--;
        if (depth === 0) return sub.slice(0, i + 1);
      }
    }
  }
  const first = trimmed.indexOf("{");
  const last = trimmed.lastIndexOf("}");
  if (first !== -1 && last > first) return trimmed.slice(first, last + 1);
  return trimmed;
}

// ── Main ──
async function main() {
  const raw = await readStdin();
  const input = JSON.parse(raw);

  const repoDir: string = input.repo_dir;
  const entityReviews: EntityReview[] = input.entity_reviews ?? [];
  const findings: DetectorFinding[] = input.findings ?? [];
  const prTitle: string = input.pr_title ?? "";
  const diff: string = input.diff ?? "";
  const provider = input.provider ?? "anthropic";
  const modelId = input.model ?? "claude-sonnet-4-6";

  entityReviews.sort((a, b) => b.risk_score - a.risk_score);
  process.stderr.write(`[review-entry] ${entityReviews.length} entities, ${findings.length} findings\n`);

  // Build entity map
  const entityMap = new Map<string, EntityReview>();
  for (const e of entityReviews) {
    entityMap.set(`${e.entity_name}::${e.file_path}`, e);
  }

  // Build slices
  const clusters = buildClusters(entityReviews, findings, entityMap);
  const slices = clusters.map((c, i) => clusterToSlice(c, i, entityMap, diff));

  // Add non-code file review slice
  const nonCodeSlice = buildNonCodeSlice(diff);
  if (nonCodeSlice) {
    slices.push(nonCodeSlice);
  }

  process.stderr.write(`[review-entry] ${slices.length} slices built (${nonCodeSlice ? "incl non-code" : "code only"})\n`);
  for (const s of slices) {
    process.stderr.write(`  - ${s.id}: ${s.title} (${s.prompt.length} chars)\n`);
  }

  if (slices.length === 0) {
    const output: ValidateOutput = { verdicts: [] };
    process.stdout.write(JSON.stringify(output) + "\n");
    return;
  }

  // Setup model for slice review
  const model = getModel(provider as any, modelId);

  // Setup tools
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

  // Run parallel review (candidate generation)
  const result = await reviewSlicesParallel(slices, model, tools, {
    concurrency: 4,
    thinkingLevel: "high",
  });

  process.stderr.write(
    `[review-entry] Slice review done: ${result.merged_issues.length} raw issues, ${result.total_tool_calls} tools, ${(result.total_elapsed_ms / 1000).toFixed(1)}s\n`,
  );

  // Dedup raw issues — global across all files, semantic matching
  let rawIssues = result.merged_issues;

  // Extract key identifiers from issue text for better dedup
  function extractKeyTerms(text: string): Set<string> {
    // Extract backtick-quoted identifiers (function names, variables)
    const quoted = text.match(/`([^`]+)`/g)?.map((m) => m.slice(1, -1).toLowerCase()) ?? [];
    // Extract words > 5 chars
    const words = text.toLowerCase().split(/\s+/).filter((w) => w.length > 5);
    return new Set([...quoted, ...words]);
  }

  // Extract function/variable names mentioned in backticks or as camelCase words
  function extractIdentifiers(text: string): Set<string> {
    const ids: string[] = [];
    // Backtick-quoted identifiers
    const quoted = text.match(/`([A-Za-z_]\w*)`/g);
    if (quoted) ids.push(...quoted.map((m) => m.slice(1, -1).toLowerCase()));
    // CamelCase identifiers (at least 2 parts, e.g., deleteScheduledEmailReminder)
    const camel = text.match(/\b[a-z][a-zA-Z]{6,}\b/g);
    if (camel) ids.push(...camel.map((m) => m.toLowerCase()));
    return new Set(ids.filter((id) => id.length > 6));
  }

  function extractBugPattern(text: string): string {
    // Normalize common bug patterns for dedup
    const lower = text.toLowerCase();
    if (lower.includes("foreach") && lower.includes("async")) return "foreach-async";
    if (lower.includes("not awaited") || lower.includes("without await") || lower.includes("fire-and-forget")) return "foreach-async";
    if ((lower.includes("missing") && lower.includes("await")) || lower.includes("missing `await`")) return "foreach-async";
    if (lower.includes("null") && (lower.includes("undefined") || lower.includes("crash"))) return "null-safety";
    if (lower.includes("race condition") || lower.includes("concurrent")) return "race-condition";
    return "";
  }

  const deduped: typeof rawIssues = [];
  for (const issue of rawIssues) {
    const issueTerms = extractKeyTerms(issue.issue);
    const issuePattern = extractBugPattern(issue.issue);

    const isDup = deduped.some((k) => {
      // Same file + high word overlap = duplicate
      const kTerms = extractKeyTerms(k.issue);
      const overlap = [...issueTerms].filter((w) => kTerms.has(w)).length;
      const sameFile = (issue.file ?? "") === (k.file ?? "");
      if (sameFile && overlap > Math.min(issueTerms.size, kTerms.size) * 0.4) return true;

      // Same bug pattern + any shared identifiers = duplicate (even across files)
      if (issuePattern && issuePattern === extractBugPattern(k.issue)) {
        const issueIds = extractIdentifiers(issue.issue);
        const kIds = extractIdentifiers(k.issue);
        const sharedIds = [...issueIds].filter((id) => kIds.has(id));
        if (sharedIds.length >= 1) return true;
      }

      // Very high overlap even across files
      if (overlap > Math.min(issueTerms.size, kTerms.size) * 0.6) return true;

      return false;
    });
    if (!isDup) deduped.push(issue);
  }
  rawIssues = deduped;

  process.stderr.write(
    `[review-entry] After dedup: ${rawIssues.length} issues\n`,
  );

  // Output each issue as a separate verdict (no verifier)
  const output: ValidateOutput = {
    verdicts: rawIssues.map((issue) => ({
      rule_id: "review",
      entity_name: issue.file ?? "unknown",
      verdict: "true_positive" as const,
      explanation: `[${issue.severity}] ${issue.issue} | evidence: ${issue.evidence ?? "none"} | file: ${issue.file ?? "unknown"}`,
    })),
  };

  process.stdout.write(JSON.stringify(output) + "\n");
}

main().catch((e) => {
  process.stderr.write(`[fatal] ${e}\n`);
  process.exit(1);
});
