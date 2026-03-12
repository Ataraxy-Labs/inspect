import {
  SYSTEM_PRECISE,
  SYSTEM_DATA,
  SYSTEM_CONCURRENCY,
  SYSTEM_CONTRACTS,
  SYSTEM_SECURITY,
  SYSTEM_TYPOS,
  SYSTEM_RUNTIME,
  SYSTEM_VALIDATE,
  PROMPT_DATA,
  PROMPT_CONCURRENCY,
  PROMPT_CONTRACTS,
  PROMPT_SECURITY,
  PROMPT_TYPOS,
  PROMPT_RUNTIME,
  PROMPT_GENERAL,
  PROMPT_VALIDATE,
  truncateDiff,
} from "./prompts";

export interface Finding {
  issue: string;
  evidence?: string;
  severity?: string;
  file?: string;
}

interface TriageEntity {
  name: string;
  type: string;
  file: string;
  risk: string;
  score: string;
  classification: string;
  change_type: string;
  public_api: boolean;
}

/** Call inspect-api /v1/triage for entity-level risk analysis. */
export async function fetchTriage(
  apiKey: string,
  apiUrl: string,
  repo: string,
  prNumber: number
): Promise<string> {
  try {
    const resp = await fetch(`${apiUrl}/v1/triage`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ repo, pr_number: prNumber }),
    });

    if (!resp.ok) return "";

    const data = await resp.json();
    const entities: TriageEntity[] = data.entities || [];
    if (entities.length === 0) return "";

    const meaningful = entities
      .filter((e) => ["modified", "added"].includes(e.change_type) && e.type !== "chunk")
      .sort((a, b) => parseFloat(b.score) - parseFloat(a.score))
      .slice(0, 20);

    if (meaningful.length === 0) return "";

    const byFile: Record<string, TriageEntity[]> = {};
    for (const e of meaningful) {
      (byFile[e.file] ??= []).push(e);
    }

    const fileEntries = Object.entries(byFile).sort(
      (a, b) =>
        Math.max(...b[1].map((e) => parseFloat(e.score))) -
        Math.max(...a[1].map((e) => parseFloat(e.score)))
    );

    const lines = ["## Entity-level triage (highest-risk changes):"];
    for (const [fp, ents] of fileEntries) {
      lines.push(`\n**${fp}**:`);
      for (const e of ents) {
        const pub = e.public_api ? " [PUBLIC API]" : "";
        lines.push(`  - ${e.name} (${e.type}, ${e.change_type}, ${e.classification})${pub}`);
      }
    }
    return lines.join("\n");
  } catch {
    return "";
  }
}

function stripCodeFences(text: string): string {
  const trimmed = text.trim();
  if (trimmed.startsWith("```")) {
    let content = trimmed.slice(3);
    if (content.startsWith("json")) content = content.slice(4);
    const end = content.lastIndexOf("```");
    if (end !== -1) return content.slice(0, end).trim();
    return content.trim();
  }
  return trimmed;
}

function parseIssues(text: string): Finding[] {
  const cleaned = stripCodeFences(text);
  try {
    const data = JSON.parse(cleaned);
    const issues = data.issues || [];
    return issues
      .map((item: any) => {
        if (typeof item === "string") {
          return { issue: item };
        }
        if (typeof item === "object" && item.issue) {
          return {
            issue: item.issue,
            evidence: item.evidence || undefined,
            severity: item.severity || undefined,
            file: item.file || undefined,
          };
        }
        return null;
      })
      .filter(Boolean) as Finding[];
  } catch {
    return [];
  }
}

async function callOpenAI(
  apiKey: string,
  model: string,
  system: string,
  prompt: string,
  temperature: number,
  seed?: number
): Promise<string> {
  const body: any = {
    model,
    messages: [
      { role: "system", content: system },
      { role: "user", content: prompt },
    ],
    temperature,
  };
  if (seed !== undefined) body.seed = seed;

  const resp = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });

  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(`OpenAI API error ${resp.status}: ${text}`);
  }

  const data = await resp.json();
  return data.choices?.[0]?.message?.content || "";
}

/** Extract file basenames from a unified diff. */
function extractDiffFiles(diff: string): Set<string> {
  const basenames = new Set<string>();
  for (const line of diff.split("\n")) {
    if (line.startsWith("+++ b/") || line.startsWith("--- a/")) {
      const path = line.slice(6);
      if (path && path !== "/dev/null") {
        const base = path.split("/").pop()!.toLowerCase();
        basenames.add(base);
      }
    }
  }
  return basenames;
}

const CODE_EXTS = new Set([
  ".py", ".js", ".ts", ".tsx", ".jsx", ".java", ".go", ".rs", ".rb",
  ".c", ".cpp", ".cs", ".swift", ".kt", ".scala", ".hbs", ".erb",
  ".ex", ".exs", ".hcl",
]);

/** Drop issues that mention code files not present in the diff. */
function structuralFileFilter(issues: Finding[], diffBasenames: Set<string>): Finding[] {
  if (diffBasenames.size === 0) return issues;

  return issues.filter((f) => {
    const text = f.issue.toLowerCase();
    const words = text.replace(/\//g, " / ").split(/\s+/);
    for (const word of words) {
      if ([...CODE_EXTS].some((ext) => word.endsWith(ext))) {
        const base = word.split("/").pop()!;
        if (!diffBasenames.has(base)) return false;
      }
    }
    return true;
  });
}

function fillPrompt(template: string, prTitle: string, diff: string, triage: string): string {
  return template
    .replace("{pr_title}", prTitle)
    .replace("{triage}", triage)
    .replace("{diff}", diff);
}

/** v26: 9 lenses (6 specialized + 1 general + 2 diversity) with structural filter + validation. */
export async function reviewV26(
  apiKey: string,
  model: string,
  prTitle: string,
  diff: string,
  triage: string = ""
): Promise<Finding[]> {
  const truncated = truncateDiff(diff, 80000);
  const diffBasenames = extractDiffFiles(diff);

  // Build all prompts with triage context
  const pData = fillPrompt(PROMPT_DATA, prTitle, truncated, triage);
  const pConcurrency = fillPrompt(PROMPT_CONCURRENCY, prTitle, truncated, triage);
  const pContracts = fillPrompt(PROMPT_CONTRACTS, prTitle, truncated, triage);
  const pSecurity = fillPrompt(PROMPT_SECURITY, prTitle, truncated, triage);
  const pTypos = fillPrompt(PROMPT_TYPOS, prTitle, truncated, triage);
  const pRuntime = fillPrompt(PROMPT_RUNTIME, prTitle, truncated, triage);
  const pGeneral = fillPrompt(PROMPT_GENERAL, prTitle, truncated, triage);

  // 9 lenses in parallel
  const results = await Promise.allSettled([
    // 6 specialized @ T=0, seed=42
    callOpenAI(apiKey, model, SYSTEM_DATA, pData, 0, 42),
    callOpenAI(apiKey, model, SYSTEM_CONCURRENCY, pConcurrency, 0, 42),
    callOpenAI(apiKey, model, SYSTEM_CONTRACTS, pContracts, 0, 42),
    callOpenAI(apiKey, model, SYSTEM_SECURITY, pSecurity, 0, 42),
    callOpenAI(apiKey, model, SYSTEM_TYPOS, pTypos, 0, 42),
    callOpenAI(apiKey, model, SYSTEM_RUNTIME, pRuntime, 0, 42),
    // 1 general @ T=0, seed=42
    callOpenAI(apiKey, model, SYSTEM_PRECISE, pGeneral, 0, 42),
    // 2 diversity (general) @ T=0.15, seeds 42 and 123
    callOpenAI(apiKey, model, SYSTEM_PRECISE, pGeneral, 0.15, 42),
    callOpenAI(apiKey, model, SYSTEM_PRECISE, pGeneral, 0.15, 123),
  ]);

  // Merge + dedup (first-80-char lowercase key)
  const allFindings: Finding[] = [];
  const seen = new Set<string>();

  for (const result of results) {
    if (result.status === "fulfilled") {
      for (const f of parseIssues(result.value)) {
        const key = f.issue.toLowerCase().slice(0, 80);
        if (!seen.has(key)) {
          seen.add(key);
          allFindings.push(f);
        }
      }
    }
  }

  // Structural file filter
  const filtered = structuralFileFilter(allFindings, diffBasenames);

  if (filtered.length === 0) return [];
  if (filtered.length <= 2) return filtered;

  // Validation pass
  const candidatesText = filtered
    .map(
      (f, i) =>
        `${i + 1}. ${f.issue}${f.evidence ? `\n   Evidence: ${f.evidence}` : ""}`
    )
    .join("\n");

  const validatePrompt = PROMPT_VALIDATE.replace("{pr_title}", prTitle)
    .replace("{diff}", truncated)
    .replace("{candidates}", candidatesText);

  try {
    const validateText = await callOpenAI(apiKey, model, SYSTEM_VALIDATE, validatePrompt, 0, 42);
    const validated = parseIssues(validateText);
    return validated.slice(0, 7);
  } catch {
    return filtered.slice(0, 5);
  }
}
