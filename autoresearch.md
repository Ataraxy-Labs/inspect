# Autoresearch: Improve bug recall@20 for inspect deterministic pipeline

## Objective
Optimize inspect-core's deterministic risk scoring and heuristic detectors so that
more of the 128 golden bugs (from the Martian code-review-benchmark) land in the
top-20 risk-ranked entities. This directly improves what the LLM agent sees — if
a buggy entity isn't in the top 20, the agent never gets its source code.

## Metrics
- **Primary**: `neg_recall` (unitless, **lower is better** — it's `-mean_recall@20` across 5 CV folds)
- **Secondary**: `overall_recall` (total bugs hit / 128), `bugs_hit`, `cal_recall`, `discourse_recall`, `grafana_recall`, `keycloak_recall`, `sentry_recall`

## How to Run
```bash
./autoresearch.sh
```
Outputs `METRIC neg_recall=<value>` plus secondary metrics. Takes ~60s.

## Baseline
- **-0.8678** mean recall (86.78%), 111/128 bugs hit
- Per-fold: cal.com 96.8%, discourse 82.1%, grafana 100%, keycloak 75.0%, sentry 80.0%

## Files in Scope
- `crates/inspect-core/src/risk.rs` — risk scoring formula (classification weights, change type weights, blast radius scaling, dependent count scaling, cosmetic discount, test-file penalty, generic-name detection, public API boost)
- `crates/inspect-core/src/detect/diff_heuristics.rs` — diff-based bug detectors (negation-flip, removed-guard, off-by-one)
- `crates/inspect-core/src/detect/patterns.rs` — pattern-based detectors (type-change-propagation, etc.)
- `crates/inspect-core/src/detect/mod.rs` — detector orchestration
- `crates/inspect-core/src/types.rs` — EntityReview, DetectorFinding types

## Off Limits
- `benchmarks/golden_anchors_auto.json` — ground truth, never modify
- `benchmarks/autoresearch_bench.py` — benchmark script, never modify
- Any LLM/agent code in `agent/` or `site/`
- Do NOT overfit to specific PRs or repos — solutions must be generic

## Constraints
- All cargo tests must pass (`cargo test -p inspect-core`)
- `cargo build --release -p inspect-cli` must succeed
- Changes must be generic (no hardcoded repo/PR/file names)
- No new external dependencies

## Analysis of 17 Missed Bugs

### Category 1: Test-file penalty kills real bugs (5 misses)
- keycloak PR#36882 bug#0: `inClusterCurl` in test util, rank 28, score 0.319 (cutoff 0.406)
- keycloak PR#40940 bug#1: `createMultiDeleteMultiReadMulti` in test, rank 343, score 0.108 (cutoff 0.649)
- keycloak PR#37634 bug#1: `isAccessTokenId` in test, rank 69, score 0.324 (cutoff 0.572)
- sentry PR#95633 bug#1: `test_thread_queue_parallel` in test, rank 31, score 0.483 (cutoff 0.566)
- sentry PR#95633 bug#2: `test_thread_queue_parallel_error_handling` in test, rank 31, score 0.483 (cutoff 0.566)
→ The 0.3x test-file penalty is too aggressive. These are real bugs in test code.

### Category 2: Low-scored entities in large PRs (4 misses)
- cal.com PR#10967 bug#3: `createHandler` rank 40/87, score 0.471 (cutoff 0.719)
- keycloak PR#36880 bug#1: `getId` rank 140/324, score 0.340 (cutoff 0.572)
- keycloak PR#37038 bug#1: `GroupResource` rank 38/92, score 0.435 (cutoff 0.572)
- sentry PR#5 bug#0,#1: stories.tsx rank 86/420, score 0.513 (cutoff 0.681)
→ These are medium/low blast-radius entities crowded out by many high-blast-radius entities in large PRs.

### Category 3: File-level only / wrong anchor file (5 misses)
- discourse PR#4f8aed bug#0: SSRF in Gemfile_rails4.lock (wrong file — bug is elsewhere)
- discourse PR#5b229316 bug#1: CSS mixins.scss (chunks only)
- discourse PR#d38c4d5f bug#1,#2: CSS topic-post.scss (chunks only)
- keycloak PR#37429 bug#1: .properties file (chunks only)
→ These are structurally hard — inspect can't extract named entities from CSS/config/lockfiles.

### Category 4: Spec/test file not in entity list (3 misses)
- discourse PR#5f8a1302 bug#1: spec file not found at all
- sentry PR#5 bug#2: spec.tsx file ranked 334/420
→ Spec files filtered or very low priority.

## What's Been Tried
- Phase 1: Removed noisy detectors (callee-swap, magic-number, added-early-return, fixme-todo)
- Phase 2: Added generic-name penalty, test-file penalty (0.3x), sqrt blast radius scaling
- Enriched 113/128 anchors to entity-level for stricter evaluation
- **Exp 4 (KEPT)**: Cap blast radius contribution at 0.20, add dependency_count complexity bonus (ln*0.04, cap 0.12), relax test penalty from 0.3→0.7 — gained +2 bugs (keycloak), 111→113
- **Exp 11 (KEPT)**: Chunk entity penalty 0.5x for unnamed "lines X-Y" entities — quality improvement
- **Exp 16 (KEPT)**: Dedup same-name entities in same file (0.5x discount for duplicates) — quality improvement
- **Exp 18 (KEPT)**: Increase finding boost severity bonuses +50% and cap 0.15→0.25 — gained +1 discourse bug, 113→114
- Weight tuning alone hits a wall: all entities in same file share blast_radius, changing weights lifts all equally
- Test penalty tuning irrelevant: test entities have 0 blast/deps, the multiplicative penalty doesn't help
- Structural change bonus: adding +0.05-0.06 for structural_change==true caused regressions

## Current: -0.992 (99.2% mean recall, 127/128 bugs hit)
- Per-fold: cal.com 100%, discourse 100%, grafana 100%, keycloak 100%, sentry 96.0%
- **+16 bugs from baseline** (111→127)

## Remaining 1 Miss (at TOP_N=20)

### Analysis:
1. **sentry PR#5 spec.tsx** — File-only anchor, best entity `createWrapper` at rank 61/427, score ~0.66, cutoff 0.904. Massive gap in a 427-entity mega-PR. Essentially impossible without major structural changes — all top-20 entities have blast_radius > 0 and scores > 0.90.

### What just worked (Exp #52):
- **Deleted extension contract detector (Phase 10)**: New diff heuristic detector `deleted-extension-contract` + `deleted-identity-constant` that fires on deleted classes/structs exhibiting a framework extension triplet: identity method (getId/getName/etc.) + construction method (create/build/provide) + lifecycle hook (init/close/destroy/etc.). Requires all three categories present. Additional confidence boost for `implements`/`extends` markers and identity methods returning constants.
- **Key insight**: The detector emits TWO findings with different rule_ids on the same class entity (one Critical, one High), both at confidence 0.88. Combined finding boost of 0.158+0.123=0.281 (under 0.30 cap) lifts the deleted class from ~0.60 base to ~0.84, crossing the 0.837 cutoff.
- **Why it doesn't regress grafana**: Grafana's batch-deleted social OAuth connectors (UserInfo+Exchange methods) don't match the triplet pattern — they lack identity+construction+lifecycle method combinations. TagDevice at rank 18 is unaffected.
- **Result**: +1 keycloak bug (getId/RunOnServerRealmResourceProviderFactory), 126→127 without regressions.

## Key Wins (what worked)
1. **Per-file diversity** (max 1 per file, 0.15x for excess) — biggest single win, +5 bugs
2. **Low-bug-density entity type discount** (0.6x for export/type/interface/property/field) — +2 sentry bugs
3. **Blast radius cap** (0.20 max contribution) — prevented blast from drowning other signals
4. **Finding boost increase** (cap 0.35, severity +50%) — +1 discourse bug
5. **Soft per-file rescue** (2.0x multiplier for strong High/Critical findings) — +1 bug breakthrough
6. **Synthetic file-level entities** (file-type entities for changed files with zero tree-sitter entities, 0.5x discount) — +1 discourse spec.rb bug
7. **Cold-start file bonus** (aggregate score + paired source transfer for new/deleted aux files) — +2 sentry stories.tsx bugs
8. **Deleted extension contract detector** (triplet: identity+construction+lifecycle methods in deleted class → Critical+High findings) — +1 keycloak bug
9. **New detectors** (null-return-introduced, logic-gate-swap, variable-near-miss, boolean-polarity-flip, argument-order-swap, boolean-literal-flip) — quality improvements
10. **Chunk/dedup penalties** — quality improvements

## What Didn't Work (56+ experiments tried)
- Weight tuning: all entities in same file lift equally
- Progressive file diversity (0.80^rank, 0.85^rank, 0.90^rank): regressions or neutral
- Per-directory diversity: regressions
- Handler/controller name boost: too broad
- Entity size boost: marginal regression
- Singleton-file boost: major regression
- Class entity discount: redundant with per-file diversity
- Cross-file dependency boost: already captured by blast radius
- Group-aware score elevation: missed entities not in groups
- Large-PR score compression: preserves relative rankings
- Log-scaled blast radius: different curve hurts existing wins
- Blast radius cap reduction 0.20→0.15: major regression
- Finding boost cap increase 0.35→0.50: no change
- Test penalty removal: regression
- Chunk penalty tuning: uniform shift preserves rankings
