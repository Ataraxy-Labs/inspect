import { NextRequest, NextResponse } from "next/server";
import { fetchPr, isNoiseFile } from "@/lib/github";
import { validateApiKey } from "@/lib/validate-key";

export async function POST(req: NextRequest) {
  const keyResult = await validateApiKey(req);
  if (!keyResult.valid) return keyResult.response;

  const githubToken = process.env.GITHUB_TOKEN;

  if (!githubToken) {
    return NextResponse.json(
      { error: "Server missing GITHUB_TOKEN" },
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
    const pr = await fetchPr(githubToken, repo, pr_number);

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

    const timingMs = Date.now() - start;

    return NextResponse.json({
      pr: {
        number: pr.number,
        title: pr.title,
        state: pr.state,
        additions: pr.additions,
        deletions: pr.deletions,
      },
      files_analyzed: files.length,
      files_skipped: pr.files.length - files.length,
      files,
      timing_ms: timingMs,
    });
  } catch (e: any) {
    return NextResponse.json(
      { error: e.message || "Triage failed" },
      { status: 500 }
    );
  }
}
