# Benchmark Improvement Plan

**Run**: Opus full 47-PR run (2026-03-23)
**Final scores** (45 PRs scored manually): **P=27.5%, R=40.7%, F1=32.8%**
**Target**: Beat Qodo's 64.3% F1 — **gap is -31.5%**

### Per-repo breakdown
| Repo | PRs | TP | FP | FN | P | R | F1 |
|------|-----|----|----|-----|----|----|-----|
| cal.com | 10 | 20 | 25 | 11 | 44.4% | 64.5% | 52.6% |
| discourse | 10 | 9 | 28 | 19 | 24.3% | 32.1% | 27.7% |
| grafana | 9 | 7 | 32 | 13 | 17.9% | 35.0% | 23.7% |
| keycloak | 10 | 9 | 35 | 15 | 20.5% | 37.5% | 26.5% |
| sentry | 6 | 5 | 12 | 15 | 29.4% | 25.0% | 27.0% |
| **TOTAL** | **45** | **50** | **132** | **73** | **27.5%** | **40.7%** | **32.8%** |

---

## 1. DEDUP FAILURES (est. impact: +8-12% precision)

### Problem
Same bug reported 2-5x across different slices. The two dedup layers both fail:
- `review-parallel.ts` dedup: only matches `file + first 80 chars` — trivially bypassed by different wording
- `review-entry.ts` dedup: word-overlap + bug-pattern matching — misses cross-file restatements

### Worst cases
| PR | Issue | Dupes | FP cost |
|----|-------|-------|---------|
| #11059 | fetch Response has no `.data` | 5x (Google, Hubspot, Bigin, Zoho, Lark) | 4 FP |
| #11059 | SafeParseResult stored as key | 2x | 1 FP |
| #14943 | missing SMS method filter | 2x | 1 FP |
| #22345 | unreachable branch getBaseConditions | 2x | 1 FP |
| #6669a2d | typo stopNotificiationsText | 2x | 1 FP |

### Fix: Aggressive semantic dedup in review-entry.ts
```
- Extract the "core bug pattern" from each issue (what function/variable is wrong, what the fix is)
- Use identifier overlap + same-root-cause matching
- When same bug manifests across N callers, merge into one issue listing all affected files
- Cap at 1 issue per unique root cause
```

### Fix: Cross-slice dedup prompt
```
- After all slices complete, show the agent a summary of all found issues
- Ask it to merge duplicates before final output
- Or: add previous slice issues to next slice's prompt to avoid re-reporting
```

---

## 2. OVER-GENERATION (est. impact: +5-8% precision)

### Problem
Agent reports plausible-sounding but incorrect issues. Avg 4.6 issues/PR, should be ~2.5.

### Worst cases
| PR | Our issues | Golden | FP | FP examples |
|----|-----------|--------|-----|-------------|
| #4f8aed | 10 | 6 | 8 | case-sensitive hostname, mutating contents string, absolutize_urls port, PostCreator always returns post |
| #11059 | 16 | 5 | 12 | isTokenValid inverted, case-sensitive email, expiryDate wrong units, dead throwIfNotHaveAdminAccessToTeam |
| #22532 | 5 | 2 | 4 | missing await on patch, wrong CalendarCache init, empty div wrapper, cleanup exit code |

### Fix: Hard cap on total issues per PR
```
- After dedup, keep only top N issues (N=4-5) sorted by severity + confidence
- In the system prompt: "Report at most 3 bugs per slice"
- Currently slices can each produce 3-5 issues → 10 slices × 3 = 30 raw issues
```

### Fix: Confidence scoring
```
- Ask agent to rate confidence 1-5 for each issue
- Filter issues below confidence 3
- Or: require "evidence" field to contain actual code snippet (not just description)
```

### Fix: Stricter prompt language
```
Current: "Report ALL real bugs you find, including medium-severity ones"
Better: "Report only bugs you are CERTAIN about. Each issue must have:
  1. Exact function/variable name
  2. Exact code line demonstrating the bug
  3. Clear explanation of what goes wrong at runtime
  Skip speculative concerns, style issues, and theoretical problems."
```

---

## 3. MISSED GOLDEN BUGS (est. impact: +5-10% recall)

### Patterns of misses
| Category | Missed count | Example |
|----------|-------------|---------|
| Domain logic | 4 | hardcoded maxSizeKB ignoring SiteSettings, gifsicle format, org-level members |
| Security (deep) | 3 | indexOf bypass, postMessage targetOrigin, X-Frame-Options ALLOWALL |
| Race conditions | 2 | stale retryCount, concurrent backup codes |
| Try-catch | 1 | missing error handling around await import |

### Fix: Domain-specific prompts
```
- For security-sensitive code (auth, embed, iframe): add security-focused review instructions
- For concurrent/DB code: add concurrency checklist (read-modify-write, TOCTOU)
```

### Fix: Non-code slice enhancement
```
- The non-code slice caught CSS/translation bugs well
- Extend it with explicit checklist: locale mismatches, CSS value inversions
```

---

## 4. GIT REPO CORRUPTION (FIXED)

### Problem
31/47 PRs failed with INSPECT_FAIL because git repos in /tmp/martian-eval/repos/ were missing HEAD, config, refs, and commondir files.

### Root cause
Unknown — repos may have been corrupted by a cleanup script or git gc. The worktree directories are file snapshots, not proper git worktrees.

### Fix applied
- Created HEAD, config, packed-refs, refs/heads, refs/tags for all repos
- Created .git files in worktree dirs pointing to parent repo
- Created commondir files in worktree metadata
- Re-running full benchmark

### Permanent fix needed
```
- Add repo health check at start of run_martian_official.py
- If repos are broken, automatically repair them
- Or: store entity_reviews in the AST cache so inspect doesn't need to re-run
```

---

## 5. SLICE QUALITY (est. impact: +3-5% on both P and R)

### Problem: Overlapping slices
Multiple slices analyze the same function, leading to duplicate findings.

### Problem: Missing context
Some slices lack caller/callee context needed to understand the bug.

### Fix: Smarter cluster merging
```
- If two clusters share >50% entities, merge them
- Include diff hunks for ALL entities in a cluster, not just the anchor
- Add "previously reported issues" to each slice to prevent re-reporting
```

---

## 6. SCORING METHODOLOGY

### Current manual scoring rules
- TP: our issue identifies the same core problem as a golden bug
- FP: our issue doesn't match any golden bug (even if it's a real bug)
- FN: golden bug not identified by any of our issues
- Partial matches: count as TP if the same root cause is identified

### Note on "real but not golden" bugs
Several FPs are actually real bugs not in the golden set:
- PR#8087: user.id self-comparison (always true), ternary precedence, .length<0
- PR#10600: isChecked() returns Promise (always truthy)
- PR#7232: missing method filter, try-catch loop
These would be TPs in a real review but hurt our benchmark score.

---

## Priority order for implementation

1. **Hard cap total issues to 4 per PR** — simplest change, biggest precision gain
2. **Aggressive dedup with identifier matching** — fixes the #11059 disaster
3. **Stricter prompt: certainty required** — reduces speculative FPs
4. **Merge cross-file instances of same bug** — catches fetch Response × 5 pattern
5. **Add verifier pass** — second LLM call to filter low-confidence issues
6. **Repo health check** — prevent INSPECT_FAIL silently

---

## 7. RUN-TO-RUN VARIANCE (observed in rerun)

### Problem
Rebuilding the inspect binary changes detector output slightly (e.g., 19f→18f for same PR).
Opus with high thinking generates more issues per slice than before — same prompt, more output.

### Evidence (rerun vs first run)
| PR | Run 1 issues | Run 2 issues | Delta |
|----|-------------|-------------|-------|
| #8087 | 4 | 7 | +3 |
| #10600 | 4 | 7 | +3 |
| #10967 | 7 | 9 | +2 |
| #7232 | 4 | 9 | +5 |
| #8330 | 2 | 6 | +4 (was perfect, now 4 FP + 1 dup) |
| #11059 | 16 | 9 | -7 (improved by detector change) |
| #14943 | 2 | 1 | -1 |

### Key example: PR#8330
Was a perfect 2/2 match. Now 6 issues:
- #1 slotStartTime ✓ (golden match)
- #2 dayjs === ✓ (golden match)
- #3 find() short-circuit on multiple overrides (speculative FP)
- #4 timezone midnight edge case (speculative FP)
- #5 dayjs === (DUPLICATE of #2)
- #6 dayjs === (DUPLICATE of #2)

### Fix
- Hard cap per PR (not per slice) eliminates the long tail
- Dedup must catch identical bugs across slices even with different wording
- Consider deterministic temperature=0 or seed for reproducibility

---

## 8. DISCOURSE/RUBY WEAKNESS (observed in rerun)

### Problem
Ruby/Rails code analysis has specific blind spots:
1. Missing `-ms-align-items` → `-ms-flex-align` CSS vendor prefix knowledge (missed in rerun)
2. Deep security bugs like SSRF via `open(url)`, X-Frame-Options ALLOWALL, postMessage targetOrigin
3. CSS float vs flexbox conflict understanding
4. ERB `end if` syntax error detection

### Evidence
- PR#5b22931 (flexbox): 0/2 goldens matched (was 1/2 in run 1)
- PR#4f8aed (embed): Only 2/6 goldens matched — all security bugs missed
- PR#ffbaf8c (downsize): 1/3 — domain logic bugs (hardcoded KB, gifsicle format) missed

### Fix: Security-focused slice
```
- Add a dedicated "security review" slice for PRs with web-facing code
- Include checklist: SSRF, clickjacking, origin validation, XSS, open redirect
- Trigger when: iframe/embed code, URL handling, HTTP headers, open() calls
```

### Fix: Non-code slice for CSS
```
- The non-code slice already exists but needs CSS vendor prefix knowledge
- Add: "Check vendor prefix correctness (e.g., -ms-flex-align vs -ms-align-items)"
```

---

## 9. DUPLICATE BUG ACROSS SLICES — DETAILED PATTERNS

### Pattern A: Same function, different slices
Example: `dayjs ===` in PR#8330 reported 3x because 3 slices each saw the same `checkIfIsAvailable` function.
**Root cause**: Function appears in multiple clusters due to being a dependency of multiple changed entities.
**Fix**: Track reviewed function names globally, skip if already reported.

### Pattern B: Same bug type, different callers  
Example: `fetch Response.data` in PR#11059 reported for Google, Hubspot, Bigin, Zoho callers.
**Root cause**: Each caller is a separate entity in a separate cluster.
**Fix**: Detect when issues share the same root-cause function (e.g., `refreshOAuthTokens`) and merge.

### Pattern C: Semantically identical, different wording
Example: "case-sensitive email comparison" reported twice with different evidence strings.
**Root cause**: Two slices see the same code, agent describes the bug differently.
**Fix**: Extract identifiers (function names, variable names) from issue text. If ≥2 identifiers match → duplicate.

---

## LIVE SCORING TRACKER (Run 2)

### Cal.com (10 PRs) — TBD (need to re-score with new output)
### Discourse (10 PRs)
| PR | TP | FP | FN | Notes |
|----|----|----|-----|-------|
| ffbaf8c | 1 | 3 | 2 | dup downsize, speculative tempfile bugs |
| 6669a2d | 2 | 1 | 0 | ✅ nil deref + typo matched |
| 5f8a130 | 2 | 4 | 0 | dup race condition, dup case-sensitive |
| 4f8aed2 | 2 | 2 | 4 | missed SSRF, postMessage, X-Frame, end if |
| 5b22931 | 0 | 1 | 2 | missed -ms-align-items, missed float/flex |
| 267d8be | 1 | 4 | 0 | dup frozen_string, dup case-sensitive |
| d38c4d5 | 0 | 0 | 3 | CSS value inversions — complete miss, 0 findings |
| 060cda7 | 1 | 6 | 2 | off-by-one match, but massive FPs. dup delete(userId) |
| ecfa17b | 0 | 0 | 2 | thread-safety + symbol normalization — too subtle |
| d1c6918 | 0 | 7 | 4 | ALL FP — nil host, case-sensitive, migration — all missed |
| **Total** | **9** | **28** | **19** | P=24.3%, R=32.1%, F1=27.7% |

**Discourse is terrible** — 27.7% F1. Key failures:
- 4 PRs produced 0 issues (d38c, ecfa, d1c6 complete miss + 5b22 nearly)
- When we do produce issues, precision is awful (28 FP across 10 PRs)
- Domain-specific Ruby/Rails knowledge (serializer `?` suffix, migration normalization, `end if` ERB) barely caught
- CSS bugs completely invisible without findings from non-code slice

### Grafana (9 PRs)
| PR | TP | FP | FN | Notes |
|----|----|----|-----|-------|
| 79265 | 1 | 6 | 4 | race condition matched, but missed Exec compile error, ErrDeviceLimitReached |
| 103633 | 1 | 2 | 1 | test comment mismatch matched, missed asymmetric cache trust |
| 76186 | 0 | 1 | 2 | missed nil req panic, missed traceID removal |
| 107534 | 0 | 0 | 1 | complete miss (unused test parameter) |
| 106778 | 0 | 6 | 2 | ALL FP — missed React key prop, SilenceGrafanaRuleDrawer dep |
| 90045 | 3 | 3 | 0 | 🎉 all 3 goldens matched! but 3 duplicate metric bugs |
| 80329 | 1 | 4 | 0 | wrong log level matched, but 4 related FPs |
| 94942 | 1 | 2 | 1 | enableSqlExpressions matched, dup, missed not-implemented |
| 97529 | 0 | 8 | 2 | ALL FP — found various bugs but missed specific BuildIndex+TotalDocs races |
| **Total** | **7** | **32** | **13** | P=17.9%, R=35.0%, F1=23.7% |

**Grafana is the worst repo** — 23.7% F1. Massive FPs (32!) with low recall.
Key issue: agent finds real-looking Go bugs but they don't match the specific golden bugs.

### Keycloak (10 PRs)
| PR | TP | FP | FN | Notes |
|----|----|----|-----|-------|
| 41249 | 0 | 0 | 2 | complete miss — Java passkey auth, 9 findings but 0 output |
| 32918 | 1 | 5 | 1 | alias cleanup matched, missed recursive caching (session vs delegate) |
| 33832 | 1 | 4 | 1 | dead code ASN1Encoder matched, missed wrong provider return |
| 36882 | 0 | 5 | 1 | ALL FP — missed picocli.exit() calls System.exit() |
| 36880 | 0 | 3 | 3 | ALL FP — missed permission flag bug, resource lookup, getClientsWithPermission |
| 37038 | 1 | 6 | 1 | canManage() VIEW scope matched, missed resource ID issue |
| 37429 | 2 | 2 | 2 | 🎉 Italian in .lt file + Trad Chinese in zh_CN matched! Missed santize typo + anchor validation |
| 37634 | 1 | 2 | 3 | isAccessTokenId inverted logic matched, missed wrong null check + javadoc + broad catch |
| 38446 | 1 | 4 | 1 | Optional.get() without isPresent matched, missed missing setId |
| 40940 | 2 | 4 | 0 | 🎉 getSubGroupsCount null + flaky test both matched |
| **Total** | **9** | **35** | **15** | P=20.5%, R=37.5%, F1=26.5% |

**Keycloak struggles with Java domain logic.** Agent finds generic null-safety/error-handling issues but misses Keycloak-specific bugs (permission schemas, provider delegation, auth flow subtleties).

### Sentry (6 PRs scored, 2 remaining)
| PR | TP | FP | FN | Notes |
|----|----|----|-----|-------|
| 92393 | 0 | 2 | 3 | missed negative slicing + datetime TypeError — agent found unrelated __reduce__ bug |
| 67876 | 0 | 3 | 3 | missed null state, static OAuth state, missing key check — agent found test issues |
| 5 | 1 | 3 | 2 | zip/dict order matched, missed breaking error format + wrong key |
| 93824 | 2 | 1 | 3 | shard/shards tag + monkeypatched sleep matched! but 3 goldens missed |
| 77754 | 1 | 1 | 3 | shared mutable default matched, missed typo + naming + datetime serialization |
| 80528 | 1 | 2 | 1 | returns original config matched, missed unnecessary DB query |
| **Total** | **5** | **12** | **15** | P=29.4%, R=25.0%, F1=27.0% |

**Sentry has lowest recall (25%)** — Python bugs around Django querysets, OAuth security, and data serialization need domain knowledge the LLM lacks.

---

## 10. WHY EACH REPO FAILS — ROOT CAUSE ANALYSIS

### Cal.com (F1=52.6%) — "Works OK, precision kills it"
- **What works**: TypeScript pattern bugs (forEach-async, ===, wrong variable, case-sensitivity)
- **What fails**: Over-generation (avg 5.8 issues/PR, should be ~3). Dedup failures on #11059 alone cost 5 FP.
- **Fix path**: Hard cap + dedup → estimated F1 ~68%

### Discourse (F1=27.7%) — "Ruby domain blindness"
- **What works**: Nil derefs, typos when detector flags them
- **What fails completely**:
  - CSS bugs (3 golden bugs about $lightness values — 0 detected, 0 findings from inspect)
  - ERB syntax (`end if` — a Ruby template error)
  - Rails conventions (serializer `?` suffix, ActiveRecord patterns)
  - Security reasoning (SSRF via `open(url)`, indexOf bypass, X-Frame-Options)
  - Thread-safety + symbol normalization
- **Fix path**: 
  - Non-code slice needs CSS value comparison rules
  - Ruby-specific detector for method override, ERB syntax
  - Security-focused slice with checklist for web PRs
  - Estimated F1 improvement: +10-15% → ~40%

### Grafana (F1=23.7%) — "Finds wrong Go bugs"
- **What works**: When golden bugs are about metric recording (PR#90045 perfect 3/3)
- **What fails completely**:
  - Agent generates Go anti-pattern issues (nil checks, missing error checks, context capture) that aren't in golden set
  - Misses framework-specific bugs (React key prop, Go compilation errors with wrong Exec signature)
  - Misses subtle logic (asymmetric cache trust, traceID removal during refactor)
  - 32 FPs across 9 PRs — agent hallucinates plausible Go bugs
- **Fix path**:
  - Stricter prompt: "Only report bugs confirmed by reading the code, not generic Go anti-patterns"
  - Hard cap of 3 issues/PR would eliminate long tail
  - Go-specific detector for rows.Err(), type mismatch in function signatures
  - Estimated F1 improvement: +15-20% → ~40-45%

### Keycloak (F1=26.5%) — "Java domain logic is opaque"
- **What works**: Generic patterns (null checks, dead code, inverted logic)
- **What fails completely**:
  - Keycloak auth model (permission schemas, provider delegation, credential flows)
  - 35 FPs — agent reports plausible Java issues that aren't golden bugs
  - PR#41249: 9 detector findings but 0 output (agent couldn't reason about passkey auth)
  - PR#36880: 3 golden bugs about permission model, we reported 3 completely unrelated issues
- **Fix path**:
  - This is the hardest repo. Most golden bugs need understanding of Keycloak's internal abstractions.
  - Better slicing could help (include interface + implementation pairs)
  - Estimated F1 improvement: +5-10% → ~35%

### Sentry (F1=27.0%) — "Python edge cases missed"
- **What works**: Data structure bugs (shared mutable default, dict ordering, metric tags)
- **What fails completely**:
  - Django queryset negative slicing (Python-specific — `qs[-5:]` fails)
  - OAuth security patterns (static state, missing key checks)
  - Test quality bugs (monkeypatched sleep, typos in method names)
  - Agent reports wrong issues (test HMAC signatures, unrelated code)
- **Fix path**:
  - Python-specific detector for negative queryset slicing
  - Typo detector (Levenshtein on method names)
  - Estimated F1 improvement: +10-15% → ~40%

---

## 11. THE FUNDAMENTAL PROBLEM: HALLUCINATED BUGS

Across all repos, the #1 issue is: **the agent generates plausible-sounding bugs that aren't real.**

| Category | Count | % of FP |
|----------|-------|---------|
| **Speculative/generic anti-patterns** | ~50 | 38% |
| **Dedup failures (same bug 2-5×)** | ~35 | 27% |
| **Wrong code context (found a different bug)** | ~25 | 19% |
| **Real bug but not in golden set** | ~22 | 17% |

The "speculative" category is the killer. The agent sees code and thinks: "this COULD be a null deref" or "this COULD have a race condition" — and reports it without verifying. On cal.com (TypeScript), these guesses are often right. On Go/Java/Ruby, they're usually wrong because the agent doesn't understand the framework guarantees.

### Key insight: precision vs recall tradeoff
- If we cap at 2 issues/PR and keep only highest-confidence: P jumps to ~50-60% but R drops to ~25-30%
- If we cap at 4 issues/PR with aggressive dedup: P ~40-45%, R ~35-40%, F1 ~37-42%
- To beat Qodo (64.3%), we need BOTH better precision AND better recall
- Better recall requires domain-specific detectors (not just LLM reasoning)

---

## REVISED PRIORITY ORDER

Based on full 45-PR analysis:

### Tier 1: Quick wins (+10-15% F1)
1. **Hard cap 4 issues per PR** — removes ~40 FPs from speculative long tail
2. **Aggressive dedup** — removes ~35 FPs from duplicate bugs across slices
3. **Stricter prompt** — "report only bugs you verified by reading code, not generic anti-patterns"

### Tier 2: Language-specific improvements (+10-15% F1)
4. **CSS value comparison in non-code slice** — catches 3 discourse golden bugs (lightness inversions)
5. **Python negative-slice detector** — catches 3 sentry golden bugs (Django queryset)
6. **Go rows.Err() detector** — structural pattern, catches a few grafana bugs
7. **Ruby method override / ERB syntax detector** — catches discourse bugs
8. **Typo detector** (Levenshtein on identifiers) — catches 3-4 bugs across repos

### Tier 3: Architecture changes (+5-10% F1)
9. **Verifier pass** — second LLM call to validate each candidate
10. **Security-focused slice** — checklist for SSRF, clickjacking, origin validation
11. **Better slice context** — include interface+implementation pairs, caller chains
12. **Domain hint injection** — detect framework (Rails/Django/Spring) and add framework-specific review instructions
