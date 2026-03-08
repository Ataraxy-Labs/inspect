"use client";

import { useEffect, useState } from "react";

interface ApiKey {
  id: string;
  name: string;
  prefix: string;
  created_at: string;
  last_used_at: string | null;
  request_count: number;
}

export default function KeysPage() {
  const [keys, setKeys] = useState<ApiKey[]>([]);
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
        API Keys
      </h1>

      {/* Create key */}
      <div
        style={{
          border: "1px solid var(--border)",
          borderRadius: 8,
          padding: 20,
          marginBottom: 32,
        }}
      >
        <p
          style={{
            fontSize: 11,
            color: "var(--dim)",
            textTransform: "uppercase",
            letterSpacing: "0.05em",
            marginBottom: 16,
          }}
        >
          Create API Key
        </p>
        <div style={{ display: "flex", gap: 12 }}>
          <input
            type="text"
            value={newKeyName}
            onChange={(e) => setNewKeyName(e.target.value)}
            placeholder="Key name (e.g. CI pipeline)"
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
            onKeyDown={(e) => e.key === "Enter" && createKey()}
          />
          <button
            onClick={createKey}
            disabled={creating || !newKeyName.trim()}
            style={{
              padding: "8px 20px",
              background: "var(--accent)",
              color: "var(--bg)",
              fontWeight: 600,
              borderRadius: 6,
              border: "none",
              fontSize: 13,
              fontFamily: "var(--mono)",
              cursor: "pointer",
              opacity: creating || !newKeyName.trim() ? 0.5 : 1,
            }}
          >
            {creating ? "Creating..." : "Create"}
          </button>
        </div>
      </div>

      {/* Show created key */}
      {createdKey && (
        <div
          style={{
            border: "1px solid var(--yellow)",
            background: "#facc1508",
            borderRadius: 8,
            padding: 20,
            marginBottom: 32,
          }}
        >
          <p
            style={{
              color: "var(--yellow)",
              fontSize: 13,
              marginBottom: 12,
            }}
          >
            Copy this key now. It won&apos;t be shown again.
          </p>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <code
              style={{
                flex: 1,
                padding: "8px 12px",
                background: "var(--bg)",
                borderRadius: 4,
                fontSize: 13,
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
                color: "var(--fg)",
                fontSize: 13,
                fontFamily: "var(--mono)",
                cursor: "pointer",
                flexShrink: 0,
              }}
            >
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <button
            onClick={() => setCreatedKey(null)}
            style={{
              color: "var(--dim)",
              fontSize: 12,
              marginTop: 12,
              background: "none",
              border: "none",
              cursor: "pointer",
              fontFamily: "var(--mono)",
            }}
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Keys table */}
      {loading ? (
        <p style={{ color: "var(--dim)" }}>Loading...</p>
      ) : keys.length === 0 ? (
        <p style={{ color: "var(--dim)" }}>
          No API keys yet. Create one above.
        </p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Key</th>
              <th>Created</th>
              <th>Last Used</th>
              <th style={{ textAlign: "right" }}>Requests</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {keys.map((k) => (
              <tr key={k.id}>
                <td style={{ color: "var(--accent)" }}>{k.name}</td>
                <td style={{ color: "var(--dim)", fontSize: 11 }}>
                  {k.prefix}...
                </td>
                <td style={{ color: "var(--dim)" }}>
                  {new Date(k.created_at).toLocaleDateString()}
                </td>
                <td style={{ color: "var(--dim)" }}>
                  {k.last_used_at
                    ? new Date(k.last_used_at).toLocaleDateString()
                    : "Never"}
                </td>
                <td style={{ textAlign: "right", color: "var(--accent)" }}>
                  {k.request_count}
                </td>
                <td style={{ textAlign: "right" }}>
                  <button
                    onClick={() => revokeKey(k.id)}
                    style={{
                      color: "var(--red)",
                      fontSize: 12,
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      fontFamily: "var(--mono)",
                    }}
                  >
                    Revoke
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* Quick start */}
      {keys.length > 0 && (
        <div style={{ marginTop: 48 }}>
          <h2
            style={{
              fontSize: 18,
              fontWeight: 600,
              color: "var(--accent)",
              marginBottom: 16,
              letterSpacing: "-0.5px",
            }}
          >
            Quick start
          </h2>
          <div
            style={{
              background: "#0d0d0d",
              border: "1px solid var(--border)",
              borderRadius: 10,
              overflow: "hidden",
            }}
          >
            <div
              style={{
                background: "var(--surface2)",
                padding: "10px 16px",
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ width: 10, height: 10, borderRadius: "50%", background: "#333", display: "inline-block" }} />
                <span style={{ width: 10, height: 10, borderRadius: "50%", background: "#333", display: "inline-block" }} />
                <span style={{ width: 10, height: 10, borderRadius: "50%", background: "#333", display: "inline-block" }} />
                <span style={{ fontSize: 12, color: "var(--dim)", marginLeft: 8 }}>~/project</span>
              </div>
              <button
                onClick={() => {
                  const cmd = `curl -X POST https://inspect.ataraxy-labs.com/api/triage \\\n  -H "Authorization: Bearer ${keys[0].prefix}..." \\\n  -H "Content-Type: application/json" \\\n  -d '{"repo":"owner/repo","pr_number":123}'`;
                  navigator.clipboard.writeText(cmd);
                  setCopiedCmd(true);
                  setTimeout(() => setCopiedCmd(false), 2000);
                }}
                style={{
                  padding: "4px 12px",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  background: "transparent",
                  color: copiedCmd ? "var(--green)" : "var(--dim)",
                  fontSize: 11,
                  fontFamily: "var(--mono)",
                  cursor: "pointer",
                }}
              >
                {copiedCmd ? "Copied" : "Copy"}
              </button>
            </div>
            <div style={{ padding: "20px 24px", fontSize: 13, lineHeight: 1.7 }}>
              <span style={{ color: "var(--dim)" }}>$ </span>
              <span style={{ color: "var(--accent)" }}>curl</span>
              {" -X POST https://inspect.ataraxy-labs.com/api/triage \\\n"}
              {"  -H \"Authorization: Bearer "}
              <span style={{ color: "var(--cyan)" }}>{keys[0].prefix}...</span>
              {"\" \\\n"}
              {"  -H \"Content-Type: application/json\" \\\n"}
              {"  -d '{\"repo\":\"owner/repo\",\"pr_number\":123}'"}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
