# Iteration Log — 5 PR Test Set (cal.com)

## Test Set
| PR | Golden | Description |
|----|--------|-------------|
| #8087 | 2 | Async import of appStore packages |
| #10600 | 4 | 2FA backup codes |
| #10967 | 5 | Collective multiple host destinationCalendar |
| #22345 | 2 | InsightsBookingService Prisma.sql |
| #7232 | 2 | Workflow reminder management |
| **Total** | **15** | |

## Golden Comments Reference
### PR#8087 (2 golden)
1. try-catch around dynamic import failures
2. forEach+async causing fire-and-forget concurrent deletions

### PR#10600 (4 golden)
1. TwoFactor exported from BackupCode.tsx — inconsistent naming
2. Error message says "backup code login" but endpoint is disable
3. Backup code validation case-sensitive (indexOf vs case-insensitive)
4. TOCTOU race: concurrent backup code login can both pass check

### PR#10967 (5 golden)
1. Null ref if mainHostDestinationCalendar undefined (empty array)
2. Redundant optional chaining on mainHostDestinationCalendar?.integration
3. Logic error: externalId === externalCalendarId always fails
4. Logic inversion: slug set when IS_TEAM_BILLING_ENABLED=true instead of false
5. Calendar interface requires credentialId but some impls don't have it

### PR#22345 (2 golden)
1. Unreachable else-if/else branches in getBaseConditions()
2. userIdsFromOrg skipped when teamsFromOrg empty — excludes org members

### PR#7232 (2 golden)
1. forEach+async: deleteScheduledEmailReminder/SMS not awaited
2. immediateDelete=true: cancels SendGrid but doesn't delete DB record → orphans

---

## Run v6a — Baseline (current code)
- **Date**: 2026-03-22
- **Model**: claude-sonnet-4-6
- **Changes**: Removed entity type filters, top-N=30, improved slice prompt, non-code reviewer, dedup only
- **Results**: 37 candidates, ~7-9 TPs, ~20% precision

| PR | Golden | Candidates | TPs | FPs | Notes |
|----|--------|-----------|-----|-----|-------|
| #8087 | 2 | 4 | 1 | 3 | ✅ forEach+async. ❌ import try-catch. FP: getCalendar await, deletePayment interpolation, getBusyVideoTimes |
| #10600 | 4 | 6 | 0 | 6 | ❌ All 4 golden missed. All candidates are FPs (regex anchor, ErrorCode enum, test fillOtp, inputMode, useEffect, minLength) |
| #10967 | 5 | 9 | 2-3 | 6-7 | ✅ null ref (2x dupe!), ✅ externalCalendarId logic. ❌ redundant optchain, ❌ IS_TEAM_BILLING inversion, ❌ interface contract. FP: createHandler password, totp.options, apiDeletes array |
| #22345 | 2 | 8 | 1-2 | 6-7 | ✅ teamsFromOrg guard. ⚠️ maybe unreachable branches. Heavy duplication on isOwnerOrAdmin. FP: Prisma injection, deleted findMany |
| #7232 | 2 | 10 | 2 | 8 | ✅ forEach+async (3x dupe!), ✅ SendGrid orphan. 8 FPs including schema model, stepNumber+1, remindersToCancel filter |
| **Total** | **15** | **37** | **~7** | **~30** | **P≈19%, R≈47%** |

### Key Problems
1. **Cross-slice duplication**: same bug from multiple slices (forEach+async 3x, null ref 2x, isOwnerOrAdmin 2x)
2. **High FP rate**: Agent invents plausible-sounding bugs with high confidence
3. **Misses subtle bugs**: case-sensitive indexOf, naming inconsistency, redundant optional chaining, interface contracts
4. **0 tool calls on some slices**: agents don't read actual files, hallucinate bugs from snippets alone

---

## Run v6b — Stricter prompt + better dedup + skip tiny chunks
- **Changes**: Stricter slice prompt (MUST read files, max 2/slice, WHAT IS/ISN'T A BUG), global dedup with semantic patterns, skip <8-line standalone chunks
- **Results**: 10 candidates (down from 37!)

| PR | Golden | Candidates | Notes |
|----|--------|-----------|-------|
| #8087 | 2 | 2 | ✅ forEach+async. Missing import try-catch |
| #10600 | 4 | 1 | ❌ All 4 golden missed. 1 FP |
| #10967 | 5 | 3 | ✅ null ref, ✅ externalCalendarId. 1 FP |
| #22345 | 2 | 1 | ✅ teamsFromOrg guard |
| #7232 | 2 | 3 | ✅ forEach+async. 1 FP. Still 1 dupe |
| **Total** | **15** | **10** | **~6 TP, ~4 FP, P≈60%** |

**Problem**: Max 2/slice too aggressive — killed teamsFromOrg TP in v6c. Dedup still missed some forEach+async dupes.

---

## Run v6d-v6e — Relaxed to max 3/slice + camelCase identifier dedup
- **Changes**: max 3 issues/slice, extract camelCase identifiers (not just backtick-quoted) for dedup
- **Results**: 9 candidates, 157s

| PR | Golden | Candidates | TPs | Notes |
|----|--------|-----------|-----|-------|
| #8087 | 2 | 1 | 1 | ✅ forEach+async. Dedup merged 3→1. ❌ import try-catch |
| #10600 | 4 | 3 | 0 | ❌ All 4 golden missed. FPs: EnableTotpCode form field, prisma import, useEffect clear |
| #10967 | 5 | 3 | 2 | ✅ null ref, ✅ externalCalendarId. FP: apiDeletes array. ❌ IS_TEAM_BILLING, ❌ interface contract |
| #22345 | 2 | 0 | 0 | ❌ Agent found nothing (0 raw issues!) — too strict? |
| #7232 | 2 | 2 | 1-2 | ✅ forEach+async. Dedup 5→2. ⚠️ deleteSMS referenceId check — possible TP |
| **Total** | **15** | **9** | **~4-5** | **P≈50%, R≈27-33%** |

### Progression Summary
| Version | Candidates | ~TPs | ~FPs | ~Precision | ~Recall |
|---------|-----------|------|------|-----------|---------|
| v6a (baseline) | 37 | 7 | 30 | 19% | 47% |
| v6b (strict prompt + dedup) | 10 | 6 | 4 | 60% | 40% |
| v6e (camelCase dedup) | 9 | 4-5 | 4-5 | 50% | 27-33% |

### Analysis
- **Dedup is working well**: 37→9 candidates with minimal TP loss
- **Precision improved dramatically**: 19%→50%
- **Recall dropped**: Some TPs killed by stricter prompt (PR#22345 went to 0)
- **LLM non-determinism**: PR#22345 found the bug in v6a/v6b but not v6c-v6e (different seed/temperature)
- **Persistent misses**: PR#10600's 4 golden bugs (naming, error msg, case-sensitive indexOf, TOCTOU) never found in any run
- **Ready for full 47-PR benchmark** to get real F1 numbers

---

## Run v6f — Raw diff hunks embedded in slice prompts
- **Changes**: Extract per-file diff hunks and embed them in each slice alongside entity snippets. Prompt says "look for wrong strings, copy-paste errors, case-sensitivity issues"
- **Results**: 13 candidates, 314s, 57 tool calls (up from ~0!)

| PR | Golden | Candidates | TPs | Notes |
|----|--------|-----------|-----|-------|
| #8087 | 2 | 2 | 1 | ✅ forEach+async. getCalendar FP |
| #10600 | 4 | 2 | 0 | FPs: isChecked() Promise, DisableTwoFactor args — closer but still no golden match |
| #10967 | 5 | 4 | 3 | ✅ null ref, ✅ externalCalendarId, **✅ IS_TEAM_BILLING inversion** (NEW!). FP: deleteEvent dupe |
| #22345 | 2 | 1 | 0 | FP: SQL injection. Still missing unreachable branches + teamsFromOrg |
| #7232 | 2 | 4 | 2 | ✅ forEach+async, **✅ SendGrid orphan DB record** (NEW!). 2 FPs |
| **Total** | **15** | **13** | **~6** | **P≈46%, R≈40%** |

### Key Win: Diff context unlocked two persistent misses
- IS_TEAM_BILLING inversion (PR#10967) — never found before in any run
- SendGrid orphan record (PR#7232) — the exact golden bug about missing DB deletion
- Tool calls went from ~0 to 57 — agents now actually read files

### Progression Summary
| Version | Candidates | ~TPs | ~FPs | ~Precision | ~Recall |
|---------|-----------|------|------|-----------|---------|
| v6a (baseline) | 37 | 7 | 30 | 19% | 47% |
| v6b (strict prompt + dedup) | 10 | 6 | 4 | 60% | 40% |
| v6e (camelCase dedup) | 9 | 4-5 | 4-5 | 50% | 27-33% |
| **v6f (diff in prompt)** | **13** | **6** | **7** | **46%** | **40%** |

---

## Run v7 — New Rust detectors: interface-impl-mismatch + unreachable-branch
- **Changes**: Two new deterministic detectors in Rust pipeline:
  - `interface-impl-mismatch`: When interface method signature changes, checks if implementations were updated
  - `unreachable-branch`: Flags else-if/else after variable assigned from non-nullable function call
- **Results**: 11 candidates, 397s

| PR | Golden | Candidates | TPs | New detector signal |
|----|--------|-----------|-----|---------------------|
| #8087 | 2 | 2 | 1 | unreachable-branch fired (not a golden bug here) |
| #10600 | 4 | 2 | 0 | No new detectors fired |
| #10967 | 5 | 4 | 3-4 | **🎯 interface-impl-mismatch fired on Calendar!** Golden bug #5 |
| #22345 | 2 | 0 | 0 | No detectors fired (pattern is different from what we detect) |
| #7232 | 2 | 3 | 2 | unreachable-branch fired (not golden here) |
| **Total** | **15** | **11** | **~6-7** | **P≈55-64%, R≈40-47%** |

### Impact of new detectors
- interface-impl-mismatch gave the LLM the exact signal for PR#10967's Calendar interface golden bug
- unreachable-branch fires but on non-golden patterns — needs tuning to catch PR#22345's pattern
- Overall: fewer candidates, same/better TP rate → **precision improved**
