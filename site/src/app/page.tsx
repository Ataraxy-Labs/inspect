"use client";

import Nav from "@/components/nav";

function copyCmd(el: HTMLElement, cmd: string) {
  navigator.clipboard.writeText(cmd);
  const copied = el.querySelector(".copied") as HTMLElement | null;
  if (copied) {
    copied.classList.add("show");
    setTimeout(() => copied.classList.remove("show"), 1500);
  }
}

export default function HomePage() {
  return (
    <div className="container">
      <Nav active="home" />

      {/* Header */}
      <div style={{ padding: "48px 0 32px", textAlign: "center" }}>
        <h1
          style={{
            fontSize: 36,
            fontWeight: 700,
            color: "var(--accent)",
            letterSpacing: "-1.5px",
            marginBottom: 12,
          }}
        >
          Review intent, not diffs.
        </h1>
        <p
          style={{
            fontSize: 14,
            color: "var(--dim)",
            marginBottom: 24,
            lineHeight: 1.7,
            maxWidth: 540,
            marginLeft: "auto",
            marginRight: "auto",
          }}
        >
          Entity-level code review for Git. Know if a change is a rename,
          a signature change, or a new dependency before reading a single line.
          Graph-based risk scoring tells you what's dangerous vs cosmetic.
          No LLM, no API key. 95% recall on the Greptile benchmark. 5-67ms per commit.
        </p>
        <div
          className="install-box"
          onClick={(e) =>
            copyCmd(
              e.currentTarget,
              "cargo install --git https://github.com/Ataraxy-Labs/inspect inspect-cli"
            )
          }
          style={{
            display: "inline-block",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            padding: "10px 20px",
            fontSize: 14,
            color: "var(--fg)",
            cursor: "pointer",
            position: "relative",
          }}
        >
          <span
            className="copied"
            style={{
              position: "absolute",
              top: -28,
              left: "50%",
              transform: "translateX(-50%)",
              fontSize: 12,
              color: "var(--green)",
              opacity: 0,
              transition: "opacity 0.2s",
            }}
          >
            copied
          </span>
          <span style={{ color: "var(--dim)" }}>$</span> cargo install --git
          https://github.com/Ataraxy-Labs/inspect inspect-cli
        </div>
      </div>

      {/* Terminal demo */}
      <div className="terminal">
        <div className="terminal-bar">
          <div className="terminal-dot" />
          <div className="terminal-dot" />
          <div className="terminal-dot" />
          <div className="terminal-title">~/project</div>
        </div>
        <div className="terminal-body">
          <pre
            dangerouslySetInnerHTML={{
              __html: `<span class="cmd">$ inspect diff HEAD~1</span>

<span class="w">inspect</span> 12 entities changed
  <span class="r">1 critical</span>, <span class="o">4 high</span>, <span class="y">3 medium</span>, <span class="d">4 low</span>

<span class="w">groups</span> 3 logical groups:
  <span class="d">[0]</span> src/merge/ <span class="d">(5 entities)</span>
  <span class="d">[1]</span> src/driver/ <span class="d">(4 entities)</span>
  <span class="d">[2]</span> validate <span class="d">(3 entities)</span>

<span class="w">entities</span> <span class="d">(by risk):</span>

  <span class="y">~</span> <span class="r">CRITICAL</span> <span class="d">function</span> <span class="w">merge_entities</span> <span class="d">(src/merge/core.rs)</span>
    classification: functional  score: 0.82  blast: 171  deps: 3/12
    <span class="c">public API</span>
    <span class="r">&gt;&gt;&gt; 12 dependents may be affected</span>

  <span class="r">-</span> <span class="o">HIGH</span> <span class="d">function</span> <span class="w">old_validate</span> <span class="d">(src/validate.rs)</span>
    classification: functional  score: 0.65  blast: 8  deps: 0/3
    <span class="c">public API</span>

  <span class="g">+</span> <span class="y">MEDIUM</span> <span class="d">function</span> <span class="w">parse_config</span> <span class="d">(src/config.rs)</span>
    classification: functional  score: 0.32  blast: 0  deps: 2/0

  <span class="y">~</span> <span class="d">LOW</span> <span class="d">function</span> <span class="w">format_output</span> <span class="d">(src/display.rs)</span>
    classification: text  score: 0.02  blast: 0  deps: 0/0
    <span class="d">cosmetic only (no structural change)</span>`,
            }}
          />
        </div>
      </div>

      {/* The Problem */}
      <section>
        <h2>The problem</h2>
        <p className="section-desc">
          <code
            style={{
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
              color: "var(--cyan)",
            }}
          >
            git diff
          </code>{" "}
          says 12 files changed. But which changes actually matter? A renamed
          variable, a reformatted function, and a deleted public API method all
          look the same in a line-level diff.
        </p>
        <p
          style={{
            fontSize: 14,
            color: "var(--dim)",
            lineHeight: 1.7,
          }}
        >
          This gets worse with AI-generated code. DORA 2025 found that AI
          adoption led to{" "}
          <strong style={{ color: "var(--accent)" }}>+154% PR size</strong>,{" "}
          <strong style={{ color: "var(--accent)" }}>+91% review time</strong>,
          and{" "}
          <strong style={{ color: "var(--accent)" }}>
            +9% more bugs shipped
          </strong>
          . Reviewers are drowning in noise. inspect works at the entity level:
          functions, structs, traits, classes. It uses the dependency graph to
          identify which changes have real impact.
        </p>
      </section>

      {/* How it works */}
      <section>
        <h2>How it works</h2>
        <p className="section-desc">
          Four phases. No LLM, no network calls, all local.
        </p>

        <div className="phase-cards">
          <div className="phase-card" style={{ borderColor: "var(--green)" }}>
            <div
              className="tag"
              style={{ background: "#4ade8022", color: "var(--green)" }}
            >
              EXTRACT
            </div>
            <h3>Parse</h3>
            <p>
              tree-sitter extracts entities from all tracked source files. Builds
              a full-repo dependency graph via call/reference analysis.
            </p>
          </div>
          <div className="phase-card" style={{ borderColor: "var(--cyan)" }}>
            <div
              className="tag"
              style={{ background: "#22d3ee22", color: "var(--cyan)" }}
            >
              CLASSIFY
            </div>
            <h3>Categorize</h3>
            <p>
              Compare before/after. Classify each change as text (comments),
              syntax (signatures), functional (logic), or a combination.
            </p>
          </div>
          <div className="phase-card" style={{ borderColor: "var(--yellow)" }}>
            <div
              className="tag"
              style={{ background: "#facc1522", color: "var(--yellow)" }}
            >
              SCORE
            </div>
            <h3>Risk</h3>
            <p>
              Graph-centric scoring. Dependents and blast radius are the primary
              signals. Public API, classification, and change type set the
              baseline.
            </p>
          </div>
          <div className="phase-card" style={{ borderColor: "var(--purple)" }}>
            <div
              className="tag"
              style={{ background: "#a78bfa22", color: "var(--purple)" }}
            >
              GROUP
            </div>
            <h3>Untangle</h3>
            <p>
              Union-Find on dependency edges between changed entities. Separates
              independent logical changes within tangled commits.
            </p>
          </div>
        </div>
      </section>

      {/* Key numbers */}
      <section>
        <h2>Results</h2>
        <p className="section-desc">
          Evaluated on{" "}
          <a
            href="https://arxiv.org/abs/2601.19494"
            style={{ color: "var(--cyan)" }}
          >
            AACR-Bench
          </a>
          . 158 PRs, 50 repos, 10 languages, 1,169 ground truth issues from
          human reviewers.
        </p>

        <div className="stat-cards">
          <div className="stat-card" style={{ borderColor: "var(--green)" }}>
            <div className="stat-value" style={{ color: "var(--green)" }}>
              48%
            </div>
            <div className="stat-label">recall (High/Critical only)</div>
            <div className="stat-detail">reviewing 9.5% of the diff</div>
          </div>
          <div className="stat-card" style={{ borderColor: "var(--cyan)" }}>
            <div className="stat-value" style={{ color: "var(--cyan)" }}>
              78%
            </div>
            <div className="stat-label">
              recall (High/Critical + Medium)
            </div>
            <div className="stat-detail">reviewing 19% of the diff</div>
          </div>
          <div className="stat-card" style={{ borderColor: "var(--purple)" }}>
            <div className="stat-value" style={{ color: "var(--purple)" }}>
              82%
            </div>
            <div className="stat-label">total coverage</div>
            <div className="stat-detail">
              issues within any changed entity
            </div>
          </div>
        </div>

        <p
          style={{
            fontSize: 14,
            color: "var(--dim)",
            lineHeight: 1.7,
            textAlign: "center",
          }}
        >
          83.5% High/Critical recall on the{" "}
          <a
            href="https://www.greptile.com/benchmarks"
            style={{ color: "var(--cyan)" }}
          >
            Greptile benchmark
          </a>{" "}
          (50 PRs, 5 repos, 97 golden comments), beating every LLM-based tool
          at zero cost. 100% recall at the Medium threshold.{" "}
          <a href="/benchmarks" style={{ color: "var(--cyan)" }}>
            Full benchmark results {"\u2192"}
          </a>
        </p>
      </section>

      {/* Part of the stack */}
      <section>
        <h2>Part of the Ataraxy Labs stack</h2>
        <p className="section-desc">
          Three tools, same foundation: sem-core&apos;s entity extraction and
          structural hashing.
        </p>

        <div
          className="phase-cards"
          style={{ gridTemplateColumns: "1fr 1fr 1fr" }}
        >
          <div className="phase-card" style={{ borderColor: "var(--green)" }}>
            <h3>
              <a
                href="https://github.com/Ataraxy-Labs/sem"
                style={{ color: "var(--green)" }}
              >
                sem
              </a>
            </h3>
            <p>
              Understand code history. What changed, who changed it, what
              depends on it, what might break.
            </p>
          </div>
          <div className="phase-card" style={{ borderColor: "var(--cyan)" }}>
            <h3>
              <a
                href="https://github.com/Ataraxy-Labs/weave"
                style={{ color: "var(--cyan)" }}
              >
                weave
              </a>
            </h3>
            <p>
              Merge without false conflicts. 31/31 clean merges on concurrent
              edit scenarios vs Git&apos;s 15/31.
            </p>
          </div>
          <div className="phase-card" style={{ borderColor: "var(--orange)" }}>
            <h3>inspect</h3>
            <p>
              Review what matters. Graph-based risk scoring, change
              classification, commit untangling.
            </p>
          </div>
        </div>
      </section>

      <footer>
        <p>
          Built by <a href="https://ataraxy-labs.com">Ataraxy Labs</a>
        </p>
      </footer>
    </div>
  );
}
