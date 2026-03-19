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
- **Exp 5 (discarded)**: Boost Modified/Deleted change type weights — no effect
- **Exp 6 (discarded)**: Further lower blast cap to 0.15, test penalty to 0.85 — no effect
- **Key insight**: Weight tuning alone hits a wall. All entities in same file share blast_radius, so they sort identically. The remaining 15 misses need structural scoring changes or new signals, not just weight adjustments.
- **IMPORTANT**: autoresearch.sh was missing `cargo build --release` — experiments 2-3 were stale! Fixed in exp 4.

## Ideas to Try
1. **Content-length signal**: Larger functions have more surface area for bugs — use line count as a tiebreaker
2. **Structural change boost for Modified**: If structural_change == Some(true), give extra boost since the code logic actually changed
3. **Named entity boost over chunks**: Entities with real names (not "lines 81-100") should score higher than chunk placeholders
4. **Per-file entity diversity**: If multiple entities in same file, boost the one with most dependents/dependencies as the "representative"
5. **Increase classification weights**: Functional classification at 0.22 may be too low — try 0.30+
6. ~~Test-file penalty tuning~~ (diminishing returns)
7. ~~Blast radius cap tuning~~ (diminishing returns)
8. **New detector signals**: Detect async-in-forEach, case-sensitivity issues, type confusion patterns
