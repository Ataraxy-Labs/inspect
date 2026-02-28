import type { VercelRequest, VercelResponse } from "@vercel/node";
import { fetchPrMeta, isNoiseFile } from "./_lib/github";

export default async function handler(req: VercelRequest, res: VercelResponse) {
  if (req.method !== "POST") {
    return res.status(405).json({ error: "POST only" });
  }

  const githubToken = process.env.GITHUB_TOKEN;
  if (!githubToken) {
    return res.status(500).json({ error: "Server missing GITHUB_TOKEN" });
  }

  const { repo, pr_number } = req.body || {};
  if (!repo || !pr_number) {
    return res.status(400).json({ error: 'Required: "repo" (owner/repo), "pr_number" (integer)' });
  }

  const start = Date.now();

  try {
    const pr = await fetchPrMeta(githubToken, repo, pr_number);

    const files = pr.files
      .filter((f) => !isNoiseFile(f.filename))
      .map((f) => ({
        file: f.filename,
        status: f.status,
        additions: f.additions,
        deletions: f.deletions,
        change_size: f.additions + f.deletions,
      }))
      .sort((a, b) => b.change_size - a.change_size);

    return res.json({
      pr: { number: pr.number, title: pr.title, state: pr.state, additions: pr.additions, deletions: pr.deletions },
      files_analyzed: files.length,
      files_skipped: pr.files.length - files.length,
      files,
      timing_ms: Date.now() - start,
    });
  } catch (e: any) {
    return res.status(500).json({ error: e.message || "Triage failed" });
  }
}
