import React from "react";
import Link from "next/link";

export default function Home() {
  return (
    <div className="min-h-screen flex flex-col items-center justify-center px-6">
      <div className="max-w-2xl mx-auto text-center">
        <h1
          className="text-6xl md:text-8xl font-bold mb-6"
          style={{ fontFamily: "var(--font-heading)" }}
        >
          inspect
        </h1>
        <p className="text-xl md:text-2xl text-gray-300 mb-4">
          Entity-level code review for Git.
        </p>
        <p className="text-gray-500 mb-12 max-w-lg mx-auto">
          Risk scoring, blast radius analysis, and change classification. 95% recall on real bugs. 92% token reduction vs full-diff review.
        </p>

        <div className="flex flex-wrap gap-4 justify-center mb-16">
          <Link
            href="/sign-up"
            className="px-8 py-3 bg-white text-black font-semibold rounded-lg hover:bg-gray-200 transition-colors text-sm"
            style={{ fontFamily: "var(--font-heading)" }}
          >
            Get Started
          </Link>
          <Link
            href="/docs"
            className="px-8 py-3 border border-white/20 rounded-lg hover:border-white/40 transition-colors text-sm"
            style={{ fontFamily: "var(--font-heading)" }}
          >
            Docs
          </Link>
        </div>

        <div className="grid grid-cols-3 gap-8 max-w-md mx-auto">
          <div>
            <p
              className="text-3xl font-bold text-white"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              95%
            </p>
            <p className="text-gray-500 text-sm mt-1">recall</p>
          </div>
          <div>
            <p
              className="text-3xl font-bold text-white"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              92%
            </p>
            <p className="text-gray-500 text-sm mt-1">token reduction</p>
          </div>
          <div>
            <p
              className="text-3xl font-bold text-white"
              style={{ fontFamily: "var(--font-heading)" }}
            >
              13
            </p>
            <p className="text-gray-500 text-sm mt-1">languages</p>
          </div>
        </div>
      </div>

      <footer className="absolute bottom-6 text-gray-600 text-sm">
        <a
          href="https://ataraxy-labs.com"
          className="hover:text-gray-400 transition-colors"
        >
          Ataraxy Labs
        </a>
      </footer>
    </div>
  );
}
