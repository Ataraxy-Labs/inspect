#!/usr/bin/env node
/**
 * Test hand-crafted "slice" prompts to validate the hypothesis that
 * better framing (relationship slices + hypothesis generation) helps
 * the model find golden bugs it currently misses.
 *
 * Usage: node --import tsx/esm agent/src/test-slice.ts [keycloak|grafana]
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
import { resolve } from "path";

// Load .env
const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

const target = process.argv[2] || "keycloak";

// --- Slice definitions ---

const KEYCLOAK_SLICE = {
  repoDir: "/tmp/martian-eval/worktrees/keycloak__keycloak/744e031019af_25bf964a844e",
  systemPrompt: `You are an expert code reviewer. You will be given a "change-impact slice" — a small set of related code changes connected by caller→callee or override→interface relationships.

Your job:
1. Read the slice carefully. Understand the call chain and data flow.
2. Generate 2-4 specific bug hypotheses about what could go wrong in this slice.
3. For each hypothesis, use the tools to verify it (read callers, interfaces, related code).
4. Report only confirmed bugs.

Focus on: null safety, argument mismatches, missing side effects, contract violations, state assumptions.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.`,

  userPrompt: `# Change-Impact Slice: Keycloak Authentication Flow

## What changed
This PR adds **conditional passkeys support** to the username/password login form. The key change is a new method \`isConditionalPasskeysEnabled(UserModel currentUser)\` that gates whether WebAuthn passkey UI is set up during authentication.

## Call chain (follow the data flow carefully)

### 1. Entry point: \`UsernamePasswordForm.authenticate(AuthenticationFlowContext context)\`
This is the main authentication flow entry. Note: \`requiresUser()\` returns \`false\`, so \`context.getUser()\` **can be null** on initial page load (before the user has typed anything).

\`\`\`java
@Override
public void authenticate(AuthenticationFlowContext context) {
    // ... form setup ...
    if (context.getUser() != null) {
        // user already identified
        // ...
    } else {
        // initial login page — NO USER YET
        // ...
    }
    // setup webauthn data when passkeys enabled
    if (isConditionalPasskeysEnabled(context.getUser())) {   // ← context.getUser() may be null here
        webauthnAuth.fillContextForm(context);
    }
    Response challengeResponse = challenge(context, formData);
    context.challenge(challengeResponse);
}
\`\`\`

### 2. The gate: \`isConditionalPasskeysEnabled(UserModel currentUser)\`
\`\`\`java
protected boolean isConditionalPasskeysEnabled(UserModel currentUser) {
    return webauthnAuth != null && webauthnAuth.isPasskeysEnabled() &&
            (currentUser == null || currentUser.credentialManager().isConfiguredFor(webauthnAuth.getCredentialType()));
}
\`\`\`

### 3. The side effect: \`WebAuthnConditionalUIAuthenticator.fillContextForm(context)\`
This method sets up the passkey JavaScript/template data needed for the login page.

### 4. Another caller: \`challenge(AuthenticationFlowContext context, String error, String field)\`
\`\`\`java
@Override
protected Response challenge(AuthenticationFlowContext context, String error, String field) {
    if (isConditionalPasskeysEnabled(context.getUser())) {  // ← also calls with context.getUser()
        webauthnAuth.fillContextForm(context);
    }
    return super.challenge(context, error, field);
}
\`\`\`

### 5. PasswordForm.authenticate also calls it:
\`\`\`java
@Override
public void authenticate(AuthenticationFlowContext context) {
    if (alreadyAuthenticatedUsingPasswordlessCredential(context)) {
        context.success();
        return;
    }
    // ...
    if (isConditionalPasskeysEnabled(context.getUser())) {
        webauthnAuth.fillContextForm(context);
    }
    // ...
}
\`\`\`

## Key question
Trace the flow when a user first visits the login page (no user identified yet). What is \`context.getUser()\`? What does \`isConditionalPasskeysEnabled(null)\` return? What does \`fillContextForm\` do or not do? Is the passkeys UI correctly set up for the initial login page?

## Files to investigate
- \`services/src/main/java/org/keycloak/authentication/authenticators/browser/UsernamePasswordForm.java\`
- \`services/src/main/java/org/keycloak/authentication/authenticators/browser/PasswordForm.java\`
- \`services/src/main/java/org/keycloak/authentication/authenticators/browser/WebAuthnConditionalUIAuthenticator.java\`
- \`themes/src/main/resources/theme/base/login/passkeys.ftl\` (the template that renders passkey UI)
`,
};

const GRAFANA_SLICE = {
  repoDir: "/tmp/martian-eval/worktrees/grafana__grafana/50f4e78a3991_bbd8c507cdf5",
  systemPrompt: `You are an expert code reviewer. You will be given a "change-impact slice" — a small set of related code changes connected by caller→callee relationships.

Your job:
1. Read the slice carefully. Understand the call chain and data flow.
2. Generate 3-5 specific bug hypotheses about what could go wrong in this slice.
3. For each hypothesis, use the tools to verify it (read callers, interfaces, related code).
4. Report only confirmed bugs.

Focus on: race conditions, argument type mismatches, error handling changes, API contract violations, concurrency.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.`,

  userPrompt: `# Change-Impact Slice: Grafana Anonymous Device Tracking

## Summary of changes
This PR adds **device limit enforcement** to anonymous device tracking. Key changes:
1. \`ProvideAnonDBStore\` now takes a \`deviceLimit\` parameter
2. New \`updateDevice()\` method and \`ErrDeviceLimitReached\` error
3. \`CreateOrUpdateDevice()\` now checks device count before inserting
4. \`TagDevice()\` now returns errors (previously swallowed them)
5. \`Authenticate()\` in client.go: TagDevice call moved from async goroutine to **synchronous** — and now returns \`ErrDeviceLimitReached\` as an auth failure

## Call chain (trace carefully)

### 1. Authentication entry: \`Anonymous.Authenticate()\` in client.go
**BEFORE** (async, fire-and-forget):
\`\`\`go
go func() {
    newCtx, cancel := context.WithTimeout(context.Background(), timeoutTag)
    defer cancel()
    if err := a.anonDeviceService.TagDevice(newCtx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
        a.log.Warn("Failed to tag anonymous session", "error", err)
    }
}()
\`\`\`

**AFTER** (synchronous, error propagated):
\`\`\`go
if err := a.anonDeviceService.TagDevice(ctx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
    if errors.Is(err, anonstore.ErrDeviceLimitReached) {
        return nil, err  // ← authentication FAILS
    }
    a.log.Warn("Failed to tag anonymous session", "error", err)
}
\`\`\`

### 2. \`TagDevice()\` in impl.go
Now returns error from \`tagDeviceUI\`:
\`\`\`go
err = a.tagDeviceUI(ctx, httpReq, taggedDevice)
if err != nil {
    a.log.Debug("Failed to tag device for UI", "error", err)
    return err  // ← NEW: was previously swallowed
}
\`\`\`

### 3. \`CreateOrUpdateDevice()\` in database.go — the device limit check
\`\`\`go
func (s *AnonDBStore) CreateOrUpdateDevice(ctx context.Context, device *Device) error {
    if s.deviceLimit > 0 {
        count, err := s.CountDevices(ctx, ...)
        if count >= s.deviceLimit {
            return s.updateDevice(ctx, device)
        }
    }
    // ... INSERT with ON CONFLICT DO UPDATE ...
    args = append([]any{query}, args...)
    _, err := dbSession.Exec(args...)
    return err
}
\`\`\`

### 4. \`updateDevice()\` in database.go — new method
\`\`\`go
func (s *AnonDBStore) updateDevice(ctx context.Context, device *Device) error {
    args := []interface{}{device.ClientIP, device.UserAgent, device.UpdatedAt.UTC(), device.DeviceID,
        device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration), device.UpdatedAt.UTC().Add(time.Minute),
    }
    err := s.sqlStore.WithDbSession(ctx, func(dbSession *sqlstore.DBSession) error {
        args = append([]interface{}{query}, args...)
        result, err := dbSession.Exec(args...)  // ← check the Exec signature carefully
        // ...
        if rowsAffected == 0 {
            return ErrDeviceLimitReached  // ← is this the right error for "device not found"?
        }
        return nil
    })
    return err
}
\`\`\`

## Bug hypothesis targets
1. **Race condition**: \`CreateOrUpdateDevice\` does count-then-insert without a transaction lock. Two concurrent requests could both pass the count check.
2. **Exec args**: \`dbSession.Exec(args...)\` where args is \`[]interface{}{query, ...}\` — check what Exec's signature expects. Is the first arg the query string or is it splatted?
3. **Auth blocking**: TagDevice errors now block anonymous authentication. Previously async. Is ErrDeviceLimitReached the right reason to deny access?
4. **Misleading error**: \`updateDevice\` returns \`ErrDeviceLimitReached\` when rowsAffected==0, but the device might just not exist.
5. **Time window**: \`device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration)\` uses the device's own timestamp, not a server-consistent time.

## Files to investigate
- \`pkg/services/anonymous/anonimpl/anonstore/database.go\`
- \`pkg/services/anonymous/anonimpl/impl.go\`
- \`pkg/services/anonymous/anonimpl/client.go\`
- Check \`dbSession.Exec\` signature in grafana's sqlstore package
`,
};

const slices: Record<string, typeof KEYCLOAK_SLICE> = {
  keycloak: KEYCLOAK_SLICE,
  grafana: GRAFANA_SLICE,
};

const slice = slices[target];
if (!slice) {
  console.error(`Unknown target: ${target}. Use 'keycloak' or 'grafana'`);
  process.exit(1);
}

// --- Run the agent ---
const model = getModel("anthropic" as any, "claude-sonnet-4-6");
model.baseUrl = "http://localhost:8317";
process.env.ANTHROPIC_API_KEY = "6cf41538d16fcc1ac937a906dcdc5f92f31894b38978bf97a72a46ed8d5791c7";

const tools = [
  createReadTool(slice.repoDir),
  createGrepTool(slice.repoDir),
  createFindTool(slice.repoDir),
  createBashTool(slice.repoDir),
];

const agent = new Agent({
  initialState: {
    systemPrompt: slice.systemPrompt,
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
console.error(`[test-slice] Running ${target} slice test...`);
console.error(`[test-slice] Repo: ${slice.repoDir}`);
console.error(`[test-slice] System prompt: ${slice.systemPrompt.length} chars`);
console.error(`[test-slice] User prompt: ${slice.userPrompt.length} chars`);
console.error("");

await agent.prompt(slice.userPrompt);
await agent.waitForIdle();

const elapsed = ((Date.now() - start) / 1000).toFixed(1);
console.error(`\n[test-slice] Done in ${elapsed}s, ${toolCalls} tool calls`);
console.error("─".repeat(60));
console.log(finalText);
