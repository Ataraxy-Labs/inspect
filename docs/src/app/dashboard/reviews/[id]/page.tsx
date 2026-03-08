"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { useParams } from "next/navigation";

interface Finding {
  issue: string;
  evidence?: string;
  severity?: string;
  file?: string;
}

interface ReviewDetail {
  id: string;
  repo: string;
  pr_number: number;
  pr_title: string | null;
  status: string;
  error: string | null;
  findings: Finding[] | null;
  summary: {
    total_findings: number;
    files_analyzed: number;
    files_skipped: number;
  } | null;
  timing: {
    triage_ms: number;
    review_ms: number;
    total_ms: number;
  } | null;
  pr_meta: {
    additions: number;
    deletions: number;
    changed_files: number;
  } | null;
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

const SEVERITY_COLORS: Record<string, string> = {
  critical: "var(--red)",
  high: "var(--red)",
  medium: "var(--yellow)",
  low: "var(--dim)",
};

export default function ReviewDetailPage() {
  const params = useParams();
  const [review, setReview] = useState<ReviewDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [notFound, setNotFound] = useState(false);

  useEffect(() => {
    fetch(`/api/reviews/${params.id}`)
      .then((r) => {
        if (!r.ok) {
          setNotFound(true);
          setLoading(false);
          return null;
        }
        return r.json();
      })
      .then((data) => {
        if (data) {
          setReview(data);
          setLoading(false);
        }
      })
      .catch(() => {
        setNotFound(true);
        setLoading(false);
      });
  }, [params.id]);

  if (loading) {
    return <p style={{ color: "var(--dim)" }}>Loading...</p>;
  }

  if (notFound || !review) {
    return (
      <div>
        <p style={{ color: "var(--dim)", marginBottom: 16 }}>
          Review not found.
        </p>
        <Link
          href="/dashboard/reviews"
          style={{ color: "var(--accent)", fontSize: 13 }}
        >
          Back to reviews
        </Link>
      </div>
    );
  }

  return (
    <div>
      {/* Header */}
      <div style={{ marginBottom: 32 }}>
        <Link
          href="/dashboard/reviews"
          style={{
            color: "var(--dim)",
            fontSize: 12,
            textDecoration: "none",
            marginBottom: 16,
            display: "inline-block",
          }}
        >
          Reviews
        </Link>

        <h1
          style={{
            fontSize: 22,
            fontWeight: 600,
            color: "var(--accent)",
            marginBottom: 8,
            letterSpacing: "-0.5px",
          }}
        >
          {review.pr_title || `PR #${review.pr_number}`}
        </h1>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 16,
            fontSize: 13,
          }}
        >
          <span style={{ color: "var(--dim)" }}>{review.repo}</span>
          <span style={{ color: "var(--dim)" }}>#{review.pr_number}</span>
          <span
            style={{
              color: STATUS_COLORS[review.status] || "var(--dim)",
              fontWeight: 600,
            }}
          >
            {review.status}
          </span>
        </div>
      </div>

      {/* Error */}
      {review.status === "error" && review.error && (
        <div
          style={{
            border: "1px solid var(--red)",
            borderRadius: 8,
            padding: 16,
            marginBottom: 24,
            fontSize: 13,
            color: "var(--red)",
          }}
        >
          {review.error}
        </div>
      )}

      {/* PR metadata + timing */}
      {(review.pr_meta || review.timing) && (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 16,
            marginBottom: 32,
          }}
        >
          {review.pr_meta && (
            <div
              style={{
                border: "1px solid var(--border)",
                borderRadius: 8,
                padding: 16,
              }}
            >
              <p
                style={{
                  fontSize: 11,
                  color: "var(--dim)",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  marginBottom: 12,
                }}
              >
                Changes
              </p>
              <div
                style={{
                  display: "flex",
                  gap: 24,
                  fontSize: 13,
                }}
              >
                <span>
                  <span style={{ color: "var(--green)" }}>
                    +{review.pr_meta.additions}
                  </span>
                </span>
                <span>
                  <span style={{ color: "var(--red)" }}>
                    -{review.pr_meta.deletions}
                  </span>
                </span>
                <span style={{ color: "var(--dim)" }}>
                  {review.pr_meta.changed_files} files
                </span>
              </div>
            </div>
          )}

          {review.timing && (
            <div
              style={{
                border: "1px solid var(--border)",
                borderRadius: 8,
                padding: 16,
              }}
            >
              <p
                style={{
                  fontSize: 11,
                  color: "var(--dim)",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  marginBottom: 12,
                }}
              >
                Timing
              </p>
              <div
                style={{
                  display: "flex",
                  gap: 24,
                  fontSize: 13,
                  color: "var(--dim)",
                }}
              >
                <span>
                  Triage: {(review.timing.triage_ms / 1000).toFixed(1)}s
                </span>
                <span>
                  Review: {(review.timing.review_ms / 1000).toFixed(1)}s
                </span>
                <span style={{ color: "var(--accent)" }}>
                  Total: {(review.timing.total_ms / 1000).toFixed(1)}s
                </span>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Findings */}
      {review.findings && review.findings.length > 0 && (
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
            Findings ({review.findings.length})
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {review.findings.map((f, i) => (
              <div
                key={i}
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 16,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "flex-start",
                    justifyContent: "space-between",
                    gap: 12,
                    marginBottom: f.evidence || f.file ? 12 : 0,
                  }}
                >
                  <p style={{ fontSize: 13, color: "var(--fg)", flex: 1 }}>
                    {f.issue}
                  </p>
                  {f.severity && (
                    <span
                      style={{
                        fontSize: 10,
                        fontWeight: 600,
                        padding: "2px 8px",
                        borderRadius: 4,
                        color:
                          SEVERITY_COLORS[f.severity.toLowerCase()] ||
                          "var(--dim)",
                        border: `1px solid ${
                          SEVERITY_COLORS[f.severity.toLowerCase()] ||
                          "var(--border)"
                        }`,
                        whiteSpace: "nowrap",
                        flexShrink: 0,
                      }}
                    >
                      {f.severity}
                    </span>
                  )}
                </div>

                {f.file && (
                  <p
                    style={{
                      fontSize: 11,
                      color: "var(--cyan)",
                      marginBottom: f.evidence ? 8 : 0,
                    }}
                  >
                    {f.file}
                  </p>
                )}

                {f.evidence && (
                  <pre
                    style={{
                      fontSize: 12,
                      color: "var(--dim)",
                      background: "var(--surface)",
                      padding: 12,
                      borderRadius: 4,
                      overflow: "auto",
                      whiteSpace: "pre-wrap",
                      lineHeight: 1.5,
                    }}
                  >
                    {f.evidence}
                  </pre>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {review.status === "complete" &&
        (!review.findings || review.findings.length === 0) && (
          <p style={{ color: "var(--green)", fontSize: 13 }}>
            No issues found.
          </p>
        )}
    </div>
  );
}
