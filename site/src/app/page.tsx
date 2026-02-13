import React from "react";

export default function Home() {
  return (
    <div className="min-h-screen">
      {/* Hero */}
      <section className="px-6 pt-20 pb-16 max-w-5xl mx-auto">
        <p className="text-sm tracking-widest text-gray-500 uppercase mb-4" style={{ fontFamily: "var(--font-heading)" }}>
          by <a href="https://ataraxy-labs.com" className="hover:text-gray-300 transition-colors">Ataraxy Labs</a>
        </p>
        <h1 className="text-5xl md:text-7xl font-bold mb-6 leading-tight" style={{ fontFamily: "var(--font-heading)" }}>
          inspect
        </h1>
        <p className="text-xl md:text-2xl text-gray-300 mb-4 max-w-3xl leading-relaxed">
          Entity-level code review for Git. Risk scoring, blast radius, change classification, and commit untangling.
        </p>
        <p className="text-lg text-gray-500 mb-10 max-w-2xl">
          Git diff says N files changed. inspect says which functions matter, scores them by risk, and groups them by logical dependency.
        </p>

        <div className="flex flex-wrap gap-4 mb-16">
          <a
            href="https://github.com/Ataraxy-Labs/inspect"
            target="_blank"
            rel="noopener noreferrer"
            className="px-6 py-3 bg-white text-black font-semibold rounded-lg hover:bg-gray-200 transition-colors text-sm"
            style={{ fontFamily: "var(--font-heading)" }}
          >
            GitHub
          </a>
          <a
            href="/inspect/llms.txt"
            className="px-6 py-3 border border-white/20 rounded-lg hover:border-white/40 transition-colors text-sm"
            style={{ fontFamily: "var(--font-heading)" }}
          >
            llms.txt
          </a>
        </div>

        {/* Install */}
        <div className="mb-16">
          <pre><code>cargo install --git https://github.com/Ataraxy-Labs/inspect inspect-cli</code></pre>
        </div>
      </section>

      {/* The Problem */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            The Problem
          </h2>
          <p className="text-gray-400 mb-6 max-w-3xl leading-relaxed">
            AI is generating more code than ever. Human reviewers are drowning. DORA 2025 found that AI adoption led to +154% PR size, +91% review time, and +9% more bugs shipped.
          </p>
          <p className="text-gray-400 max-w-3xl leading-relaxed">
            Every code review tool today works at the file or line level. CodeRabbit, Qodo, SonarQube. They show you every line that changed and leave you to figure out what matters. inspect works at the entity level: functions, structs, traits, classes. It scores each change by risk and groups them by logical dependency.
          </p>
        </div>
      </section>

      {/* Benchmark */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            Benchmark: 9,182 Entities Across 102 Commits
          </h2>
          <p className="text-gray-400 mb-8 max-w-3xl">
            Real results from running <code>inspect bench</code> against three Rust codebases.
          </p>
          <div className="grid md:grid-cols-3 gap-8 mb-8">
            <div className="border border-white/10 rounded-lg p-8">
              <p className="text-sm text-gray-500 uppercase tracking-wider mb-2" style={{ fontFamily: "var(--font-heading)" }}>Max blast radius</p>
              <p className="text-5xl font-bold text-white" style={{ fontFamily: "var(--font-heading)" }}>595</p>
              <p className="text-gray-400 mt-2">entities affected by one change in agenthub</p>
            </div>
            <div className="border border-white/10 rounded-lg p-8">
              <p className="text-sm text-gray-500 uppercase tracking-wider mb-2" style={{ fontFamily: "var(--font-heading)" }}>Cross-file impact</p>
              <p className="text-5xl font-bold text-white" style={{ fontFamily: "var(--font-heading)" }}>70.7%</p>
              <p className="text-gray-400 mt-2">of changes in agenthub ripple across files</p>
            </div>
            <div className="border border-white/10 rounded-lg p-8">
              <p className="text-sm text-gray-500 uppercase tracking-wider mb-2" style={{ fontFamily: "var(--font-heading)" }}>Tangled commits</p>
              <p className="text-5xl font-bold text-white" style={{ fontFamily: "var(--font-heading)" }}>96.8%</p>
              <p className="text-gray-400 mt-2">of sem commits contain multiple logical changes</p>
            </div>
          </div>

          <div className="grid md:grid-cols-3 gap-6 mb-8">
            {[
              { repo: "sem", commits: 38, entities: "5,216", blast: "0", hc: "0%", cross: "0%" },
              { repo: "weave", commits: 45, entities: "2,854", blast: "176", hc: "6.4%", cross: "10.9%" },
              { repo: "agenthub", commits: 19, entities: "1,112", blast: "595", hc: "36.0%", cross: "70.7%" },
            ].map((r) => (
              <div key={r.repo} className="border border-white/10 rounded-lg p-6">
                <h4 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>{r.repo} ({r.commits} commits)</h4>
                <div className="space-y-2 text-sm">
                  <div className="flex justify-between"><span className="text-gray-500">Entities reviewed</span><span className="text-white">{r.entities}</span></div>
                  <div className="flex justify-between"><span className="text-gray-500">Max blast radius</span><span className="text-white">{r.blast}</span></div>
                  <div className="flex justify-between"><span className="text-gray-500">High/Critical</span><span className="text-white">{r.hc}</span></div>
                  <div className="flex justify-between"><span className="text-gray-500">Cross-file impact</span><span className="text-white">{r.cross}</span></div>
                </div>
              </div>
            ))}
          </div>

          <p className="text-gray-500 text-sm">
            Blast radius 595 means one entity change could affect 595 other entities transitively. A line-level diff won&apos;t tell you this. 70.7% cross-file impact means reviewing one file in isolation misses the picture.
          </p>
        </div>
      </section>

      {/* Speed */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            Speed
          </h2>
          <p className="text-gray-400 mb-8 max-w-3xl">
            Entity extraction, dependency graph construction, change classification, risk scoring, and commit untangling. All in milliseconds. No API calls, everything local.
          </p>
          <div className="grid md:grid-cols-2 gap-8 mb-8">
            <div>
              <h3 className="text-lg font-semibold mb-4" style={{ fontFamily: "var(--font-heading)" }}>Single commit review</h3>
              <div className="space-y-3">
                {[
                  { repo: "sem", desc: "25 files", time: "24ms" },
                  { repo: "weave", desc: "80 files", time: "30ms" },
                  { repo: "agenthub", desc: "130 files, 9K LOC", time: "49ms" },
                ].map((r) => (
                  <div key={r.repo} className="flex justify-between items-center border-b border-white/5 pb-2">
                    <span className="text-gray-400 text-sm">{r.repo} <span className="text-gray-600">({r.desc})</span></span>
                    <span className="text-white font-semibold" style={{ fontFamily: "var(--font-heading)" }}>{r.time}</span>
                  </div>
                ))}
              </div>
            </div>
            <div>
              <h3 className="text-lg font-semibold mb-4" style={{ fontFamily: "var(--font-heading)" }}>Full repo history</h3>
              <div className="space-y-3">
                {[
                  { repo: "sem", desc: "38 commits, 5K entities", time: "0.87s" },
                  { repo: "agenthub", desc: "19 commits, 1K entities", time: "0.56s" },
                  { repo: "weave", desc: "45 commits, 3K entities", time: "1.81s" },
                ].map((r) => (
                  <div key={r.repo} className="flex justify-between items-center border-b border-white/5 pb-2">
                    <span className="text-gray-400 text-sm">{r.repo} <span className="text-gray-600">({r.desc})</span></span>
                    <span className="text-white font-semibold" style={{ fontFamily: "var(--font-heading)" }}>{r.time}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
          <p className="text-gray-500 text-sm">
            Powered by <a href="https://ataraxy-labs.com/sem" className="text-white underline hover:text-gray-300">sem-core</a> v0.3.0 with xxHash64 structural hashing, parallel tree-sitter parsing via rayon, cached git tree resolution, and LTO-optimized release builds.
          </p>
        </div>
      </section>

      {/* How it works */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-12" style={{ fontFamily: "var(--font-heading)" }}>
            How It Works
          </h2>
          <div className="grid md:grid-cols-2 gap-10">
            <div>
              <h3 className="text-lg font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>1. Extract</h3>
              <p className="text-gray-400 leading-relaxed">
                Parse source files with tree-sitter. Extract functions, structs, classes, traits as entities. Build a full-repo dependency graph from all tracked source files.
              </p>
            </div>
            <div>
              <h3 className="text-lg font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>2. Classify</h3>
              <p className="text-gray-400 leading-relaxed">
                Compare before/after content line by line. Categorize each change as text (comments), syntax (signatures), functional (logic), or a combination using the ConGra taxonomy.
              </p>
            </div>
            <div>
              <h3 className="text-lg font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>3. Score</h3>
              <p className="text-gray-400 leading-relaxed">
                Compute risk from classification weight, blast radius, dependent count, public API exposure, and change type. Cosmetic-only changes get a 70% discount.
              </p>
            </div>
            <div>
              <h3 className="text-lg font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>4. Group</h3>
              <p className="text-gray-400 leading-relaxed">
                Untangle commits into logical groups using Union-Find on dependency edges between changed entities. Each group can be reviewed independently.
              </p>
            </div>
          </div>
        </div>
      </section>

      {/* Commands */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-12" style={{ fontFamily: "var(--font-heading)" }}>
            Commands
          </h2>

          <div className="mb-12">
            <h3 className="text-xl font-semibold mb-4" style={{ fontFamily: "var(--font-heading)" }}>inspect diff</h3>
            <p className="text-gray-400 mb-4">Review entity-level changes for any commit or range.</p>
            <pre><code>{`$ inspect diff HEAD~1

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
    cosmetic only (no structural change)`}</code></pre>
          </div>

          <div className="mb-12">
            <h3 className="text-xl font-semibold mb-4" style={{ fontFamily: "var(--font-heading)" }}>inspect pr</h3>
            <p className="text-gray-400 mb-4">Review all changes in a GitHub pull request. Uses <code>gh</code> CLI to resolve base/head refs.</p>
            <pre><code>{`$ inspect pr 42
$ inspect pr 42 --min-risk high --format json`}</code></pre>
          </div>

          <div className="mb-12">
            <h3 className="text-xl font-semibold mb-4" style={{ fontFamily: "var(--font-heading)" }}>inspect file</h3>
            <p className="text-gray-400 mb-4">Review uncommitted changes in a specific file.</p>
            <pre><code>{`$ inspect file src/main.rs --context`}</code></pre>
          </div>

          <div className="mb-12">
            <h3 className="text-xl font-semibold mb-4" style={{ fontFamily: "var(--font-heading)" }}>inspect bench</h3>
            <p className="text-gray-400 mb-4">Benchmark entity-level review across a repo&apos;s commit history. Outputs JSON with per-commit details and aggregate metrics.</p>
            <pre><code>{`$ inspect bench --repo ~/weave --limit 100 > bench.json`}</code></pre>
          </div>
        </div>
      </section>

      {/* Change Classification */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            Change Classification
          </h2>
          <p className="text-gray-400 mb-8 max-w-3xl">
            Based on <a href="https://arxiv.org/abs/2409.14121" className="text-white underline hover:text-gray-300" target="_blank" rel="noopener noreferrer">ConGra (arXiv:2409.14121)</a>. Every change is classified along three dimensions: text, syntax, and functional. This produces 7 possible categories.
          </p>
          <div className="grid md:grid-cols-3 gap-6">
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>Text</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                Only comments, whitespace, or documentation changed. Safe to skip in most reviews.
              </p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>Syntax</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                Signatures, types, or declarations changed without logic changes. Type annotation updates, visibility modifiers.
              </p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>Functional</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                Logic or behavior changed. Conditionals, return values, control flow. Requires careful review.
              </p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>Text+Syntax</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                Comments and signatures both changed. Doc comment updated alongside a type change.
              </p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>Text+Functional</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                Comments and logic both changed. Bug fix with an updated explanation.
              </p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-3" style={{ fontFamily: "var(--font-heading)" }}>Syntax+Functional</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                Signatures and logic both changed. New parameter added and used in the function body.
              </p>
            </div>
          </div>
        </div>
      </section>

      {/* Risk Scoring */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            Risk Scoring
          </h2>
          <p className="text-gray-400 mb-6 max-w-3xl">
            Each entity gets a risk score from 0.0 to 1.0 combining five signals. Cosmetic-only changes (structural hash unchanged) get a 70% discount.
          </p>
          <div className="space-y-6">
            <div className="border-l-2 border-white/20 pl-6">
              <h3 className="text-lg font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>Classification Weight</h3>
              <p className="text-gray-400 text-sm">Text changes score 0.05, functional changes score 0.4, mixed changes up to 0.55.</p>
            </div>
            <div className="border-l-2 border-white/20 pl-6">
              <h3 className="text-lg font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>Blast Radius</h3>
              <p className="text-gray-400 text-sm">Transitive impact normalized by total entity count. An entity affecting 100 of 500 entities scores 0.06.</p>
            </div>
            <div className="border-l-2 border-white/20 pl-6">
              <h3 className="text-lg font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>Dependent Count</h3>
              <p className="text-gray-400 text-sm">Logarithmic scale. More dependents means more risk, but diminishing returns after ~10.</p>
            </div>
            <div className="border-l-2 border-white/20 pl-6">
              <h3 className="text-lg font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>Public API</h3>
              <p className="text-gray-400 text-sm">Exported functions, pub methods, capitalized Go/Java names get a 0.15 boost.</p>
            </div>
            <div className="border-l-2 border-white/20 pl-6">
              <h3 className="text-lg font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>Change Type</h3>
              <p className="text-gray-400 text-sm">Deletions score highest (0.2), modifications and renames (0.1), additions lowest (0.05).</p>
            </div>
          </div>
          <p className="text-gray-500 text-sm mt-8">
            Risk levels: <strong className="text-white">Critical</strong> (&gt;= 0.7), <strong className="text-white">High</strong> (&gt;= 0.5), <strong className="text-white">Medium</strong> (&gt;= 0.3), <strong className="text-white">Low</strong> (&lt; 0.3).
          </p>
        </div>
      </section>

      {/* Languages */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            13 Languages
          </h2>
          <div className="flex flex-wrap gap-3">
            {["Rust", "TypeScript", "TSX", "JavaScript", "Python", "Go", "Java", "C", "C++", "Ruby", "C#", "PHP", "Fortran"].map((lang) => (
              <span key={lang} className="px-4 py-2 border border-white/15 rounded-lg text-sm text-gray-300">
                {lang}
              </span>
            ))}
          </div>
          <p className="text-gray-500 text-sm mt-6">
            Each language has a tree-sitter parser compiled into the binary. No runtime dependencies. Powered by <a href="https://ataraxy-labs.com/sem" className="text-white underline hover:text-gray-300">sem-core</a>.
          </p>
        </div>
      </section>

      {/* Architecture */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={{ fontFamily: "var(--font-heading)" }}>
            Architecture
          </h2>
          <pre><code>{`inspect/crates/
  inspect-core/   Analysis engine: classify, risk score, untangle
  inspect-cli/    CLI: diff, pr, file, bench commands`}</code></pre>
          <p className="text-gray-400 mt-4 text-sm">
            Built in Rust. Entity extraction powered by <a href="https://github.com/Ataraxy-Labs/sem" className="text-white underline hover:text-gray-300">sem-core</a> with tree-sitter. Full-repo entity graph built from all tracked source files via <code>git ls-files</code>.
          </p>
        </div>
      </section>

      {/* Companion tools */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-4" style={{ fontFamily: "var(--font-heading)" }}>
            Works with sem and Weave
          </h2>
          <p className="text-gray-400 mb-6 max-w-3xl leading-relaxed">
            inspect, <a href="https://ataraxy-labs.com/sem" className="text-white underline hover:text-gray-300">sem</a>, and <a href="https://ataraxy-labs.com/weave" className="text-white underline hover:text-gray-300">Weave</a> are complementary tools built on the same foundation: sem-core&apos;s entity extraction and structural hashing.
          </p>
          <div className="grid md:grid-cols-3 gap-8">
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>sem</h3>
              <p className="text-gray-400 text-sm">Understand code history. What changed, who changed it, what depends on it, what might break.</p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>Weave</h3>
              <p className="text-gray-400 text-sm">Merge without false conflicts. 31/31 clean merges on concurrent edit scenarios vs Git&apos;s 15/31.</p>
            </div>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-base font-semibold mb-2" style={{ fontFamily: "var(--font-heading)" }}>inspect</h3>
              <p className="text-gray-400 text-sm">Review what matters. Risk scoring, blast radius, change classification, commit untangling.</p>
            </div>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="px-6 py-12 border-t border-white/10 text-center text-gray-600 text-sm">
        <p>MIT License. Built by <a href="https://ataraxy-labs.com" className="text-gray-400 hover:text-white transition-colors">Ataraxy Labs</a>.</p>
      </footer>
    </div>
  );
}
