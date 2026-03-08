import { fetchPr, fetchPrDiff, isNoiseFile, PrInfo } from "./github";
import { Finding, reviewDeepV2 } from "./openai";

export interface ReviewResult {
  pr: {
    number: number;
    title: string;
    state: string;
    additions: number;
    deletions: number;
    changed_files: number;
  };
  findings: Finding[];
  summary: {
    total_findings: number;
    files_analyzed: number;
    files_skipped: number;
  };
  timing: {
    triage_ms: number;
    review_ms: number;
    total_ms: number;
  };
}

export async function runReview(
  repo: string,
  prNumber: number
): Promise<ReviewResult> {
  const openaiKey = process.env.OPENAI_API_KEY;
  const githubToken = process.env.GITHUB_TOKEN;
  const model = process.env.OPENAI_MODEL || "gpt-4o";

  if (!openaiKey || !githubToken) {
    throw new Error("Server missing OPENAI_API_KEY or GITHUB_TOKEN");
  }

  const start = Date.now();

  const [pr, diff] = await Promise.all([
    fetchPr(githubToken, repo, prNumber),
    fetchPrDiff(githubToken, repo, prNumber),
  ]);

  const triageMs = Date.now() - start;
  const visibleFiles = pr.files.filter((f) => !isNoiseFile(f.filename));

  const reviewStart = Date.now();
  const findings = await reviewDeepV2(openaiKey, model, pr.title, diff, 15);
  const reviewMs = Date.now() - reviewStart;
  const totalMs = Date.now() - start;

  return {
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
  };
}
