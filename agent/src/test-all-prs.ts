#!/usr/bin/env node
/**
 * Run parallel slice review on ALL 4 remaining test PRs.
 * (grafana already tested separately)
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

const envPath = resolve(import.meta.dirname, "../../.env");
for (const line of readFileSync(envPath, "utf-8").split("\n")) {
  const m = line.match(/^([A-Z_]+)=(.+)$/);
  if (m) process.env[m[1]] = m[2].trim().replace(/^["']|["']$/g, "");
}

const model = getModel("anthropic" as any, "claude-sonnet-4-6");
model.baseUrl = "http://localhost:8317";
process.env.ANTHROPIC_API_KEY = "6cf41538d16fcc1ac937a906dcdc5f92f31894b38978bf97a72a46ed8d5791c7";

// ── PR configs ──
interface PRConfig {
  name: string;
  repoDir: string;
  slices: ReviewSlice[];
  golden: string[];
}

const prs: PRConfig[] = [
  {
    name: "calcom",
    repoDir: "/tmp/martian-eval/worktrees/calcom__cal.com/ba9688a04a83_820d7fa87e0c",
    golden: ["appStore import error handling", "forEach+async in reschedule.ts"],
    slices: [
      {
        id: "calcom-async-patterns",
        title: "forEach+async and dynamic imports",
        prompt: `# Slice: cal.com Async Patterns

## Change summary
This PR converts static imports in \`packages/app-store/index.ts\` to dynamic \`import()\` calls, making \`appStore\` a map of Promises. All callers now need \`await\`.

## Code: packages/app-store/index.ts (AFTER)
\`\`\`typescript
const appStore = {
  applecalendar: import("./applecalendar"),
  caldavcalendar: import("./caldavcalendar"),
  // ... 25+ more entries, all import("./name") ...
  facetime: import("./facetime"),
  sylapsvideo: import("./sylapsvideo"),
};
export default appStore;
\`\`\`

## Code: packages/app-store/vital/lib/reschedule.ts (AFTER — forEach+async)
\`\`\`typescript
bookingRefsFiltered.forEach(async (bookingRef) => {
  if (bookingRef.uid) {
    if (bookingRef.type.endsWith("_calendar")) {
      const calendar = await getCalendar(credentialsMap.get(bookingRef.type));
      return calendar?.deleteEvent(bookingRef.uid, builder.calendarEvent);
    } else if (bookingRef.type.endsWith("_video")) {
      return deleteMeeting(credentialsMap.get(bookingRef.type), bookingRef.uid);
    }
  }
});
\`\`\`

## Code: handleCancelBooking.ts (AFTER — another forEach+async)
\`\`\`typescript
.forEach(async (credential) => {
  const calendar = await getCalendar(credential);
  for (const updBooking of updatedBookings) {
    const bookingRef = updBooking.references.find((ref) => ref.type.includes("_calendar"));
    if (bookingRef) {
      await calendar?.updateEvent(bookingRef.uid, ...);
    }
  }
});
\`\`\`

## Code: handleCancelBooking.ts (AFTER — dynamic import without error handling)
\`\`\`typescript
const paymentApp = await appStore[paymentAppCredential?.app?.dirName as keyof typeof appStore];
if (!(paymentApp && "lib" in paymentApp && "PaymentService" in paymentApp.lib)) {
  console.warn(\`payment App service of type \${paymentApp} is not implemented\`);
  return null;
}
\`\`\`

## Bug hypotheses
1. **forEach+async**: \`.forEach(async ...)\` doesn't await the callbacks — they fire concurrently without error handling. Calendar/video deletions run as fire-and-forget promises. If any fail, the error is silently swallowed.
2. **Dynamic import error handling**: \`appStore\` entries are now \`import()\` Promises. If a module fails to load (typo, missing file, circular dep), the Promise rejects. Callers like \`await appStore[key]\` have no try-catch — an unhandled rejection crashes the request.
3. **appStore key access**: \`paymentAppCredential?.app?.dirName\` could be undefined or a string that doesn't match any key, making \`appStore[key]\` return \`undefined\`, and \`await undefined\` resolves to \`undefined\` — then the \`"lib" in paymentApp\` check throws TypeError on \`in\` operator with non-object.`,
      },
    ],
  },
  {
    name: "discourse",
    repoDir: "/tmp/martian-eval/worktrees/discourse__discourse/3f71fa15c93b_ffbaf8c54269",
    golden: ["duplicate downsize method", "maxSizeKB hardcoded", "80% animated GIF dimensions"],
    slices: [
      {
        id: "discourse-image-processing",
        title: "Image upload: downsize + animated GIF + maxSize",
        prompt: `# Slice: Discourse Image Upload Processing

## Change 1: utilities.js — hardcoded maxSizeKB
\`\`\`javascript
// BEFORE:
var maxSizeKB = Discourse.SiteSettings['max_' + type + '_size_kb'];
// AFTER:
var maxSizeKB = 10 * 1024; // 10MB
\`\`\`
The server-side setting is completely ignored — client now always uses 10MB.

## Change 2: uploads_controller.rb — new downsize loop
\`\`\`ruby
# allow users to upload large images that will be automatically reduced
if tempfile && tempfile.size > 0 && SiteSetting.max_image_size_kb > 0 && FileHelper.is_image?(filename)
  attempt = 5
  while attempt > 0 && tempfile.size > SiteSetting.max_image_size_kb.kilobytes
    OptimizedImage.downsize(tempfile.path, tempfile.path, "80%", allow_animation: SiteSetting.allow_animated_thumbnails)
    attempt -= 1
  end
end
\`\`\`

## Change 3: optimized_image.rb — duplicate method definition + refactored signatures
\`\`\`ruby
# Method 1 (4-arg: from, to, max_width, max_height)
def self.downsize(from, to, max_width, max_height, opts={})
  optimize("downsize", from, to, "#{max_width}x#{max_height}", opts)
end

# Method 2 (3-arg: from, to, dimensions_string)
def self.downsize(from, to, dimensions, opts={})
  optimize("downsize", from, to, dimensions, opts)
end
\`\`\`
Ruby has NO method overloading — the second \`def self.downsize\` **silently replaces** the first.

## Change 4: How animated GIFs are downsized
\`\`\`ruby
def self.optimize(operation, from, to, dimensions, opts={})
  method_name = "#{operation}_instructions"
  method_name += "_animated" if !!opts[:allow_animation] && from =~ /\\.GIF$/i
  instructions = self.send(method_name.to_sym, from, to, dimensions, opts)
  convert_with(instructions, to)
end

def self.downsize_instructions_animated(from, to, dimensions, opts={})
  resize_instructions_animated(from, to, dimensions, opts)
end

def self.resize_instructions_animated(from, to, dimensions, opts={})
  %W{gifsicle #{from} --colors=256 --resize-fit #{dimensions} --optimize=3 --output #{to}}
end
\`\`\`

## Bug hypotheses
1. **Ruby duplicate method**: The first \`downsize(from, to, max_width, max_height)\` is silently overwritten by the second \`downsize(from, to, dimensions)\`. Any callers using the 4-arg form will break.
2. **maxSizeKB hardcoded**: Client-side size check uses \`10 * 1024\` instead of reading from SiteSettings. If admin sets a lower limit, client allows uploads the server will reject.
3. **Animated GIF + "80%"**: The controller calls \`OptimizedImage.downsize(path, path, "80%", allow_animation: ...)\`. For animated GIFs, this routes to \`gifsicle --resize-fit 80%\`. But gifsicle's \`--resize-fit\` expects dimensions like \`WxH\`, not a percentage string like \`"80%"\`. This will either error or produce unexpected results.`,
      },
    ],
  },
  {
    name: "keycloak",
    repoDir: "/tmp/martian-eval/worktrees/keycloak__keycloak/744e031019af_25bf964a844e",
    golden: ["isConditionalPasskeysEnabled null user", "fillContextForm not called"],
    slices: [
      {
        id: "keycloak-passkeys-auth",
        title: "Conditional passkeys auth flow",
        prompt: `# Slice: Keycloak Conditional Passkeys Auth Flow

## authenticate() — entry point (requiresUser() returns false, so getUser() CAN be null)
\`\`\`java
public void authenticate(AuthenticationFlowContext context) {
    if (context.getUser() != null) {
        // user already identified
    } else {
        // initial login page — NO USER YET
    }
    if (isConditionalPasskeysEnabled(context.getUser())) {
        webauthnAuth.fillContextForm(context);
    }
    Response challengeResponse = challenge(context, formData);
    context.challenge(challengeResponse);
}
\`\`\`

## isConditionalPasskeysEnabled()
\`\`\`java
protected boolean isConditionalPasskeysEnabled(UserModel currentUser) {
    return webauthnAuth != null && webauthnAuth.isPasskeysEnabled() &&
            (currentUser == null || currentUser.credentialManager().isConfiguredFor(...));
}
\`\`\`
Note: \`currentUser == null\` short-circuits to TRUE — passkeys always "enabled" when no user.

## challenge(context, error, field) — also calls it
\`\`\`java
protected Response challenge(AuthenticationFlowContext context, String error, String field) {
    if (isConditionalPasskeysEnabled(context.getUser())) {
        webauthnAuth.fillContextForm(context);
    }
    return super.challenge(context, error, field);
}
\`\`\`

## WebAuthnConditionalUIAuthenticator.fillContextForm()
\`\`\`java
public LoginFormsProvider fillContextForm(AuthenticationFlowContext context) {
    context.form().setAttribute(ENABLE_WEBAUTHN_CONDITIONAL_UI, Boolean.TRUE); // set BEFORE super
    return super.fillContextForm(context);
    // super returns null when authenticators.getAuthenticators().isEmpty()
    // BUT ENABLE_WEBAUTHN_CONDITIONAL_UI is already set to TRUE
}
\`\`\`

## passkeys.ftl template
Uses \`\${isUserIdentified}\` and \`\${userVerification}\` without null-safe operators inside \`<#if enableWebAuthnConditionalUI?has_content>\` block.

## Hypotheses
1. **Null user → unconditional passkeys**: \`isConditionalPasskeysEnabled(null)\` returns true, bypassing per-user credential check.
2. **fillContextForm partial state → template crash**: Returns null but ENABLE flag already set. Template crashes on missing isUserIdentified.`,
      },
    ],
  },
  {
    name: "sentry",
    repoDir: "/tmp/martian-eval/worktrees/getsentry__sentry/28e3db2520d4_eb9623b4e787",
    golden: ["OptimizedCursorPaginator negative offset", "BasePaginator negative offset", "get_item_key floor/ceil on datetime"],
    slices: [
      {
        id: "sentry-spans-buffer",
        title: "SpansBuffer: sadd→zadd, sscan→zscan, __reduce__",
        prompt: `# Slice: Sentry SpansBuffer Redis Migration

## Change summary
SpansBuffer migrates from Redis SETs (sadd/sscan) to SORTED SETs (zadd/zscan), adding \`end_timestamp_precise\` as the score. The Span namedtuple gains a new field.

## Span namedtuple change
\`\`\`python
class Span(NamedTuple):
    parent_span_id: str | None
    project_id: int
    payload: bytes
    end_timestamp_precise: float  # ← NEW FIELD
    is_segment_span: bool = False
\`\`\`

## Write path change (process_spans)
\`\`\`python
# BEFORE: p.sadd(set_key, *[span.payload for span in subsegment])
# AFTER:
p.zadd(set_key, {span.payload: span.end_timestamp_precise for span in subsegment})
\`\`\`

## Read path change (_load_segment_data)
\`\`\`python
# BEFORE: p.sscan(key, cursor=cursor, count=self.segment_page_size)
# AFTER:
p.zscan(key, cursor=cursor, count=self.segment_page_size)
# ...
# BEFORE: sizes[key] += sum(len(span) for span in spans)
# AFTER:  sizes[key] += sum(len(span) for span, _ in zscan_values)
# BEFORE: payloads[key].extend(spans)
# AFTER:  payloads[key].extend(span for span, _ in zscan_values)
\`\`\`

## Removed guard
\`\`\`python
# REMOVED from _load_segment_data:
if len(payloads[key]) > self.max_segment_spans:
    metrics.incr("spans.buffer.flush_segments.segment_span_count_exceeded")
    logger.error("Skipping too large segment, span count %s", len(payloads[key]))
    del payloads[key]
    del cursors[key]
    continue
\`\`\`

## SpansBuffer.__init__ and __reduce__
Check if \`__reduce__\` argument ordering matches \`__init__\` parameter ordering — new fields may have shifted positions.

## Hypotheses
1. **__reduce__ arg mismatch**: New \`end_timestamp_precise\` or \`span_buffer_root_timeout_secs\` field may have shifted \`__init__\` params without updating \`__reduce__\`.
2. **Removed span count guard**: The max_segment_spans safety check was removed. Is there still protection against oversized segments?
3. **zscan return format**: zscan returns (member, score) tuples. Verify all destructuring is correct.`,
      },
      {
        id: "sentry-paginator",
        title: "BasePaginator: negative offset + datetime floor/ceil",
        prompt: `# Slice: Sentry Paginator Edge Cases (unchanged code with latent bugs)

This slice examines paginator code that is NOT changed in this PR but is used by SpansBuffer-related endpoints.

## BasePaginator.get_result (src/sentry/api/paginator.py)
\`\`\`python
def get_result(self, limit=100, cursor=None, count_hits=False, known_hits=None, max_hits=None):
    if cursor is None:
        cursor = Cursor(0, 0, 0)
    limit = min(limit, self.max_limit)
    # ...
    offset = cursor.offset
    extra = 1
    if cursor.is_prev and cursor.value:
        extra += 1
    stop = offset + limit + extra
    results = list(queryset[offset:stop])    # ← NO negative offset check
\`\`\`
Note: Other paginators like OffsetPaginator have \`raise BadPaginationError("Pagination offset cannot be negative")\` but BasePaginator does NOT.

## Paginator.get_item_key (used for integer keys)
\`\`\`python
class Paginator(BasePaginator):
    def get_item_key(self, item, for_prev=False):
        value = getattr(item, self.key)
        return int(math.floor(value) if self._is_asc(for_prev) else math.ceil(value))
\`\`\`

## DateTimePaginator.get_item_key (used for datetime keys)
\`\`\`python
class DateTimePaginator(BasePaginator):
    multiplier = 1000
    def get_item_key(self, item, for_prev=False):
        value = getattr(item, self.key)
        value = float(value.strftime("%s.%f")) * self.multiplier
        return int(math.floor(value) if self._is_asc(for_prev) else math.ceil(value))
\`\`\`

## OptimizedCursorPaginator (inherits from BasePaginator)
Also has a get_item_key that uses floor/ceil. Check if it handles datetime values correctly.

## Hypotheses
1. **Negative offset in BasePaginator**: \`cursor.offset\` can be negative (crafted cursor). \`queryset[negative:stop]\` silently slices from the end of the QuerySet, returning wrong results. Other paginators guard against this but BasePaginator does not.
2. **get_item_key floor/ceil on datetime**: If \`order_by='-datetime'\` is used with Paginator (not DateTimePaginator), \`math.floor(datetime_value)\` raises TypeError because floor/ceil don't work on datetime objects.
3. **OptimizedCursorPaginator** also inherits BasePaginator's missing negative offset check.

## File to check
- \`src/sentry/api/paginator.py\``,
      },
    ],
  },
];

// ── Run all PRs ──
const target = process.argv[2]; // optional: "calcom", "discourse", "keycloak", "sentry", or omit for all
const selectedPrs = target ? prs.filter((p) => p.name === target) : prs;

for (const pr of selectedPrs) {
  const tools = [
    createReadTool(pr.repoDir),
    createGrepTool(pr.repoDir),
    createFindTool(pr.repoDir),
    createBashTool(pr.repoDir),
  ];

  console.error(`\n${"═".repeat(60)}`);
  console.error(`PR: ${pr.name} (${pr.slices.length} slices)`);
  console.error(`Golden bugs: ${pr.golden.join(" | ")}`);
  console.error(`${"═".repeat(60)}`);

  const result = await reviewSlicesParallel(pr.slices, model, tools, {
    concurrency: 4,
    thinkingLevel: "low",
  });

  console.error(`\nResult: ${result.merged_issues.length} issues, ${result.total_tool_calls} tools, ${(result.total_elapsed_ms / 1000).toFixed(1)}s`);
  for (const issue of result.merged_issues) {
    console.error(`  [${issue.severity}] ${issue.issue.slice(0, 120)}`);
  }
  console.error("");
}
