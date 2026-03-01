import { NextResponse } from "next/server";
import { getSupabase } from "./supabase";
import { hashApiKey } from "./keys";

export async function validateApiKey(
  req: Request
): Promise<{ valid: true; keyId: string } | { valid: false; response: NextResponse }> {
  const authHeader = req.headers.get("authorization");
  if (!authHeader?.startsWith("Bearer ")) {
    return {
      valid: false,
      response: NextResponse.json(
        { error: "Missing Authorization: Bearer <api_key> header" },
        { status: 401 }
      ),
    };
  }

  const rawKey = authHeader.slice(7);
  const keyHash = hashApiKey(rawKey);
  const supabase = getSupabase();

  const { data, error } = await supabase
    .from("api_keys")
    .select("id, revoked_at, request_count")
    .eq("key_hash", keyHash)
    .single();

  if (error || !data) {
    return {
      valid: false,
      response: NextResponse.json({ error: "Invalid API key" }, { status: 401 }),
    };
  }

  if (data.revoked_at) {
    return {
      valid: false,
      response: NextResponse.json({ error: "API key has been revoked" }, { status: 401 }),
    };
  }

  // Increment usage (fire and forget)
  supabase
    .from("api_keys")
    .update({
      last_used_at: new Date().toISOString(),
      request_count: (data.request_count || 0) + 1,
    })
    .eq("id", data.id)
    .then(() => {});

  return { valid: true, keyId: data.id };
}
