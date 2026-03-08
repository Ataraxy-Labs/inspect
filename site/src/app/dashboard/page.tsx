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
  const [newKeyName, setNewKeyName] = useState("");
  const [createdKey, setCreatedKey] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copiedCmd, setCopiedCmd] = useState(false);

  const fetchKeys = () => {
    fetch("/api/keys")
      .then((r) => r.json())
      .then((data) => {
        setKeys(data.keys || []);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  };

  useEffect(() => {
    fetchKeys();
  }, []);

  const createKey = async () => {
    if (!newKeyName.trim()) return;
    setCreating(true);
    try {
      const res = await fetch("/api/keys", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: newKeyName.trim() }),
      });
      const data = await res.json();
      if (data.key) {
        setCreatedKey(data.key);
        setNewKeyName("");
        fetchKeys();
      }
    } finally {
      setCreating(false);
    }
  };

  const revokeKey = async (id: string) => {
    await fetch(`/api/keys/${id}`, { method: "DELETE" });
    fetchKeys();
  };

  const copyKey = () => {
    if (createdKey) {
      navigator.clipboard.writeText(createdKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

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
          fontSize: 28,
          fontWeight: 700,
          color: "var(--accent)",
          letterSpacing: "-1px",
          marginBottom: 12,
        }}
      >
        Dashboard
      </h1>
      <p
        style={{
          fontSize: 14,
          color: "var(--dim)",
          lineHeight: 1.7,
          marginBottom: 32,
        }}
      >
        Manage your API keys and monitor usage.
      </p>

      {loading ? (
        <p style={{ fontSize: 13, color: "var(--dim)" }}>Loading...</p>
      ) : (
        <>
          <div className="stat-cards">
            <Link
              href="/dashboard/keys"
              className="stat-card"
              style={{ borderColor: "var(--green)", textDecoration: "none" }}
            >
              <div className="stat-value" style={{ color: "var(--green)" }}>
                {keys.length}
              </div>
              <div className="stat-label">API keys</div>
            </Link>

            <Link
              href="/dashboard/usage"
              className="stat-card"
              style={{ borderColor: "var(--cyan)", textDecoration: "none" }}
            >
              <div className="stat-value" style={{ color: "var(--cyan)" }}>
                {totalRequests}
              </div>
              <div className="stat-label">total requests</div>
            </Link>

            <div className="stat-card" style={{ borderColor: "var(--border)" }}>
              <div
                className="stat-value"
                style={{
                  color: "var(--dim)",
                  fontSize: lastActivity ? 32 : 16,
                }}
              >
                {lastActivity
                  ? new Date(lastActivity).toLocaleDateString()
                  : "No activity yet"}
              </div>
              <div className="stat-label">last activity</div>
            </div>
          </div>

          {/* Create key */}
          <section style={{ borderTop: "1px solid var(--border)", paddingTop: 32 }}>
            <h2 style={{ fontSize: 18, fontWeight: 600, color: "var(--accent)", margin: 0, marginBottom: 16 }}>
              Create API key
            </h2>
            <div style={{ display: "flex", gap: 10, marginBottom: 20 }}>
              <input
                type="text"
                value={newKeyName}
                onChange={(e) => setNewKeyName(e.target.value)}
                placeholder="Key name (e.g. CI pipeline)"
                onKeyDown={(e) => e.key === "Enter" && createKey()}
                style={{
                  flex: 1,
                  padding: "8px 12px",
                  background: "var(--surface)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--fg)",
                  fontSize: 13,
                  fontFamily: "var(--mono)",
                  outline: "none",
                }}
              />
              <button
                onClick={createKey}
                disabled={creating || !newKeyName.trim()}
                style={{
                  padding: "8px 20px",
                  background: "var(--accent)",
                  color: "var(--bg)",
                  border: "none",
                  borderRadius: 6,
                  fontSize: 13,
                  fontWeight: 600,
                  fontFamily: "var(--mono)",
                  cursor: creating || !newKeyName.trim() ? "default" : "pointer",
                  opacity: creating || !newKeyName.trim() ? 0.4 : 1,
                }}
              >
                {creating ? "Creating..." : "Create"}
              </button>
            </div>

            {/* Show created key */}
            {createdKey && (
              <div
                style={{
                  border: "1px solid var(--yellow)",
                  borderRadius: 8,
                  padding: 16,
                  marginBottom: 20,
                  background: "#facc1508",
                }}
              >
                <p style={{ fontSize: 13, color: "var(--yellow)", marginBottom: 10 }}>
                  Copy this key now. It won&apos;t be shown again.
                </p>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <code
                    style={{
                      flex: 1,
                      padding: "8px 12px",
                      background: "var(--surface)",
                      borderRadius: 6,
                      fontSize: 12,
                      color: "var(--fg)",
                      wordBreak: "break-all",
                    }}
                  >
                    {createdKey}
                  </code>
                  <button
                    onClick={copyKey}
                    style={{
                      padding: "8px 16px",
                      border: "1px solid var(--border)",
                      borderRadius: 6,
                      background: "transparent",
                      color: copied ? "var(--green)" : "var(--fg)",
                      fontSize: 13,
                      fontFamily: "var(--mono)",
                      cursor: "pointer",
                    }}
                  >
                    {copied ? "Copied" : "Copy"}
                  </button>
                </div>
                <button
                  onClick={() => setCreatedKey(null)}
                  style={{
                    marginTop: 10,
                    background: "none",
                    border: "none",
                    color: "var(--dim)",
                    fontSize: 12,
                    fontFamily: "var(--mono)",
                    cursor: "pointer",
                  }}
                >
                  Dismiss
                </button>
              </div>
            )}
          </section>

          {/* Keys table */}
          {keys.length > 0 && (
            <section style={{ borderTop: "1px solid var(--border)", paddingTop: 32 }}>
              <h2 style={{ fontSize: 18, fontWeight: 600, color: "var(--accent)", margin: 0, marginBottom: 16 }}>
                Your keys
              </h2>
              <table>
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Key</th>
                    <th>Created</th>
                    <th style={{ textAlign: "right" }}>Requests</th>
                    <th style={{ textAlign: "right" }}></th>
                  </tr>
                </thead>
                <tbody>
                  {keys.map((k) => (
                    <tr key={k.id}>
                      <td style={{ color: "var(--accent)" }}>{k.name}</td>
                      <td>
                        <code style={{ fontSize: 12, color: "var(--cyan)" }}>
                          {k.prefix}...
                        </code>
                      </td>
                      <td>{new Date(k.created_at).toLocaleDateString()}</td>
                      <td style={{ textAlign: "right", color: "var(--accent)" }}>
                        {k.request_count}
                      </td>
                      <td style={{ textAlign: "right" }}>
                        <button
                          onClick={() => revokeKey(k.id)}
                          style={{
                            background: "none",
                            border: "none",
                            color: "var(--red)",
                            fontSize: 13,
                            fontFamily: "var(--mono)",
                            cursor: "pointer",
                          }}
                        >
                          Revoke
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}

          {/* Quick start */}
          <section style={{ borderTop: "1px solid var(--border)", paddingTop: 32 }}>
            <h2 style={{ fontSize: 18, fontWeight: 600, color: "var(--accent)", margin: 0, marginBottom: 16 }}>
              Quick start
            </h2>
            <div className="terminal">
              <div className="terminal-bar" style={{ justifyContent: "space-between" }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <div className="terminal-dot" />
                  <div className="terminal-dot" />
                  <div className="terminal-dot" />
                  <div className="terminal-title">~/project</div>
                </div>
                <button
                  onClick={() => {
                    const cmd = `curl -X POST https://inspect.ataraxy-labs.com/api/triage \\\n  -H "Authorization: Bearer insp_..." \\\n  -H "Content-Type: application/json" \\\n  -d '{"repo":"owner/repo","pr_number":123}'`;
                    navigator.clipboard.writeText(cmd);
                    setCopiedCmd(true);
                    setTimeout(() => setCopiedCmd(false), 2000);
                  }}
                  style={{
                    padding: "4px 12px",
                    border: "1px solid #444",
                    borderRadius: 4,
                    background: "var(--surface)",
                    color: copiedCmd ? "var(--green)" : "var(--fg)",
                    fontSize: 12,
                    fontFamily: "var(--mono)",
                    cursor: "pointer",
                  }}
                >
                  {copiedCmd ? "Copied" : "Copy"}
                </button>
              </div>
              <div className="terminal-body">
                <pre
                  dangerouslySetInnerHTML={{
                    __html: `<span class="cmd">$ curl -X POST https://inspect.ataraxy-labs.com/api/triage \\</span>
<span class="cmd">    -H "Authorization: Bearer insp_..." \\</span>
<span class="cmd">    -H "Content-Type: application/json" \\</span>
<span class="cmd">    -d '{"repo":"owner/repo","pr_number":123}'</span>`,
                  }}
                />
              </div>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
