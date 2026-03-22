#!/usr/bin/env node
/**
 * Test parallel slice review on grafana PR#79265.
 *
 * Runs multiple slice agents concurrently, each focused on a
 * different relationship in the anonymous device code.
 */
import { getModel } from "@mariozechner/pi-ai";
import {
  createReadTool,
  createGrepTool,
  createBashTool,
  createFindTool,
} from "@mariozechner/pi-coding-agent";
import { readFileSync } from "fs";
import { resolve } from "path";

import { reviewSlicesParallel, type ReviewSlice } from "./review-parallel.js";

// Load .env
const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

const repoDir =
  "/tmp/martian-eval/worktrees/grafana__grafana/50f4e78a3991_bbd8c507cdf5";

const model = getModel("anthropic" as any, "claude-sonnet-4-6");
model.baseUrl = "http://localhost:8317";
process.env.ANTHROPIC_API_KEY =
  "6cf41538d16fcc1ac937a906dcdc5f92f31894b38978bf97a72a46ed8d5791c7";

const tools = [
  createReadTool(repoDir),
  createGrepTool(repoDir),
  createFindTool(repoDir),
  createBashTool(repoDir),
];

// ---------------------------------------------------------------------------
// Slices — each focused on a specific relationship/hypothesis cluster
// ---------------------------------------------------------------------------

const slices: ReviewSlice[] = [
  {
    id: "device-limit-atomicity",
    title: "CreateOrUpdateDevice: race condition + Exec args",
    prompt: `# Slice: Device Limit Enforcement Atomicity

## Changed code: CreateOrUpdateDevice() in database.go

\`\`\`go
func (s *AnonDBStore) CreateOrUpdateDevice(ctx context.Context, device *Device) error {
    if s.deviceLimit > 0 {
        count, err := s.CountDevices(ctx, time.Now().UTC().Add(-anonymousDeviceExpiration), time.Now().UTC().Add(time.Minute))
        if err != nil { return err }
        if count >= s.deviceLimit {
            return s.updateDevice(ctx, device)
        }
    }
    args := []any{device.DeviceID, device.ClientIP, device.UserAgent, device.CreatedAt.UTC(), device.UpdatedAt.UTC()}
    // ... dialect-specific INSERT query built into 'query' variable ...
    err := s.sqlStore.WithDbSession(ctx, func(dbSession *sqlstore.DBSession) error {
        args = append([]any{query}, args...)
        _, err := dbSession.Exec(args...)
        return err
    })
    return err
}
\`\`\`

## Changed code: updateDevice() (NEW) in database.go
\`\`\`go
func (s *AnonDBStore) updateDevice(ctx context.Context, device *Device) error {
    const query = \`UPDATE anon_device SET client_ip = ?, user_agent = ?, updated_at = ?
WHERE device_id = ? AND updated_at BETWEEN ? AND ?\`
    args := []interface{}{device.ClientIP, device.UserAgent, device.UpdatedAt.UTC(), device.DeviceID,
        device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration), device.UpdatedAt.UTC().Add(time.Minute)}
    err := s.sqlStore.WithDbSession(ctx, func(dbSession *sqlstore.DBSession) error {
        args = append([]interface{}{query}, args...)
        result, err := dbSession.Exec(args...)
        if err != nil { return err }
        rowsAffected, err := result.RowsAffected()
        if err != nil { return err }
        if rowsAffected == 0 { return ErrDeviceLimitReached }
        return nil
    })
    return err
}
\`\`\`

## Hypotheses
1. **TOCTOU race**: CountDevices + INSERT run in separate DB sessions. No transaction. Concurrent requests both pass the count check.
2. **Exec args type**: \`dbSession.Exec(args...)\` where args = \`[]interface{}{query, ...}\`. Check if Exec expects \`(string, ...interface{})\` or \`(...interface{})\`. Read \`pkg/util/xorm/session_raw.go\` to verify.

## Files
- \`pkg/services/anonymous/anonimpl/anonstore/database.go\`
- \`pkg/util/xorm/session_raw.go\` (Exec signature)`,
  },
  {
    id: "auth-blocking-regression",
    title: "TagDevice → Authenticate: behavioral regression",
    prompt: `# Slice: Authentication Blocking Regression

## BEFORE (client.go — Authenticate):
\`\`\`go
// TagDevice was fire-and-forget in a goroutine
go func() {
    defer func() {
        if err := recover(); err != nil { a.log.Warn("panic", "err", err) }
    }()
    newCtx, cancel := context.WithTimeout(context.Background(), timeoutTag)
    defer cancel()
    if err := a.anonDeviceService.TagDevice(newCtx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
        a.log.Warn("Failed to tag anonymous session", "error", err)
    }
}()
\`\`\`

## AFTER (client.go — Authenticate):
\`\`\`go
// TagDevice is now SYNCHRONOUS and ErrDeviceLimitReached blocks auth
if err := a.anonDeviceService.TagDevice(ctx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
    if errors.Is(err, anonstore.ErrDeviceLimitReached) {
        return nil, err  // ← returns nil identity = auth FAILURE
    }
    a.log.Warn("Failed to tag anonymous session", "error", err)
}
\`\`\`

## CHANGE in TagDevice (impl.go line 144):
\`\`\`go
// BEFORE: error swallowed
if err != nil { a.log.Debug(...) }
return nil

// AFTER: error propagated
if err != nil { a.log.Debug(...); return err }
\`\`\`

## Hypothesis
**Behavioral regression**: Previously, device tagging failures never affected authentication. Now, ANY TagDevice error (including ErrDeviceLimitReached from updateDevice) causes Authenticate() to return nil, completely denying anonymous access. Is it correct that hitting a device tracking limit should deny the user's ability to view the Grafana dashboard anonymously?

## Files
- \`pkg/services/anonymous/anonimpl/client.go\`
- \`pkg/services/anonymous/anonimpl/impl.go\``,
  },
  {
    id: "update-device-errors",
    title: "updateDevice: misleading error + time window gap",
    prompt: `# Slice: updateDevice Error Semantics

## Code: updateDevice() in database.go (NEW)
\`\`\`go
func (s *AnonDBStore) updateDevice(ctx context.Context, device *Device) error {
    const query = \`UPDATE anon_device SET client_ip = ?, user_agent = ?, updated_at = ?
WHERE device_id = ? AND updated_at BETWEEN ? AND ?\`
    args := []interface{}{device.ClientIP, device.UserAgent, device.UpdatedAt.UTC(), device.DeviceID,
        device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration), device.UpdatedAt.UTC().Add(time.Minute)}
    // ...
    if rowsAffected == 0 { return ErrDeviceLimitReached }
    return nil
}
\`\`\`

## Context: cleanup runs at 61 days
\`\`\`go
const keepFor = time.Hour * 24 * 61  // in impl.go
const anonymousDeviceExpiration = 30 * 24 * time.Hour  // in database.go
\`\`\`

## Call chain: updateDevice → TagDevice → Authenticate
ErrDeviceLimitReached propagates up and blocks authentication (see client.go).

## Hypotheses
1. **Misleading error**: \`rowsAffected == 0\` returns ErrDeviceLimitReached, but the device might simply not exist in the DB, or it might exist but with \`updated_at\` outside the 30-day window. The error name implies the limit is the problem, but the actual cause could be different.

2. **Time window gap**: Devices aged 30-61 days still exist in the DB (cleanup at 61 days via \`keepFor\`), but the UPDATE's WHERE clause \`updated_at BETWEEN (now-30d) AND (now+1m)\` excludes them. A returning user whose device was last seen 35 days ago will get ErrDeviceLimitReached → auth blocked — even though their device legitimately exists.

## Files
- \`pkg/services/anonymous/anonimpl/anonstore/database.go\`
- \`pkg/services/anonymous/anonimpl/impl.go\` (keepFor constant)`,
  },
];

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

const start = Date.now();
console.error(`[test] Running ${slices.length} parallel slices on grafana PR#79265\n`);

const result = await reviewSlicesParallel(slices, model, tools, {
  concurrency: 3,
  thinkingLevel: "low",
});

const totalElapsed = ((Date.now() - start) / 1000).toFixed(1);

console.error(`\n${"─".repeat(60)}`);
console.error(`Total: ${totalElapsed}s, ${result.total_tool_calls} tool calls, ${result.merged_issues.length} issues`);
console.error(`${"─".repeat(60)}\n`);

// Output merged issues as JSON
const output = {
  issues: result.merged_issues.map(({ slice_id, ...rest }) => rest),
};
console.log(JSON.stringify(output, null, 2));

// Also print per-slice summary
console.error("\nPer-slice summary:");
for (const sr of result.slice_results) {
  console.error(`  ${sr.slice_id}: ${sr.issues.length} issues, ${sr.tool_calls} tools, ${(sr.elapsed_ms / 1000).toFixed(1)}s${sr.error ? ` ERROR: ${sr.error}` : ""}`);
}
