export const dynamic = "force-dynamic";

import Link from "next/link";
import { UserButton } from "@clerk/nextjs";

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div style={{ minHeight: "100vh" }}>
      <nav
        style={{
          borderBottom: "1px solid var(--border)",
          padding: "16px 24px",
        }}
      >
        <div
          style={{
            maxWidth: 800,
            margin: "0 auto",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 32 }}>
            <Link
              href="/dashboard"
              style={{
                fontSize: 18,
                fontWeight: 700,
                color: "var(--accent)",
                letterSpacing: "-0.5px",
                textDecoration: "none",
              }}
            >
              inspect
            </Link>
            <div style={{ display: "flex", gap: 24, fontSize: 13 }}>
              <Link
                href="/dashboard/keys"
                style={{ color: "var(--dim)", textDecoration: "none" }}
              >
                Keys
              </Link>
              <Link
                href="/dashboard/usage"
                style={{ color: "var(--dim)", textDecoration: "none" }}
              >
                Usage
              </Link>
            </div>
          </div>
          <UserButton />
        </div>
      </nav>
      <main style={{ maxWidth: 800, margin: "0 auto", padding: "40px 24px" }}>
        {children}
      </main>
    </div>
  );
}
