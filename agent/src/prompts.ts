import { diffLines } from "diff";
import type { DetectorFinding, EntityReview } from "./types.js";

/** System prompt: review protocol, not just a bug list. */
export const SYSTEM_PROMPT_CLAUDE = `You are a precision code reviewer. Find only high-confidence, concrete correctness bugs in changed code.

Review protocol:
1. FIRST: analyze all code snippets provided below WITHOUT using tools. Identify concrete suspicions before any tool use.
2. For every method shown (especially @Override, public API, or interface implementations), check:
   - Contract check: does the implementation satisfy what the method name, interface, Javadoc, and callers expect?
   - Return-value check: does it return the correct type/object, not a related but wrong one? (e.g., getX() must return X, not Y)
   - Fluent/builder check: are return values from builder/fluent APIs captured, or silently discarded (making the call a no-op)?
   - Dead code check: is any computed result unused or overwritten?
   - Guard check: are safety checks (null guards, bounds checks, assertions) present where needed?
3. Use tools ONLY to verify a specific suspicion — never for broad exploration.

Do NOT report: style, naming, missing tests, documentation, suggestions, or issues in deleted-only code.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.`;

export const SYSTEM_PROMPT_GPT = SYSTEM_PROMPT_CLAUDE;

export const SYSTEM_PROMPT = SYSTEM_PROMPT_CLAUDE;

/**
 * Build a focused user prompt from the top-N ranked entities.
 *
 * Key design decisions:
 * - Top-30 entities by risk_score (covers 127/128 golden bugs at top-20, +10 buffer)
 * - No file expansion — only ranked entities, preserving pipeline signal
 * - Contract co-location: for @Override methods, inline the interface signature/Javadoc
 * - Collapse repeated boilerplate (e.g., multiple UnsupportedOperationException stubs)
 */
export function buildUserPrompt(
  prTitle: string,
  _diff: string,
  _triageSection: string,
  findings: DetectorFinding[],
  entityReviews: EntityReview[],
): string {
  const TOP_N = 30;
  const topEntities = entityReviews.slice(0, TOP_N);

  // Build a lookup of ALL entities by name for contract co-location
  const entitiesByName = new Map<string, EntityReview[]>();
  for (const e of entityReviews) {
    const arr = entitiesByName.get(e.entity_name) ?? [];
    arr.push(e);
    entitiesByName.set(e.entity_name, arr);
  }

  // Detect which entities are interface/abstract declarations (contracts)
  const topEntityIds = new Set(topEntities.map((e) => e.entity_id));

  // Group top entities by file for display
  const byFile = new Map<string, EntityReview[]>();
  for (const e of topEntities) {
    const arr = byFile.get(e.file_path) ?? [];
    arr.push(e);
    byFile.set(e.file_path, arr);
  }
  const sortedFiles = [...byFile.entries()].sort((a, b) => {
    const aMax = Math.max(...a[1].map((e) => e.risk_score));
    const bMax = Math.max(...b[1].map((e) => e.risk_score));
    return bMax - aMax;
  });

  const lines: string[] = [];
  lines.push(`# PR: ${prTitle || "(untitled)"}`);
  lines.push(`Top ${topEntities.length} entities across ${sortedFiles.length} files (from ${entityReviews.length} total)`);
  lines.push("");

  // Findings — strongest signals first
  if (findings.length > 0) {
    lines.push(`## Detector Findings (${findings.length})`);
    for (const f of findings) {
      const entity = entityReviews.find((e) => e.entity_id === f.entity_id);
      const risk = entity
        ? ` | risk=${entity.risk_level}(${entity.risk_score.toFixed(2)}) deps=${entity.dependent_count}`
        : "";
      lines.push(
        `- **[${f.severity.toUpperCase()}] ${f.rule_id}** \`${f.entity_name}\` ${f.file_path}:${f.start_line}`,
      );
      lines.push(`  ${f.message}`);
      lines.push(`  \`${f.evidence}\`${risk}`);
    }
    lines.push("");
  }

  // Per-file entity sections
  for (const [filePath, entities] of sortedFiles) {
    const maxLevel = entities.reduce((best, e) =>
      e.risk_score > (best?.risk_score ?? 0) ? e : best,
    ).risk_level;

    lines.push(`## ${filePath} (${maxLevel}, ${entities.length} entities)`);

    // Collapse UnsupportedOperationException stubs
    const stubs: EntityReview[] = [];
    const nonStubs: EntityReview[] = [];
    for (const e of entities) {
      const content = e.after_content ?? "";
      if (
        e.change_type === "Added" &&
        content.includes("throw new UnsupportedOperationException(") &&
        content.length < 400
      ) {
        stubs.push(e);
      } else {
        nonStubs.push(e);
      }
    }

    if (stubs.length > 1) {
      const stubNames = stubs.map((s) => `\`${s.entity_name}\``).join(", ");
      const maxDeps = Math.max(...stubs.map((s) => s.dependent_count));
      lines.push(
        `**${stubs.length} stub methods** (throw UnsupportedOperationException): ${stubNames} — max ${maxDeps} callers each`,
      );
      lines.push("");
    }

    for (const e of stubs.length > 1 ? nonStubs : entities) {
      const pub = e.is_public_api ? " **PUBLIC**" : "";
      const callers = e.dependent_names
        .slice(0, 4)
        .map(([n]) => n)
        .join(", ");
      const callerStr = callers
        ? ` | callers: ${callers}${e.dependent_names.length > 4 ? ` +${e.dependent_names.length - 4}` : ""}`
        : "";

      lines.push(
        `### \`${e.entity_name}\` (${e.entity_type}, ${e.change_type}) :${e.start_line}-${e.end_line}`,
      );
      lines.push(
        `risk=${e.risk_level}(${e.risk_score.toFixed(2)}) blast=${e.blast_radius} deps=${e.dependent_count}${pub}${callerStr}`,
      );

      // Contract co-location: for @Override methods, find the interface/parent signature
      const after = e.after_content ?? "";
      if (after.includes("@Override") && e.entity_type === "method") {
        const contract = findContract(e, entityReviews, entitiesByName);
        if (contract) {
          lines.push("**Contract:**");
          lines.push("```");
          lines.push(contract);
          lines.push("```");
        }
      }

      const before = e.before_content ?? "";

      if (before && after && e.change_type === "Modified") {
        const changes = diffLines(before, after);
        const diffStr: string[] = [];
        for (const part of changes) {
          const prefix = part.added ? "+" : part.removed ? "-" : " ";
          const partLines = part.value.split("\n").filter((l) => l !== "");
          if (prefix === " " && partLines.length > 6) {
            diffStr.push(" " + partLines.slice(0, 2).join("\n "));
            diffStr.push(`  ... (${partLines.length - 4} unchanged lines)`);
            diffStr.push(" " + partLines.slice(-2).join("\n "));
          } else {
            for (const line of partLines) {
              diffStr.push(prefix + line);
            }
          }
        }
        lines.push("```diff");
        lines.push(diffStr.join("\n"));
        lines.push("```");
      } else if (after) {
        if (after.length <= 800) {
          lines.push("```");
          lines.push(after);
          lines.push("```");
        } else {
          lines.push("```");
          lines.push(after.slice(0, 600));
          lines.push(`... (${after.length - 600} more chars)`);
          lines.push("```");
        }
      } else if (before) {
        if (before.length <= 800) {
          lines.push("```");
          lines.push(before);
          lines.push("```");
        } else {
          lines.push("```");
          lines.push(before.slice(0, 600));
          lines.push(`... (${before.length - 600} more chars — deleted)`);
          lines.push("```");
        }
      }
      lines.push("");
    }
  }

  return lines.join("\n");
}

/**
 * Find the interface/parent contract for an @Override method.
 * Looks for an entity with the same name in a different file that is an
 * interface method or has Javadoc describing the contract.
 */
function findContract(
  entity: EntityReview,
  allEntities: EntityReview[],
  byName: Map<string, EntityReview[]>,
): string | null {
  // Strategy 1: look for same-named method entity that is an interface/abstract method
  const candidates = byName.get(entity.entity_name) ?? [];
  for (const c of candidates) {
    if (c.entity_id === entity.entity_id) continue;
    if (c.file_path === entity.file_path) continue;
    const content = c.after_content ?? c.before_content ?? "";
    if (!content) continue;
    // Pure interface method (no body, just signature) — best contract source
    if (!content.includes("{") || content.includes("interface ")) {
      return content.trim();
    }
  }

  // Strategy 2: look for an interface/class entity that CONTAINS this method name
  // (the method is defined inside a class/interface body, with Javadoc)
  for (const c of allEntities) {
    if (c.entity_id === entity.entity_id) continue;
    if (c.file_path === entity.file_path) continue;
    if (c.entity_type !== "interface" && c.entity_type !== "class") continue;

    const content = c.after_content ?? c.before_content ?? "";
    if (!content.includes(entity.entity_name)) continue;
    // Only use interface entities, not other class implementations
    if (!content.includes("interface ")) continue;

    const contract = extractMethodContract(content, entity.entity_name);
    if (contract) return contract;
  }

  return null;
}

/**
 * Extract a method signature + preceding Javadoc from a class/interface body.
 */
function extractMethodContract(
  classContent: string,
  methodName: string,
): string | null {
  const lines = classContent.split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes(methodName) && (lines[i].includes("(") || lines[i].includes(";"))) {
      // Walk back to find Javadoc
      let start = i;
      for (let j = i - 1; j >= 0; j--) {
        const trimmed = lines[j].trim();
        if (trimmed === "" || trimmed.startsWith("*") || trimmed.startsWith("/**") || trimmed.startsWith("*/") || trimmed.startsWith("@")) {
          start = j;
        } else {
          break;
        }
      }
      // Include the signature line + Javadoc
      const contractLines = lines.slice(start, i + 1);
      const result = contractLines.map((l) => l.trimStart()).join("\n").trim();
      if (result.length > 10) return result;
    }
  }
  return null;
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
