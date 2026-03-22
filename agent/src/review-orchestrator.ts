/**
 * Review Orchestrator v2 — Deterministic coverage-first slice planner.
 *
 * Design rationale (from oracle review):
 * - Deterministic planning beats LLM orchestrator because our input is already
 *   richly structured by the Rust pipeline (entities, findings, graph edges)
 * - The LLM should spend tokens on reasoning over well-formed slices, not
 *   on deciding what those slices should be
 * - Pre-read files deterministically based on entity file paths and graph edges
 * - "Specialist personas" are mostly placebo — what matters is different
 *   invariants, context, and evidence per slice
 *
 * Slice seeding priority:
 * 1. Detector-backed slices (findings are the seed, not just appended text)
 * 2. Contract slices (changed public APIs, overrides, structural changes)
 * 3. Cross-entity logic slices (top-risk entities with dependency chains)
 * 4. Fallback coverage slices (uncovered high-risk entities)
 * 5. Non-code file slice (config, CSS, translations)
 */
import { readFileSync } from "fs";
import { join } from "path";
import type { EntityReview, DetectorFinding } from "./types.js";
import type { ReviewSlice } from "./review-parallel.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface PlannedArea {
  id: string;
  title: string;
  concern: string;
  justification: string;
  checks: string[];
  entities: EntityReview[];
  findings: DetectorFinding[];
  preReadFiles: string[];
  priority: "critical" | "high" | "medium";
}

export interface OrchestrateInput {
  entityReviews: EntityReview[];
  findings: DetectorFinding[];
  prTitle: string;
  diff: string;
  repoDir: string;
}

type OnLog = (msg: string) => void;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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
  if (inRelevantFile && currentChunk.length > 0) {
    relevantChunks.push(currentChunk.join("\n"));
  }

  const result = relevantChunks.join("\n");
  if (result.length > 10000) {
    return result.slice(0, 10000) + "\n... (diff truncated)";
  }
  return result;
}

function preReadFile(filePath: string, repoDir: string): string | null {
  try {
    const content = readFileSync(join(repoDir, filePath), "utf-8");
    if (content.length > 15000) {
      return content.slice(0, 15000) + "\n... (truncated)";
    }
    return content;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Slice prompt builders
// ---------------------------------------------------------------------------

/** Build a mission-specific slice prompt for a planned area */
function buildAreaPrompt(
  area: PlannedArea,
  entityMap: Map<string, EntityReview>,
  diff: string,
  repoDir: string,
): string {
  const lines: string[] = [];

  // Mission header
  lines.push(`You are reviewing a focused investigation area. Find REAL correctness bugs only.

STRICT RULES:
- You MUST use the read tool to open the actual source file for EVERY entity before reporting ANY bug. Do NOT trust snippets alone.
- Use grep to find callers/callees when a signature or behavior changed.
- Report ONLY bugs you confirmed by reading actual files.
- Do NOT report: style issues, naming, missing tests, documentation, suggestions, or theoretical concerns.
- Maximum 3 issues per investigation. Only report bugs you have high confidence in.

WHAT COUNTS AS A BUG:
- Wrong variable/function in a condition or expression
- Swapped or missing arguments in a function call
- Inverted logic (== vs !=, && vs ||)
- Missing null/error check that WILL crash at runtime
- forEach+async without await (fire-and-forget)
- Interface contract violation (method signature doesn't match interface)
- Resource leak (opened but never closed)

WHAT IS NOT A BUG:
- Redundant code or unnecessary checks (harmless)
- Missing error handling for unlikely scenarios
- Hardcoded values that look suspicious but work correctly
- Code that "could be better" but isn't wrong`);
  lines.push("");

  // Mission: one concrete concern
  lines.push(`# Mission: ${area.title}`);
  lines.push("");

  // Why this slice exists
  lines.push(`## Why this investigation exists`);
  lines.push(area.justification);
  lines.push("");

  // Concrete failure-mode checks
  if (area.checks.length > 0) {
    lines.push("## Check these exact failure modes:");
    for (let i = 0; i < area.checks.length; i++) {
      lines.push(`${i + 1}. ${area.checks[i]}`);
    }
    lines.push("");
  }

  // Detector findings
  if (area.findings.length > 0) {
    lines.push(`## Detector Findings (${area.findings.length})`);
    for (const f of area.findings) {
      lines.push(`- **[${f.severity.toUpperCase()}] ${f.rule_id}**: \`${f.entity_name}\` — ${f.message}`);
      lines.push(`  Evidence: \`${f.evidence}\``);
      const question = findingToCheck(f);
      if (question) {
        lines.push(`  → **Investigate:** ${question}`);
      }
    }
    lines.push("");
  }

  // Pre-read file contents
  const preReadContents = new Map<string, string>();
  for (const filePath of area.preReadFiles) {
    const content = preReadFile(filePath, repoDir);
    if (content) preReadContents.set(filePath, content);
  }

  // Entity code with inline context
  const uniqueFiles = [...new Set(area.entities.map((e) => e.file_path))];
  lines.push(`## Code Entities (${area.entities.length})`);
  lines.push(`**Files to read for full context:** ${uniqueFiles.join(", ")}`);
  lines.push("");

  for (const e of area.entities) {
    const content = e.after_content ?? e.before_content ?? "";
    if (!content) continue;

    lines.push(`### \`${e.entity_name}\` (${e.entity_type}, ${e.change_type}) in ${e.file_path}:${e.start_line}-${e.end_line}`);
    lines.push(`risk=${e.risk_level}(${e.risk_score.toFixed(2)}) blast=${e.blast_radius} deps=${e.dependent_count}${e.is_public_api ? " **PUBLIC**" : ""}`);

    // Callers/callees
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

    // Contract co-location for @Override methods
    const afterContent = e.after_content ?? "";
    if (afterContent.includes("@Override") && e.entity_type === "method") {
      const contract = findContractSnippet(e, area.entities, entityMap);
      if (contract) {
        lines.push("**Contract (interface/abstract):**");
        lines.push("```");
        lines.push(contract);
        lines.push("```");
      }
    }

    // Entity code
    const display = content.length > 2500 ? content.slice(0, 2500) + "\n... (truncated)" : content;
    lines.push("```");
    lines.push(display);
    lines.push("```");
    lines.push("");
  }

  // Raw diff hunks
  const diffHunks = extractDiffForFiles(diff, uniqueFiles);
  if (diffHunks) {
    lines.push("## Raw Diff");
    lines.push("```diff");
    lines.push(diffHunks);
    lines.push("```");
    lines.push("");
  }

  // Pre-read file contents (for files not covered by entity snippets)
  for (const [filePath, content] of preReadContents) {
    if (!uniqueFiles.includes(filePath)) {
      lines.push(`## Pre-read: ${filePath}`);
      lines.push("```");
      lines.push(content.length > 3000 ? content.slice(0, 3000) + "\n... (truncated)" : content);
      lines.push("```");
      lines.push("");
    }
  }

  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// Finding-to-check mapping
// ---------------------------------------------------------------------------

function findingToCheck(f: DetectorFinding): string | null {
  const ruleChecks: Record<string, string> = {
    "removed-guard": `What scenario did the removed guard \`${f.evidence}\` protect against? Is that scenario still handled?`,
    "nil-check-missing": `What happens if this call fails? Grep for all callers of \`${f.entity_name}\` and check error handling.`,
    "foreach-async": `Are the async operations inside forEach awaited? Check if Promise.all or for...of is needed.`,
    "missing-await": `Is the return value of this async call used? Check if a missing await causes a race condition.`,
    "catch-swallow": `What errors are being swallowed? Check if the catch block should re-throw or log.`,
    "signature-change-with-callers": `This function's signature changed. Grep for all callers and verify they pass the correct arguments.`,
    "arity-change-with-callers": `Parameter count changed. Grep for all callers — do they pass the right number of arguments?`,
    "type-change-propagation": `A type changed but dependents may not be updated. Check all usages of this entity.`,
    "logic-gate-swap": `AND/OR logic changed. Verify the new logic matches the intended behavior.`,
    "null-return-introduced": `A null/None return was added. Do all callers handle null?`,
    "argument-order-swap": `Arguments may have been swapped. Verify the order matches the function signature.`,
    "callee-swap": `A different function is being called. Verify the replacement has compatible behavior.`,
    "duplicate-method-def": `Ruby has no method overloading — the second definition replaces the first. Check all callers use the surviving signature.`,
    "reduce-init-mismatch": `Verify the positional arguments in __reduce__'s return tuple match __init__'s parameter order exactly.`,
    "negation-flip": `A condition was negated. Verify the new logic is correct in all branches.`,
    "boolean-polarity-flip": `A boolean value was flipped. Check all consumers of this value.`,
    "variable-near-miss": `A similar variable name exists in scope. Verify the correct one is used.`,
    "interface-impl-mismatch": `Interface signature doesn't match implementation. Check all implementations.`,
    "unreachable-branch": `A code branch may be unreachable. Verify the condition can actually be true/false.`,
  };
  return ruleChecks[f.rule_id] ?? null;
}

// ---------------------------------------------------------------------------
// Contract co-location helper
// ---------------------------------------------------------------------------

function findContractSnippet(
  entity: EntityReview,
  areaEntities: EntityReview[],
  entityMap: Map<string, EntityReview>,
): string | null {
  // Look for same-named entity in a different file (interface/abstract)
  for (const [key, e] of entityMap) {
    if (e.entity_name !== entity.entity_name) continue;
    if (e.file_path === entity.file_path) continue;
    const content = e.after_content ?? e.before_content ?? "";
    if (!content) continue;
    if (!content.includes("{") || content.includes("interface ")) {
      return content.trim().slice(0, 500);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Phase 1: Seed detector-backed slices
// ---------------------------------------------------------------------------

function seedDetectorSlices(
  findings: DetectorFinding[],
  entityMap: Map<string, EntityReview>,
  allEntities: EntityReview[],
): PlannedArea[] {
  const areas: PlannedArea[] = [];
  const usedFindingIds = new Set<string>();

  // Group findings by entity to avoid 1 slice per finding
  const findingsByEntity = new Map<string, DetectorFinding[]>();
  for (const f of findings) {
    const key = f.entity_id;
    const arr = findingsByEntity.get(key) ?? [];
    arr.push(f);
    findingsByEntity.set(key, arr);
  }

  for (const [entityId, entityFindings] of findingsByEntity) {
    if (entityFindings.every((f) => usedFindingIds.has(f.entity_id + "::" + f.rule_id))) continue;

    const entity = allEntities.find((e) => e.entity_id === entityId);
    if (!entity) continue;

    // Collect related entities (dependencies + dependents)
    const related: EntityReview[] = [];
    const relatedFiles: string[] = [entity.file_path];
    for (const [name, file] of [...entity.dependency_names, ...entity.dependent_names].slice(0, 5)) {
      const dep = entityMap.get(`${name}::${file}`);
      if (dep && dep.entity_id !== entityId) {
        related.push(dep);
        if (!relatedFiles.includes(file)) relatedFiles.push(file);
      }
    }

    // Build checks from findings
    const checks = entityFindings.map((f) => findingToCheck(f) ?? f.message).slice(0, 4);

    // Determine priority from finding severity
    const maxSev = entityFindings.reduce((best, f) => {
      const order: Record<string, number> = { critical: 4, high: 3, medium: 2, low: 1 };
      return (order[f.severity] ?? 0) > (order[best] ?? 0) ? f.severity : best;
    }, "medium");
    const priority = maxSev === "critical" || maxSev === "high" ? (maxSev as "critical" | "high") : "high";

    areas.push({
      id: `det-${areas.length + 1}`,
      title: `${entityFindings[0].rule_id}: ${entity.entity_name}`,
      concern: entityFindings.map((f) => f.message).join("; "),
      justification: `Detector flagged ${entityFindings.length} finding(s) on \`${entity.entity_name}\` in ${entity.file_path}. ` +
        `Risk score: ${entity.risk_score.toFixed(2)}, ${entity.dependent_count} dependents.`,
      checks,
      entities: [entity, ...related.slice(0, 4)],
      findings: entityFindings,
      preReadFiles: relatedFiles.slice(0, 3),
      priority,
    });

    for (const f of entityFindings) {
      usedFindingIds.add(f.entity_id + "::" + f.rule_id);
    }
  }

  return areas;
}

// ---------------------------------------------------------------------------
// Phase 2: Seed contract slices
// ---------------------------------------------------------------------------

function seedContractSlices(
  allEntities: EntityReview[],
  entityMap: Map<string, EntityReview>,
  coveredEntityIds: Set<string>,
): PlannedArea[] {
  const areas: PlannedArea[] = [];

  for (const entity of allEntities) {
    if (coveredEntityIds.has(entity.entity_id)) continue;
    if (!entity.is_public_api && !entity.structural_change) continue;
    if (entity.entity_type === "file") continue;
    if (!entity.after_content && !entity.before_content) continue;

    // Collect implementations/callers
    const related: EntityReview[] = [];
    const relatedFiles: string[] = [entity.file_path];
    for (const [name, file] of entity.dependent_names.slice(0, 4)) {
      const dep = entityMap.get(`${name}::${file}`);
      if (dep) {
        related.push(dep);
        if (!relatedFiles.includes(file)) relatedFiles.push(file);
      }
    }

    // Build checks based on change type
    const checks: string[] = [];
    if (entity.change_type === "Modified") {
      checks.push(`Compare before/after signature of \`${entity.entity_name}\`. Did parameters, return type, or nullability change?`);
      if (entity.dependent_count > 0) {
        checks.push(`Grep for all ${entity.dependent_count} callers. Do they pass the correct arguments to the new signature?`);
      }
    }
    if (entity.structural_change) {
      checks.push(`Structural change detected. Verify all implementations/overrides match the new contract.`);
    }
    if (checks.length === 0) {
      checks.push(`Verify this public API change is backward-compatible or all callers are updated.`);
    }

    areas.push({
      id: `contract-${areas.length + 1}`,
      title: `Contract: ${entity.entity_name} (${entity.change_type})`,
      concern: `Changed public API/structural entity with ${entity.dependent_count} dependents`,
      justification: `\`${entity.entity_name}\` is a ${entity.is_public_api ? "public API" : "structural"} ${entity.entity_type} ` +
        `that was ${entity.change_type.toLowerCase()} with ${entity.dependent_count} dependents. ` +
        `Risk: ${entity.risk_level}(${entity.risk_score.toFixed(2)}), blast radius: ${entity.blast_radius}.`,
      checks,
      entities: [entity, ...related.slice(0, 3)],
      findings: [],
      preReadFiles: relatedFiles.slice(0, 3),
      priority: entity.risk_score > 0.7 ? "critical" : "high",
    });

    coveredEntityIds.add(entity.entity_id);
    for (const r of related) coveredEntityIds.add(r.entity_id);

    if (areas.length >= 3) break; // cap contract slices
  }

  return areas;
}

// ---------------------------------------------------------------------------
// Phase 3: Seed cross-entity logic slices
// ---------------------------------------------------------------------------

function seedLogicSlices(
  allEntities: EntityReview[],
  entityMap: Map<string, EntityReview>,
  coveredEntityIds: Set<string>,
  diff: string,
): PlannedArea[] {
  const areas: PlannedArea[] = [];

  for (const entity of allEntities) {
    if (coveredEntityIds.has(entity.entity_id)) continue;
    if (entity.entity_type === "file") continue;
    if (entity.risk_score < 0.3) continue;
    if (!entity.after_content && !entity.before_content) continue;

    // Skip tiny chunks without context
    const content = entity.after_content ?? entity.before_content ?? "";
    if (entity.entity_type === "chunk" && content.split("\n").length < 8) continue;

    // Collect one caller + one callee for context
    const related: EntityReview[] = [];
    const relatedFiles: string[] = [entity.file_path];

    for (const [name, file] of entity.dependency_names.slice(0, 2)) {
      const dep = entityMap.get(`${name}::${file}`);
      if (dep) {
        related.push(dep);
        if (!relatedFiles.includes(file)) relatedFiles.push(file);
      }
    }
    for (const [name, file] of entity.dependent_names.slice(0, 2)) {
      const dep = entityMap.get(`${name}::${file}`);
      if (dep && !related.some((r) => r.entity_id === dep.entity_id)) {
        related.push(dep);
        if (!relatedFiles.includes(file)) relatedFiles.push(file);
      }
    }

    // Build checks based on code patterns
    const checks = generatePatternChecks(entity, content);

    areas.push({
      id: `logic-${areas.length + 1}`,
      title: `${entity.entity_name} (${entity.entity_type}, ${entity.change_type})`,
      concern: `High-risk ${entity.change_type.toLowerCase()} entity with ${entity.dependent_count} dependents`,
      justification: `\`${entity.entity_name}\` has risk ${entity.risk_level}(${entity.risk_score.toFixed(2)}) ` +
        `and blast radius ${entity.blast_radius}. ${entity.change_type} in ${entity.file_path}.`,
      checks,
      entities: [entity, ...related.slice(0, 3)],
      findings: [],
      preReadFiles: relatedFiles.slice(0, 3),
      priority: entity.risk_score > 0.6 ? "high" : "medium",
    });

    coveredEntityIds.add(entity.entity_id);
    for (const r of related) coveredEntityIds.add(r.entity_id);

    if (areas.length >= 4) break; // cap logic slices
  }

  return areas;
}

/** Generate checks from code patterns detected in entity content */
function generatePatternChecks(entity: EntityReview, content: string): string[] {
  const checks: string[] = [];

  // forEach + async
  if (content.includes("forEach") && (content.includes("async") || content.includes("await"))) {
    checks.push("Check forEach with async callbacks — are operations properly awaited?");
  }

  // String comparison without case normalization
  if ((content.includes("indexOf(") || content.includes("includes(") || content.includes(".find(")) &&
      !content.includes("toLowerCase") && !content.includes("toUpperCase")) {
    if (content.includes("code") || content.includes("token") || content.includes("email") ||
        content.includes("key") || content.includes("hex") || content.includes("hash")) {
      checks.push("String comparison may need case normalization for user input (codes/tokens/emails).");
    }
  }

  // Read-modify-write without transaction
  if ((content.includes("findFirst") || content.includes("findUnique") || content.includes("findOne") || content.includes(".first")) &&
      (content.includes(".update(") || content.includes(".delete(") || content.includes(".save")) &&
      !content.includes("transaction") && !content.includes("$transaction") && !content.includes("atomic")) {
    checks.push("Read-then-write without transaction. Can concurrent requests cause a race condition?");
  }

  // Null/undefined access
  if (content.includes("?.") || content.includes("?.[") || content.includes("?.(")) {
    checks.push("Optional chaining suggests nullable values. Are all null paths handled correctly?");
  }

  // Fallback check
  if (checks.length === 0) {
    checks.push(`Verify \`${entity.entity_name}\` logic is correct — check variables, conditions, return values.`);
    if (entity.dependent_count > 0) {
      checks.push(`Check if callers of \`${entity.entity_name}\` handle the changed behavior correctly.`);
    }
  }

  return checks.slice(0, 4);
}

// ---------------------------------------------------------------------------
// Phase 4: Fallback coverage slices
// ---------------------------------------------------------------------------

function seedFallbackSlices(
  allEntities: EntityReview[],
  entityMap: Map<string, EntityReview>,
  findings: DetectorFinding[],
  coveredEntityIds: Set<string>,
): PlannedArea[] {
  const areas: PlannedArea[] = [];

  for (const entity of allEntities) {
    if (coveredEntityIds.has(entity.entity_id)) continue;
    if (entity.entity_type === "file") continue;
    if (!entity.after_content && !entity.before_content) continue;
    if (entity.risk_score < 0.2) continue;

    // Skip tiny chunks
    const content = entity.after_content ?? entity.before_content ?? "";
    if (entity.entity_type === "chunk" && content.split("\n").length < 8) continue;

    const entityFindings = findings.filter((f) => f.entity_id === entity.entity_id);

    areas.push({
      id: `fallback-${areas.length + 1}`,
      title: `${entity.entity_name} (${entity.entity_type}, ${entity.change_type})`,
      concern: `Uncovered entity with risk ${entity.risk_level}`,
      justification: `\`${entity.entity_name}\` was not covered by detector, contract, or logic slices. ` +
        `Risk: ${entity.risk_level}(${entity.risk_score.toFixed(2)}).`,
      checks: [`Look for logic errors, null safety issues, argument mismatches in \`${entity.entity_name}\`.`],
      entities: [entity],
      findings: entityFindings,
      preReadFiles: [entity.file_path],
      priority: "medium",
    });

    coveredEntityIds.add(entity.entity_id);

    if (areas.length >= 3) break; // cap fallback slices
  }

  return areas;
}

// ---------------------------------------------------------------------------
// Non-code file slice
// ---------------------------------------------------------------------------

function buildNonCodeSlice(diff: string): ReviewSlice | null {
  if (!diff) return null;

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

  const diffLines = diff.split("\n");
  let nonCodeDiff = "";
  let inNonCodeFile = false;
  let currentFile = "";

  for (const line of diffLines) {
    const fileMatch = line.match(/^diff --git a\/(.+?) b\/(.+?)$/);
    if (fileMatch) {
      currentFile = fileMatch[2];
      inNonCodeFile = nonCodeExts.test(currentFile);
      if (inNonCodeFile) nonCodeDiff += line + "\n";
      continue;
    }
    if (inNonCodeFile) nonCodeDiff += line + "\n";
  }

  if (nonCodeDiff.length < 50) return null;
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

// ---------------------------------------------------------------------------
// Merge small areas into combined slices
// ---------------------------------------------------------------------------

function mergeSmallAreas(areas: PlannedArea[]): PlannedArea[] {
  const merged: PlannedArea[] = [];
  const consumed = new Set<number>();

  for (let i = 0; i < areas.length; i++) {
    if (consumed.has(i)) continue;
    const area = {
      ...areas[i],
      entities: [...areas[i].entities],
      findings: [...areas[i].findings],
      checks: [...areas[i].checks],
      preReadFiles: [...areas[i].preReadFiles],
    };

    // Try to merge with a following small area in the same file
    if (area.entities.length <= 2) {
      for (let j = i + 1; j < areas.length; j++) {
        if (consumed.has(j)) continue;
        const other = areas[j];
        if (other.entities.length > 2) continue;

        // Same file?
        const aFiles = new Set(area.entities.map((e) => e.file_path));
        const bFiles = new Set(other.entities.map((e) => e.file_path));
        const shared = [...aFiles].filter((f) => bFiles.has(f));

        if (shared.length > 0 && area.entities.length + other.entities.length <= 6) {
          const existingIds = new Set(area.entities.map((e) => e.entity_id));
          for (const e of other.entities) {
            if (!existingIds.has(e.entity_id)) {
              area.entities.push(e);
              existingIds.add(e.entity_id);
            }
          }
          for (const f of other.findings) {
            if (!area.findings.some((af) => af.entity_id === f.entity_id && af.rule_id === f.rule_id)) {
              area.findings.push(f);
            }
          }
          for (const c of other.checks) {
            if (!area.checks.includes(c)) area.checks.push(c);
          }
          for (const f of other.preReadFiles) {
            if (!area.preReadFiles.includes(f)) area.preReadFiles.push(f);
          }
          area.title += ` + ${other.title.split(":").pop()?.trim() ?? other.title}`;
          consumed.add(j);
        }
      }
    }

    merged.push(area);
  }

  return merged;
}

// ---------------------------------------------------------------------------
// Main orchestrator entry point
// ---------------------------------------------------------------------------

export async function orchestrate(
  input: OrchestrateInput,
  onLog: OnLog = (msg) => process.stderr.write(msg + "\n"),
): Promise<ReviewSlice[]> {
  const { entityReviews, findings, prTitle, diff, repoDir } = input;

  // Sort entities by risk
  const sorted = [...entityReviews].sort((a, b) => b.risk_score - a.risk_score);
  const top = sorted.slice(0, 30);

  // Include entities with findings even if not in top-30
  const topIds = new Set(top.map((e) => e.entity_id));
  const findingEntityIds = new Set(findings.map((f) => f.entity_id));
  for (const e of sorted) {
    if (findingEntityIds.has(e.entity_id) && !topIds.has(e.entity_id)) {
      top.push(e);
      topIds.add(e.entity_id);
    }
  }

  // Small-PR mode: include all entities
  if (sorted.length <= 15) {
    for (const e of sorted) {
      if (!topIds.has(e.entity_id)) {
        top.push(e);
        topIds.add(e.entity_id);
      }
    }
  }

  // Build entity lookup map
  const entityMap = new Map<string, EntityReview>();
  for (const e of entityReviews) {
    entityMap.set(`${e.entity_name}::${e.file_path}`, e);
  }

  onLog(`[orchestrator] ${top.length} entities in scope, ${findings.length} findings`);

  // Phase 1: Detector-backed slices (highest priority)
  const coveredEntityIds = new Set<string>();
  const detectorAreas = seedDetectorSlices(findings, entityMap, top);
  for (const a of detectorAreas) {
    for (const e of a.entities) coveredEntityIds.add(e.entity_id);
  }
  onLog(`[orchestrator] Phase 1: ${detectorAreas.length} detector-backed areas`);

  // Phase 2: Contract slices (changed public APIs, structural changes)
  const contractAreas = seedContractSlices(top, entityMap, coveredEntityIds);
  onLog(`[orchestrator] Phase 2: ${contractAreas.length} contract areas`);

  // Phase 3: Cross-entity logic slices (top-risk uncovered entities)
  const logicAreas = seedLogicSlices(top, entityMap, coveredEntityIds, diff);
  onLog(`[orchestrator] Phase 3: ${logicAreas.length} logic areas`);

  // Phase 4: Fallback coverage (remaining high-risk uncovered)
  const fallbackAreas = seedFallbackSlices(top, entityMap, findings, coveredEntityIds);
  onLog(`[orchestrator] Phase 4: ${fallbackAreas.length} fallback areas`);

  // Combine all areas
  let allAreas = [...detectorAreas, ...contractAreas, ...logicAreas, ...fallbackAreas];

  // Merge small same-file areas
  allAreas = mergeSmallAreas(allAreas);

  // Sort by priority
  const priorityOrder: Record<string, number> = { critical: 3, high: 2, medium: 1 };
  allAreas.sort((a, b) => (priorityOrder[b.priority] ?? 0) - (priorityOrder[a.priority] ?? 0));

  // Cap total slices
  allAreas = allAreas.slice(0, 10);

  onLog(`[orchestrator] ${allAreas.length} final areas after merge/cap`);
  for (const a of allAreas) {
    onLog(`  - ${a.id}: ${a.title} (${a.priority}, ${a.entities.length} entities, ${a.findings.length} findings)`);
  }

  // Build ReviewSlice objects
  const slices: ReviewSlice[] = allAreas.map((area) => ({
    id: area.id,
    title: area.title,
    prompt: buildAreaPrompt(area, entityMap, diff, repoDir),
  }));

  // Add non-code file slice
  const nonCodeSlice = buildNonCodeSlice(diff);
  if (nonCodeSlice) {
    slices.push(nonCodeSlice);
    onLog(`[orchestrator] Added non-code file review slice`);
  }

  // Coverage stats
  const totalEntities = top.length;
  const covered = coveredEntityIds.size;
  const coveragePct = totalEntities > 0 ? ((covered / totalEntities) * 100).toFixed(0) : "0";
  onLog(`[orchestrator] Coverage: ${covered}/${totalEntities} entities covered (${coveragePct}%)`);

  return slices;
}
