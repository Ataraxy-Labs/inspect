import { diffLines } from "diff";
import type { DetectorFinding, EntityReview } from "./types.js";

/** System prompt: review protocol, not just a bug list. */
export const SYSTEM_PROMPT_CLAUDE = `You are an expert senior engineer performing a thorough code review.

Review the provided code entities and their diffs. For each entity:
1. Understand what changed and why.
2. Read any other relevant files (callers, interfaces, tests, related modules) using tools to fully understand the context.
3. Call out concrete correctness bugs: logic errors, race conditions, null safety, API misuse, argument mismatches, missing awaits, broken control flow, type errors, security issues.

Use grep and read freely to explore callers, callees, interfaces, and related code. Follow dependency chains — if a function signature changed, check all callers. If a method overrides an interface, read the interface contract.

Do NOT report: style, naming, missing tests, documentation, suggestions, or issues in deleted-only code. Only report bugs you are confident are real.

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
  const topEntityIds = new Set(topEntities.map((e) => e.entity_id));

  // Promote entities with detector findings that aren't already in top-N
  // Skip test files to avoid noise
  const findingEntityIds = new Set(findings.map((f) => f.entity_id));
  for (const e of entityReviews) {
    if (
      findingEntityIds.has(e.entity_id) &&
      !topEntityIds.has(e.entity_id) &&
      !e.file_path.includes("test") &&
      !e.file_path.includes("spec") &&
      !e.file_path.includes("_test.")
    ) {
      topEntities.push(e);
      topEntityIds.add(e.entity_id);
    }
  }

  // Build a lookup of ALL entities by name for contract co-location
  const entitiesByName = new Map<string, EntityReview[]>();
  for (const e of entityReviews) {
    const arr = entitiesByName.get(e.entity_name) ?? [];
    arr.push(e);
    entitiesByName.set(e.entity_name, arr);
  }

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

  // Domain-specific review hints
  const domainHints = generateDomainHints(entityReviews);
  if (domainHints.length > 0) {
    lines.push("## Language-Specific Review Hints");
    for (const hint of domainHints) {
      lines.push(`- ${hint}`);
    }
    lines.push("");
  }

  // Findings — strongest signals first
  if (findings.length > 0) {
    lines.push(`## Detector Findings (${findings.length})`);
    lines.push(`These are automatically detected issues. Investigate each one — read the code and callers to confirm or refute.`);
    lines.push("");
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
      // Add targeted review question based on finding type
      const question = generateFindingQuestion(f);
      if (question) {
        lines.push(`  → **Investigate:** ${question}`);
      }
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

      // Caller context: for entities with dependents, show how they're called
      // This helps the agent spot argument mismatches, null-unsafe calls, race conditions
      if (e.dependent_count > 0 && e.dependent_names.length > 0) {
        const callerSnippets = getCallerSnippets(e, entityReviews);
        if (callerSnippets.length > 0) {
          lines.push("**Called from:**");
          for (const snippet of callerSnippets) {
            lines.push(snippet);
          }
        }
      }

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

      // Review cue: flag when method name implies a specific thing but body doesn't reference it
      const cue = generateReviewCue(e);
      if (cue) {
        lines.push(`⚠️ **Review cue:** ${cue}`);
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

/**
 * Generate a review cue if the method name implies something specific
 * but the implementation body doesn't reference it.
 */
function generateReviewCue(entity: EntityReview): string | null {
  if (entity.entity_type !== "method") return null;
  const body = entity.after_content ?? "";
  if (!body) return null;

  // Skip stubs — they're already flagged separately
  if (body.includes("throw new UnsupportedOperationException(")) return null;

  // Extract key terms from method name (e.g., "getBouncyCastleProvider" → ["bouncy", "castle"])
  const name = entity.entity_name;
  // Split camelCase/PascalCase into words
  const nameWords = name
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w.length > 3 && !["get", "set", "create", "make", "build", "find", "load", "init", "with", "provider", "factory", "util", "utils", "helper", "service", "manager"].includes(w));

  if (nameWords.length === 0) return null;

  // Strip the method signature line(s) — only check the body
  const bodyLines = body.split("\n");
  const braceIdx = bodyLines.findIndex((l) => l.includes("{"));
  const implBody = braceIdx >= 0 ? bodyLines.slice(braceIdx + 1).join("\n").toLowerCase() : body.toLowerCase();
  const missingTerms = nameWords.filter((w) => !implBody.includes(w));

  // If significant terms from the name are absent from the body, flag it
  if (missingTerms.length > 0 && missingTerms.length >= nameWords.length / 2) {
    return `Method name contains "${missingTerms.join(", ")}" but implementation body does not reference ${missingTerms.length === 1 ? "it" : "them"}. Verify the implementation matches the contract.`;
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

/**
 * Generate a targeted review question for a detector finding.
 */
function generateFindingQuestion(f: DetectorFinding): string | null {
  const ruleQuestions: Record<string, string> = {
    "removed-guard": `What scenario did the removed guard \`${f.evidence}\` protect against? Is that scenario still handled?`,
    "nil-check-missing": `What happens if this call fails? Grep for all callers of \`${f.entity_name}\` and check error handling.`,
    "foreach-async": `Are the async operations inside forEach awaited? Check if Promise.all or for...of is needed.`,
    "missing-await": `Is the return value of this async call used? Check if a missing await causes a race condition.`,
    "catch-swallow": `What errors are being swallowed? Check if the catch block should re-throw or log.`,
    "signature-change-with-callers": `This function's signature changed. Grep for all callers and verify they pass the correct arguments.`,
    "arity-change-with-callers": `Parameter count changed. Grep for all callers — do they pass the right number of arguments?`,
    "type-change-propagation": `A type changed but dependents may not be updated. Check all usages of this entity.`,
    "hardcoded-secret": `Is this a hardcoded credential or token? Verify it's not a real secret.`,
    "logic-gate-swap": `AND/OR logic changed. Verify the new logic matches the intended behavior.`,
    "null-return-introduced": `A null/None return was added. Do all callers handle null?`,
    "argument-order-swap": `Arguments may have been swapped. Verify the order matches the function signature.`,
    "callee-swap": `A different function is being called. Verify the replacement has compatible behavior.`,
  };
  return ruleQuestions[f.rule_id] ?? null;
}

/**
 * Extract calling-context snippets from dependent entities.
 * Shows how this entity is invoked, helping the agent spot:
 * - Argument mismatches (wrong types, wrong order)
 * - Null-unsafe calls (passing nullable values)
 * - Race conditions (concurrent access patterns)
 */
function getCallerSnippets(
  entity: EntityReview,
  allEntities: EntityReview[],
): string[] {
  const snippets: string[] = [];
  const entityName = entity.entity_name;

  // Look through dependent entities for call sites
  for (const [depName, depFile] of entity.dependent_names.slice(0, 3)) {
    // Find the dependent entity in our review set
    const caller = allEntities.find(
      (e) => e.entity_name === depName && e.file_path === depFile,
    );
    if (!caller) continue;

    const callerBody = caller.after_content ?? caller.before_content ?? "";
    if (!callerBody.includes(entityName)) continue;

    // Extract the lines around the call site
    const callerLines = callerBody.split("\n");
    for (let i = 0; i < callerLines.length; i++) {
      if (callerLines[i].includes(entityName)) {
        const start = Math.max(0, i - 1);
        const end = Math.min(callerLines.length, i + 3);
        const context = callerLines
          .slice(start, end)
          .map((l) => l.trimStart())
          .join("\n");
        if (context.length < 300) {
          snippets.push(
            `- \`${depName}\` in ${depFile.split("/").pop()}:\n  \`\`\`\n  ${context}\n  \`\`\``,
          );
        }
        break;
      }
    }
  }
  return snippets;
}

/**
 * Generate language/framework-specific review hints based on file extensions
 * and detected patterns in the PR.
 */
function generateDomainHints(entityReviews: EntityReview[]): string[] {
  const hints: string[] = [];
  const extensions = new Set(
    entityReviews.map((e) => {
      const parts = e.file_path.split(".");
      return parts.length > 1 ? parts[parts.length - 1] : "";
    }),
  );
  const allContent = entityReviews
    .slice(0, 30)
    .map((e) => (e.after_content ?? "") + (e.before_content ?? ""))
    .join("\n");

  if (extensions.has("go")) {
    hints.push(
      "**Go:** Check Exec/Query first-arg must be string (not splatted interface{}). Check concurrent map/struct access for race conditions. Check error returns are not silently ignored.",
    );
  }
  if (extensions.has("java")) {
    hints.push(
      "**Java:** Check null safety on getUser()/getAuthenticationSession()/getContext() — these return null before auth completes. Check @Override methods satisfy interface contracts. Check synchronized access.",
    );
  }
  if (extensions.has("py")) {
    hints.push(
      "**Python:** Check __reduce__/__init__ argument ordering for pickle safety. Check mutable default args. Check Redis key expiry/TTL logic.",
    );
  }
  if (extensions.has("rb")) {
    hints.push(
      "**Ruby:** Ruby has no method overloading — duplicate def silently overwrites. Check method arity matches all callers. Check string interpolation in SQL.",
    );
  }
  if (extensions.has("ts") || extensions.has("tsx") || extensions.has("js")) {
    hints.push(
      "**JS/TS:** Check forEach+async (must use for...of or Promise.all). Check await on async calls. Check error handling in try/catch with async.",
    );
  }
  if (allContent.includes("transaction") || allContent.includes("concurrent") || allContent.includes("mutex") || allContent.includes("synchronized")) {
    hints.push(
      "**Concurrency:** Check-then-act patterns without locks are race conditions. Verify atomicity of read-modify-write sequences.",
    );
  }

  return hints;
}
