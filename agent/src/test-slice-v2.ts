#!/usr/bin/env node
/**
 * Test v2: tighter slice prompts optimized for speed + coverage.
 * Changes from v1:
 * - System prompt tells agent to reason FIRST, tools SECOND
 * - Hypotheses are more explicit about Exec type signature
 * - Added behavioral regression hypothesis for TagDevice
 */
import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";
import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";

const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

const target = process.argv[2] || "grafana";

const FAST_SYSTEM = `You are an expert code reviewer analyzing a change-impact slice — a focused set of related code changes connected by caller→callee relationships.

WORKFLOW:
1. FIRST: Read the slice carefully. Reason about each hypothesis using the code already provided.
2. ONLY use tools if you need ONE specific piece of evidence to confirm/refute (e.g., a function signature, an interface contract). Do NOT explore broadly.
3. For each confirmed bug, provide the exact code path and evidence.

You already have the critical code in the prompt. Most bugs can be confirmed by reasoning alone. Use tools sparingly — max 10 calls.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.`;

const GRAFANA_V2 = {
  repoDir: "/tmp/martian-eval/worktrees/grafana__grafana/50f4e78a3991_bbd8c507cdf5",
  systemPrompt: FAST_SYSTEM,
  userPrompt: `# Change-Impact Slice: Grafana Anonymous Device Limit Enforcement

## What changed
This PR adds device limit enforcement. The critical behavioral changes:
1. \`TagDevice()\` now **returns errors** instead of swallowing them
2. \`Authenticate()\` calls TagDevice **synchronously** (was async goroutine)
3. \`ErrDeviceLimitReached\` from TagDevice now **blocks authentication entirely**
4. New \`updateDevice()\` method with time-windowed WHERE clause
5. \`CreateOrUpdateDevice()\` does count-then-insert (no transaction)

## Complete code for analysis

### client.go — Authenticate() BEFORE:
\`\`\`go
go func() {
    defer func() {
        if err := recover(); err != nil {
            a.log.Warn("Tag anon session panic", "err", err)
        }
    }()
    newCtx, cancel := context.WithTimeout(context.Background(), timeoutTag)
    defer cancel()
    if err := a.anonDeviceService.TagDevice(newCtx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
        a.log.Warn("Failed to tag anonymous session", "error", err)
    }
}()
\`\`\`

### client.go — Authenticate() AFTER:
\`\`\`go
if err := a.anonDeviceService.TagDevice(ctx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
    if errors.Is(err, anonstore.ErrDeviceLimitReached) {
        return nil, err  // ← returns nil identity = authentication FAILURE
    }
    a.log.Warn("Failed to tag anonymous session", "error", err)
}
\`\`\`

### impl.go — TagDevice() change (line 144):
\`\`\`go
// BEFORE: error was swallowed
err = a.tagDeviceUI(ctx, httpReq, taggedDevice)
if err != nil {
    a.log.Debug("Failed to tag device for UI", "error", err)
}
return nil

// AFTER: error is propagated
err = a.tagDeviceUI(ctx, httpReq, taggedDevice)
if err != nil {
    a.log.Debug("Failed to tag device for UI", "error", err)
    return err  // ← NEW: returns error to Authenticate()
}
return nil
\`\`\`

### database.go — CreateOrUpdateDevice():
\`\`\`go
func (s *AnonDBStore) CreateOrUpdateDevice(ctx context.Context, device *Device) error {
    if s.deviceLimit > 0 {
        count, err := s.CountDevices(ctx, time.Now().UTC().Add(-anonymousDeviceExpiration), time.Now().UTC().Add(time.Minute))
        if err != nil { return err }
        if count >= s.deviceLimit {
            return s.updateDevice(ctx, device)
        }
    }
    // ... build dialect-specific INSERT query ...
    args := []any{device.DeviceID, device.ClientIP, device.UserAgent, device.CreatedAt.UTC(), device.UpdatedAt.UTC()}
    err := s.sqlStore.WithDbSession(ctx, func(dbSession *sqlstore.DBSession) error {
        args = append([]any{query}, args...)
        _, err := dbSession.Exec(args...)  // ← NOTE: args is []any{query, val1, val2, ...} splatted
        return err
    })
    return err
}
\`\`\`

### database.go — updateDevice() (NEW):
\`\`\`go
func (s *AnonDBStore) updateDevice(ctx context.Context, device *Device) error {
    const query = \`UPDATE anon_device SET client_ip = ?, user_agent = ?, updated_at = ?
WHERE device_id = ? AND updated_at BETWEEN ? AND ?\`

    args := []interface{}{device.ClientIP, device.UserAgent, device.UpdatedAt.UTC(), device.DeviceID,
        device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration), device.UpdatedAt.UTC().Add(time.Minute),
    }
    err := s.sqlStore.WithDbSession(ctx, func(dbSession *sqlstore.DBSession) error {
        args = append([]interface{}{query}, args...)
        result, err := dbSession.Exec(args...)  // ← NOTE: same pattern — args is []interface{}{query, ...} splatted
        if err != nil { return err }
        rowsAffected, err := result.RowsAffected()
        if err != nil { return err }
        if rowsAffected == 0 {
            return ErrDeviceLimitReached  // ← when device simply doesn't exist or is outside time window
        }
        return nil
    })
    return err
}
\`\`\`

## Bug hypotheses to verify

1. **Race condition (TOCTOU)**: \`CreateOrUpdateDevice\` does \`CountDevices\` then INSERT in separate DB sessions. No transaction wrapping. Two concurrent requests can both pass the count check.

2. **Behavioral regression — auth now blocks on device limit**: Previously TagDevice was fire-and-forget in a goroutine. Now it's synchronous and \`ErrDeviceLimitReached\` causes \`Authenticate()\` to return \`nil, err\` — completely denying anonymous access. Is it correct to deny ALL anonymous access just because a device tracking limit is hit?

3. **Exec args type mismatch**: \`dbSession.Exec(args...)\` where \`args\` is \`[]interface{}{query, val1, val2, ...}\`. Check what \`Exec\`'s actual Go signature is. Does it expect \`Exec(sqlOrArgs ...interface{})\` or \`Exec(sql string, args ...interface{})\`? If the latter, splatting \`[]interface{}{query, ...}\` would pass the query string as an \`interface{}\`, not as a \`string\`.

4. **Misleading error**: \`updateDevice\` returns \`ErrDeviceLimitReached\` when \`rowsAffected == 0\`, but this could mean the device doesn't exist or is outside the 30-day time window — not that the limit is reached.

5. **Time window gap**: Devices aged 30-61 days exist (cleanup at 61 days) but \`updateDevice\`'s WHERE excludes them. Returning users get \`ErrDeviceLimitReached\` → auth fails.

## Key file to check
- \`pkg/util/xorm/session_raw.go\` — find \`func (session *Session) Exec(sqlOrArgs ...interface{})\` signature
`,
};

const KEYCLOAK_V2 = {
  repoDir: "/tmp/martian-eval/worktrees/keycloak__keycloak/744e031019af_25bf964a844e",
  systemPrompt: FAST_SYSTEM,
  userPrompt: `# Change-Impact Slice: Keycloak Conditional Passkeys Auth Flow

## What changed
New method \`isConditionalPasskeysEnabled(UserModel currentUser)\` gates passkey UI setup. Called from \`authenticate()\` and \`challenge()\`, both passing \`context.getUser()\`.

## Complete code (UsernamePasswordForm.java)

### authenticate():
\`\`\`java
@Override
public void authenticate(AuthenticationFlowContext context) {
    // ... form setup ...
    if (context.getUser() != null) {
        // user already identified — set form attributes
    } else {
        // initial login page — NO USER YET, context.getUser() == null
    }
    // This runs AFTER the if/else above:
    if (isConditionalPasskeysEnabled(context.getUser())) {   // ← getUser() can be null
        webauthnAuth.fillContextForm(context);
    }
    Response challengeResponse = challenge(context, formData);
    context.challenge(challengeResponse);
}
\`\`\`

### isConditionalPasskeysEnabled():
\`\`\`java
protected boolean isConditionalPasskeysEnabled(UserModel currentUser) {
    return webauthnAuth != null && webauthnAuth.isPasskeysEnabled() &&
            (currentUser == null || currentUser.credentialManager().isConfiguredFor(webauthnAuth.getCredentialType()));
}
\`\`\`
Note: \`currentUser == null\` makes the whole expression \`true\` (short-circuit), meaning passkeys UI is ALWAYS shown when no user is identified.

### challenge(context, error, field):
\`\`\`java
@Override
protected Response challenge(AuthenticationFlowContext context, String error, String field) {
    if (isConditionalPasskeysEnabled(context.getUser())) {
        webauthnAuth.fillContextForm(context);
    }
    return super.challenge(context, error, field);
}
\`\`\`

### WebAuthnConditionalUIAuthenticator.fillContextForm():
\`\`\`java
public LoginFormsProvider fillContextForm(AuthenticationFlowContext context) {
    context.form().setAttribute(WebAuthnConstants.ENABLE_WEBAUTHN_CONDITIONAL_UI, Boolean.TRUE);
    return super.fillContextForm(context);
    // super.fillContextForm returns null when authenticators.getAuthenticators().isEmpty()
    // But ENABLE_WEBAUTHN_CONDITIONAL_UI is already set to TRUE
}
\`\`\`

### passkeys.ftl template (uses the attributes):
The template uses \`\${isUserIdentified}\` without null-safe check inside an \`enableWebAuthnConditionalUI\` block.

## Bug hypotheses

1. **Null user → unconditional passkeys**: When \`context.getUser() == null\` (initial login page), \`isConditionalPasskeysEnabled(null)\` returns \`true\` because of \`currentUser == null\` short-circuit. This means \`fillContextForm\` is called for ALL users on initial page load, even those without any passkeys. The method signature \`isConditionalPasskeysEnabled(UserModel user)\` implies it should check the user's credentials, but with null it skips the check entirely.

2. **fillContextForm partial state → template crash**: When \`fillContextForm\` returns null (user has no WebAuthn credentials), the \`ENABLE_WEBAUTHN_CONDITIONAL_UI\` flag is already set to TRUE, but \`IS_USER_IDENTIFIED\` and \`USER_VERIFICATION\` are never set. The passkeys.ftl template then crashes with FreeMarker InvalidReferenceException on \`\${isUserIdentified}\`.

## Files if you need to verify
- \`services/src/main/java/org/keycloak/authentication/authenticators/browser/WebAuthnConditionalUIAuthenticator.java\`
- \`services/src/main/java/org/keycloak/authentication/authenticators/browser/WebAuthnAuthenticator.java\` (parent fillContextForm)
- \`themes/src/main/resources/theme/base/login/passkeys.ftl\`
`,
};

const slices: Record<string, typeof GRAFANA_V2> = {
  keycloak: KEYCLOAK_V2,
  grafana: GRAFANA_V2,
};

const slice = slices[target];
if (!slice) {
  console.error(`Unknown target: ${target}. Use 'keycloak' or 'grafana'`);
  process.exit(1);
}

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
console.error(`[test-slice-v2] Running ${target} slice test (v2 — fast mode)...`);
console.error(`[test-slice-v2] System prompt: ${slice.systemPrompt.length} chars`);
console.error(`[test-slice-v2] User prompt: ${slice.userPrompt.length} chars`);
console.error("");

await agent.prompt(slice.userPrompt);
await agent.waitForIdle();

const elapsed = ((Date.now() - start) / 1000).toFixed(1);
console.error(`\n[test-slice-v2] Done in ${elapsed}s, ${toolCalls} tool calls`);
console.error("─".repeat(60));
console.log(finalText);

// Also dump for inspection
writeFileSync(resolve(import.meta.dirname, `../../slice-v2-${target}.md`), finalText);
