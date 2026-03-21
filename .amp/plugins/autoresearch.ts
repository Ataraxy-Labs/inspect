// @i-know-the-amp-plugin-api-is-wip-and-very-experimental-right-now
import type {
  PluginAPI,
  PluginEventContext,
  AgentEndEvent,
} from "@ampcode/plugin";
import { readFileSync, appendFileSync, existsSync, unlinkSync } from "node:fs";

// ── State ────────────────────────────────────────────────────────────────────
interface ExperimentConfig {
  name: string;
  metric_name: string;
  metric_unit: string;
  direction: "lower" | "higher";
}

interface ExperimentEntry {
  timestamp: string;
  commit: string;
  metric: number;
  status: "keep" | "discard" | "crash" | "checks_failed";
  description: string;
  metrics?: Record<string, number>;
}

interface SessionState {
  active: boolean;
  config: ExperimentConfig | null;
  entries: ExperimentEntry[];
  secondaryMetricKeys: string[];
  maxIterations: number;
  autoResumeCount: number;
  lastResumeTime: number;
}

const state: SessionState = {
  active: false,
  config: null,
  entries: [],
  secondaryMetricKeys: [],
  maxIterations: 50,
  autoResumeCount: 0,
  lastResumeTime: 0,
};

const JSONL_FILE = "autoresearch.jsonl";
const MD_FILE = "autoresearch.md";
const CHECKS_SCRIPT = "autoresearch.checks.sh";
const CONFIG_FILE = "autoresearch.config.json";
const AUTO_RESUME_COOLDOWN_MS = 5 * 60 * 1000;
const AUTO_RESUME_MAX_TURNS = 20;

// ── Helpers ──────────────────────────────────────────────────────────────────

function isBetter(newVal: number, bestVal: number, direction: string): boolean {
  return direction === "lower" ? newVal < bestVal : newVal > bestVal;
}

/** Run a shell command via Bun.$ with stdout captured (.quiet()) */
async function run(
  cmd: string,
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  try {
    const proc = await Bun.$`bash -c ${cmd}`.quiet();
    return {
      exitCode: proc.exitCode,
      stdout: proc.stdout.toString(),
      stderr: proc.stderr.toString(),
    };
  } catch (e: any) {
    return {
      exitCode: e.exitCode ?? 1,
      stdout: e.stdout?.toString() ?? "",
      stderr: e.stderr?.toString() ?? "",
    };
  }
}

function bestMetric(): number | null {
  const kept = state.entries.filter((e) => e.status === "keep");
  if (kept.length === 0) return null;
  return kept.reduce(
    (best, e) => {
      if (best === null) return e.metric;
      return isBetter(e.metric, best, state.config?.direction ?? "lower")
        ? e.metric
        : best;
    },
    null as number | null,
  );
}

function deltaPercent(current: number, baseline: number): string {
  if (baseline === 0) return "N/A";
  const pct = ((current - baseline) / Math.abs(baseline)) * 100;
  const sign = pct > 0 ? "+" : "";
  return `${sign}${pct.toFixed(1)}%`;
}

function summaryLine(): string {
  const total = state.entries.length;
  const kept = state.entries.filter((e) => e.status === "keep").length;
  const best = bestMetric();
  const bestStr =
    best !== null
      ? ` │ ★ ${state.config?.metric_name ?? "metric"}: ${best}${state.config?.metric_unit ?? ""}`
      : "";
  return `🔬 autoresearch ${total} runs ${kept} kept${bestStr}`;
}

function dashboardTable(): string {
  if (state.entries.length === 0) return "No experiments recorded yet.";
  const best = bestMetric();
  const header = `| # | Commit | ${state.config?.metric_name ?? "Metric"} | Δ% | Status | Description |`;
  const sep = "|---|--------|--------|----|--------|-------------|";
  const rows = state.entries.map((e, i) => {
    const delta = best !== null ? deltaPercent(e.metric, best) : "";
    const icon =
      e.status === "keep"
        ? "✅"
        : e.status === "discard"
          ? "❌"
          : e.status === "crash"
            ? "💥"
            : "⚠️";
    return `| ${i + 1} | ${e.commit} | ${e.metric}${state.config?.metric_unit ?? ""} | ${delta} | ${icon} ${e.status} | ${e.description} |`;
  });
  return [summaryLine(), "", header, sep, ...rows].join("\n");
}

// ── Plugin ───────────────────────────────────────────────────────────────────
export default function (amp: PluginAPI) {
  // ── Tool: init_experiment ──────────────────────────────────────────────
  amp.registerTool({
    name: "init_experiment",
    description:
      "Initialize an autoresearch session. Call once at the start to define the optimization target. " +
      "Call again with different metric to start a new baseline segment.",
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description:
            'Human-readable session name (e.g. "Optimizing test speed")',
        },
        metric_name: {
          type: "string",
          description:
            'Display name for the metric (e.g. "total_µs", "bundle_kb")',
        },
        metric_unit: {
          type: "string",
          description:
            'Unit string: "µs", "ms", "s", "kb", "mb", or "" for unitless',
        },
        direction: {
          type: "string",
          description:
            '"lower" or "higher" — which direction is better. Default: "lower"',
        },
      },
      required: ["name", "metric_name"],
    },
    async execute(input) {
      const name = input.name as string;
      const metric_name = input.metric_name as string;
      const metric_unit = (input.metric_unit as string) ?? "";
      const direction = ((input.direction as string) ?? "lower") as
        | "lower"
        | "higher";

      state.config = { name, metric_name, metric_unit, direction };
      state.active = true;
      state.entries = [];
      state.secondaryMetricKeys = [];
      state.autoResumeCount = 0;

      // Load config if present
      try {
        if (existsSync(CONFIG_FILE)) {
          const parsed = JSON.parse(readFileSync(CONFIG_FILE, "utf-8"));
          if (parsed.maxIterations) state.maxIterations = parsed.maxIterations;
        }
      } catch {
        /* no config file, use defaults */
      }

      // Load existing entries from jsonl if resuming
      try {
        if (existsSync(JSONL_FILE)) {
          const lines = readFileSync(JSONL_FILE, "utf-8")
            .trim()
            .split("\n")
            .filter(Boolean);
          for (const line of lines) {
            try {
              const parsed = JSON.parse(line);
              if (parsed.type === "config") continue;
              if (parsed.commit) state.entries.push(parsed as ExperimentEntry);
            } catch {
              /* skip malformed lines */
            }
          }
        }
      } catch {
        /* no existing file */
      }

      // Write config header to jsonl
      const configLine = JSON.stringify({
        type: "config",
        name,
        metric_name,
        metric_unit,
        direction,
        timestamp: new Date().toISOString(),
      });
      appendFileSync(JSONL_FILE, configLine + "\n");

      return (
        `✅ Autoresearch session initialized: "${name}"\n` +
        `Metric: ${metric_name} (${metric_unit || "unitless"}, ${direction} is better)\n` +
        `Max iterations: ${state.maxIterations}\n` +
        `Loaded ${state.entries.length} previous entries.`
      );
    },
  });

  // ── Tool: run_experiment ──────────────────────────────────────────────
  amp.registerTool({
    name: "run_experiment",
    description:
      "Run a benchmark command, measure wall-clock time, capture output. " +
      "If autoresearch.checks.sh exists and benchmark passed, it runs checks too. " +
      "Returns pass/fail, duration, and last 80 lines of output.",
    inputSchema: {
      type: "object",
      properties: {
        command: {
          type: "string",
          description: 'Shell command to run (e.g. "pnpm test", "cargo bench")',
        },
        timeout_seconds: {
          type: "number",
          description: "Kill timeout (default: 600s)",
        },
        checks_timeout_seconds: {
          type: "number",
          description: "Kill timeout for checks script (default: 300s)",
        },
      },
      required: ["command"],
    },
    async execute(input) {
      if (!state.active || !state.config) {
        return "❌ No active autoresearch session. Call init_experiment first.";
      }

      if (state.entries.length >= state.maxIterations) {
        return `🛑 Max iterations (${state.maxIterations}) reached. Session complete.`;
      }

      const command = input.command as string;

      const startTime = Date.now();
      const benchResult = await run(command);
      const durationMs = Date.now() - startTime;
      const durationSec = (durationMs / 1000).toFixed(2);

      const passed = benchResult.exitCode === 0;
      const fullOutput = (
        benchResult.stdout +
        "\n" +
        benchResult.stderr
      ).trim();
      const outputLines = fullOutput.split("\n");
      const last80 = outputLines.slice(-80).join("\n");

      // Parse METRIC lines from output
      const metricRegex = /^METRIC\s+([\w.µ]+)=(\S+)\s*$/gm;
      const parsedMetrics: Record<string, number> = {};
      let match;
      while ((match = metricRegex.exec(fullOutput)) !== null) {
        const value = Number(match[2]);
        if (Number.isFinite(value)) {
          parsedMetrics[match[1]] = value;
        }
      }
      const hasParsedMetrics = Object.keys(parsedMetrics).length > 0;
      const parsedPrimary = parsedMetrics[state.config.metric_name] ?? null;

      // Run checks if benchmark passed and checks script exists
      let checksResult: { passed: boolean; output: string } | null = null;
      if (passed && existsSync(CHECKS_SCRIPT)) {
        const checksProc = await run(`bash ${CHECKS_SCRIPT}`);
        checksResult = {
          passed: checksProc.exitCode === 0,
          output: (checksProc.stdout + "\n" + checksProc.stderr)
            .trim()
            .split("\n")
            .slice(-40)
            .join("\n"),
        };
      }

      const statusEmoji = passed
        ? checksResult === null || checksResult.passed
          ? "✅"
          : "⚠️"
        : "❌";
      let result = `${statusEmoji} Benchmark ${passed ? "PASSED" : "FAILED"} in ${durationSec}s\n`;
      result += `Exit code: ${benchResult.exitCode}\n`;

      if (state.config && bestMetric() !== null) {
        result += `📊 Current best ${state.config.metric_name}: ${bestMetric()}${state.config.metric_unit}\n`;
      }

      if (checksResult) {
        result += `Checks: ${checksResult.passed ? "PASSED ✅" : "FAILED ❌"}\n`;
        if (!checksResult.passed) {
          result += `Checks output:\n${checksResult.output}\n`;
        }
      }

      if (hasParsedMetrics) {
        result += `\n📐 Parsed metrics:`;
        if (parsedPrimary !== null) {
          result += ` ★ ${state.config.metric_name}=${parsedPrimary}`;
        }
        const secondary = Object.entries(parsedMetrics).filter(
          ([k]) => k !== state.config!.metric_name,
        );
        for (const [name, value] of secondary) {
          result += ` ${name}=${value}`;
        }
        result += `\nUse these values directly in log_experiment (metric: ${parsedPrimary ?? "?"}, metrics: {${secondary.map(([k, v]) => `"${k}": ${v}`).join(", ")}})\n`;
      }

      result += `\n--- Last 80 lines ---\n${last80}`;

      return result;
    },
  });

  // ── Tool: log_experiment ──────────────────────────────────────────────
  amp.registerTool({
    name: "log_experiment",
    description:
      'Record experiment result. Auto-commits on "keep", auto-reverts on "discard"/"crash"/"checks_failed". ' +
      "Appends to autoresearch.jsonl. Shows updated dashboard.",
    inputSchema: {
      type: "object",
      properties: {
        commit: {
          type: "string",
          description: "Git commit hash (short, 7 chars)",
        },
        metric: {
          type: "number",
          description: "Primary metric value (0 for crashes)",
        },
        status: {
          type: "string",
          description: '"keep" | "discard" | "crash" | "checks_failed"',
        },
        description: {
          type: "string",
          description: "Short description of what was tried",
        },
        metrics: {
          type: "object",
          description: "Additional secondary metrics to track",
          additionalProperties: { type: "number" },
        },
        force: {
          type: "boolean",
          description:
            "Allow adding a new secondary metric not previously tracked",
        },
      },
      required: ["commit", "metric", "status", "description"],
    },
    async execute(input) {
      if (!state.active || !state.config) {
        return "❌ No active autoresearch session. Call init_experiment first.";
      }

      const commit = input.commit as string;
      const metric = input.metric as number;
      const status = input.status as ExperimentEntry["status"];
      const description = input.description as string;
      const metrics = (input.metrics as Record<string, number>) ?? undefined;
      const force = (input.force as boolean) ?? false;

      // Validate secondary metrics
      if (metrics) {
        const newKeys = Object.keys(metrics).filter(
          (k) => !state.secondaryMetricKeys.includes(k),
        );
        if (newKeys.length > 0 && !force) {
          return `❌ New secondary metrics detected: ${newKeys.join(", ")}. Pass force: true to allow.`;
        }
        const missingKeys = state.secondaryMetricKeys.filter(
          (k) => !(k in metrics),
        );
        if (missingKeys.length > 0) {
          return `❌ Missing previously tracked secondary metrics: ${missingKeys.join(", ")}`;
        }
        for (const k of newKeys) state.secondaryMetricKeys.push(k);
      } else if (state.secondaryMetricKeys.length > 0) {
        return `❌ Missing previously tracked secondary metrics: ${state.secondaryMetricKeys.join(", ")}`;
      }

      const entry: ExperimentEntry = {
        timestamp: new Date().toISOString(),
        commit,
        metric,
        status,
        description,
        ...(metrics ? { metrics } : {}),
      };

      state.entries.push(entry);

      // Persist to jsonl
      appendFileSync(JSONL_FILE, JSON.stringify(entry) + "\n");

      // Auto-commit or auto-revert
      if (status === "keep") {
        await run(
          `git add -A && git commit -m "autoresearch: ${description.replace(/"/g, '\\"')}"`,
        );
      } else {
        await run(
          `git add ${JSONL_FILE} ${MD_FILE} 2>/dev/null; git checkout -- . ; git clean -fd 2>/dev/null`,
        );
      }

      const best = bestMetric();
      const delta =
        best !== null ? ` (${deltaPercent(metric, best)} vs best)` : "";
      const icon =
        status === "keep"
          ? "✅"
          : status === "discard"
            ? "❌"
            : status === "crash"
              ? "💥"
              : "⚠️";

      return (
        `${icon} Experiment #${state.entries.length}: ${status}\n` +
        `${state.config.metric_name}: ${metric}${state.config.metric_unit}${delta}\n` +
        `Description: ${description}\n\n` +
        dashboardTable()
      );
    },
  });

  // ── Tool: autoresearch_status ──────────────────────────────────────────
  amp.registerTool({
    name: "autoresearch_status",
    description:
      "Show the current autoresearch session status and experiment dashboard.",
    inputSchema: { type: "object", properties: {} },
    async execute() {
      if (!state.active || !state.config) {
        return "No active autoresearch session.";
      }
      return dashboardTable();
    },
  });

  // ── Commands ──────────────────────────────────────────────────────────
  amp.registerCommand(
    "autoresearch",
    {
      title: "Start / Resume",
      category: "Autoresearch",
      description: "Start or resume an autoresearch optimization loop",
    },
    async (ctx) => {
      const goal = await ctx.ui.input({
        title: "Autoresearch Goal",
        helpText: "e.g. optimize unit test runtime, monitor correctness",
        submitButtonText: "Start",
      });
      if (!goal) return;

      state.active = true;
      state.autoResumeCount = 0;

      await ctx.ui.notify(
        `Autoresearch mode ON. Send a message to start the loop: "${goal}"`,
      );
    },
  );

  amp.registerCommand(
    "autoresearch-off",
    {
      title: "Stop",
      category: "Autoresearch",
      description: "Stop the autoresearch loop (keeps data)",
    },
    async (ctx) => {
      state.active = false;
      state.autoResumeCount = 0;
      await ctx.ui.notify(
        "Autoresearch stopped. Data preserved in autoresearch.jsonl.",
      );
    },
  );

  amp.registerCommand(
    "autoresearch-clear",
    {
      title: "Clear",
      category: "Autoresearch",
      description: "Delete autoresearch.jsonl and reset all state",
    },
    async (ctx) => {
      const confirmed = await ctx.ui.confirm({
        title: "Clear autoresearch data?",
        message:
          "This will delete autoresearch.jsonl and reset all session state.",
        confirmButtonText: "Clear",
      });
      if (!confirmed) return;

      state.active = false;
      state.config = null;
      state.entries = [];
      state.secondaryMetricKeys = [];
      state.autoResumeCount = 0;

      try {
        unlinkSync(JSONL_FILE);
      } catch {
        /* ignore */
      }

      await ctx.ui.notify("Autoresearch data cleared.");
    },
  );

  amp.registerCommand(
    "autoresearch-dashboard",
    {
      title: "Dashboard",
      category: "Autoresearch",
      description: "Show the experiment results dashboard",
    },
    async (ctx) => {
      if (!state.active || !state.config) {
        await ctx.ui.notify("No active autoresearch session.");
        return;
      }
      await ctx.ui.notify(dashboardTable());
    },
  );

  // ── Auto-resume on agent.end ──────────────────────────────────────────
  // amp.on('agent.end', async (event: AgentEndEvent, ctx: PluginEventContext) => {
  // 	if (!state.active || !state.config) return
  // 	if (event.status !== 'done') return
  // 	if (state.autoResumeCount >= AUTO_RESUME_MAX_TURNS) return
  // 	if (state.entries.length >= state.maxIterations) return
  //
  // 	const now = Date.now()
  // 	if (now - state.lastResumeTime < AUTO_RESUME_COOLDOWN_MS) return
  //
  // 	state.autoResumeCount++
  // 	state.lastResumeTime = now
  //
  // 	return {
  // 		action: 'continue' as const,
  // 		userMessage:
  // 			`Continue the autoresearch loop. ${summaryLine()}\n` +
  // 			'Read autoresearch.md and git log for context. Pick the next idea, edit code, run_experiment, and log_experiment. Keep iterating.',
  // 	}
  // })

  // ── Inject autoresearch context into agent.start ──────────────────────
  // amp.on("agent.start", async () => {
  //   if (!state.active || !state.config) return {};
  //
  //   let mdContent = "";
  //   try {
  //     if (existsSync(MD_FILE))
  //       mdContent = readFileSync(MD_FILE, "utf-8").trim();
  //   } catch {
  //     /* no md file */
  //   }
  //
  //   const context = [
  //     "## Autoresearch Mode (ACTIVE)",
  //     `You are in autoresearch mode optimizing "${state.config.name}".`,
  //     `Primary metric: ${state.config.metric_name} (${state.config.metric_unit || "unitless"}, ${state.config.direction} is better).`,
  //     "Use init_experiment, run_experiment, and log_experiment tools. NEVER STOP until interrupted.",
  //     "Be careful not to overfit to the benchmarks and do not cheat on the benchmarks.",
  //   ];
  //
  //   if (mdContent) {
  //     context.push(`\nExperiment rules from ${MD_FILE}:\n${mdContent}`);
  //   }
  //
  //   return {
  //     message: { content: context.join("\n"), display: true },
  //   };
  // });
}
