#!/usr/bin/env node
/**
 * Agentic code review powered by pi-core.
 *
 * Usage:
 *   echo '{"pr_title":...}' | node --import tsx/esm src/validate.ts
 *
 * Reads ValidateInput JSON from stdin, runs an agentic loop with file/grep/bash/find tools,
 * writes ValidateOutput JSON to stdout.
 */
import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";

import type { ValidateInput, ValidateOutput } from "./types.js";
import { SYSTEM_PROMPT_CLAUDE, SYSTEM_PROMPT_GPT, buildUserPrompt } from "./prompts.js";

/** Read all of stdin as a string. */
async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf-8");
}

/** Extract JSON from the last assistant message (strips markdown fences). */
function extractJson(text: string): string {
  const trimmed = text.trim();
  const fenceMatch = trimmed.match(/```(?:json)?\s*([\s\S]*?)```/);
  if (fenceMatch) return fenceMatch[1].trim();
  const jsonMatch = trimmed.match(/\{[\s\S]*\}/);
  if (jsonMatch) return jsonMatch[0];
  return trimmed;
}

async function main() {
  const raw = await readStdin();
  const input: ValidateInput = JSON.parse(raw);

  const provider = input.provider ?? "anthropic";
  const modelId = input.model ?? "claude-sonnet-4-6";
  const cwd = input.repo_dir;
  const isAnthropic = provider === "anthropic";
  const systemPrompt = isAnthropic ? SYSTEM_PROMPT_CLAUDE : SYSTEM_PROMPT_GPT;

  const tools = [
    createReadTool(cwd),
    createGrepTool(cwd),
    createFindTool(cwd),
    createBashTool(cwd, {
      spawnHook: ({ command }) => {
        const dangerous = ["rm ", "mv ", "cp ", "chmod", "chown", "kill", "mkfs", ">", ">>", "sudo"];
        if (dangerous.some((d) => command.includes(d))) {
          throw new Error(`Blocked dangerous command: ${command}`);
        }
      },
    }),
  ];

  const userPrompt = buildUserPrompt(
    input.pr_title,
    input.diff,
    input.triage_section,
    input.findings,
    input.entity_reviews,
  );

  const model = getModel(provider as any, modelId);

  const agent = new Agent({
    initialState: {
      systemPrompt,
      model,
      thinkingLevel: isAnthropic ? "low" : "low",
      tools,
    },
    toolExecution: "parallel",
    beforeToolCall: async ({ toolCall, args }) => {
      process.stderr.write(`[tool] ${toolCall.name}(${JSON.stringify(args).slice(0, 200)})\n`);
      return undefined;
    },
  });

  let finalText = "";
  agent.subscribe((event) => {
    if (event.type === "message_update" && event.assistantMessageEvent?.type === "text_delta") {
      finalText += event.assistantMessageEvent.delta;
    }
    if (event.type === "message_end") {
      if (event.message.role === "assistant") {
        finalText = "";
        for (const part of event.message.content ?? []) {
          if (typeof part === "string") finalText += part;
          else if (part.type === "text") finalText += part.text;
        }
      }
    }
  });

  await agent.prompt(userPrompt);
  await agent.waitForIdle();

  const jsonStr = extractJson(finalText);

  let output: ValidateOutput;
  try {
    const parsed = JSON.parse(jsonStr);
    const issues = parsed.issues ?? [];
    output = {
      verdicts: issues.map((issue: any) => ({
        rule_id: issue.rule_id ?? "review",
        entity_name: issue.file ?? "unknown",
        verdict: "true_positive" as const,
        explanation: `${issue.issue} | evidence: ${issue.evidence ?? "none"} | severity: ${issue.severity ?? "medium"}`,
      })),
    };
  } catch (e) {
    process.stderr.write(`[error] Failed to parse agent response: ${e}\n`);
    process.stderr.write(`[raw] ${finalText.slice(0, 500)}\n`);
    output = { verdicts: [] };
  }

  process.stdout.write(JSON.stringify(output) + "\n");
}

main().catch((e) => {
  process.stderr.write(`[fatal] ${e}\n`);
  process.exit(1);
});
