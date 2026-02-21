# inspect

Entity-level code review for Git. Every code review tool today works at the file or line level. inspect works at the entity level: functions, structs, classes, traits. It scores each change by risk and groups them by logical dependency.

## The Problem

`git diff` tells you 12 files changed. But which changes actually matter? A renamed variable, a reformatted function, and a deleted public API method all look the same in a line-level diff. You have to read every line to figure out what needs careful review and what can be skipped.

This gets worse with AI-generated code. DORA 2025 found that AI adoption led to +154% PR size, +91% review time, and +9% more bugs shipped. Reviewers are drowning in noise.

## What inspect Does

For every changed entity, inspect computes:

- **Classification**: What kind of change is this? Text-only (comments/whitespace), syntax (signature/type change), functional (logic change), or a combination. Based on [ConGra](https://arxiv.org/abs/2409.14121).
- **Risk score**: 0.0 to 1.0, combining classification, blast radius, dependent count, public API exposure, and change type. Cosmetic-only changes get a 70% discount.
- **Blast radius**: How many entities are transitively affected if this change breaks something. Computed from the full repo entity graph, not just changed files.
- **Grouping**: Union-Find untangling separates independent logical changes within a single commit, so tangled commits can be reviewed as separate units.

```
$ inspect diff HEAD~1

inspect 12 entities changed
  1 critical, 4 high, 6 medium, 1 low

groups 3 logical groups:
  [0] src/merge/ (5 entities)
  [1] src/driver/ (4 entities)
  [2] validate (3 entities)

entities (by risk):

  ~ CRITICAL function merge_entities (src/merge/core.rs)
    classification: functional  score: 0.82  blast: 171  deps: 3/12
    public API
    >>> 12 dependents may be affected

  - HIGH function old_validate (src/validate.rs)
    classification: functional  score: 0.65  blast: 8  deps: 0/3
    public API

  + MEDIUM function parse_config (src/config.rs)
    classification: functional  score: 0.45  blast: 0  deps: 2/0

  ~ LOW function format_output (src/display.rs)
    classification: text  score: 0.05  blast: 0  deps: 0/0
    cosmetic only (no structural change)
```

## Install

```bash
cargo install --git https://github.com/Ataraxy-Labs/inspect inspect-cli
```

Or build from source:

```bash
git clone https://github.com/Ataraxy-Labs/inspect
cd inspect && cargo build --release
```

## Commands

### `inspect diff <ref>`

Review entity-level changes for a commit or range.

```bash
inspect diff HEAD~1              # last commit
inspect diff main..feature       # branch comparison
inspect diff abc123              # specific commit
inspect diff HEAD~1 --context    # show dependency details
inspect diff HEAD~1 --min-risk high  # only high/critical
inspect diff HEAD~1 --format json    # JSON output
inspect diff HEAD~1 --format markdown  # markdown output (for agents)
```

### `inspect pr <number>`

Review all changes in a GitHub pull request. Uses `gh` CLI to resolve base/head refs.

```bash
inspect pr 42
inspect pr 42 --min-risk medium
inspect pr 42 --format json
```

### `inspect file <path>`

Review uncommitted changes in a file.

```bash
inspect file src/main.rs
inspect file src/main.rs --context
```

### `inspect bench --repo <path>`

Benchmark entity-level review across a repo's commit history. Outputs JSON with per-commit details and aggregate metrics.

```bash
inspect bench --repo ~/my-project --limit 50
```

## MCP Server

inspect ships an MCP server so any coding agent (Claude Code, Cursor, etc.) can use entity-level review as a tool.

```bash
# Build the MCP server
cargo build -p inspect-mcp

# Binary at target/debug/inspect-mcp
```

**6 tools:**

| Tool | Purpose |
|------|---------|
| `inspect_triage` | Primary entry point. Full analysis sorted by risk with verdict. |
| `inspect_entity` | Drill into one entity: before/after content, dependents, dependencies. |
| `inspect_group` | Get all entities in a logical change group. |
| `inspect_file` | Scope review to a single file. |
| `inspect_stats` | Lightweight summary: stats, verdict, timing. No entity details. |
| `inspect_risk_map` | File-level risk heatmap with per-file aggregate scores. |

**Review verdict** (returned by triage and stats):
- `likely_approvable`: All changes are cosmetic
- `standard_review`: Normal changes, no high-risk entities
- `requires_review`: High-risk entities present
- `requires_careful_review`: Critical-risk entities present

Add to your Claude Code config:
```json
{
  "mcpServers": {
    "inspect": {
      "command": "/path/to/inspect-mcp"
    }
  }
}
```

## Code Review Benchmark

inspect + LLM vs Greptile vs CodeRabbit on the same dataset, same judge, same methodology. 141 planted bugs across 52 PRs in 5 production repos (Sentry, Cal.com, Grafana, Keycloak, Discourse).

| Metric | inspect + LLM | Greptile API | CodeRabbit CLI |
|--------|--------------|-------------|----------------|
| Recall | **95.0%** | 91.5% | 56.0% |
| Precision | 33.3% | 21.9% | **48.2%** |
| F1 Score | 49.4% | 35.3% | **51.8%** |
| HC Recall | **100%** | 94.1% | 60.8% |
| Findings | 402 | 590 | 164 |

inspect catches **95% of all bugs** and **100% of high-severity bugs**. CodeRabbit misses 44% of bugs overall and 39% of high-severity ones. Greptile has decent recall but produces 3x more noise.

The approach: entity-level triage cuts 100+ changed entities to the 60 riskiest, then sends each to an LLM for review. This costs a fraction of reviewing the full diff, with higher recall than tools that scan everything.

Dataset: [HuggingFace](https://huggingface.co/datasets/rs545837/inspect-greptile-bench). Judge: heuristic keyword matching applied identically to all tools.

## Triage Benchmark

Results from running `inspect bench` against three Rust codebases (89 commits, 8,870 entities total):

| Metric | sem | weave | agenthub |
|--------|-----|-------|----------|
| Commits analyzed | 31 | 39 | 19 |
| Entities reviewed | 4,955 | 2,803 | 1,112 |
| Avg entities/commit | 159.8 | 71.9 | 58.5 |
| Avg blast radius | 0.0 | 3.4 | 42.5 |
| Max blast radius | 0 | 171 | **595** |
| High/Critical ratio | 15.1% | 40.6% | **77.1%** |
| Cross-file impact | 0% | 10.6% | **70.7%** |
| Tangled commits | 96.8% | 69.2% | 94.7% |

Key takeaways:

- **Blast radius 595** means one entity change in agenthub could affect 595 other entities transitively. A line-level diff won't tell you this.
- **70.7% cross-file impact** means most changes in agenthub ripple across file boundaries. Reviewing one file in isolation misses the picture.
- **96.8% tangled commits** means almost every commit in sem contains multiple independent logical changes that should be reviewed separately.

## Change Classification

Based on [ConGra (arXiv:2409.14121)](https://arxiv.org/abs/2409.14121). Every change is classified along three dimensions, producing 7 categories:

| Classification | What changed |
|---------------|-------------|
| Text | Comments, whitespace, docs only |
| Syntax | Signatures, types, declarations (no logic) |
| Functional | Logic or behavior |
| Text+Syntax | Comments and signatures |
| Text+Functional | Comments and logic |
| Syntax+Functional | Signatures and logic |
| Text+Syntax+Functional | All three dimensions |

## Risk Scoring

Each entity gets a risk score from 0.0 to 1.0:

```
score = classification_weight     (0.05 to 0.55)
      + blast_ratio * 0.3         (normalized by total entities)
      + ln(1 + dependents) * 0.1  (logarithmic)
      + public_api_boost           (0.15 if public)
      + change_type_weight         (0.05 to 0.2)

if cosmetic_only: score *= 0.3
```

Risk levels: **Critical** (>= 0.7), **High** (>= 0.5), **Medium** (>= 0.3), **Low** (< 0.3)

## Languages

Rust, TypeScript, TSX, JavaScript, Python, Go, Java, C, C++, Ruby, C#, Fortran

Powered by tree-sitter parsers from [sem-core](https://github.com/Ataraxy-Labs/sem).

## Architecture

Three crates:

- **inspect-core**: Analysis engine. Entity extraction (via sem-core), change classification, risk scoring, Union-Find untangling, review verdict.
- **inspect-cli**: CLI interface with terminal, JSON, and markdown formatters.
- **inspect-mcp**: MCP server exposing 6 tools for agent integration.

```
Git diff
  -> sem-core: extract entities, compute semantic diff
  -> classify: ConGra taxonomy (text/syntax/functional)
  -> risk: score from classification + blast radius + dependents + public API
  -> untangle: Union-Find grouping on dependency edges
  -> verdict: LikelyApprovable / StandardReview / RequiresReview / RequiresCarefulReview
  -> format: terminal, JSON, or markdown output
```

## Part of the Ataraxy Labs stack

- [**sem**](https://github.com/Ataraxy-Labs/sem): Entity-level diff, blame, graph, and impact analysis
- [**weave**](https://github.com/Ataraxy-Labs/weave): Entity-level semantic merge driver for Git
- [**inspect**](https://github.com/Ataraxy-Labs/inspect): Entity-level code review (this repo)
