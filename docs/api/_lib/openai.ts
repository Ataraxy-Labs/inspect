import { SYSTEM_REVIEW, SYSTEM_VALIDATE, PROMPT_DEEP, PROMPT_VALIDATE, truncateDiff } from "./prompts";

export interface Finding {
  issue: string;
  evidence?: string;
  severity?: string;
  file?: string;
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
  try {
    const data = JSON.parse(stripCodeFences(text));
    return (data.issues || [])
      .map((item: any) => {
        if (typeof item === "string") return { issue: item };
        if (typeof item === "object" && item.issue) {
          return { issue: item.issue, evidence: item.evidence, severity: item.severity, file: item.file };
        }
        return null;
      })
      .filter(Boolean) as Finding[];
  } catch {
    return [];
  }
}

async function callOpenAI(apiKey: string, model: string, system: string, prompt: string, temperature: number): Promise<string> {
  const resp = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json" },
    body: JSON.stringify({
      model,
      messages: [{ role: "system", content: system }, { role: "user", content: prompt }],
      temperature,
    }),
  });
  if (!resp.ok) throw new Error(`OpenAI error ${resp.status}: ${await resp.text()}`);
  const data = await resp.json();
  return data.choices?.[0]?.message?.content || "";
}

/** deep_v2: two-temperature merge + diff-aware validation. */
export async function reviewDeepV2(apiKey: string, model: string, prTitle: string, diff: string, maxFindings = 15): Promise<Finding[]> {
  const truncated = truncateDiff(diff, 80000);
  const prompt = PROMPT_DEEP.replace("{pr_title}", prTitle).replace("{diff}", truncated);

  // Two passes in parallel
  const [pass0, pass1] = await Promise.allSettled([
    callOpenAI(apiKey, model, SYSTEM_REVIEW, prompt, 0),
    callOpenAI(apiKey, model, SYSTEM_REVIEW, prompt, 0.3),
  ]);

  const allFindings: Finding[] = [];
  const seen = new Set<string>();

  for (const result of [pass0, pass1]) {
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

  if (allFindings.length <= 2) return allFindings;

  // Validation pass
  const candidatesText = allFindings.map((f, i) =>
    `${i + 1}. ${f.issue}${f.evidence ? `\n   Evidence: ${f.evidence}` : ""}`
  ).join("\n");

  const validatePrompt = PROMPT_VALIDATE
    .replace("{pr_title}", prTitle)
    .replace("{diff}", truncated)
    .replace("{candidates}", candidatesText);

  try {
    const text = await callOpenAI(apiKey, model, SYSTEM_VALIDATE, validatePrompt, 0);
    return parseIssues(text).slice(0, maxFindings);
  } catch {
    return allFindings.slice(0, maxFindings);
  }
}
