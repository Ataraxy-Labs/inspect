import Nav from "@/components/nav";

export const metadata = { title: "Docs | inspect" };

export default function DocsPage() {
  return (
    <div className="container">
      <Nav active="docs" />

      <div style={{ padding: "48px 0 12px" }}>
        <h1
          style={{
            fontSize: 28,
            fontWeight: 700,
            color: "var(--accent)",
            letterSpacing: "-1px",
            marginBottom: 12,
          }}
        >
          Documentation
        </h1>
        <p
          style={{
            fontSize: 14,
            color: "var(--dim)",
            lineHeight: 1.7,
            maxWidth: 600,
          }}
        >
          Commands, change classification, risk scoring, and supported
          languages.
        </p>
      </div>

      {/* Commands */}
      <section>
        <h2>Commands</h2>
        <p className="section-desc">
          All commands support{" "}
          <code
            style={{
              color: "var(--cyan)",
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
            }}
          >
            --format json
          </code>{" "}
          and{" "}
          <code
            style={{
              color: "var(--cyan)",
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
            }}
          >
            --format markdown
          </code>{" "}
          for machine-readable output.
        </p>

        <div className="cmd-doc">
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">
              inspect diff &lt;ref&gt;
            </span>
            <span className="cmd-doc-desc">
              Review entity-level changes for a commit or range
            </span>
          </div>
          <div className="cmd-doc-flags">
            <div className="flag">
              <code>--context</code>{" "}
              <span>Show dependency details for each entity</span>
            </div>
            <div className="flag">
              <code>--min-risk &lt;level&gt;</code>{" "}
              <span>
                Filter by minimum risk (low, medium, high, critical)
              </span>
            </div>
            <div className="flag">
              <code>--format &lt;fmt&gt;</code>{" "}
              <span>terminal (default), json, or markdown</span>
            </div>
          </div>
        </div>

        <div className="cmd-doc">
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">
              inspect pr &lt;number&gt;
            </span>
            <span className="cmd-doc-desc">
              Review all changes in a GitHub pull request
            </span>
          </div>
        </div>

        <div className="cmd-doc">
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">
              inspect file &lt;path&gt;
            </span>
            <span className="cmd-doc-desc">
              Review uncommitted changes in a specific file
            </span>
          </div>
        </div>

        <div className="cmd-doc">
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">
              inspect bench --repo &lt;path&gt;
            </span>
            <span className="cmd-doc-desc">
              Benchmark entity-level review across a repo&apos;s commit
              history
            </span>
          </div>
        </div>
      </section>

      {/* Change Classification */}
      <section>
        <h2>Change classification</h2>
        <p className="section-desc">
          Based on{" "}
          <a
            href="https://arxiv.org/abs/2409.14121"
            style={{ color: "var(--cyan)" }}
          >
            ConGra (arXiv:2409.14121)
          </a>
          . Every change is classified along three dimensions: text, syntax,
          and functional.
        </p>
        <table>
          <thead>
            <tr>
              <th>Classification</th>
              <th>What changed</th>
              <th>Review needed?</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>
                <span style={{ color: "var(--green)" }}>Text</span>
              </td>
              <td>Comments, whitespace, docs only</td>
              <td className="lose">usually skip</td>
            </tr>
            <tr>
              <td>
                <span style={{ color: "var(--cyan)" }}>Syntax</span>
              </td>
              <td>Signatures, types, declarations (no logic)</td>
              <td className="mid">check API surface</td>
            </tr>
            <tr>
              <td>
                <span style={{ color: "var(--red)" }}>Functional</span>
              </td>
              <td>Logic or behavior</td>
              <td className="high">careful review</td>
            </tr>
            <tr>
              <td>
                <span style={{ color: "var(--fg)" }}>Mixed</span>
              </td>
              <td>Combinations of the above</td>
              <td className="high">careful review</td>
            </tr>
          </tbody>
        </table>
      </section>

      {/* Risk Scoring */}
      <section>
        <h2>Risk scoring</h2>
        <p className="section-desc">
          Graph-centric. Dependents and blast radius are the primary
          discriminators. Entities at the center of the dependency graph score
          highest. Cosmetic-only changes get an 80% discount.
        </p>

        <div className="flow">
          <div className="flow-step">
            <div
              className="flow-num"
              style={{ borderColor: "var(--red)", color: "var(--red)" }}
            >
              1
            </div>
            <div className="flow-content">
              <div className="title">Dependent count (primary)</div>
              <div className="desc">
                How many other entities call or reference this one. Logarithmic
                scale. An entity with 10 dependents scores significantly higher
                than one with 0.
              </div>
            </div>
          </div>
          <div className="flow-connector">
            <div className="line" />
          </div>
          <div className="flow-step">
            <div
              className="flow-num"
              style={{
                borderColor: "var(--orange)",
                color: "var(--orange)",
              }}
            >
              2
            </div>
            <div className="flow-content">
              <div className="title">Blast radius (primary)</div>
              <div className="desc">
                Transitive impact via BFS through the dependency graph.
                Normalized by repo size, sqrt-scaled.
              </div>
            </div>
          </div>
          <div className="flow-connector">
            <div className="line" />
          </div>
          <div className="flow-step">
            <div
              className="flow-num"
              style={{
                borderColor: "var(--green)",
                color: "var(--green)",
              }}
            >
              3
            </div>
            <div className="flow-content">
              <div className="title">Classification</div>
              <div className="desc">
                Functional changes score higher than syntax changes, which score
                higher than text-only changes.
              </div>
            </div>
          </div>
          <div className="flow-connector">
            <div className="line" />
          </div>
          <div className="flow-step">
            <div
              className="flow-num"
              style={{ borderColor: "var(--cyan)", color: "var(--cyan)" }}
            >
              4
            </div>
            <div className="flow-content">
              <div className="title">Public API</div>
              <div className="desc">
                Exported functions, pub methods, capitalized Go/Java names.
                Changes to public surface area are riskier.
              </div>
            </div>
          </div>
          <div className="flow-connector">
            <div className="line" />
          </div>
          <div className="flow-step">
            <div
              className="flow-num"
              style={{
                borderColor: "var(--purple)",
                color: "var(--purple)",
              }}
            >
              5
            </div>
            <div className="flow-content">
              <div className="title">Change type</div>
              <div className="desc">
                Deletions and modifications score higher than additions.
                Cosmetic changes (structural hash unchanged) get an 80%
                discount.
              </div>
            </div>
          </div>
        </div>

        <p
          style={{
            fontSize: 13,
            color: "var(--dim)",
            marginTop: 24,
            lineHeight: 1.7,
          }}
        >
          Risk levels:{" "}
          <span style={{ color: "var(--red)", fontWeight: 600 }}>
            Critical
          </span>{" "}
          (&gt;= 0.7) &middot;{" "}
          <span style={{ color: "var(--orange)", fontWeight: 600 }}>
            High
          </span>{" "}
          (&gt;= 0.5) &middot;{" "}
          <span style={{ color: "var(--yellow)", fontWeight: 600 }}>
            Medium
          </span>{" "}
          (&gt;= 0.3) &middot;{" "}
          <span style={{ color: "var(--dim)" }}>Low</span> (&lt; 0.3)
        </p>
      </section>

      {/* Languages */}
      <section>
        <h2>13 languages</h2>
        <p className="section-desc">
          Entity extraction powered by{" "}
          <a
            href="https://github.com/Ataraxy-Labs/sem"
            style={{ color: "var(--green)" }}
          >
            sem-core
          </a>{" "}
          and tree-sitter. All parsers compiled into the binary.
        </p>
        <table>
          <thead>
            <tr>
              <th>Language</th>
              <th>Extensions</th>
              <th>Entities</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Rust</td>
              <td>.rs</td>
              <td>functions, structs, enums, impls, traits</td>
            </tr>
            <tr>
              <td>TypeScript</td>
              <td>.ts .tsx</td>
              <td>functions, classes, interfaces, types, enums</td>
            </tr>
            <tr>
              <td>JavaScript</td>
              <td>.js .jsx .mjs .cjs</td>
              <td>functions, classes, variables</td>
            </tr>
            <tr>
              <td>Python</td>
              <td>.py</td>
              <td>functions, classes, decorators</td>
            </tr>
            <tr>
              <td>Go</td>
              <td>.go</td>
              <td>functions, methods, types</td>
            </tr>
            <tr>
              <td>Java</td>
              <td>.java</td>
              <td>classes, methods, interfaces, enums, fields</td>
            </tr>
            <tr>
              <td>C</td>
              <td>.c .h</td>
              <td>functions, structs, enums, unions, typedefs</td>
            </tr>
            <tr>
              <td>C++</td>
              <td>.cpp .cc .cxx .hpp</td>
              <td>functions, classes, structs, enums, namespaces</td>
            </tr>
            <tr>
              <td>Ruby</td>
              <td>.rb</td>
              <td>methods, classes, modules</td>
            </tr>
            <tr>
              <td>C#</td>
              <td>.cs</td>
              <td>methods, classes, interfaces, enums, structs</td>
            </tr>
            <tr>
              <td>PHP</td>
              <td>.php</td>
              <td>
                functions, classes, methods, interfaces, traits, enums
              </td>
            </tr>
            <tr>
              <td>Fortran</td>
              <td>.f90 .f95 .f03 .f08</td>
              <td>functions, subroutines, modules, programs</td>
            </tr>
          </tbody>
        </table>
      </section>

      {/* HTTP API */}
      <section>
        <h2>HTTP API</h2>
        <p className="section-desc">
          REST API for integrating inspect into CI pipelines, bots, and custom
          workflows. Submit a PR, get back findings. Uses the deep_v2 strategy:
          two-temperature LLM review with diff-aware validation.
        </p>

        <div className="cmd-doc">
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">POST /api/review</span>
            <span className="cmd-doc-desc">
              Submit a PR for review. Returns findings.
            </span>
          </div>
          <div className="terminal" style={{ marginTop: 12 }}>
            <div className="terminal-body" style={{ padding: "16px 20px" }}>
              <pre
                dangerouslySetInnerHTML={{
                  __html: `<span class="cmd">$ curl -X POST https://inspect-review.vercel.app/api/review \\</span>
<span class="cmd">    -H "Content-Type: application/json" \\</span>
<span class="cmd">    -d '{"repo":"owner/repo","pr_number":123}'</span>

<span class="d">// Response</span>
{
  <span class="c">"pr"</span>: { <span class="c">"number"</span>: 123, <span class="c">"title"</span>: <span class="g">"Fix auth bypass"</span>, ... },
  <span class="c">"findings"</span>: [
    {
      <span class="c">"issue"</span>: <span class="g">"Missing origin validation in CORS handler"</span>,
      <span class="c">"evidence"</span>: <span class="g">"if (origin.indexOf('example.com') !== -1)"</span>,
      <span class="c">"severity"</span>: <span class="r">"critical"</span>,
      <span class="c">"file"</span>: <span class="g">"src/middleware/cors.ts"</span>
    }
  ],
  <span class="c">"summary"</span>: { <span class="c">"total_findings"</span>: 3, <span class="c">"files_analyzed"</span>: 12 },
  <span class="c">"timing"</span>: { <span class="c">"triage_ms"</span>: 1200, <span class="c">"review_ms"</span>: 18000, <span class="c">"total_ms"</span>: 19200 }
}`,
                }}
              />
            </div>
          </div>
        </div>

        <div className="cmd-doc" style={{ marginTop: 24 }}>
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">POST /api/triage</span>
            <span className="cmd-doc-desc">
              File-level triage only. No LLM call. Returns in 1-3s.
            </span>
          </div>
          <div className="terminal" style={{ marginTop: 12 }}>
            <div className="terminal-body" style={{ padding: "16px 20px" }}>
              <pre
                dangerouslySetInnerHTML={{
                  __html: `<span class="cmd">$ curl -X POST https://inspect-review.vercel.app/api/triage \\</span>
<span class="cmd">    -H "Content-Type: application/json" \\</span>
<span class="cmd">    -d '{"repo":"owner/repo","pr_number":123}'</span>

<span class="d">// Response</span>
{
  <span class="c">"pr"</span>: { <span class="c">"number"</span>: 123, <span class="c">"title"</span>: <span class="g">"Fix auth bypass"</span> },
  <span class="c">"files_analyzed"</span>: 8,
  <span class="c">"files"</span>: [
    { <span class="c">"file"</span>: <span class="g">"src/auth.ts"</span>, <span class="c">"additions"</span>: 45, <span class="c">"deletions"</span>: 12 }
  ],
  <span class="c">"timing_ms"</span>: 1400
}`,
                }}
              />
            </div>
          </div>
        </div>

        <div className="cmd-doc" style={{ marginTop: 24 }}>
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">GET /api/health</span>
            <span className="cmd-doc-desc">Health check</span>
          </div>
          <div className="terminal" style={{ marginTop: 12 }}>
            <div className="terminal-body" style={{ padding: "16px 20px" }}>
              <pre
                dangerouslySetInnerHTML={{
                  __html: `<span class="cmd">$ curl https://inspect-review.vercel.app/api/health</span>
{<span class="c">"status"</span>: <span class="g">"ok"</span>}`,
                }}
              />
            </div>
          </div>
        </div>
      </section>

      {/* MCP Server */}
      <section>
        <h2>MCP server</h2>
        <p className="section-desc">
          inspect ships an MCP server so any coding agent (Claude Code, Cursor,
          etc.) can use entity-level review as a tool. Build with{" "}
          <code
            style={{
              color: "var(--cyan)",
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
            }}
          >
            cargo build -p inspect-mcp
          </code>
          .
        </p>

        <table>
          <thead>
            <tr>
              <th>Tool</th>
              <th>Purpose</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>
                <code style={{ color: "var(--cyan)" }}>inspect_triage</code>
              </td>
              <td>
                Primary entry point. Full analysis sorted by risk with verdict.
              </td>
            </tr>
            <tr>
              <td>
                <code style={{ color: "var(--cyan)" }}>inspect_entity</code>
              </td>
              <td>
                Drill into one entity: before/after content, dependents,
                dependencies.
              </td>
            </tr>
            <tr>
              <td>
                <code style={{ color: "var(--cyan)" }}>inspect_group</code>
              </td>
              <td>Get all entities in a logical change group.</td>
            </tr>
            <tr>
              <td>
                <code style={{ color: "var(--cyan)" }}>inspect_file</code>
              </td>
              <td>Scope review to a single file.</td>
            </tr>
            <tr>
              <td>
                <code style={{ color: "var(--cyan)" }}>inspect_stats</code>
              </td>
              <td>
                Lightweight summary: stats, verdict, timing. No entity details.
              </td>
            </tr>
            <tr>
              <td>
                <code style={{ color: "var(--cyan)" }}>inspect_risk_map</code>
              </td>
              <td>
                File-level risk heatmap with per-file aggregate scores.
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      {/* Review Verdict */}
      <section>
        <h2>Review verdict</h2>
        <p className="section-desc">
          Returned by{" "}
          <code
            style={{
              color: "var(--cyan)",
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
            }}
          >
            inspect_triage
          </code>{" "}
          and{" "}
          <code
            style={{
              color: "var(--cyan)",
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
            }}
          >
            inspect_stats
          </code>
          . A quick signal for agents and humans.
        </p>

        <table>
          <thead>
            <tr>
              <th>Verdict</th>
              <th>Meaning</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>
                <span style={{ color: "var(--green)" }}>
                  likely_approvable
                </span>
              </td>
              <td>
                All changes are cosmetic (comments, whitespace, formatting)
              </td>
            </tr>
            <tr>
              <td>
                <span style={{ color: "var(--fg)" }}>standard_review</span>
              </td>
              <td>Normal changes, no high-risk entities</td>
            </tr>
            <tr>
              <td>
                <span style={{ color: "var(--orange)" }}>
                  requires_review
                </span>
              </td>
              <td>High-risk entities present</td>
            </tr>
            <tr>
              <td>
                <span style={{ color: "var(--red)" }}>
                  requires_careful_review
                </span>
              </td>
              <td>Critical-risk entities present</td>
            </tr>
          </tbody>
        </table>
      </section>

      {/* Installation */}
      <section>
        <h2>Installation</h2>
        <p className="section-desc">
          Rust toolchain required. Single binary, no runtime dependencies.
        </p>

        <div className="cmd-doc">
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">From source</span>
          </div>
          <div className="terminal" style={{ marginTop: 12 }}>
            <div className="terminal-body" style={{ padding: "16px 20px" }}>
              <pre
                dangerouslySetInnerHTML={{
                  __html: `<span class="cmd">$ cargo install --git https://github.com/Ataraxy-Labs/inspect inspect-cli</span>`,
                }}
              />
            </div>
          </div>
        </div>

        <div className="cmd-doc" style={{ marginTop: 24 }}>
          <div className="cmd-doc-header">
            <span className="cmd-doc-name">Build from repo</span>
          </div>
          <div className="terminal" style={{ marginTop: 12 }}>
            <div className="terminal-body" style={{ padding: "16px 20px" }}>
              <pre
                dangerouslySetInnerHTML={{
                  __html: `<span class="cmd">$ git clone https://github.com/Ataraxy-Labs/inspect</span>
<span class="cmd">$ cd inspect && cargo build --release</span>
<span class="cmd">$ ./target/release/inspect diff HEAD~1</span>`,
                }}
              />
            </div>
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
