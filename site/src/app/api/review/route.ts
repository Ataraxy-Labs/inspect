import { NextRequest, NextResponse } from "next/server";
import { fetchPr, fetchPrDiff, isNoiseFile } from "@/lib/github";
import { reviewDeepV2 } from "@/lib/openai";
import { validateApiKey } from "@/lib/validate-key";

export const maxDuration = 60;

export async function POST(req: NextRequest) {
  const keyResult = await validateApiKey(req);
  if (!keyResult.valid) return keyResult.response;

  const openaiKey = process.env.OPENAI_API_KEY;
  const githubToken = process.env.GITHUB_TOKEN;
  const model = process.env.OPENAI_MODEL || "gpt-4o";

  if (!openaiKey || !githubToken) {
    return NextResponse.json(
      { error: "Server missing OPENAI_API_KEY or GITHUB_TOKEN" },
      { status: 500 }
    );
  }

  let body: { repo?: string; pr_number?: number };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  const { repo, pr_number } = body;
  if (!repo || !pr_number) {
    return NextResponse.json(
      { error: 'Required fields: "repo" (owner/repo), "pr_number" (integer)' },
      { status: 400 }
    );
  }

  const start = Date.now();

  try {
    // Fetch PR metadata and diff in parallel
    const [pr, diff] = await Promise.all([
      fetchPr(githubToken, repo, pr_number),
      fetchPrDiff(githubToken, repo, pr_number),
    ]);

    const triageMs = Date.now() - start;

    const visibleFiles = pr.files.filter((f) => !isNoiseFile(f.filename));

    // Run LLM review
    const reviewStart = Date.now();
    const findings = await reviewDeepV2(openaiKey, model, pr.title, diff, 15);
    const reviewMs = Date.now() - reviewStart;

    const totalMs = Date.now() - start;

    return NextResponse.json({
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
    return NextResponse.json(
      { error: e.message || "Review failed" },
      { status: 500 }
    );
  }
}
