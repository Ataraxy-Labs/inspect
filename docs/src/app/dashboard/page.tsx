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
        style={{
          fontSize: 22,
          fontWeight: 600,
          color: "var(--accent)",
          marginBottom: 32,
          letterSpacing: "-0.5px",
        }}
      >
        Dashboard
      </h1>

      {loading ? (
        <p style={{ color: "var(--dim)" }}>Loading...</p>
      ) : (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr 1fr",
            gap: 16,
            marginBottom: 40,
          }}
        >
          <Link
            href="/dashboard/keys"
            style={{
              border: "1px solid var(--border)",
              borderRadius: 8,
              padding: 20,
              textDecoration: "none",
              transition: "border-color 0.2s",
            }}
          >
            <p
              style={{
                fontSize: 11,
                color: "var(--dim)",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                marginBottom: 8,
              }}
            >
              API Keys
            </p>
            <p
              style={{
                fontSize: 32,
                fontWeight: 700,
                color: "var(--accent)",
              }}
            >
              {keys.length}
            </p>
          </Link>

          <Link
            href="/dashboard/usage"
            style={{
              border: "1px solid var(--border)",
              borderRadius: 8,
              padding: 20,
              textDecoration: "none",
              transition: "border-color 0.2s",
            }}
          >
            <p
              style={{
                fontSize: 11,
                color: "var(--dim)",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                marginBottom: 8,
              }}
            >
              Total Requests
            </p>
            <p
              style={{
                fontSize: 32,
                fontWeight: 700,
                color: "var(--accent)",
              }}
            >
              {totalRequests}
            </p>
          </Link>

          <div
            style={{
              border: "1px solid var(--border)",
              borderRadius: 8,
              padding: 20,
            }}
          >
            <p
              style={{
                fontSize: 11,
                color: "var(--dim)",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                marginBottom: 8,
              }}
            >
              Last Activity
            </p>
            <p style={{ fontSize: 16, color: "var(--accent)" }}>
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
