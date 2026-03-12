import { fetchPr, fetchPrDiff, isNoiseFile, PrInfo } from "./github";
import { Finding, reviewV26, fetchTriage } from "./openai";

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
    entity_triage: boolean;
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
  const model = process.env.OPENAI_MODEL || "gpt-5.2";
  const inspectApiUrl = (process.env.INSPECT_API_URL || "").replace(/"/g, "");
  const inspectApiKey = (process.env.INSPECT_API_KEY || "").replace(/"/g, "");

  console.log(`[review] inspect-api: url=${inspectApiUrl ? "set" : "empty"}, key=${inspectApiKey ? "set" : "empty"}`);

  if (!openaiKey || !githubToken) {
    throw new Error("Server missing OPENAI_API_KEY or GITHUB_TOKEN");
  }

  const start = Date.now();

  // Fetch PR metadata, diff, and entity triage in parallel
  const [pr, diff, triage] = await Promise.all([
    fetchPr(githubToken, repo, prNumber),
    fetchPrDiff(githubToken, repo, prNumber),
    inspectApiUrl && inspectApiKey
      ? fetchTriage(inspectApiKey, inspectApiUrl, repo, prNumber)
      : Promise.resolve(""),
  ]);

  const triageMs = Date.now() - start;
  const visibleFiles = pr.files.filter((f) => !isNoiseFile(f.filename));

  const reviewStart = Date.now();
  const findings = await reviewV26(openaiKey, model, pr.title, diff, triage);
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
      entity_triage: triage ? true : false,
    },
    timing: {
      triage_ms: triageMs,
      review_ms: reviewMs,
      total_ms: totalMs,
    },
  };
}
