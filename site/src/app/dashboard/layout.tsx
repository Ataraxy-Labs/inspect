export const dynamic = "force-dynamic";

import Link from "next/link";
import { UserButton } from "@clerk/nextjs";

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div className="min-h-screen">
      <nav className="border-b border-white/10 px-6 py-4">
        <div className="max-w-5xl mx-auto flex items-center justify-between">
          <div className="flex items-center gap-8">
            <Link
              href="/dashboard"
              className="text-lg font-bold"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              inspect
            </Link>
            <div className="flex gap-6 text-sm text-gray-400">
              <Link href="/dashboard/keys" className="hover:text-white transition-colors">
                Keys
              </Link>
              <Link href="/dashboard/usage" className="hover:text-white transition-colors">
                Usage
              </Link>
            </div>
          </div>
          <UserButton />
        </div>
      </nav>
      <main className="max-w-5xl mx-auto px-6 py-10">
        {children}
      </main>
    </div>
  );
}
