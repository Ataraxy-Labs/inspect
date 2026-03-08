"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

interface Review {
  id: string;
  repo: string;
  pr_number: number;
  pr_title: string | null;
  status: string;
  summary: { total_findings: number } | null;
  timing: { total_ms: number } | null;
  created_at: string;
}

interface KeySummary {
  request_count: number;
}

const STATUS_COLORS: Record<string, string> = {
  pending: "var(--dim)",
  triaging: "var(--yellow)",
  reviewing: "var(--blue)",
  complete: "var(--green)",
  error: "var(--red)",
};

export default function DashboardPage() {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [keyCount, setKeyCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [repo, setRepo] = useState("");
  const [prNumber, setPrNumber] = useState("");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchData = () => {
    Promise.all([
      fetch("/api/reviews?limit=10").then((r) => r.json()),
      fetch("/api/keys").then((r) => r.json()),
    ])
      .then(([reviewData, keyData]) => {
        setReviews(reviewData.reviews || []);
        setKeyCount((keyData.keys || []).length);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  };

  useEffect(() => {
    fetchData();
  }, []);

  const startReview = async () => {
    if (!repo.trim() || !prNumber.trim()) return;
    setRunning(true);
    setError(null);

    try {
      const res = await fetch("/api/reviews", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          repo: repo.trim(),
          pr_number: parseInt(prNumber, 10),
        }),
      });

      if (!res.ok) {
        const data = await res.json();
        setError(data.error || "Review failed");
      } else {
        setRepo("");
        setPrNumber("");
        fetchData();
      }
    } catch {
      setError("Network error");
    } finally {
      setRunning(false);
    }
  };

  const totalFindings = reviews.reduce(
    (sum, r) => sum + (r.summary?.total_findings || 0),
    0
  );

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

      {/* Run Review */}
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
          Run Review
        </p>
        <div style={{ display: "flex", gap: 12 }}>
          <input
            type="text"
            value={repo}
            onChange={(e) => setRepo(e.target.value)}
            placeholder="owner/repo"
            style={{
              flex: 2,
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
          <input
            type="text"
            value={prNumber}
            onChange={(e) => setPrNumber(e.target.value.replace(/\D/g, ""))}
            placeholder="PR #"
            style={{
              width: 80,
              padding: "8px 12px",
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              color: "var(--fg)",
              fontSize: 13,
              fontFamily: "var(--mono)",
              outline: "none",
            }}
            onKeyDown={(e) => e.key === "Enter" && startReview()}
          />
          <button
            onClick={startReview}
            disabled={running || !repo.trim() || !prNumber.trim()}
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
              opacity: running || !repo.trim() || !prNumber.trim() ? 0.5 : 1,
              whiteSpace: "nowrap",
            }}
          >
            {running ? "Reviewing..." : "Run Review"}
          </button>
        </div>
        {error && (
          <p style={{ color: "var(--red)", fontSize: 12, marginTop: 12 }}>
            {error}
          </p>
        )}
      </div>

      {/* Stat cards */}
      {loading ? (
        <p style={{ color: "var(--dim)" }}>Loading...</p>
      ) : (
        <>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "1fr 1fr 1fr",
              gap: 16,
              marginBottom: 40,
            }}
          >
            <Link
              href="/dashboard/reviews"
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
                Reviews
              </p>
              <p
                style={{
                  fontSize: 32,
                  fontWeight: 700,
                  color: "var(--accent)",
                }}
              >
                {reviews.length}
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
                Findings
              </p>
              <p
                style={{
                  fontSize: 32,
                  fontWeight: 700,
                  color: "var(--accent)",
                }}
              >
                {totalFindings}
              </p>
            </div>

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
                {keyCount}
              </p>
            </Link>
          </div>

          {/* Recent reviews */}
          {reviews.length > 0 && (
            <div>
              <p
                style={{
                  fontSize: 11,
                  color: "var(--dim)",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  marginBottom: 16,
                }}
              >
                Recent Reviews
              </p>
              <table>
                <thead>
                  <tr>
                    <th>Repo</th>
                    <th>PR</th>
                    <th>Status</th>
                    <th style={{ textAlign: "right" }}>Findings</th>
                    <th style={{ textAlign: "right" }}>Date</th>
                  </tr>
                </thead>
                <tbody>
                  {reviews.map((r) => (
                    <tr key={r.id}>
                      <td>
                        <Link
                          href={`/dashboard/reviews/${r.id}`}
                          style={{
                            color: "var(--accent)",
                            textDecoration: "none",
                          }}
                        >
                          {r.repo}
                        </Link>
                      </td>
                      <td style={{ color: "var(--dim)" }}>#{r.pr_number}</td>
                      <td>
                        <span
                          style={{
                            color: STATUS_COLORS[r.status] || "var(--dim)",
                            fontSize: 12,
                          }}
                        >
                          {r.status}
                        </span>
                      </td>
                      <td style={{ textAlign: "right", color: "var(--accent)" }}>
                        {r.summary?.total_findings ?? "-"}
                      </td>
                      <td
                        style={{
                          textAlign: "right",
                          color: "var(--dim)",
                        }}
                      >
                        {new Date(r.created_at).toLocaleDateString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}
