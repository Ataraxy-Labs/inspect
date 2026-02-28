import type { VercelRequest, VercelResponse } from "@vercel/node";
import { fetchPrMeta, fetchPrDiff, isNoiseFile } from "./_lib/github";
import { reviewDeepV2 } from "./_lib/openai";

export default async function handler(req: VercelRequest, res: VercelResponse) {
  if (req.method !== "POST") {
    return res.status(405).json({ error: "POST only" });
  }

  const openaiKey = process.env.OPENAI_API_KEY;
  const githubToken = process.env.GITHUB_TOKEN;
  const model = process.env.OPENAI_MODEL || "gpt-4o";

  if (!openaiKey || !githubToken) {
    return res.status(500).json({ error: "Server missing OPENAI_API_KEY or GITHUB_TOKEN" });
  }

  const { repo, pr_number } = req.body || {};
  if (!repo || !pr_number) {
    return res.status(400).json({ error: 'Required: "repo" (owner/repo), "pr_number" (integer)' });
  }

  const start = Date.now();

  try {
    const [pr, diff] = await Promise.all([
      fetchPrMeta(githubToken, repo, pr_number),
      fetchPrDiff(githubToken, repo, pr_number),
    ]);

    const triageMs = Date.now() - start;
    const visibleFiles = pr.files.filter((f) => !isNoiseFile(f.filename));

    const reviewStart = Date.now();
    const findings = await reviewDeepV2(openaiKey, model, pr.title, diff, 15);
    const reviewMs = Date.now() - reviewStart;
    const totalMs = Date.now() - start;

    return res.json({
      pr: {
        number: pr.number,
        title: pr.title,
        state: pr.state,
        additions: pr.additions,
        deletions: pr.deletions,
        changed_files: pr.changed_files,
      },
      findings,
      summary: {
        total_findings: findings.length,
        files_analyzed: visibleFiles.length,
        files_skipped: pr.files.length - visibleFiles.length,
      },
      timing: {
        triage_ms: triageMs,
        review_ms: reviewMs,
        total_ms: totalMs,
      },
    });
  } catch (e: any) {
    return res.status(500).json({ error: e.message || "Review failed" });
  }
}
