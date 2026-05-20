> **Part of the [Ataraxy Labs](https://ataraxy-labs.com) stack** — agent-native infrastructure for software development. See also: [sem](https://ataraxy-labs.com/sem) (semantic version control) · [weave](https://ataraxy-labs.com/weave) (entity-level merge driver) · [opensessions](https://github.com/Ataraxy-Labs/opensessions) (tmux sidebar for coding agents).
>
> Read the manifesto: https://ataraxy-labs.com/#thesis · Essays: https://ataraxy-labs.com/blogs · LLMs: https://ataraxy-labs.com/llms.txt

<p align="center">
  <img src="assets/logo.svg" alt="inspect" width="80" />
</p>

<p align="center">
  <strong>inspect</strong>
</p>

<p align="center">
  <strong>Entity-level code review for Git.</strong><br>
  Triage PRs by structural risk, not line count.
</p>

inspect is a code review tool that works at the entity level instead of files or lines. It parses your diff with tree-sitter, classifies each changed entity (text-only, syntax, functional), scores it by risk using the cross-file dependency graph, and groups independent changes so tangled commits can be reviewed as separate units.

Every code review tool today shows you files and lines. inspect shows you which functions matter. A renamed variable, a reformatted function, and a deleted public API method all look the same in a line diff. inspect scores the API deletion as critical (high blast radius, many dependents) and the rename as low risk. Review the critical stuff first, skip the noise.

On the Greptile benchmark (141 findings across 52 PRs), inspect achieved 95.0% recall vs Greptile's 91.5% and CodeRabbit's 56.0%. On the Martian benchmark (137 findings, 50 PRs), inspect scored 46.2% F1, beating Augment Code (45.8%) for #1 on their leaderboard.

Agents and CI pipelines consume inspect through JSON output or the built-in MCP server (6 tools). Developers use the CLI to triage locally before pushing.

## The Problem

`git diff` tells you 12 files changed. But which changes actually matter? A renamed variable, a reformatted function, and a deleted public API method all look the same in a line-level diff. You have to read every line to figure out what needs careful review and what can be skipped.

This gets worse with AI-generated code. DORA 2025 found that AI adoption led to +154% PR size, +91% review time, and +9% more bugs shipped. Reviewers are drowning in noise.

inspect gives you two ways to handle this: local triage that ranks changes by structural risk, and optional LLM-powered review that finds the actual bugs.

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

## Two ways to use inspect

**Local (free, open source):** CLI + MCP server. Entity triage, risk scoring, blast radius, commit untangling. `inspect review` sends the riskiest entities to any LLM you choose (Anthropic, OpenAI, Ollama, or any OpenAI-compatible server). No vendor lock-in.

**Hosted API (optional):** Full review via `inspect.ataraxy-labs.com`. Goes further than local review with 9 specialized lenses, cross-model ensemble, and validation passes. Submit a PR, get back findings.

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

Review all changes in a GitHub pull request. By default, uses `gh` CLI to resolve base/head commit OIDs and local git objects. Use `--fetch github` to fetch PR files through the GitHub API without local refs.

```bash
inspect pr 42
inspect pr 42 --fetch github --remote owner/repo
inspect pr 42 --base abc123 --head def456
inspect pr 42 --min-risk medium
inspect pr 42 --format json
```

### `inspect file <path>`

Review uncommitted changes in a file.

```bash
inspect file src/main.rs
inspect file src/main.rs --context
```

### `inspect review <ref>`

Triage + LLM review. Triages entities by risk, sends the highest-risk ones to an LLM for review.

```bash
inspect review HEAD~1                          # Anthropic (default)
inspect review HEAD~1 --provider ollama --model llama3  # local Ollama
inspect review HEAD~1 --api-base http://localhost:8000/v1 --model my-model  # any OpenAI-compatible server
inspect review 42 --fetch github --remote owner/repo --provider anthropic  # review a GitHub PR locally
inspect review 42 --engine hosted --remote owner/repo  # submit to hosted inspect API
inspect review HEAD~1 --min-risk medium        # review more entities
inspect review HEAD~1 --max-entities 20        # send more to LLM
```

### `inspect bench --repo <path>`

Benchmark entity-level review across a repo's commit history. Outputs JSON with per-commit details and aggregate metrics.

```bash
inspect bench --repo ~/my-project --limit 50
```

## LLM Providers

`inspect review` works with Anthropic, OpenAI, and any OpenAI-compatible server (Ollama, vLLM, LM Studio, llama.cpp). Pass `--api-base` and it auto-detects the right client.

```bash
# Anthropic (default)
export ANTHROPIC_API_KEY=sk-ant-...
inspect review HEAD~1

# OpenAI
export OPENAI_API_KEY=sk-...
inspect review HEAD~1 --provider openai --model gpt-4o

# Ollama (local, no API key)
inspect review HEAD~1 --provider ollama --model llama3

# Any OpenAI-compatible endpoint (vLLM, LM Studio, etc.)
inspect review HEAD~1 --api-base http://localhost:8000/v1 --model my-model
```

| Provider | API key env var | Default base URL |
|----------|----------------|-----------------|
| `anthropic` | ANTHROPIC_API_KEY | api.anthropic.com |
| `openai` | OPENAI_API_KEY | api.openai.com/v1 |
| `ollama` | none | localhost:11434/v1 |

`--api-base` implies the OpenAI-compatible client, so you don't need `--provider` with it. `--provider ollama` implies `localhost:11434`, so you don't need `--api-base` with it.

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

## Hosted API

For teams that don't want to manage LLM infrastructure, we run a hosted review service at `inspect.ataraxy-labs.com`. It goes beyond what `inspect review` does locally.

**What the hosted API does differently:**

1. Entity triage ranks changes by graph signals (same as local)
2. 9 parallel review lenses: 6 specialized (data correctness, concurrency, contracts, security, typos, runtime) + 3 general
3. Cross-model ensemble for higher recall
4. Structural filter drops findings that reference files not in the diff
5. Validation pass confirms each finding against the actual code

```bash
# Submit a PR for review
curl -X POST https://inspect.ataraxy-labs.com/api/review \
  -H "Authorization: Bearer insp_..." \
  -H "Content-Type: application/json" \
  -d '{"repo":"owner/repo","pr_number":123}'

# Entity triage only (no LLM, returns in 1-3s)
curl -X POST https://inspect.ataraxy-labs.com/api/triage \
  -H "Authorization: Bearer insp_..." \
  -H "Content-Type: application/json" \
  -d '{"repo":"owner/repo","pr_number":123}'
```

Get an API key at [inspect.ataraxy-labs.com/dashboard/keys](https://inspect.ataraxy-labs.com/dashboard/keys).

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

TypeScript, TSX, JavaScript, Python, Go, Rust, Java, C, C++, Ruby, C#, PHP, Swift, Kotlin, Elixir, Bash, HCL/Terraform, Fortran, Vue

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
