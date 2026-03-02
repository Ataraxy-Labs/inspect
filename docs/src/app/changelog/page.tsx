import Nav from "@/components/nav";

export const metadata = { title: "Changelog | inspect" };

export default function ChangelogPage() {
  return (
    <div className="container">
      <Nav active="changelog" />

      <section style={{ borderTop: "none" }}>
        <h2>Changelog</h2>
        <p className="section-desc">
          How inspect evolved, including the mistakes.
        </p>

        <div className="changelog-entry">
          <div className="changelog-title">
            <span className="changelog-sha">6b2e4fd</span>
            <span className="changelog-tag">feat</span>
            MCP server, review verdict, markdown formatter
          </div>
          <p className="changelog-body">
            New <code style={{ color: "var(--fg)" }}>inspect-mcp</code> crate: 6
            MCP tools so any coding agent can use entity-level review.{" "}
            <code style={{ color: "var(--fg)" }}>inspect_triage</code> is the
            primary entry point, returning entities sorted by risk with a
            verdict.{" "}
            <code style={{ color: "var(--fg)" }}>inspect_entity</code> lets
            agents drill into one entity with before/after content, dependents,
            and dependencies. Plus{" "}
            <code style={{ color: "var(--fg)" }}>inspect_group</code>,{" "}
            <code style={{ color: "var(--fg)" }}>inspect_file</code>,{" "}
            <code style={{ color: "var(--fg)" }}>inspect_stats</code>, and{" "}
            <code style={{ color: "var(--fg)" }}>inspect_risk_map</code>. Added{" "}
            <code style={{ color: "var(--fg)" }}>ReviewVerdict</code> (4 levels:
            LikelyApprovable, StandardReview, RequiresReview,
            RequiresCarefulReview) as a quick signal for agents. New{" "}
            <code style={{ color: "var(--fg)" }}>--format markdown</code> for all
            commands. Extended{" "}
            <code style={{ color: "var(--fg)" }}>EntityReview</code> with
            before/after content and dependency names for drill-down.
          </p>
          <p className="changelog-stats">
            +820 lines &bull; 12 files &bull; 3 crates (up from 2) &bull; 12
            tests passing
          </p>
          <div className="changelog-mistake">
            <strong>Lesson: cache the analysis, not the tools.</strong> MCP
            tools like inspect_entity and inspect_group are drill-downs into the
            same analysis that inspect_triage already computed. Re-running the
            full 4-phase pipeline for each tool call would multiply latency by
            6x. We cache the ReviewResult keyed by (repo_path, target) so
            sequential drill-downs are instant. The expensive work happens once
            in triage; everything else is just a filtered view.
          </div>
        </div>

        <div className="changelog-entry">
          <div className="changelog-title">
            <span className="changelog-sha">33c94fb</span>
            <span className="changelog-tag">bench</span>
            Greptile benchmark: 83.5% HC recall
          </div>
          <p className="changelog-body">
            Ran inspect against the full{" "}
            <a
              href="https://www.greptile.com/benchmarks"
              style={{ color: "var(--cyan)" }}
            >
              Greptile benchmark
            </a>
            : 50 PRs, 5 repos, 97 golden comments from human reviewers. 83.5%
            High/Critical recall. 100% recall at the Medium threshold, meaning
            every golden comment fell within a flagged entity. Per-repo:
            Keycloak (Java) 100%, Cal.com (TypeScript) 91%, Grafana (Go) 81%,
            Discourse (Ruby) 79%, Sentry (Python) 67%. Beats Augment (55%),
            Greptile (45%), CodeRabbit (43%), Cursor (41%), and Copilot (34%).
            No LLM, no API key, runs locally in milliseconds.
          </p>
          <p className="changelog-stats">
            50 PRs &bull; 5 repos &bull; 97 golden comments &bull; 83.5% HC
            recall &bull; zero cost
          </p>
          <div className="changelog-mistake">
            <strong>Lesson: the graph is the moat.</strong> LLM-based tools look
            at code content. inspect looks at the dependency graph. A function
            that 12 other entities depend on is risky regardless of how the diff
            looks. This is why a zero-cost static tool beats tools backed by
            frontier models: the signal comes from structure, not language
            understanding. Content tells you what changed; the graph tells you
            what matters.
          </div>
        </div>

        <div className="changelog-entry">
          <div className="changelog-title">
            <span className="changelog-sha">9731d47</span>
            <span className="changelog-tag">rewrite</span>
            Graph-centric risk scoring
          </div>
          <p className="changelog-body">
            Rewrote the risk scoring formula. Previously, classification was the
            primary signal: functional changes scored high, text changes scored
            low, regardless of context. Now dependents and blast radius are the
            primary discriminators. A functional change to a leaf function with
            zero dependents scores Medium. A syntax change to a hub function
            with 50 dependents scores High. Cosmetic discount increased from
            0.7x to 0.2x.
          </p>
          <p className="changelog-stats">
            Risk formula rewrite &bull; AACR-Bench: 48.3% HC recall (was ~30%
            before) &bull; 78.2% HC+M recall
          </p>
          <div className="changelog-mistake">
            <strong>
              Mistake: classification-first scoring doesn&apos;t work.
            </strong>{" "}
            The first risk formula weighted classification at 55% and graph
            signals at 30%. This meant every functional change scored High,
            flooding the output with noise. A one-line logic fix in a helper
            function with no dependents isn&apos;t risky, but
            classification-first scoring can&apos;t tell the difference.
            Flipping the formula to graph-first (dependents + blast radius as
            primary) cut false positives in half and jumped AACR-Bench HC recall
            from ~30% to 48%.
          </div>
        </div>

        <div className="changelog-entry">
          <div className="changelog-title">
            <span className="changelog-sha">f403560</span>
            <span className="changelog-tag">perf</span>
            Parallel graph building, large codebase support
          </div>
          <p className="changelog-body">
            inspect was built on small repos (sem, weave). Then we ran it on
            Sentry: 16,000 files, 100,000+ entities. It took 40 seconds.
            HashSet for O(1) lookups in classify and analyze. Replaced full BFS
            collection with{" "}
            <code style={{ color: "var(--fg)" }}>impact_count()</code> that
            counts without allocating. Added per-phase timing (diff, graph
            build, scoring) so bottlenecks are visible. Sentry dropped from 40s
            to ~4s. Small repos stayed under 50ms.
          </p>
          <p className="changelog-stats">
            40s {"\u2192"} 4s (Sentry, 16k files) &bull; per-phase timing &bull;
            parallel graph build
          </p>
          <div className="changelog-mistake">
            <strong>Mistake: building for small repos only.</strong> We tested on
            sem (25 files) and weave (80 files). Everything was fast, so we
            assumed it was fine. The first time we pointed inspect at a real
            open-source project (Sentry), the graph build alone took 35 seconds
            because we were doing O(n&sup2;) lookups with Vec::contains instead
            of HashSet::contains. Always test on repos 100x bigger than your own.
          </div>
        </div>

        <div className="changelog-entry">
          <div className="changelog-title">
            <span className="changelog-sha">80fb20e</span>
            <span className="changelog-tag">feat</span>
            Full-repo entity graph + bench command
          </div>
          <p className="changelog-body">
            Entity graph now covers all source files (via{" "}
            <code style={{ color: "var(--fg)" }}>git ls-files</code>) instead of
            only changed files. Before this, blast radius was always zero
            because dependents from unchanged files weren&apos;t in the graph.
            New <code style={{ color: "var(--fg)" }}>inspect bench</code>{" "}
            command iterates commits and collects aggregate metrics. 96.8% of
            commits in sem contained tangled logical changes.
          </p>
          <p className="changelog-stats">
            bench command &bull; full-repo graph &bull; tangled commit detection
          </p>
          <div className="changelog-mistake">
            <strong>Mistake: graphing only changed files.</strong> The initial
            implementation built the entity graph from just the files that
            appeared in the diff. This made blast radius and dependent count
            always zero, because the callers were in unchanged files that
            weren&apos;t in the graph. It took an embarrassingly long time to
            realize why every entity scored Low. The fix was obvious: build the
            graph from the full repo via git ls-files, then score the changed
            entities against it.
          </div>
        </div>

        <div className="changelog-entry">
          <div className="changelog-title">
            <span className="changelog-sha">bdfa473</span>
            <span className="changelog-tag">feat</span>
            Initial release
          </div>
          <p className="changelog-body">
            First working version. Cargo workspace:{" "}
            <code style={{ color: "var(--fg)" }}>inspect-core</code> (library) +{" "}
            <code style={{ color: "var(--fg)" }}>inspect-cli</code> (binary).
            ConGra change classification (7 variants), risk scoring, Union-Find
            untangling into logical groups. Three commands:{" "}
            <code style={{ color: "var(--fg)" }}>inspect diff</code>,{" "}
            <code style={{ color: "var(--fg)" }}>inspect pr</code>,{" "}
            <code style={{ color: "var(--fg)" }}>inspect file</code>. Terminal
            (colored) + JSON output. 10 tests.
          </p>
          <p className="changelog-stats">
            Cargo workspace &bull; 2 crates &bull; 10 tests &bull; 3 commands
          </p>
          <div className="changelog-mistake">
            <strong>Lesson: Union-Find untangling is underrated.</strong> We
            almost shipped without it. The initial version just sorted entities
            by risk. But a commit that touches auth, caching, and logging has
            three independent changes that shouldn&apos;t be reviewed together.
            Union-Find on dependency edges between changed entities separates
            them into groups. 96.8% of commits in sem were tangled. Without
            untangling, reviewers have to mentally separate the changes
            themselves.
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
