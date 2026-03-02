"use client";

import { useEffect, useState } from "react";

interface ApiKey {
  id: string;
  name: string;
  prefix: string;
  request_count: number;
  last_used_at: string | null;
}

export default function UsagePage() {
  const [keys, setKeys] = useState<ApiKey[]>([]);
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
        Usage
      </h1>

      {loading ? (
        <p style={{ color: "var(--dim)" }}>Loading...</p>
      ) : keys.length === 0 ? (
        <p style={{ color: "var(--dim)" }}>No API keys yet.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Key</th>
              <th style={{ textAlign: "right" }}>Requests</th>
              <th>Last Used</th>
            </tr>
          </thead>
          <tbody>
            {keys.map((k) => (
              <tr key={k.id}>
                <td style={{ color: "var(--accent)" }}>{k.name}</td>
                <td style={{ color: "var(--dim)", fontSize: 11 }}>
                  {k.prefix}...
                </td>
                <td style={{ textAlign: "right", color: "var(--accent)" }}>
                  {k.request_count}
                </td>
                <td style={{ color: "var(--dim)" }}>
                  {k.last_used_at
                    ? new Date(k.last_used_at).toLocaleDateString()
                    : "Never"}
                </td>
              </tr>
            ))}
            <tr style={{ borderTop: "1px solid var(--border)" }}>
              <td
                style={{
                  color: "var(--accent)",
                  fontWeight: 600,
                }}
              >
                Total
              </td>
              <td></td>
              <td
                style={{
                  textAlign: "right",
                  color: "var(--accent)",
                  fontWeight: 600,
                }}
              >
                {totalRequests}
              </td>
              <td></td>
            </tr>
          </tbody>
        </table>
      )}
    </div>
  );
}
