"use client";

import { useEffect } from "react";
import Nav from "@/components/nav";

export default function BenchmarksPage() {
  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const bars = entry.target.querySelectorAll<HTMLElement>(
              ".bench-bar[data-width]"
            );
            bars.forEach((bar, i) => {
              setTimeout(() => {
                bar.style.width = bar.dataset.width + "%";
              }, i * 80);
            });
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.2 }
    );

    document.querySelectorAll(".bench-group").forEach((group) => {
      observer.observe(group);
    });

    return () => observer.disconnect();
  }, []);

  return (
    <div className="container">
      <Nav active="benchmarks" />

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
          Benchmarks
        </h1>
        <p
          style={{
            fontSize: 14,
            color: "var(--dim)",
            lineHeight: 1.7,
            maxWidth: 600,
          }}
        >
          inspect + LLM evaluated on the same benchmarks used to measure
          frontier code review tools. Entity-level triage focuses the LLM on
          the code that matters.
        </p>
      </div>

      {/* Greptile Benchmark (primary) */}
      <section>
        <h2>Greptile Benchmark (141 planted bugs, 52 PRs, 5 repos)</h2>
        <p className="section-desc">
          <a
            href="https://huggingface.co/datasets/rs545837/inspect-greptile-bench"
            style={{ color: "var(--cyan)" }}
          >
            Dataset on HuggingFace
          </a>
          . 141 bugs planted across Sentry, Cal.com, Grafana, Keycloak, and
          Discourse by the Greptile team. Same heuristic keyword-matching judge
          applied identically to all three tools.
        </p>

        <div className="stat-cards">
          <div className="stat-card" style={{ borderColor: "var(--green)" }}>
            <div className="stat-value" style={{ color: "var(--green)" }}>
              95.0%
            </div>
            <div className="stat-label">recall (inspect + GPT-5.2)</div>
            <div className="stat-detail">134 of 141 bugs found</div>
          </div>
          <div className="stat-card" style={{ borderColor: "var(--cyan)" }}>
            <div className="stat-value" style={{ color: "var(--cyan)" }}>
              100%
            </div>
            <div className="stat-label">HC recall</div>
            <div className="stat-detail">every high/critical bug caught</div>
          </div>
          <div className="stat-card" style={{ borderColor: "var(--purple)" }}>
            <div className="stat-value" style={{ color: "var(--purple)" }}>
              49.4%
            </div>
            <div className="stat-label">F1 score</div>
            <div className="stat-detail">
              +14pp over Greptile (35.3%)
            </div>
          </div>
        </div>

        <div className="comparison-table">
          <table>
            <thead>
              <tr>
                <th>Tool</th>
                <th>Recall</th>
                <th>Precision</th>
                <th>F1</th>
                <th>HC Recall</th>
                <th>Findings</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  <strong style={{ color: "var(--accent)" }}>
                    inspect + GPT-5.2
                  </strong>
                </td>
                <td className="win">95.0%</td>
                <td>33.3%</td>
                <td>49.4%</td>
                <td className="win">100%</td>
                <td>402</td>
              </tr>
              <tr>
                <td>Greptile (API)</td>
                <td>91.5%</td>
                <td>21.9%</td>
                <td>35.3%</td>
                <td>94.1%</td>
                <td>590</td>
              </tr>
              <tr>
                <td>CodeRabbit (CLI)</td>
                <td>56.0%</td>
                <td className="win">48.2%</td>
                <td className="win">51.8%</td>
                <td>60.8%</td>
                <td>164</td>
              </tr>
            </tbody>
          </table>
        </div>

        {/* Recall bars */}
        <div className="bench-group" style={{ marginTop: 24 }}>
          <h3>Recall (all severities)</h3>
          {[
            { name: <><strong style={{ color: "var(--accent)" }}>inspect + GPT-5.2</strong></>, value: "95.0%", valueColor: "var(--green)", width: 95, cls: "inspect-bar" },
            { name: "Greptile (API)", value: "91.5%", valueColor: undefined, width: 91.5, cls: "other-bar" },
            { name: "CodeRabbit (CLI)", value: "56.0%", valueColor: undefined, width: 56, cls: "dim-bar" },
          ].map((row, i) => (
            <div className="bench-row" key={i}>
              <div className="bench-label">
                <span className="name">{row.name}</span>
                <span className="value" style={row.valueColor ? { color: row.valueColor } : undefined}>
                  {row.value}
                </span>
              </div>
              <div className="bench-bar-track">
                <div className={`bench-bar ${row.cls}`} data-width={row.width}>
                  {row.value}
                </div>
              </div>
            </div>
          ))}
          <div className="bench-note">
            141 golden comments, 52 PRs, same judge
          </div>
        </div>

        {/* HC Recall bars */}
        <div className="bench-group" style={{ marginTop: 24 }}>
          <h3>HC Recall (High + Critical only)</h3>
          {[
            { name: <><strong style={{ color: "var(--accent)" }}>inspect + GPT-5.2</strong></>, value: "100%", valueColor: "var(--green)", width: 100, cls: "inspect-bar" },
            { name: "Greptile (API)", value: "94.1%", valueColor: undefined, width: 94.1, cls: "other-bar" },
            { name: "CodeRabbit (CLI)", value: "60.8%", valueColor: undefined, width: 60.8, cls: "dim-bar" },
          ].map((row, i) => (
            <div className="bench-row" key={i}>
              <div className="bench-label">
                <span className="name">{row.name}</span>
                <span className="value" style={row.valueColor ? { color: row.valueColor } : undefined}>
                  {row.value}
                </span>
              </div>
              <div className="bench-bar-track">
                <div className={`bench-bar ${row.cls}`} data-width={row.width}>
                  {row.value}
                </div>
              </div>
            </div>
          ))}
          <div className="bench-note">
            51 high/critical bugs. CodeRabbit misses 39% of them.
          </div>
        </div>

        {/* Per-severity recall */}
        <h3 style={{ fontSize: 15, color: "var(--accent)", margin: "24px 0 16px" }}>
          Per-severity recall
        </h3>
        <div className="comparison-table">
          <table>
            <thead>
              <tr>
                <th>Severity</th>
                <th>inspect</th>
                <th>Greptile</th>
                <th>CodeRabbit</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Critical (n=9)</td>
                <td className="win">100%</td>
                <td className="win">100%</td>
                <td>55.6%</td>
              </tr>
              <tr>
                <td>High (n=42)</td>
                <td className="win">100%</td>
                <td>92.9%</td>
                <td>61.9%</td>
              </tr>
              <tr>
                <td>Medium (n=49)</td>
                <td className="win">91.8%</td>
                <td>87.8%</td>
                <td>61.2%</td>
              </tr>
              <tr>
                <td>Low (n=41)</td>
                <td className="win">92.7%</td>
                <td className="win">92.7%</td>
                <td>43.9%</td>
              </tr>
            </tbody>
          </table>
        </div>

        {/* Per-repo recall */}
        <h3 style={{ fontSize: 15, color: "var(--accent)", margin: "24px 0 16px" }}>
          Per-repo recall
        </h3>
        <div className="comparison-table">
          <table>
            <thead>
              <tr>
                <th>Repo</th>
                <th>inspect</th>
                <th>Greptile</th>
                <th>CodeRabbit</th>
              </tr>
            </thead>
            <tbody>
              <tr><td>Cal.com (n=31)</td><td>96.8%</td><td className="win">100%</td><td>77.4%</td></tr>
              <tr><td>Discourse (n=28)</td><td className="win">100%</td><td className="win">100%</td><td>67.9%</td></tr>
              <tr><td>Grafana (n=22)</td><td className="win">90.9%</td><td className="win">90.9%</td><td>36.4%</td></tr>
              <tr><td>Keycloak (n=26)</td><td className="win">100%</td><td>96.2%</td><td>65.4%</td></tr>
              <tr><td>Sentry (n=34)</td><td className="win">88.2%</td><td>73.5%</td><td>32.4%</td></tr>
            </tbody>
          </table>
        </div>

        <p style={{ fontSize: 12, color: "var(--dim2)", marginTop: 12, lineHeight: 1.7 }}>
          inspect + LLM reviews top 60 entities per PR (round-robin by file,
          sorted by risk score) + 5-file gap review for uncovered diff. Top 15
          findings per PR by confidence. 10 concurrent LLM calls. Entity-level
          dedup (20-line window + identifier overlap). All tools judged with the
          same keyword-matching heuristic. Greptile via their production API.
          CodeRabbit via their free-tier CLI (rate limited, ~1 review per 8 min).
        </p>
      </section>

      {/* AACR-Bench */}
      <section>
        <h2>AACR-Bench (166 golden comments, 20 PRs, 9 languages)</h2>
        <p className="section-desc">
          <a href="https://arxiv.org/abs/2601.19494" style={{ color: "var(--cyan)" }}>
            AACR-Bench
          </a>{" "}
          is the benchmark used to evaluate GPT-5.2, Claude 4.5 Sonnet, and
          other frontier LLMs for automated code review. We ran all three tools
          on 20 diverse PRs.
        </p>

        <div className="stat-cards">
          <div className="stat-card" style={{ borderColor: "var(--green)" }}>
            <div className="stat-value" style={{ color: "var(--green)" }}>30.1%</div>
            <div className="stat-label">recall (inspect + GPT-5.2)</div>
            <div className="stat-detail">1.3x Greptile, 2.3x CodeRabbit</div>
          </div>
          <div className="stat-card" style={{ borderColor: "var(--cyan)" }}>
            <div className="stat-value" style={{ color: "var(--cyan)" }}>22.7%</div>
            <div className="stat-label">precision</div>
            <div className="stat-detail">highest of all three tools</div>
          </div>
          <div className="stat-card" style={{ borderColor: "var(--purple)" }}>
            <div className="stat-value" style={{ color: "var(--purple)" }}>25.9%</div>
            <div className="stat-label">F1 score</div>
            <div className="stat-detail">beats Greptile (22.5%) and CodeRabbit (23.8%)</div>
          </div>
        </div>

        <div className="comparison-table">
          <table>
            <thead>
              <tr>
                <th>Tool</th>
                <th>Recall</th>
                <th>Precision</th>
                <th>F1</th>
                <th>Findings</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td><strong style={{ color: "var(--accent)" }}>inspect + GPT-5.2</strong></td>
                <td className="win">30.1%</td>
                <td className="win">22.7%</td>
                <td className="win">25.9%</td>
                <td>220</td>
              </tr>
              <tr>
                <td>Greptile (API)</td>
                <td>23.5%</td>
                <td>21.7%</td>
                <td>22.5%</td>
                <td>180</td>
              </tr>
              <tr>
                <td>CodeRabbit (CLI)</td>
                <td>13.3%</td>
                <td>115.8%*</td>
                <td>23.8%</td>
                <td>19</td>
              </tr>
            </tbody>
          </table>
          <p style={{ fontSize: 11, color: "var(--dim2)", marginTop: 6 }}>
            *CodeRabbit&apos;s precision exceeds 100% because multiple golden
            comments matched the same finding (19 findings caught 22 issues).
          </p>
        </div>

        <div className="bench-group" style={{ marginTop: 24 }}>
          <h3>Recall comparison</h3>
          {[
            { name: <><strong style={{ color: "var(--accent)" }}>inspect + GPT-5.2</strong></>, value: "30.1%", valueColor: "var(--green)", width: 30.1, cls: "inspect-bar" },
            { name: "Greptile (API)", value: "23.5%", valueColor: undefined, width: 23.5, cls: "other-bar" },
            { name: "CodeRabbit (CLI)", value: "13.3%", valueColor: undefined, width: 13.3, cls: "dim-bar" },
          ].map((row, i) => (
            <div className="bench-row" key={i}>
              <div className="bench-label">
                <span className="name">{row.name}</span>
                <span className="value" style={row.valueColor ? { color: row.valueColor } : undefined}>
                  {row.value}
                </span>
              </div>
              <div className="bench-bar-track">
                <div className={`bench-bar ${row.cls}`} data-width={row.width}>
                  {row.value}
                </div>
              </div>
            </div>
          ))}
          <div className="bench-note">
            20 PRs, 166 golden comments, same judge
          </div>
        </div>

        <p style={{ fontSize: 12, color: "var(--dim2)", marginTop: 8, lineHeight: 1.7 }}>
          20 diverse PRs from AACR-Bench (round-robin across repos). Same
          keyword-matching judge for all tools. Top 15 findings per PR by
          confidence.
        </p>
      </section>

      {/* How it works */}
      <section>
        <h2>How it works</h2>
        <p className="section-desc">
          inspect runs entity-level triage locally (free, &lt;1s), then sends
          the top entities to an LLM for focused review. The LLM sees
          entity-scoped code with before/after context, not a raw diff.
        </p>

        <p style={{ fontSize: 13, color: "var(--dim)", marginTop: 20, lineHeight: 1.7 }}>
          <strong style={{ color: "var(--fg)" }}>The pipeline.</strong> Run
          inspect first (free, &lt;1s) to get entity risk rankings. Send top 30
          entities to an LLM with before/after code + file diff. Gap-review
          uncovered files. Dedup and filter by confidence. Top 15 findings per
          PR. The triage step means the LLM sees focused code, not an entire
          diff.
        </p>
      </section>

      {/* Speed */}
      <section>
        <h2>Speed</h2>
        <p className="section-desc">
          Entity extraction, dependency graph, change classification, risk
          scoring, commit untangling. All local, no API calls.
        </p>

        <h3 style={{ fontSize: 15, color: "var(--accent)", marginBottom: 16 }}>
          Single commit review
        </h3>
        <p style={{ fontSize: 13, color: "var(--dim)", marginBottom: 16, lineHeight: 1.7 }}>
          Time to run{" "}
          <code
            style={{
              background: "var(--surface)",
              padding: "2px 6px",
              borderRadius: 3,
              fontSize: 12,
              color: "var(--cyan)",
            }}
          >
            inspect diff HEAD~1
          </code>{" "}
          on a real commit. 30 runs via hyperfine, warm cache.
        </p>

        <div className="comparison-table">
          <table>
            <thead>
              <tr>
                <th>Repo</th>
                <th>Size</th>
                <th>Time</th>
              </tr>
            </thead>
            <tbody>
              <tr><td>sem</td><td>25 files, 65 entities changed</td><td className="win">6ms</td></tr>
              <tr><td>weave</td><td>80 files, 89 entities changed</td><td className="win">6ms</td></tr>
              <tr><td>inspect</td><td>50 files, large commit</td><td className="win">67ms</td></tr>
            </tbody>
          </table>
        </div>

        <h3 style={{ fontSize: 15, color: "var(--accent)", margin: "24px 0 16px" }}>
          Full repo history (inspect bench)
        </h3>
        <p style={{ fontSize: 13, color: "var(--dim)", marginBottom: 16, lineHeight: 1.7 }}>
          Time to analyze every commit in a repo&apos;s history: extract
          entities, build graph, classify changes, score risk, untangle.
        </p>

        <div className="comparison-table">
          <table>
            <thead>
              <tr>
                <th>Repo</th>
                <th>Commits</th>
                <th>Entities</th>
                <th>Wall time</th>
              </tr>
            </thead>
            <tbody>
              <tr><td>sem</td><td>38</td><td>5,216</td><td className="win">0.57s</td></tr>
              <tr><td>weave</td><td>45</td><td>2,854</td><td className="win">1.33s</td></tr>
            </tbody>
          </table>
        </div>

        <div className="bench-group" style={{ marginTop: 32 }}>
          <h3>Single commit review time (visual)</h3>
          {[
            { name: "sem (25 files)", value: "6ms", valueColor: "var(--green)", width: 9, cls: "inspect-bar" },
            { name: "weave (80 files)", value: "6ms", valueColor: "var(--green)", width: 9, cls: "inspect-bar" },
            { name: "inspect (50 files)", value: "67ms", valueColor: "var(--green)", width: 100, cls: "inspect-bar" },
          ].map((row, i) => (
            <div className="bench-row" key={i}>
              <div className="bench-label">
                <span className="name">{row.name}</span>
                <span className="value" style={{ color: row.valueColor }}>{row.value}</span>
              </div>
              <div className="bench-bar-track">
                <div className={`bench-bar ${row.cls}`} data-width={row.width}>{row.value}</div>
              </div>
            </div>
          ))}
          <div className="bench-note">30 runs via hyperfine -N, warm cache</div>
        </div>

        <p style={{ fontSize: 13, color: "var(--dim)", marginTop: 20, lineHeight: 1.7 }}>
          Powered by{" "}
          <a href="https://ataraxy-labs.com/sem" style={{ color: "var(--cyan)" }}>
            sem-core
          </a>{" "}
          v0.3.0: xxHash64 structural hashing, parallel tree-sitter parsing via
          rayon, cached git tree resolution, LTO-optimized release builds.
        </p>
      </section>

      <footer>
        <p>
          Built by <a href="https://ataraxy-labs.com">Ataraxy Labs</a>
        </p>
      </footer>
    </div>
  );
}
