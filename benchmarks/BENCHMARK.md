# Martian Code Review Benchmark

Evaluates inspect against the [withmartian/code-review-benchmark](https://github.com/withmartian/code-review-benchmark): 50 PRs across 5 repos (Sentry, Grafana, Cal.com, Discourse, Keycloak) with 137 human-curated golden comments.

## Prerequisites

1. Clone the benchmark repo:
   ```bash
   git clone https://github.com/withmartian/code-review-benchmark.git /tmp/martian-eval/benchmark
   ```

2. Build inspect (release):
   ```bash
   cargo build -p inspect-cli --release
   ```

3. Set `INSPECT_BIN` in `benchmarks/martian_eval.py` if your binary is in a non-default location.

4. (Optional) Set `GITHUB_TOKEN` env var for API rate limits during repo fetching.

## Running

### AST mode (deterministic, no LLM)

```bash
# Full run — all 50 PRs, 5 repos
python3 benchmarks/martian_eval.py --mode ast

# Single repo
python3 benchmarks/martian_eval.py --mode ast --repo keycloak

# Limit PRs (useful for iteration)
python3 benchmarks/martian_eval.py --mode ast --repo sentry --limit 3
```

### LLM mode (full pipeline with LLM judge)

```bash
# Requires OPENAI_API_KEY or ANTHROPIC_API_KEY
python3 benchmarks/martian_eval.py --mode llm --limit 5 --model gpt-4o
```

## What happens on first run

1. Repos are cloned (blobless, `--filter=blob:none`) to `/tmp/martian-eval/repos/`
2. PR base/head SHAs are resolved via GitHub API and cached in `/tmp/martian-eval/sha_cache/`
3. Git worktrees are created at `/tmp/martian-eval/worktrees/{repo}/{base}_{head}/`
4. Entity graphs are built and cached (bincode) at `/tmp/inspect-graph-cache/`

Subsequent runs reuse all caches. Worktrees and graph caches are stable per commit pair.

## Performance

Measured on M-series Mac (release build):

| Run   | Total (50 PRs) | Per-PR | Notes                          |
|-------|-----------------|--------|--------------------------------|
| Cold  | ~580s           | ~11.6s | Includes git clone/fetch       |
| Hot   | ~33s            | ~0.7s  | All caches warm                |

### Optimizations

- **Graph caching**: `EntityGraph` is serialized to `/tmp/inspect-graph-cache/` via bincode after first build. Keycloak graph (75k entities): 1650ms build → 320ms cache load.
- **BFS skip for large PRs**: When >500 entities are changed, `blast_radius` uses direct dependent count instead of transitive BFS. Sentry PR with 70k reviews: scoring 33s → 0.8s.

## Current scores

```
AST TRIAGE RESULTS (137 golden comments, 0 PR errors)
  Match:   115 (83.9%)
  Partial:  22 (16.1%)
  Miss:      0 (0.0%)

  Strict recall (match only):     83.9%
  Lenient recall (match+partial): 100.0%

  Per-repo breakdown:
  Repo            Strict  Lenient
  cal_dot_com      93.5%   100.0%
  discourse        71.4%   100.0%
  grafana          81.8%   100.0%
  keycloak        100.0%   100.0%
  sentry           75.0%   100.0%
```

## Clearing caches

```bash
# Clear graph cache (forces rebuild on next run)
rm -rf /tmp/inspect-graph-cache

# Clear everything (repos, worktrees, SHA cache, graph cache)
rm -rf /tmp/martian-eval /tmp/inspect-graph-cache
```

## Development context

### Repository layout

- **inspect** (`/Users/palanikannanm/Documents/work/inspect`): Branch `perf/graph-cache-benchmark`
  - `crates/inspect-core/src/analyze.rs` — main analysis pipeline (`analyze()`, graph caching, BFS skip)
  - `crates/inspect-core/src/risk.rs` — risk scoring (`compute_risk_score`, `score_to_level`, `is_public_api`)
  - `crates/inspect-core/src/classify.rs` — ConGra classification (text/syntax/functional)
  - `crates/inspect-core/src/types.rs` — all types: `EntityReview`, `RiskLevel`, `ChangeClassification`, `ReviewResult`
  - `crates/inspect-core/src/untangle.rs` — Union-Find grouping of related changes
  - `crates/inspect-api/src/prompts.rs` — LLM prompt templates and triage formatting
  - `crates/inspect-cli/` — CLI binary entry point
  - `benchmarks/martian_eval.py` — benchmark harness (this file documents it)

- **sem** (`/Users/palanikannanm/Documents/work/sem`): Branch `perf/graph-serde`
  - `crates/sem-core/src/parser/graph.rs` — `EntityGraph::build()`, `impact_count()`, `get_dependents()`
  - `crates/sem-core/src/parser/differ.rs` — `compute_semantic_diff()` (entity-level diff)
  - `sem-core` is linked via local path (`../../../sem/crates/sem-core`) from all inspect crates

### Analysis pipeline (analyze.rs)

1. `GitBridge::open()` → `get_changed_files(&scope)` → list of `FileChange`
2. `compute_semantic_diff()` → entity-level `SemanticChange` list (before/after content per entity)
3. `list_source_files()` → `git ls-files` filtered to supported extensions
4. `EntityGraph::build()` → parse ALL source files via tree-sitter, build symbol table + reference edges (cached via bincode)
5. For each changed entity: `get_dependents()`, `get_dependencies()`, `impact_count()` (BFS, skipped when >500 changes)
6. `classify_change()` → ConGra taxonomy (Text/Syntax/Functional/mixed)
7. `compute_risk_score()` → 0.0-1.0 based on classification + change type + public API + blast radius + dependents
8. `score_to_level()` → Critical (≥0.7), High (≥0.5), Medium (≥0.3), Low (<0.3)
9. `untangle()` → group related changes via Union-Find on dependency edges

### How AST benchmark matching works (martian_eval.py)

`ast_match_comment()` extracts identifiers from golden comments and tries to match them against inspect entities in priority order:

1. **Strategy 1**: Identifier matches a High/Critical entity name → `match`
2. **Strategy 2**: Identifier matches a High/Critical/Medium entity name → `match`
3. **Strategy 3**: Identifier matches ANY entity name → `partial`
4. **Strategy 4**: Golden identifiers appear in HCM entity code content → `match`
5. **Strategy 5**: File path from golden comment matches an HCM entity's file → `partial`
6. If none hit → `miss`

**Key insight**: The 22 partials (16.1%) are entities found by strategy 3 or 5 — the entity exists in inspect's output but its risk level is too low (Low) to be caught by strategies 1-2. Promoting these to Medium+ risk would convert partials → matches.

### What would improve scores

- **Promote partials → matches**: The 22 partials are entities with Low risk that should be Medium+. Tuning `compute_risk_score()` weights or `score_to_level()` thresholds could help, but risks inflating HCM counts.
- **Discourse (71.4% strict)**: Weakest repo. Many Ruby/JS changes where entity names don't match golden comment identifiers well.
- **Sentry (75.0% strict)**: Large PRs with many entities dilute risk scores. Python entity extraction may miss some patterns.
- **LLM mode**: Not yet optimized. The triage section (`build_rich_triage` in `prompts.rs`) feeds entity info to the LLM reviewer — improving what entities/context it sees would improve LLM-mode F1.
