#!/usr/bin/env node
/**
 * Test the agentic validator on a real Martian benchmark PR.
 * Usage: node --import tsx/esm src/test-local.ts
 */
import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";
import { readFileSync } from "fs";
import { execSync } from "child_process";
import { resolve } from "path";

import { SYSTEM_PROMPT, buildUserPrompt } from "./prompts.js";

// Load .env — strip quotes and whitespace
const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

// Use a cached Keycloak worktree from the Martian benchmark
const repoDir = "/tmp/martian-eval/worktrees/keycloak__keycloak/10aca5552314_79d11c4890cc";
const inspectBin = resolve(import.meta.dirname, "../../target/release/inspect");

console.error("[test] Running inspect detectors...");
const inspectRaw = execSync(
  `${inspectBin} diff --format json -C "${repoDir}" "10aca5552314..79d11c4890cc"`,
  { encoding: "utf-8", maxBuffer: 10 * 1024 * 1024 },
);
const inspectData = JSON.parse(inspectRaw);

const findings = inspectData.findings ?? [];
const entityReviews = inspectData.entity_reviews ?? [];

console.error(`[test] Findings: ${findings.length}, Entities: ${entityReviews.length}`);

// Get the diff
const diff = execSync(
  `git -C "${repoDir}" diff 10aca5552314..79d11c4890cc`,
  { encoding: "utf-8", maxBuffer: 10 * 1024 * 1024 },
);
console.error(`[test] Diff size: ${diff.length} chars`);

// Build triage section (simplified)
const highRisk = entityReviews
  .filter((e: any) => ["critical", "high"].includes(e.risk_level))
  .sort((a: any, b: any) => b.risk_score - a.risk_score)
  .slice(0, 10);

let triageSection = "";
if (highRisk.length > 0) {
  triageSection = "## Entity-level triage (highest-risk changes)\n";
  for (const e of highRisk) {
    triageSection += `- \`${e.entity_name}\` (${e.entity_type}, ${e.change_type}) | risk=${e.risk_level} (${e.risk_score.toFixed(2)}) | blast=${e.blast_radius} | deps=${e.dependent_count}${e.is_public_api ? " [PUBLIC API]" : ""}\n`;
  }
}

// Only send the first 5 findings to keep cost low for the test
const testFindings = findings.slice(0, 5);
console.error(`[test] Sending ${testFindings.length} findings to agent for validation`);
console.error(`[test] Findings:`);
for (const f of testFindings) {
  console.error(`  [${f.rule_id}] ${f.entity_name}: ${f.message.slice(0, 80)}`);
}

const userPrompt = buildUserPrompt(
  "refactor: Crypto provider improvements",
  diff.slice(0, 80000),
  triageSection,
  testFindings,
  entityReviews,
);

const model = getModel("anthropic" as any, "claude-sonnet-4-20250514");

const agent = new Agent({
  initialState: {
    systemPrompt: SYSTEM_PROMPT,
    model,
    thinkingLevel: "medium",
    tools: [
      createReadTool(repoDir),
      createGrepTool(repoDir),
      createFindTool(repoDir),
      createBashTool(repoDir),
    ],
  },
  toolExecution: "sequential",
});

let finalText = "";
agent.subscribe((event: any) => {
  if (event.type === "agent_start") {
    console.error("[event] agent_start");
  }
  if (event.type === "turn_start") {
    console.error("[event] turn_start");
  }
  if (event.type === "message_update" && event.assistantMessageEvent?.type === "text_delta") {
    process.stderr.write(event.assistantMessageEvent.delta);
    finalText += event.assistantMessageEvent.delta;
  }
  if (event.type === "tool_execution_start") {
    console.error(`\n[tool] ${event.toolName}(${JSON.stringify(event.args).slice(0, 120)})`);
  }
  if (event.type === "message_end") {
    console.error(`\n[message_end] role=${event.message?.role} content_parts=${event.message?.content?.length} error=${event.message?.errorMessage ?? "none"}`);
    if (event.message?.role === "assistant") {
      finalText = "";
      for (const part of event.message.content ?? []) {
        if (typeof part === "string") finalText += part;
        else if (part.type === "text") finalText += part.text;
      }
    }
  }
  if (event.type === "agent_end") {
    console.error(`[agent_end] messages=${event.messages?.length} error=${agent.state?.error ?? "none"}`);
  }
});

const start = Date.now();
console.error("\n[test] Starting agentic validation...\n");

await agent.prompt(userPrompt);
await agent.waitForIdle();

const elapsed = ((Date.now() - start) / 1000).toFixed(1);
console.error(`\n\n[test] Done in ${elapsed}s`);
console.error("─".repeat(60));
console.log(finalText);
