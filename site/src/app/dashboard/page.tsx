"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

interface KeySummary {
  id: string;
  name: string;
  prefix: string;
  created_at: string;
  last_used_at: string | null;
  request_count: number;
}

export default function DashboardPage() {
  const [keys, setKeys] = useState<KeySummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch("/api/keys")
      .then((r) => r.json())
      .then((data) => {
        setKeys(data.keys || []);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  const totalRequests = keys.reduce((sum, k) => sum + k.request_count, 0);
  const lastActivity = keys
    .map((k) => k.last_used_at)
    .filter(Boolean)
    .sort()
    .pop();

  return (
    <div>
      <h1
        className="text-3xl font-bold mb-8"
        style={{ fontFamily: "var(--font-heading)" }}
      >
        Dashboard
      </h1>

      {loading ? (
        <p className="text-gray-500">Loading...</p>
      ) : (
        <div className="grid md:grid-cols-3 gap-6 mb-10">
          <Link
            href="/dashboard/keys"
            className="border border-white/10 rounded-lg p-6 hover:border-white/20 transition-colors"
          >
            <p
              className="text-sm text-gray-500 uppercase tracking-wider mb-2"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              API Keys
            </p>
            <p
              className="text-4xl font-bold text-white"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              {keys.length}
            </p>
          </Link>

          <Link
            href="/dashboard/usage"
            className="border border-white/10 rounded-lg p-6 hover:border-white/20 transition-colors"
          >
            <p
              className="text-sm text-gray-500 uppercase tracking-wider mb-2"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              Total Requests
            </p>
            <p
              className="text-4xl font-bold text-white"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              {totalRequests}
            </p>
          </Link>

          <div className="border border-white/10 rounded-lg p-6">
            <p
              className="text-sm text-gray-500 uppercase tracking-wider mb-2"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              Last Activity
            </p>
            <p className="text-lg text-white">
              {lastActivity
                ? new Date(lastActivity).toLocaleDateString()
                : "No activity yet"}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
