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
        className="text-3xl font-bold mb-8"
        style={{ fontFamily: "var(--font-heading)" }}
      >
        API Keys
      </h1>

      {/* Create key */}
      <div className="border border-white/10 rounded-lg p-6 mb-8">
        <h2
          className="text-sm text-gray-500 uppercase tracking-wider mb-4"
          style={{ fontFamily: "var(--font-heading)" }}
        >
          Create API Key
        </h2>
        <div className="flex gap-3">
          <input
            type="text"
            value={newKeyName}
            onChange={(e) => setNewKeyName(e.target.value)}
            placeholder="Key name (e.g. CI pipeline)"
            className="flex-1 px-4 py-2 bg-white/5 border border-white/10 rounded-lg text-white placeholder-gray-600 focus:outline-none focus:border-white/30"
            onKeyDown={(e) => e.key === "Enter" && createKey()}
          />
          <button
            onClick={createKey}
            disabled={creating || !newKeyName.trim()}
            className="px-6 py-2 bg-white text-black font-semibold rounded-lg hover:bg-gray-200 transition-colors text-sm disabled:opacity-50"
            style={{ fontFamily: "var(--font-heading)" }}
          >
            {creating ? "Creating..." : "Create"}
          </button>
        </div>
      </div>

      {/* Show created key */}
      {createdKey && (
        <div className="border border-yellow-500/30 bg-yellow-500/5 rounded-lg p-6 mb-8">
          <p className="text-yellow-400 text-sm mb-3">
            Copy this key now. It won&apos;t be shown again.
          </p>
          <div className="flex items-center gap-3">
            <code className="flex-1 px-4 py-2 bg-black/50 rounded text-sm text-white font-mono break-all">
              {createdKey}
            </code>
            <button
              onClick={copyKey}
              className="px-4 py-2 border border-white/20 rounded-lg hover:border-white/40 transition-colors text-sm shrink-0"
            >
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <button
            onClick={() => setCreatedKey(null)}
            className="text-gray-500 text-sm mt-3 hover:text-gray-300"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Keys table */}
      {loading ? (
        <p className="text-gray-500">Loading...</p>
      ) : keys.length === 0 ? (
        <p className="text-gray-500">No API keys yet. Create one above.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-white/10">
                <th className="text-left py-3 pr-4 text-gray-500 font-normal">Name</th>
                <th className="text-left py-3 px-4 text-gray-500 font-normal">Key</th>
                <th className="text-left py-3 px-4 text-gray-500 font-normal">Created</th>
                <th className="text-left py-3 px-4 text-gray-500 font-normal">Last Used</th>
                <th className="text-right py-3 px-4 text-gray-500 font-normal">Requests</th>
                <th className="text-right py-3 pl-4 text-gray-500 font-normal"></th>
              </tr>
            </thead>
            <tbody>
              {keys.map((k) => (
                <tr key={k.id} className="border-b border-white/5">
                  <td className="py-3 pr-4 text-white">{k.name}</td>
                  <td className="py-3 px-4 text-gray-400 font-mono text-xs">
                    {k.prefix}...
                  </td>
                  <td className="py-3 px-4 text-gray-400">
                    {new Date(k.created_at).toLocaleDateString()}
                  </td>
                  <td className="py-3 px-4 text-gray-400">
                    {k.last_used_at
                      ? new Date(k.last_used_at).toLocaleDateString()
                      : "Never"}
                  </td>
                  <td className="py-3 px-4 text-right text-white">
                    {k.request_count}
                  </td>
                  <td className="py-3 pl-4 text-right">
                    <button
                      onClick={() => revokeKey(k.id)}
                      className="text-red-400 hover:text-red-300 text-sm"
                    >
                      Revoke
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
