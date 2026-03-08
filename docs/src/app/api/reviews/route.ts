import { NextRequest, NextResponse } from "next/server";
import { auth } from "@clerk/nextjs/server";
import { getSupabase } from "@/lib/supabase";
import { runReview } from "@/lib/run-review";

export const maxDuration = 60;

export async function POST(req: NextRequest) {
  const { userId } = await auth();
  if (!userId) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
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
      { error: 'Required: "repo" and "pr_number"' },
      { status: 400 }
    );
  }

  const supabase = getSupabase();

  // Insert pending row
  const { data: row, error: insertErr } = await supabase
    .from("reviews")
    .insert({
      user_id: userId,
      repo,
      pr_number,
      status: "pending",
    })
    .select("id")
    .single();

  if (insertErr || !row) {
    return NextResponse.json({ error: "Failed to create review" }, { status: 500 });
  }

  const reviewId = row.id;

  try {
    // Update to triaging
    await supabase.from("reviews").update({ status: "triaging" }).eq("id", reviewId);

    const result = await runReview(repo, pr_number);

    // Update with results
    await supabase
      .from("reviews")
      .update({
        status: "complete",
        pr_title: result.pr.title,
        findings: result.findings,
        summary: result.summary,
        timing: result.timing,
        pr_meta: result.pr,
        completed_at: new Date().toISOString(),
      })
      .eq("id", reviewId);

    return NextResponse.json({ id: reviewId, ...result });
  } catch (e: any) {
    await supabase
      .from("reviews")
      .update({ status: "error", error: e.message || "Review failed" })
      .eq("id", reviewId);

    return NextResponse.json(
      { error: e.message || "Review failed" },
      { status: 500 }
    );
  }
}

export async function GET(req: NextRequest) {
  const { userId } = await auth();
  if (!userId) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const limit = parseInt(req.nextUrl.searchParams.get("limit") || "20", 10);

  const supabase = getSupabase();
  const { data, error } = await supabase
    .from("reviews")
    .select("id, repo, pr_number, pr_title, status, error, summary, timing, created_at, completed_at")
    .eq("user_id", userId)
    .order("created_at", { ascending: false })
    .limit(limit);

  if (error) {
    return NextResponse.json({ error: "Failed to fetch reviews" }, { status: 500 });
  }

  return NextResponse.json({ reviews: data || [] });
}
