# Review Agent v2 — Orchestrator Architecture

## Problem with v1

The current flow is **rigid and blind**:

```
Rust → static clustering → hardcoded slices → parallel agents → merge
```

1. **Slicing is mechanical** — entities grouped by directory proximity, not by semantic concern
2. **Agents can't decide what to investigate** — they get a fixed prompt and must find bugs in it
3. **No feedback loop** — if an agent finds something suspicious, it can't recruit help or escalate
4. **Dedup is post-hoc** — same bug found 3x by different slices, wasting tokens
5. **0 tool calls** on many slices — agents hallucinate from snippets instead of reading real files
6. **One-size-fits-all** — same prompt structure for a 3-file Go PR and a 200-file TypeScript PR

## v2 Design Principles

1. **The orchestrator is an agent, not a script** — it reasons about WHAT to investigate
2. **Subagents are specialists with missions** — each has a clear focus area and freedom to explore
3. **Rust provides the map, agents do the exploration** — entities/findings/graph = intelligence brief
4. **Any coding agent can be a subagent** — the protocol is just: input JSON → output JSON
5. **The orchestrator sees everything, subagents see their slice** — prevents duplication at the source

---

## Phase 1: TRIAGE (Orchestrator reads the map)

The orchestrator agent receives the full Rust output and the diff. It has tools to read files.
Its first job is to **understand the PR** — not find bugs yet.

### Input
```json
{
  "pr_title": "...",
  "diff": "... (full diff, truncated to 80k chars)",
  "entity_reviews": [...],      // all entities ranked by risk
  "findings": [...],             // all detector findings
  "graph_edges": [...],          // dependency edges between entities
  "repo_dir": "/path/to/repo"
}
```

### Orchestrator Triage Prompt
```
You are a senior tech lead triaging a PR for code review.

You have:
- A ranked list of changed entities with risk scores and dependency info
- Deterministic detector findings (potential bugs flagged by static analysis)
- The full diff
- Tools to read any file in the repo

Your job is to produce an INVESTIGATION PLAN — what areas need deep review and why.

DO:
- Identify 3-7 investigation areas based on risk, findings, and code structure
- For each area, specify: which entities, which files to read, what to look for
- Prioritize: detector findings first, then high-risk public APIs, then everything else
- Use tools to read files if you need to understand context (e.g., an interface contract)

DO NOT:
- Report bugs yet — that's for the subagents
- Create more than 7 investigation areas
- Include test-only or doc-only changes unless they affect production behavior
```

### Output: Investigation Plan
```json
{
  "investigation_areas": [
    {
      "id": "area-1",
      "title": "Calendar interface contract change",
      "concern": "Interface CalendarService added credentialId param, but 5 implementations not updated",
      "entities": ["CalendarService", "GoogleCalendarService", "Office365CalendarService"],
      "files_to_read": ["packages/core/CalendarService.ts", "packages/app-store/google/..."],
      "findings": ["interface-impl-mismatch"],
      "priority": "critical",
      "strategy": "Read interface, then check each implementation"
    },
    {
      "id": "area-2",
      "title": "Backup code TOCTOU in 2FA disable flow",
      "concern": "Backup codes decrypted, checked, and written back without transaction",
      "entities": ["disableTwoFactor", "BackupCode"],
      "files_to_read": ["apps/web/pages/api/auth/two-factor/disable.ts"],
      "findings": [],
      "priority": "high",
      "strategy": "Read the full function, check for atomic operations"
    }
  ]
}
```

### Why this matters
- The orchestrator uses its intelligence to DECIDE what's worth investigating
- It can read files first to understand context before dispatching
- It naturally handles different PR sizes — small PRs get 2 areas, large PRs get 7
- Detector findings get promoted to investigation areas, not just appended to a prompt

---

## Phase 2: PLAN (Orchestrator designs the subagent pool)

Based on the investigation areas, the orchestrator decides:

1. **How many subagents** — 1 per investigation area (typically 3-7)
2. **What each subagent gets** — a focused brief with pre-read context
3. **Which model/thinking level** — critical areas get high thinking, routine gets low
4. **Tool access scope** — all agents get read/grep/find within repo_dir

### Key Innovation: Pre-Reading

The orchestrator can **read files before dispatching** and include the content in subagent prompts.
This solves the "0 tool calls" problem — agents get real code, not just snippets.

```
Orchestrator reads: CalendarService.ts (interface), GoogleCalendarService.ts (impl)
→ Includes both in subagent prompt
→ Subagent has full context to reason about interface-impl mismatch
```

---

## Phase 3: DISPATCH (Parallel subagent execution)

Each subagent is an independent Agent instance with:
- A focused system prompt for its specialty
- Pre-read file contents from the orchestrator
- The relevant entity snippets and findings
- Full tool access to the repo (read, grep, find, safe bash)

### Subagent Types (dynamically chosen by orchestrator)

| Type | Focus | When Used |
|------|-------|-----------|
| **contract-checker** | Interface changes, signature breaks, type propagation | When findings include contract rules |
| **logic-reviewer** | Wrong variables, inverted conditions, unreachable code | For high-risk modified entities |
| **concurrency-auditor** | TOCTOU, async misuse, race conditions | When code touches shared state |
| **data-flow-tracer** | Null safety, error propagation, return value handling | For entities with many dependents |
| **config-reviewer** | Locale files, CSS values, translation correctness | When non-code files changed |
| **deep-diver** | Generic — orchestrator specifies exact concern | For novel/unusual patterns |

### Subagent Protocol (universal — works with any coding agent)

**Input:**
```json
{
  "mission": "Check if CalendarService interface change broke implementations",
  "context": {
    "entities": [...],           // relevant entities with code
    "findings": [...],           // relevant detector findings
    "pre_read_files": {          // files the orchestrator pre-read
      "CalendarService.ts": "...",
      "GoogleCalendarService.ts": "..."
    },
    "diff_hunks": "..."          // relevant diff sections
  },
  "repo_dir": "/path/to/repo",
  "constraints": {
    "max_issues": 3,
    "max_tool_calls": 15,
    "focus": "Only report confirmed bugs with evidence. No style issues."
  }
}
```

**Output:**
```json
{
  "issues": [
    {
      "issue": "GoogleCalendarService.createEvent() still takes 1 param but CalendarService.createEvent() now requires credentialId",
      "evidence": "interface: createEvent(event, credentialId) vs impl: createEvent(event)",
      "severity": "high",
      "file": "packages/app-store/google/lib/CalendarService.ts",
      "confidence": 0.95,
      "category": "contract-break"
    }
  ],
  "explored_files": ["CalendarService.ts", "GoogleCalendarService.ts", "Office365CalendarService.ts"],
  "tool_calls": 8
}
```

### Subagent System Prompt (example: logic-reviewer)
```
You are an expert code reviewer focused on LOGIC CORRECTNESS.

You have a focused mission from the lead reviewer. Your job:
1. Read the pre-provided code carefully
2. Use tools to read additional files if you need more context (callers, callees, interfaces)
3. Report ONLY confirmed bugs — wrong variables, inverted conditions, unreachable code, reference equality mistakes

For each issue, provide:
- The exact function and line with the bug
- What the code does vs what it should do
- A code snippet as evidence

You have max 15 tool calls. Use them wisely — read callers to confirm impact, check interfaces for contracts.

Respond with JSON: {"issues": [...]}
```

---

## Phase 4: SYNTHESIZE (Orchestrator merges and validates)

After all subagents complete, the orchestrator:

1. **Collects all issues** from all subagents
2. **Deduplicates** using entity ID + issue fingerprint (not just string matching)
3. **Cross-validates** — if two agents found the same bug independently, boost confidence
4. **Ranks** by severity × confidence
5. **Optionally: validates** — for high-value findings, the orchestrator can read the actual code itself to confirm

### Dedup Strategy
```
For each issue:
  1. Extract: file, entity_name, bug_category, key_identifiers
  2. Match against existing issues:
     - Same file + same entity + same category → duplicate (keep higher confidence)
     - Same entity + same identifiers across files → duplicate (cross-slice)
     - Same bug pattern (e.g., "forEach+async") + shared identifiers → duplicate
  3. If two agents independently found it → boost confidence by 0.1
```

### Final Output
```json
{
  "verdicts": [
    {
      "rule_id": "review",
      "entity_name": "CalendarService.ts",
      "verdict": "true_positive",
      "explanation": "[high] GoogleCalendarService.createEvent() missing credentialId parameter | evidence: interface requires (event, credentialId) but impl has (event) | file: packages/app-store/google/lib/CalendarService.ts",
      "confidence": 0.95,
      "found_by": ["area-1-subagent"],
      "category": "contract-break"
    }
  ]
}
```

---

## Why This Is Better

| Aspect | v1 (current) | v2 (proposed) |
|--------|-------------|---------------|
| **Slicing** | Static clustering by directory | Intelligent investigation areas |
| **Context** | Code snippets in prompt | Pre-read full files + tool access |
| **Duplication** | Post-hoc string dedup (lossy) | Orchestrator prevents duplication at dispatch |
| **Adaptability** | Same structure for all PRs | Orchestrator adapts to PR size and type |
| **Tool usage** | Often 0 calls (hallucination) | Guaranteed context from pre-reading |
| **Scalability** | Fixed 10 slices max | 3-7 focused investigations |
| **Agent quality** | Generic "find bugs" prompt | Specialized missions with clear focus |
| **Feedback** | None | Orchestrator can validate findings |

## Implementation Plan

### Phase A: Orchestrator Agent (new file: `review-orchestrator.ts`)
- Takes full Rust output as input
- Has tools: read, grep, find within repo
- Produces investigation plan JSON
- Uses high thinking level

### Phase B: Subagent Protocol (refactor `review-parallel.ts`)
- Accept mission + context JSON (not just a prompt string)
- Each subagent is a fresh Agent instance
- Configurable tool set and thinking level per agent
- Universal output format with confidence scores

### Phase C: Synthesis Layer (new file: `review-synthesize.ts`)
- Smart dedup using entity IDs + category + identifiers
- Cross-validation: independent discovery boosts confidence
- Optional orchestrator validation pass

### Phase D: Entry Point (refactor `review-entry.ts`)
- Wire: Rust output → orchestrator → subagents → synthesize → output
- Support both v1 (direct slicing) and v2 (orchestrator) modes
- Benchmark harness compatibility

---

## Token Budget

For a typical PR (50 entities, 5 findings, 30k diff):

| Phase | Model | Thinking | Est. Tokens | Est. Time |
|-------|-------|----------|-------------|-----------|
| Triage | sonnet | high | ~15k in, ~3k out | 5-10s |
| Pre-read | (tools) | — | ~20k | 2-5s |
| Subagent ×5 | sonnet | medium | ~12k in, ~2k out each | 10-20s parallel |
| Synthesize | sonnet | low | ~8k in, ~2k out | 3-5s |
| **Total** | | | **~120k tokens** | **~25-35s** |

This is comparable to v1's token usage but with dramatically better signal-to-noise ratio.
The orchestrator adds ~18k tokens but saves by: dispatching fewer, more focused agents;
pre-reading files (so agents don't waste tokens exploring); preventing duplicate investigations.

---

## Compatibility

This design works with **any coding agent** because:
1. Subagent input/output is plain JSON — no framework dependency
2. Tools are standard (read, grep, find, bash) — any agent has these
3. The orchestrator-subagent protocol is a simple function call
4. You can swap Claude for GPT for Gemini per-agent without changing the protocol
5. You can even run subagents as external processes if needed (like mi-pi's piSpawn)
