import { auth } from "@clerk/nextjs/server";
import { NextResponse } from "next/server";
import { getSupabase } from "@/lib/supabase";
import { generateApiKey, hashApiKey, extractPrefix } from "@/lib/keys";

export async function GET() {
  const { userId } = await auth();
  if (!userId) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const supabase = getSupabase();
  const { data, error } = await supabase
    .from("api_keys")
    .select("id, name, prefix, created_at, last_used_at, request_count")
    .eq("user_id", userId)
    .is("revoked_at", null)
    .order("created_at", { ascending: false });

  if (error) {
    return NextResponse.json({ error: error.message }, { status: 500 });
  }

  return NextResponse.json({ keys: data });
}

export async function POST(req: Request) {
  const { userId } = await auth();
  if (!userId) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  let body: { name?: string };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  const name = body.name?.trim();
  if (!name) {
    return NextResponse.json({ error: "Name is required" }, { status: 400 });
  }

  const rawKey = generateApiKey();
  const keyHash = hashApiKey(rawKey);
  const prefix = extractPrefix(rawKey);

  const supabase = getSupabase();
  const { data, error } = await supabase
    .from("api_keys")
    .insert({
      user_id: userId,
      name,
      prefix,
      key_hash: keyHash,
    })
    .select("id, name, prefix, created_at")
    .single();

  if (error) {
    return NextResponse.json({ error: error.message }, { status: 500 });
  }

  return NextResponse.json({ ...data, key: rawKey }, { status: 201 });
}
