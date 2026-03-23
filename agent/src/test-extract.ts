#!/usr/bin/env node
/**
 * Quick test: run the new separate extraction call on a saved raw review.
 * Usage: cat /tmp/test_raw_review.txt | node --import tsx/esm src/test-extract.ts
 */
import { getModel, completeSimple } from "@mariozechner/pi-ai";
import type { Context } from "@mariozechner/pi-ai";
import { readFileSync } from "fs";
import { resolve } from "path";

// Load .env
const envPath = resolve(import.meta.dirname, "../../.env");
try {
  for (const line of readFileSync(envPath, "utf-8").split("\n")) {
    const m = line.match(/^([A-Z_]+)=(.+)$/);
    if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
  }
} catch {}

// Read stdin
async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf-8");
}

async function main() {
  const reviewText = await readStdin();
  console.error(`[test-extract] Review text: ${reviewText.length} chars`);

  const provider = process.argv[2] ?? "anthropic";
  const modelId = process.argv[3] ?? "claude-opus-4-6";

  const model = getModel(provider as any, modelId);

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

  const ctx: Context = {
    systemPrompt: "You extract structured bug reports from code review text. Extract issues with concrete functional impact. Include suggestions that describe real functional problems. Exclude pure style nits, naming preferences, and architectural opinions without correctness impact.",
    messages: [
      {
        role: "user" as const,
        content: extractionPrompt,
        timestamp: Date.now(),
      },
    ],
  };

  console.error(`[test-extract] Calling ${provider}/${modelId}...`);
  const t0 = Date.now();

  const result = await completeSimple(model, ctx, {
    temperature: 0,
    maxTokens: 4096,
  });

  let text = "";
  for (const part of result.content) {
    if (part.type === "text") text += part.text;
  }

  const elapsed = (Date.now() - t0) / 1000;
  console.error(`[test-extract] Done in ${elapsed.toFixed(1)}s`);

  // Parse and pretty-print
  try {
    const jsonMatch = text.match(/\{[\s\S]*\}/);
    if (jsonMatch) {
      const parsed = JSON.parse(jsonMatch[0]);
      const issues = parsed.issues ?? [];
      console.error(`[test-extract] Extracted ${issues.length} issues\n`);
      for (let i = 0; i < issues.length; i++) {
        const iss = issues[i];
        console.log(`${i + 1}. [${iss.severity}] ${iss.issue}`);
        console.log(`   file: ${iss.file}`);
        console.log(`   evidence: ${(iss.evidence ?? "").slice(0, 150)}`);
        console.log();
      }
    }
  } catch (e) {
    console.error(`[test-extract] Parse error: ${e}`);
    console.log(text);
  }
}

main().catch((e) => {
  console.error(`[fatal] ${e}`);
  process.exit(1);
});
