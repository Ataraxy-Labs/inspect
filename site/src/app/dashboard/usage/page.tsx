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
        className="text-3xl font-bold mb-8"
        style={{ fontFamily: "var(--font-heading)" }}
      >
        Usage
      </h1>

      {loading ? (
        <p className="text-gray-500">Loading...</p>
      ) : keys.length === 0 ? (
        <p className="text-gray-500">No API keys yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-white/10">
                <th className="text-left py-3 pr-4 text-gray-500 font-normal">Name</th>
                <th className="text-left py-3 px-4 text-gray-500 font-normal">Key</th>
                <th className="text-right py-3 px-4 text-gray-500 font-normal">Requests</th>
                <th className="text-left py-3 pl-4 text-gray-500 font-normal">Last Used</th>
              </tr>
            </thead>
            <tbody>
              {keys.map((k) => (
                <tr key={k.id} className="border-b border-white/5">
                  <td className="py-3 pr-4 text-white">{k.name}</td>
                  <td className="py-3 px-4 text-gray-400 font-mono text-xs">
                    {k.prefix}...
                  </td>
                  <td className="py-3 px-4 text-right text-white">
                    {k.request_count}
                  </td>
                  <td className="py-3 pl-4 text-gray-400">
                    {k.last_used_at
                      ? new Date(k.last_used_at).toLocaleDateString()
                      : "Never"}
                  </td>
                </tr>
              ))}
              <tr className="border-t border-white/10">
                <td className="py-3 pr-4 text-white font-semibold">Total</td>
                <td className="py-3 px-4"></td>
                <td className="py-3 px-4 text-right text-white font-semibold">
                  {totalRequests}
                </td>
                <td className="py-3 pl-4"></td>
              </tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
