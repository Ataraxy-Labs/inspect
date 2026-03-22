#!/usr/bin/env node
/**
 * REAL pipeline test: inspect binary → auto-build slices → parallel review.
 * NO hand-crafting. Slices are constructed from entity graph data only.
 */
import { getModel } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";
import { readFileSync, writeFileSync } from "fs";
import { execSync } from "child_process";
import { resolve } from "path";
import { reviewSlicesParallel, type ReviewSlice } from "./review-parallel.js";
import type { EntityReview, DetectorFinding } from "./types.js";

const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

const model = getModel("anthropic" as any, "claude-sonnet-4-6");
model.baseUrl = "http://localhost:8317";
process.env.ANTHROPIC_API_KEY =
  "6cf41538d16fcc1ac937a906dcdc5f92f31894b38978bf97a72a46ed8d5791c7";

// ── Config ──
const repoDir =
  process.argv[2] ||
  "/tmp/martian-eval/worktrees/keycloak__keycloak/744e031019af_25bf964a844e";
const range = process.argv[3] || "744e031019af..25bf964a844e";
const inspectBin = resolve(
  import.meta.dirname,
  "../../target/release/inspect",
);

// ── Step 1: Run inspect ──
console.error("[pipeline] Running inspect...");
const inspectRaw = execSync(
  `${inspectBin} diff --format json -C "${repoDir}" "${range}"`,
  { encoding: "utf-8", maxBuffer: 10 * 1024 * 1024 },
);
const inspectData = JSON.parse(inspectRaw);
const findings: DetectorFinding[] = inspectData.findings ?? [];
const entityReviews: EntityReview[] = inspectData.entity_reviews ?? [];

entityReviews.sort((a, b) => b.risk_score - a.risk_score);
console.error(
  `[pipeline] ${entityReviews.length} entities, ${findings.length} findings`,
);

// ── Step 2: Auto-build slices from entity graph ──

// Skip test files
function isTest(path: string): boolean {
  return /test|spec|Test\.java|_test\.|\.test\./i.test(path);
}

// Get top non-test entities
const topEntities = entityReviews
  .filter((e) => !isTest(e.file_path))
  .slice(0, 20);

// Fix 1: Finding-anchored promotion — entities with detector findings
// get into the slice pool regardless of rank
const topEntityIds = new Set(topEntities.map((e) => e.entity_id));
const findingEntityIds = new Set(findings.map((f) => f.entity_id));
for (const e of entityReviews) {
  if (
    findingEntityIds.has(e.entity_id) &&
    !topEntityIds.has(e.entity_id) &&
    !isTest(e.file_path)
  ) {
    topEntities.push(e);
    topEntityIds.add(e.entity_id);
  }
}

// Fix 2: Small-PR mode — if few non-test entities, include ALL in one mega-slice
const allNonTest = entityReviews.filter((e) => !isTest(e.file_path));
const isSmallPR = allNonTest.length <= 15;
if (isSmallPR) {
  // Add any missing non-test entities
  for (const e of allNonTest) {
    if (!topEntityIds.has(e.entity_id)) {
      topEntities.push(e);
      topEntityIds.add(e.entity_id);
    }
  }
}

console.error(
  `[pipeline] ${topEntities.length} entities for slicing (${isSmallPR ? "small-PR mode" : "top-20 + finding-promoted"})`,
);

// Build a map of all entities by (name, file) for lookup
const entityMap = new Map<string, EntityReview>();
for (const e of entityReviews) {
  entityMap.set(`${e.entity_name}::${e.file_path}`, e);
}

// Group connected entities into slices by following dependency edges.
// A slice = a high-risk entity + its callers/callees that are also changed.
interface SliceCluster {
  anchor: EntityReview;
  related: EntityReview[];
  findings: DetectorFinding[];
}

function buildClusters(): SliceCluster[] {
  const used = new Set<string>();
  const clusters: SliceCluster[] = [];

  // BFS helper: follow dependency/dependent edges up to maxHops deep,
  // only following edges to entities that exist in entityReviews (changed entities).
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
        // Follow outgoing dependencies (what this entity calls)
        for (const [name, file] of entity.dependency_names) {
          if (collected.length >= 12) break;
          const dep = entityMap.get(`${name}::${file}`);
          if (dep && !isTest(dep.file_path) && !clusterIds.has(dep.entity_id)) {
            collected.push(dep);
            clusterIds.add(dep.entity_id);
            nextFrontier.push(dep);
          }
        }
        // Follow incoming dependencies (what calls this entity)
        for (const [name, file] of entity.dependent_names) {
          if (collected.length >= 12) break;
          const dep = entityMap.get(`${name}::${file}`);
          if (dep && !isTest(dep.file_path) && !clusterIds.has(dep.entity_id)) {
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
    if (anchor.entity_type === "class" || anchor.entity_type === "file")
      continue; // prefer methods/functions

    const clusterIds = new Set([anchor.entity_id]);
    const related = collectReachable(anchor, clusterIds, 3);

    // Attach relevant findings
    const clusterFindings = findings.filter((f) =>
      clusterIds.has(f.entity_id),
    );

    // Only create a cluster if it has substance
    if (related.length > 0 || clusterFindings.length > 0) {
      used.add(anchor.entity_id);
      for (const r of related) used.add(r.entity_id);
      clusters.push({ anchor, related, findings: clusterFindings });
    }
  }

  // After building initial clusters, merge clusters that share related directories
  // (same package — e.g., CreateOrUpdateDevice in database.go should merge with
  //  Authenticate/TagDevice in client.go/impl.go when they're in the same package)
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

        // Merge if they share a directory AND combined size is reasonable
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
          // Pick higher-risk anchor
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

  return mergeClusters(clusters);
}

function clusterToSlice(cluster: SliceCluster, idx: number): ReviewSlice {
  const { anchor, related, findings } = cluster;
  const allEntities = [anchor, ...related];

  const lines: string[] = [];
  lines.push(
    `# Slice ${idx + 1}: ${anchor.entity_name} (${anchor.entity_type}, ${anchor.change_type})`,
  );
  lines.push(
    `## Anchor: \`${anchor.entity_name}\` in ${anchor.file_path}:${anchor.start_line}-${anchor.end_line}`,
  );
  lines.push(
    `risk=${anchor.risk_level}(${anchor.risk_score.toFixed(2)}) blast=${anchor.blast_radius} deps=${anchor.dependent_count}`,
  );
  lines.push("");

  // Findings for this cluster
  if (findings.length > 0) {
    lines.push("## Detector Findings");
    for (const f of findings) {
      lines.push(
        `- **[${f.severity.toUpperCase()}] ${f.rule_id}**: \`${f.entity_name}\` — ${f.message}`,
      );
      lines.push(`  Evidence: \`${f.evidence}\``);
    }
    lines.push("");
  }

  // Entity code
  lines.push("## Code");
  for (const e of allEntities) {
    const content = e.after_content ?? e.before_content ?? "";
    if (!content) continue;

    lines.push(
      `### \`${e.entity_name}\` (${e.entity_type}, ${e.change_type}) in ${e.file_path.split("/").pop()}:${e.start_line}-${e.end_line}`,
    );

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

    const display = content.length > 1200 ? content.slice(0, 1200) + "\n... (truncated)" : content;
    lines.push("```");
    lines.push(display);
    lines.push("```");
    lines.push("");
  }

  // Auto-generate hypotheses based on findings and patterns
  lines.push("## Hypotheses to verify");
  const hyps: string[] = [];

  for (const f of findings) {
    if (f.rule_id === "removed-guard")
      hyps.push(
        `Guard removed: \`${f.evidence}\`. What did it protect? Is the scenario still handled?`,
      );
    if (f.rule_id === "signature-change-with-callers")
      hyps.push(
        `\`${f.entity_name}\` signature changed with callers. Check all call sites.`,
      );
    if (f.rule_id === "type-change-propagation")
      hyps.push(
        `Type \`${f.entity_name}\` changed but dependents not updated. Check usages.`,
      );
  }

  // Pattern-based hypotheses
  for (const e of allEntities) {
    const code = e.after_content ?? "";
    if (
      code.includes("getUser()") &&
      (code.includes("null") || code.includes("!= null"))
    )
      hyps.push(
        `\`${e.entity_name}\` calls getUser() — can it be null? Check auth flow state.`,
      );
    if (code.includes("@Override"))
      hyps.push(
        `\`${e.entity_name}\` is @Override — does it satisfy the parent contract?`,
      );
    if (
      e.dependent_count > 0 &&
      e.change_type === "Added" &&
      e.is_public_api
    )
      hyps.push(
        `New public method \`${e.entity_name}\` with ${e.dependent_count} callers. Check all call sites pass correct args.`,
      );
    // TOCTOU: check-then-act without transaction
    if (
      (code.includes("Count") || code.includes("count")) &&
      (code.includes("Insert") || code.includes("Exec") || code.includes("Create"))
    )
      hyps.push(
        `\`${e.entity_name}\` reads a count then writes — is this atomic? Without a transaction, concurrent requests can both pass the count check (TOCTOU race).`,
      );
    // Misleading error from rowsAffected==0
    if (code.includes("RowsAffected") && code.includes("== 0"))
      hyps.push(
        `\`${e.entity_name}\` returns an error when rowsAffected==0, but zero rows could mean the record doesn't exist OR doesn't match the WHERE filter — not necessarily the named error.`,
      );
    // Time window gaps
    if (
      code.includes("BETWEEN") &&
      (code.includes("expiration") || code.includes("Expir") || code.includes("keepFor") || code.includes("Add(-"))
    )
      hyps.push(
        `\`${e.entity_name}\` uses a time-window filter. Check if the window aligns with cleanup/expiration — records outside the window but not yet deleted will be missed.`,
      );
    // Error propagation change
    if (e.change_type === "Modified" && code.includes("return err") && (e.before_content ?? "").includes("return nil"))
      hyps.push(
        `\`${e.entity_name}\` now propagates errors that were previously swallowed. Check all callers — do they handle this new error path? Could it break existing behavior?`,
      );
  }

  // Deduplicate
  const uniqueHyps = [...new Set(hyps)];
  for (const h of uniqueHyps.slice(0, 8)) {
    lines.push(`- ${h}`);
  }
  lines.push("");

  return {
    id: `slice-${idx + 1}-${anchor.entity_name.slice(0, 30)}`,
    title: `${anchor.entity_name} (${anchor.entity_type})`,
    prompt: lines.join("\n"),
  };
}

const clusters = buildClusters();
console.error(`[pipeline] Built ${clusters.length} clusters`);

// Collect IDs claimed by clusters
const claimedIds = new Set<string>();
for (const c of clusters) {
  claimedIds.add(c.anchor.entity_id);
  for (const r of c.related) claimedIds.add(r.entity_id);
}

// Create single-entity slices for unclaimed non-test entities with content
for (const e of topEntities) {
  if (claimedIds.has(e.entity_id)) continue;
  if (e.entity_type === "class" || e.entity_type === "file" || e.entity_type === "interface") continue;
  if (!e.after_content && !e.before_content) continue;
  const entityFindings = findings.filter((f) => f.entity_id === e.entity_id);
  clusters.push({ anchor: e, related: [], findings: entityFindings });
}

console.error(`[pipeline] ${clusters.length} total clusters (after adding unclaimed entities)`);

// Sort clusters: prioritize by (has findings, max risk score)
clusters.sort((a, b) => {
  const aHasFindings = a.findings.length > 0 ? 1 : 0;
  const bHasFindings = b.findings.length > 0 ? 1 : 0;
  if (bHasFindings !== aHasFindings) return bHasFindings - aHasFindings;
  const aMaxRisk = Math.max(a.anchor.risk_score, ...a.related.map((r) => r.risk_score));
  const bMaxRisk = Math.max(b.anchor.risk_score, ...b.related.map((r) => r.risk_score));
  return bMaxRisk - aMaxRisk;
});

// Deduplicate overlapping clusters: skip if >50% entities already covered
const selectedClusters: SliceCluster[] = [];
const coveredIds = new Set<string>();
for (const c of clusters) {
  const allIds = [c.anchor.entity_id, ...c.related.map((r) => r.entity_id)];
  const overlapCount = allIds.filter((id) => coveredIds.has(id)).length;
  if (allIds.length > 1 && overlapCount / allIds.length > 0.5) continue;
  selectedClusters.push(c);
  for (const id of allIds) coveredIds.add(id);
}

const slices = selectedClusters.slice(0, 10).map((c, i) => clusterToSlice(c, i));
console.error(
  `[pipeline] Generated ${slices.length} slices:`,
);
for (const s of slices) {
  console.error(`  - ${s.id}: ${s.title} (${s.prompt.length} chars)`);
}

// Dump slices for inspection
writeFileSync(
  resolve(import.meta.dirname, "../../auto-slices-dump.md"),
  slices.map((s) => `# ${s.title}\n\n${s.prompt}`).join("\n\n---\n\n"),
);
console.error(
  `[pipeline] Slices dumped to auto-slices-dump.md`,
);

// ── Step 3: Run parallel review ──
const tools = [
  createReadTool(repoDir),
  createGrepTool(repoDir),
  createFindTool(repoDir),
  createBashTool(repoDir),
];

console.error(`\n[pipeline] Starting parallel review...`);
const result = await reviewSlicesParallel(slices, model, tools, {
  concurrency: 4,
  thinkingLevel: "low",
});

console.error(
  `\n${"═".repeat(60)}`,
);
console.error(
  `RESULTS: ${result.merged_issues.length} issues, ${result.total_tool_calls} tools, ${(result.total_elapsed_ms / 1000).toFixed(1)}s`,
);
console.error(`${"═".repeat(60)}`);
for (const issue of result.merged_issues) {
  console.error(
    `  [${issue.severity}] ${issue.file.split("/").pop()}: ${issue.issue.slice(0, 150)}`,
  );
}

// JSON output
console.log(JSON.stringify({ issues: result.merged_issues }, null, 2));
