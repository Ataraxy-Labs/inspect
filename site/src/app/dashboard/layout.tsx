export const dynamic = "force-dynamic";

import Nav from "@/components/nav";

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div className="container">
      <Nav active="dashboard" />
      <div style={{ padding: "48px 0 12px" }}>
        {children}
      </div>
      <footer>
        <p>
          Built by <a href="https://ataraxy-labs.com">Ataraxy Labs</a>
        </p>
      </footer>
    </div>
  );
}
