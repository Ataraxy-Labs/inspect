#!/usr/bin/env node
/**
 * Test the updated prompt against a real benchmark PR.
 * Usage: node --import tsx/esm src/test-lean.ts [worktree_path] [base..head]
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

import { SYSTEM_PROMPT_CLAUDE, buildUserPrompt } from "./prompts.js";

// Load .env
const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

// --- Config ---
const repoDir = process.argv[2] || "/tmp/martian-eval/worktrees/keycloak__keycloak/10aca5552314_79d11c4890cc";
const range = process.argv[3] || "10aca5552314..79d11c4890cc";
const inspectBin = resolve(import.meta.dirname, "../../target/release/inspect");

// --- Run inspect ---
console.error("[test] Running inspect on", repoDir);
const inspectRaw = execSync(
  `${inspectBin} diff --format json -C "${repoDir}" "${range}"`,
  { encoding: "utf-8", maxBuffer: 10 * 1024 * 1024 },
);
const inspectData = JSON.parse(inspectRaw);
const findings = inspectData.findings ?? [];
const entityReviews = inspectData.entity_reviews ?? [];

console.error(`[test] ${entityReviews.length} entities, ${findings.length} findings`);

// --- Build prompt using the production buildUserPrompt ---
const userPrompt = buildUserPrompt(
  "Code review",
  "", // diff not used
  "", // triage section not used
  findings,
  entityReviews,
);

console.error(`[test] Prompt size: ${userPrompt.length.toLocaleString()} chars (~${Math.round(userPrompt.length / 4).toLocaleString()} tokens)`);
console.error(`[test] System prompt: ${SYSTEM_PROMPT_CLAUDE.length} chars`);

// Dump full prompt to file for inspection
import { writeFileSync } from "fs";
const dumpPath = resolve(import.meta.dirname, "../../prompt-dump.md");
writeFileSync(dumpPath, `# System Prompt\n\n${SYSTEM_PROMPT_CLAUDE}\n\n---\n\n# User Prompt\n\n${userPrompt}`);
console.error(`[test] Full prompt dumped to: ${dumpPath}`);

// --- Run the agent ---
const model = getModel("anthropic" as any, "claude-sonnet-4-6");
// Use local proxy
model.baseUrl = "http://localhost:8317";
process.env.ANTHROPIC_API_KEY = "6cf41538d16fcc1ac937a906dcdc5f92f31894b38978bf97a72a46ed8d5791c7";
const tools = [
  createReadTool(repoDir),
  createGrepTool(repoDir),
  createFindTool(repoDir),
  createBashTool(repoDir),
];

const agent = new Agent({
  initialState: {
    systemPrompt: SYSTEM_PROMPT_CLAUDE,
    model,
    thinkingLevel: "high",
    tools,
  },
  toolExecution: "parallel",
  beforeToolCall: async ({ toolCall, args }) => {
    process.stderr.write(`  [tool] ${toolCall.name}(${JSON.stringify(args).slice(0, 150)})\n`);
    return undefined;
  },
});

let finalText = "";
let toolCalls = 0;
agent.subscribe((event: any) => {
  if (event.type === "tool_execution_start") {
    toolCalls++;
  }
  if (event.type === "message_end" && event.message?.role === "assistant") {
    finalText = "";
    for (const part of event.message.content ?? []) {
      if (typeof part === "string") finalText += part;
      else if (part.type === "text") finalText += part.text;
    }
  }
});

const start = Date.now();
console.error("[test] Running agent...\n");

await agent.prompt(userPrompt);
await agent.waitForIdle();

const elapsed = ((Date.now() - start) / 1000).toFixed(1);
console.error(`\n[test] Done in ${elapsed}s, ${toolCalls} tool calls`);
console.error("─".repeat(60));
console.log(finalText);
