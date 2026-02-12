import React from "react";

const heading = { fontFamily: "var(--font-heading)" };

function Stat({ value, label }: { value: string; label: string }) {
  return (
    <div className="text-center">
      <div className="text-3xl md:text-4xl font-bold text-white" style={heading}>{value}</div>
      <div className="text-sm text-gray-500 mt-1">{label}</div>
    </div>
  );
}

function RiskBar({ label, value, max, color }: { label: string; value: number; max: number; color: string }) {
  const pct = max > 0 ? (value / max) * 100 : 0;
  return (
    <div className="flex items-center gap-3 mb-2">
      <span className="text-sm text-gray-400 w-16">{label}</span>
      <div className="flex-1 h-6 bg-white/5 rounded overflow-hidden">
        <div className={`h-full rounded ${color}`} style={{ width: `${pct}%` }} />
      </div>
      <span className="text-sm text-gray-400 w-12 text-right">{value}</span>
    </div>
  );
}

function BenchCard({ repo, entities, entitiesPerCommit, blastAvg, blastMax, highCritical, crossFile, tangled }: {
  repo: string;
  entities: number;
  entitiesPerCommit: number;
  blastAvg: number;
  blastMax: number;
  highCritical: string;
  crossFile: string;
  tangled: string;
}) {
  return (
    <div className="border border-white/10 rounded-lg p-6">
      <h4 className="text-lg font-semibold mb-4" style={heading}>{repo}</h4>
      <div className="space-y-2 text-sm">
        <div className="flex justify-between">
          <span className="text-gray-500">Total entities reviewed</span>
          <span className="text-white">{entities.toLocaleString()}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Avg entities/commit</span>
          <span className="text-white">{entitiesPerCommit.toFixed(1)}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Avg blast radius</span>
          <span className="text-white">{blastAvg.toFixed(1)}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Max blast radius</span>
          <span className="text-white">{blastMax}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">High/Critical ratio</span>
          <span className="text-red-400">{highCritical}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Cross-file impact</span>
          <span className="text-yellow-400">{crossFile}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Tangled commits</span>
          <span className="text-blue-400">{tangled}</span>
        </div>
      </div>
    </div>
  );
}

const classifications = [
  { name: "Text", desc: "Only comments, whitespace, or docs changed", example: "Renaming a variable in a comment", color: "bg-gray-600" },
  { name: "Syntax", desc: "Signatures or declarations changed, no logic", example: "Adding a type annotation to a parameter", color: "bg-blue-600" },
  { name: "Functional", desc: "Logic or behavior changed", example: "Changing a conditional from > to >=", color: "bg-red-600" },
  { name: "Text+Syntax", desc: "Comments and signatures both changed", example: "Updating a doc comment and return type together", color: "bg-cyan-700" },
  { name: "Text+Functional", desc: "Comments and logic both changed", example: "Fixing a bug and updating the explanation", color: "bg-orange-700" },
  { name: "Syntax+Functional", desc: "Signatures and logic both changed", example: "Adding a parameter and using it in the body", color: "bg-purple-700" },
  { name: "Text+Syntax+Functional", desc: "All three dimensions changed", example: "Full rewrite of a function", color: "bg-pink-700" },
];

const languages = [
  "Rust", "TypeScript", "TSX", "JavaScript", "Python", "Go",
  "Java", "C", "C++", "Ruby", "C#", "Fortran",
];

export default function Home() {
  return (
    <div className="min-h-screen">
      {/* Hero */}
      <section className="px-6 pt-20 pb-16 max-w-5xl mx-auto">
        <p className="text-sm tracking-widest text-gray-500 uppercase mb-4" style={heading}>
          by <a href="https://ataraxy-labs.com" className="hover:text-gray-300 transition-colors">Ataraxy Labs</a>
        </p>
        <h1 className="text-5xl md:text-7xl font-bold mb-6 leading-tight" style={heading}>
          inspect
        </h1>
        <p className="text-xl md:text-2xl text-gray-300 mb-4 max-w-3xl leading-relaxed">
          Entity-level code review. Git diff says <span className="text-white font-semibold">N files changed</span>. inspect says <span className="text-white font-semibold">M functions matter</span>.
        </p>
        <p className="text-lg text-gray-500 mb-10 max-w-2xl">
          Risk scoring, blast radius analysis, change classification, and commit untangling. Review what matters, skip what doesn't.
        </p>

        <div className="flex flex-wrap gap-4 mb-16">
          <a
            href="https://github.com/Ataraxy-Labs/inspect"
            target="_blank"
            rel="noopener noreferrer"
            className="px-6 py-3 bg-white text-black font-semibold rounded-lg hover:bg-gray-200 transition-colors text-sm"
            style={heading}
          >
            GitHub
          </a>
          <a
            href="/inspect/llms.txt"
            className="px-6 py-3 border border-white/20 rounded-lg hover:border-white/40 transition-colors text-sm"
            style={heading}
          >
            llms.txt
          </a>
        </div>

        <div className="mb-16">
          <pre><code>cargo install --git https://github.com/Ataraxy-Labs/inspect inspect-cli</code></pre>
        </div>
      </section>

      {/* The Problem */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-4" style={heading}>The Problem</h2>
          <p className="text-gray-400 mb-8 max-w-3xl leading-relaxed">
            AI is generating more code than ever. Human reviewers are drowning.
          </p>
          <div className="grid md:grid-cols-3 gap-8 mb-8">
            <Stat value="+154%" label="PR size increase (DORA 2025)" />
            <Stat value="+91%" label="Review time increase" />
            <Stat value="+9%" label="More bugs shipped" />
          </div>
          <p className="text-gray-500 max-w-3xl leading-relaxed">
            Every code review tool today works at the file or line level. CodeRabbit, Qodo, SonarQube. They show you every line that changed and leave you to figure out what matters. inspect works at the entity level: functions, structs, traits, classes. It scores each change by risk and groups them by logical dependency.
          </p>
        </div>
      </section>

      {/* Benchmark Results */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-4" style={heading}>Benchmark Results</h2>
          <p className="text-gray-400 mb-8 max-w-3xl">
            Real results from running <code>inspect bench</code> against three Rust codebases with a combined 8,870 entities across 89 commits.
          </p>

          <div className="grid md:grid-cols-3 gap-6 mb-12">
            <BenchCard
              repo="sem (31 commits)"
              entities={4955}
              entitiesPerCommit={159.8}
              blastAvg={0}
              blastMax={0}
              highCritical="15.1%"
              crossFile="0%"
              tangled="96.8%"
            />
            <BenchCard
              repo="weave (39 commits)"
              entities={2803}
              entitiesPerCommit={71.9}
              blastAvg={3.4}
              blastMax={171}
              highCritical="40.6%"
              crossFile="10.6%"
              tangled="69.2%"
            />
            <BenchCard
              repo="agenthub (19 commits)"
              entities={1112}
              entitiesPerCommit={58.5}
              blastAvg={42.5}
              blastMax={595}
              highCritical="77.1%"
              crossFile="70.7%"
              tangled="94.7%"
            />
          </div>

          {/* Risk Distribution (weave) */}
          <div className="border border-white/10 rounded-lg p-6 mb-8">
            <h4 className="text-lg font-semibold mb-4" style={heading}>Risk Distribution (weave, 2,803 entities)</h4>
            <RiskBar label="Critical" value={133} max={1663} color="bg-red-500" />
            <RiskBar label="High" value={1006} max={1663} color="bg-yellow-500" />
            <RiskBar label="Medium" value={1663} max={1663} color="bg-blue-500" />
            <RiskBar label="Low" value={1} max={1663} color="bg-gray-500" />
          </div>

          <div className="grid md:grid-cols-2 gap-8">
            <div>
              <h4 className="text-lg font-semibold mb-3" style={heading}>What the numbers mean</h4>
              <ul className="text-gray-400 space-y-2 text-sm leading-relaxed">
                <li><span className="text-white font-medium">Blast radius 595</span> means one entity change in agenthub could affect 595 other entities transitively. A line-level diff won't tell you this.</li>
                <li><span className="text-white font-medium">70.7% cross-file impact</span> means most changes in agenthub ripple across file boundaries. Reviewing one file in isolation misses the picture.</li>
                <li><span className="text-white font-medium">96.8% tangled commits</span> means almost every commit in sem contains multiple independent logical changes that should be reviewed separately.</li>
              </ul>
            </div>
            <div>
              <h4 className="text-lg font-semibold mb-3" style={heading}>What inspect enables</h4>
              <ul className="text-gray-400 space-y-2 text-sm leading-relaxed">
                <li><span className="text-white font-medium">Prioritize by risk.</span> Focus on the 40.6% high/critical entities in weave, skip the rest.</li>
                <li><span className="text-white font-medium">See blast radius.</span> Know that modifying <code>merge_entities</code> affects 171 downstream entities before you approve.</li>
                <li><span className="text-white font-medium">Untangle commits.</span> A tangled commit with 5 groups can be reviewed as 5 independent changes, each with its own risk profile.</li>
              </ul>
            </div>
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={heading}>How It Works</h2>
          <div className="grid md:grid-cols-4 gap-6">
            {[
              { step: "1", title: "Extract", desc: "Parse source files with tree-sitter. Extract functions, structs, classes, traits as entities." },
              { step: "2", title: "Classify", desc: "Compare before/after content. Categorize each change as text, syntax, functional, or mixed using ConGra taxonomy." },
              { step: "3", title: "Score", desc: "Compute risk from classification, blast radius, dependent count, public API exposure, and change type." },
              { step: "4", title: "Group", desc: "Untangle commits into logical groups using Union-Find on dependency edges between changed entities." },
            ].map((item) => (
              <div key={item.step} className="border border-white/10 rounded-lg p-5">
                <div className="text-2xl font-bold text-white/20 mb-2" style={heading}>{item.step}</div>
                <h3 className="text-lg font-semibold mb-2" style={heading}>{item.title}</h3>
                <p className="text-sm text-gray-400 leading-relaxed">{item.desc}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Commands */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-12" style={heading}>Commands</h2>

          <div className="mb-12">
            <h3 className="text-xl font-semibold mb-4" style={heading}>inspect diff</h3>
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
            <h3 className="text-xl font-semibold mb-4" style={heading}>inspect pr</h3>
            <p className="text-gray-400 mb-4">Review all changes in a GitHub pull request.</p>
            <pre><code>{`$ inspect pr 42
# Analyzes all commits in PR #42 against the base branch`}</code></pre>
          </div>

          <div className="mb-12">
            <h3 className="text-xl font-semibold mb-4" style={heading}>inspect bench</h3>
            <p className="text-gray-400 mb-4">Benchmark entity-level review across a repo's commit history. Outputs JSON.</p>
            <pre><code>{`$ inspect bench --repo ~/weave --limit 100 > bench.json
inspect bench: analyzing /Users/you/weave (limit: 100)
found 39 commits
  done.`}</code></pre>
          </div>
        </div>
      </section>

      {/* Change Classification */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-4" style={heading}>Change Classification</h2>
          <p className="text-gray-400 mb-8 max-w-3xl">
            Based on <a href="https://arxiv.org/abs/2409.14121" className="text-blue-400 hover:text-blue-300" target="_blank" rel="noopener noreferrer">ConGra (arXiv:2409.14121)</a>. Every change is classified along three dimensions: text, syntax, and functional. This produces 7 possible categories.
          </p>
          <div className="space-y-3">
            {classifications.map((c) => (
              <div key={c.name} className="flex items-start gap-4 border border-white/5 rounded-lg p-4">
                <span className={`px-3 py-1 text-xs font-semibold rounded ${c.color} text-white shrink-0`}>{c.name}</span>
                <div>
                  <p className="text-sm text-gray-300">{c.desc}</p>
                  <p className="text-xs text-gray-500 mt-1">Example: {c.example}</p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* 12 Languages */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-8" style={heading}>12 Languages</h2>
          <div className="flex flex-wrap gap-3">
            {languages.map((lang) => (
              <span key={lang} className="px-4 py-2 border border-white/15 rounded-lg text-sm text-gray-300">
                {lang}
              </span>
            ))}
          </div>
          <p className="text-gray-500 text-sm mt-4">
            Powered by tree-sitter parsers from <a href="https://ataraxy-labs.com/sem" className="text-blue-400 hover:text-blue-300">sem-core</a>.
          </p>
        </div>
      </section>

      {/* Ataraxy Labs Stack */}
      <section className="px-6 py-16 border-t border-white/10">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold mb-4" style={heading}>The Ataraxy Labs Stack</h2>
          <p className="text-gray-400 mb-8 max-w-3xl">
            inspect is built on top of sem-core and integrates with the full entity-level development toolkit.
          </p>
          <div className="grid md:grid-cols-3 gap-6">
            <a href="https://ataraxy-labs.com/sem" className="border border-white/10 rounded-lg p-6 hover:border-white/25 transition-colors group">
              <h3 className="text-lg font-semibold mb-2 group-hover:text-white" style={heading}>sem</h3>
              <p className="text-sm text-gray-400">Entity-level diff, blame, graph, and impact analysis. The foundation for everything.</p>
            </a>
            <a href="https://ataraxy-labs.com/weave" className="border border-white/10 rounded-lg p-6 hover:border-white/25 transition-colors group">
              <h3 className="text-lg font-semibold mb-2 group-hover:text-white" style={heading}>weave</h3>
              <p className="text-sm text-gray-400">Entity-level semantic merge driver for Git. 100% clean merges on a 31-scenario benchmark.</p>
            </a>
            <div className="border border-white/10 rounded-lg p-6">
              <h3 className="text-lg font-semibold mb-2" style={heading}>agenthub</h3>
              <p className="text-sm text-gray-400">Agent-native GitHub platform. Entity-level coordination, permissions, and merge for multi-agent development.</p>
            </div>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="px-6 py-12 border-t border-white/10">
        <div className="max-w-5xl mx-auto flex flex-col md:flex-row justify-between items-center gap-4">
          <div className="text-sm text-gray-500">
            <a href="https://ataraxy-labs.com" className="hover:text-gray-300 transition-colors">Ataraxy Labs</a>
          </div>
          <div className="flex gap-6 text-sm text-gray-500">
            <a href="https://github.com/Ataraxy-Labs/inspect" className="hover:text-gray-300 transition-colors">GitHub</a>
            <a href="https://github.com/Ataraxy-Labs" className="hover:text-gray-300 transition-colors">All Projects</a>
          </div>
        </div>
      </footer>
    </div>
  );
}
