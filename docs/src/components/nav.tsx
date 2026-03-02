import Link from "next/link";

export default function Nav({ active }: { active: string }) {
  const links = [
    { href: "/", label: "Home", key: "home" },
    { href: "/benchmarks", label: "Benchmarks", key: "benchmarks" },
    { href: "/docs", label: "Docs & API", key: "docs" },
    { href: "/changelog", label: "Changelog", key: "changelog" },
    {
      href: "https://github.com/Ataraxy-Labs/inspect",
      label: "GitHub",
      key: "github",
      external: true,
    },
    { href: "/llms.txt", label: "llms.txt", key: "llms" },
    { href: "/dashboard", label: "Dashboard", key: "dashboard" },
  ];

  return (
    <nav className="site-nav">
      <Link className="logo" href="/">
        inspect
      </Link>
      <div className="links">
        {links.map((l) =>
          l.external ? (
            <a key={l.key} href={l.href}>
              {l.label}
            </a>
          ) : (
            <Link
              key={l.key}
              href={l.href}
              className={active === l.key ? "active" : ""}
            >
              {l.label}
            </Link>
          )
        )}
      </div>
    </nav>
  );
}
