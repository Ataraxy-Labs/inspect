import { NextRequest, NextResponse } from "next/server";
import { validateApiKey } from "@/lib/validate-key";
import { runReview } from "@/lib/run-review";
import { getSupabase } from "@/lib/supabase";

export const maxDuration = 60;

export async function POST(req: NextRequest) {
  const keyResult = await validateApiKey(req);
  if (!keyResult.valid) return keyResult.response;

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

  try {
    const result = await runReview(repo, pr_number);

    // Store review result (fire and forget)
    const supabase = getSupabase();
    supabase
      .from("reviews")
      .insert({
        user_id: keyResult.userId,
        api_key_id: keyResult.keyId,
        repo,
        pr_number,
        pr_title: result.pr.title,
        status: "complete",
        findings: result.findings,
        summary: result.summary,
        timing: result.timing,
        pr_meta: result.pr,
        completed_at: new Date().toISOString(),
      })
      .then(() => {});

    return NextResponse.json(result);
  } catch (e: any) {
    return NextResponse.json(
      { error: e.message || "Review failed" },
      { status: 500 }
    );
  }
}
