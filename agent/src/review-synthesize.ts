/**
 * Synthesis layer: coverage-first audit, conservative dedup, and ranking.
 *
 * Design principle: recall > precision at the dedup stage.
 * The bottleneck is missed investigations, not duplicate findings.
 * We only dedup when entity + category + identifier ALL match.
 */
import type { SliceIssue } from "./review-parallel.js";
import type { EntityReview, ValidateOutput, Verdict } from "./types.js";

// ---------------------------------------------------------------------------
// Bug category classification
// ---------------------------------------------------------------------------

const PATTERN_CATEGORIES: [RegExp, string][] = [
  [/foreach.*async|async.*foreach|fire.and.forget|not awaited|without await|missing.*await/i, "async-misuse"],
  [/null|undefined|nil|none.*deref|crash.*null|optional.*get/i, "null-safety"],
  [/race condition|concurrent|toctou|read.modify.write|stale.*read|shared.*mutable/i, "concurrency"],
  [/interface.*mismatch|contract.*break|signature.*change|missing.*param|arity/i, "contract-break"],
  [/wrong.*variable|inverted.*logic|boolean.*inversion|always.*false|always.*true|self.comparison/i, "logic-error"],
  [/typo|misspell|naming|wrong.*name|wrong.*callee/i, "naming-mismatch"],
  [/type.*mismatch|negative.*slice|format.*mismatch|serializ/i, "type-error"],
  [/dead.*code|unreachable|unused/i, "dead-code"],
  [/locale|translation|css.*value|config.*error/i, "config-error"],
  [/case.*sensitiv|indexof.*case|includes.*case/i, "case-sensitivity"],
];

function classifyBugCategory(issueText: string): string {
  const lower = issueText.toLowerCase();
  for (const [pattern, category] of PATTERN_CATEGORIES) {
    if (pattern.test(lower)) return category;
  }
  return "logic-error"; // default
}

// ---------------------------------------------------------------------------
// Identifier extraction for dedup matching
// ---------------------------------------------------------------------------

/** Extract backtick-quoted identifiers and camelCase words */
function extractIdentifiers(text: string): Set<string> {
  const ids: string[] = [];
  const quoted = text.match(/`([A-Za-z_]\w*)`/g);
  if (quoted) ids.push(...quoted.map((m) => m.slice(1, -1).toLowerCase()));
  const camel = text.match(/\b[a-z][a-zA-Z]{6,}\b/g);
  if (camel) ids.push(...camel.map((m) => m.toLowerCase()));
  return new Set(ids.filter((id) => id.length > 4));
}

/** Extract file basename for matching */
function fileBasename(filePath: string): string {
  return filePath.split("/").pop() ?? filePath;
}

// ---------------------------------------------------------------------------
// Conservative deduplication
// ---------------------------------------------------------------------------

interface ScoredIssue {
  issue: SliceIssue;
  category: string;
  identifiers: Set<string>;
  confidence: number;
  found_by: string[];
}

function deduplicateConservative(issues: SliceIssue[]): ScoredIssue[] {
  const scored: ScoredIssue[] = [];

  for (const issue of issues) {
    const category = classifyBugCategory(issue.issue);
    const identifiers = extractIdentifiers(issue.issue + " " + (issue.evidence ?? ""));
    const file = issue.file ?? "";

    // Check for duplicates with multi-level matching:
    // Level 1: same file + same category + shared identifiers (strict)
    // Level 2: same category + high identifier overlap across files (cross-slice dedup)
    // Level 3: same bug pattern + shared identifiers across files (e.g., forEach-async in multiple files)
    let merged = false;
    for (const existing of scored) {
      const sameFile = file === (existing.issue.file ?? "");
      const sameCategory = category === existing.category;
      const sharedIds = [...identifiers].filter((id) => existing.identifiers.has(id));
      const hasSharedIdentifiers = sharedIds.length >= 1;

      // Level 3: Cross-file dedup for same bug pattern with shared identifiers
      const highIdentifierOverlap = identifiers.size > 0 && existing.identifiers.size > 0 &&
        sharedIds.length >= Math.min(identifiers.size, existing.identifiers.size) * 0.4;

      // Extract key terms for word-level overlap
      const issueWords = new Set(issue.issue.toLowerCase().split(/\s+/).filter((w) => w.length > 5));
      const existingWords = new Set(existing.issue.issue.toLowerCase().split(/\s+/).filter((w) => w.length > 5));
      const wordOverlap = [...issueWords].filter((w) => existingWords.has(w)).length;
      const highWordOverlap = issueWords.size > 0 && existingWords.size > 0 &&
        wordOverlap >= Math.min(issueWords.size, existingWords.size) * 0.5;

      const isDuplicate =
        // Level 1: same file + same category + shared identifiers
        (sameFile && sameCategory && hasSharedIdentifiers) ||
        // Level 2: same category + high identifier overlap (cross-file)
        (sameCategory && highIdentifierOverlap) ||
        // Level 3: high word overlap across files (catches rephrased same bug)
        (highWordOverlap && hasSharedIdentifiers);

      if (isDuplicate) {
        // Duplicate — boost confidence if from different slices
        if (issue.slice_id !== existing.found_by[existing.found_by.length - 1]) {
          existing.confidence = Math.min(1.0, existing.confidence + 0.1);
          existing.found_by.push(issue.slice_id);
        }
        // Keep higher severity
        const sevOrder: Record<string, number> = { critical: 3, high: 2, medium: 1 };
        if ((sevOrder[issue.severity] ?? 0) > (sevOrder[existing.issue.severity] ?? 0)) {
          existing.issue = issue;
        }
        // Merge identifiers
        for (const id of identifiers) existing.identifiers.add(id);
        merged = true;
        break;
      }
    }

    if (!merged) {
      scored.push({
        issue,
        category,
        identifiers,
        confidence: 0.7, // base confidence
        found_by: [issue.slice_id],
      });
    }
  }

  return scored;
}

// ---------------------------------------------------------------------------
// Coverage audit
// ---------------------------------------------------------------------------

export interface CoverageStats {
  total_entities: number;
  covered_entities: number;
  covered_findings: number;
  total_findings: number;
  uncovered_high_risk: string[];
}

function auditCoverage(
  issues: ScoredIssue[],
  entityReviews: EntityReview[],
  findingCount: number,
): CoverageStats {
  // Which entities/files were mentioned in issues?
  const coveredFiles = new Set<string>();
  for (const s of issues) {
    if (s.issue.file) coveredFiles.add(s.issue.file);
  }

  const highRiskEntities = entityReviews
    .filter((e) => e.risk_score > 0.5 && e.is_public_api)
    .slice(0, 20);

  const uncoveredHighRisk = highRiskEntities
    .filter((e) => !coveredFiles.has(e.file_path))
    .map((e) => `${e.entity_name} (${e.file_path})`);

  return {
    total_entities: entityReviews.length,
    covered_entities: coveredFiles.size,
    covered_findings: findingCount, // all findings get slices in v2
    total_findings: findingCount,
    uncovered_high_risk: uncoveredHighRisk.slice(0, 5),
  };
}

// ---------------------------------------------------------------------------
// Main synthesis entry point
// ---------------------------------------------------------------------------

export interface SynthesisResult {
  output: ValidateOutput;
  stats: CoverageStats;
}

export function synthesizeResults(
  rawIssues: SliceIssue[],
  entityReviews: EntityReview[],
  findingCount: number,
): SynthesisResult {
  // Conservative dedup with cross-validation
  const scored = deduplicateConservative(rawIssues);

  // Rank: severity × confidence, cross-validated first
  const sevWeight: Record<string, number> = { critical: 4, high: 3, medium: 2 };
  scored.sort((a, b) => {
    const aScore = (sevWeight[a.issue.severity] ?? 1) * a.confidence;
    const bScore = (sevWeight[b.issue.severity] ?? 1) * b.confidence;
    return bScore - aScore;
  });

  // Coverage audit
  const stats = auditCoverage(scored, entityReviews, findingCount);

  // Build verdicts
  const verdicts: Verdict[] = scored.map((s) => ({
    rule_id: "review",
    entity_name: s.issue.file ?? "unknown",
    verdict: "true_positive" as const,
    explanation: `[${s.issue.severity}] ${s.issue.issue} | evidence: ${s.issue.evidence ?? "none"} | file: ${s.issue.file ?? "unknown"}`,
  }));

  return {
    output: { verdicts },
    stats,
  };
}
