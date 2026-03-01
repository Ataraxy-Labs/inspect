import { randomBytes, createHash } from "crypto";

export function generateApiKey(): string {
  return "insp_" + randomBytes(16).toString("hex");
}

export function hashApiKey(key: string): string {
  return createHash("sha256").update(key).digest("hex");
}

export function extractPrefix(key: string): string {
  return key.slice(0, 13);
}
