"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

interface Review {
  id: string;
  repo: string;
  pr_number: number;
  pr_title: string | null;
  status: string;
  error: string | null;
  summary: { total_findings: number; files_analyzed: number } | null;
  timing: { total_ms: number } | null;
  created_at: string;
  completed_at: string | null;
}

const STATUS_COLORS: Record<string, string> = {
  pending: "var(--dim)",
  triaging: "var(--yellow)",
  reviewing: "var(--blue)",
  complete: "var(--green)",
  error: "var(--red)",
};

export default function ReviewsPage() {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch("/api/reviews?limit=100")
      .then((r) => r.json())
      .then((data) => {
        setReviews(data.reviews || []);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

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
        Reviews
      </h1>

      {loading ? (
        <p style={{ color: "var(--dim)" }}>Loading...</p>
      ) : reviews.length === 0 ? (
        <p style={{ color: "var(--dim)" }}>
          No reviews yet. Run one from the{" "}
          <Link
            href="/dashboard"
            style={{ color: "var(--accent)", textDecoration: "underline" }}
          >
            dashboard
          </Link>
          .
        </p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Repo</th>
              <th>PR</th>
              <th>Title</th>
              <th>Status</th>
              <th style={{ textAlign: "right" }}>Findings</th>
              <th style={{ textAlign: "right" }}>Time</th>
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
                <td
                  style={{
                    color: "var(--fg)",
                    maxWidth: 200,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {r.pr_title || "-"}
                </td>
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
                <td style={{ textAlign: "right", color: "var(--dim)" }}>
                  {r.timing ? `${(r.timing.total_ms / 1000).toFixed(1)}s` : "-"}
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
      )}
    </div>
  );
}
