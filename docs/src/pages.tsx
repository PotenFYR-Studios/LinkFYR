import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * Markdown-backed docs pages: the `docs/*.md` files remain the single
 * source of truth; this site renders them (org stack: Vite + React +
 * Tailwind), so there is no content duplication to drift.
 */

const FILES = import.meta.glob(["../content/*.md", "../content/adr/*.md"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function mdFor(slug: string): string | null {
  for (const [key, value] of Object.entries(FILES)) {
    const s = key.replace(/^\.\.\/content\//, "").replace(/\.md$/, "");
    if (s === slug) return value;
  }
  return null;
}

interface DocEntry {
  slug: string;
  title: string;
  blurb: string;
}

interface DocGroup {
  label: string;
  entries: DocEntry[];
}

export const DOC_GROUPS: DocGroup[] = [
  {
    label: "Start here",
    entries: [
      { slug: "architecture", title: "Architecture", blurb: "Process topology, crate map, data flow, budgets" },
      { slug: "roadmap", title: "Roadmap", blurb: "Every feature, classified; nothing dropped" },
      { slug: "platform-support", title: "Platform support", blurb: "Per-OS capability matrix that never fakes parity" },
      { slug: "daemon", title: "linkfyrd service", blurb: "Windows service, systemd, launchd, exam-safe background operation" },
      { slug: "mobile", title: "Mobile companion", blurb: "Build the Tauri 2 app for Android and iOS" },
    ],
  },
  {
    label: "Security",
    entries: [
      { slug: "threat-model", title: "Threat model", blurb: "STRIDE model and fail-open/fail-closed policy" },
      { slug: "security", title: "Security engineering", blurb: "IPC hardening, supply chain, signing, secrets" },
    ],
  },
  {
    label: "Design and background",
    entries: [
      { slug: "ux", title: "UX and design system", blurb: "Motion dials, tokens, accessibility standard" },
      { slug: "flow-rules", title: "Flow Rules", blurb: "The rules language design" },
      { slug: "protocol", title: "Fusion protocol", blurb: "Multipath bonding protocol design" },
      { slug: "edge", title: "Edge node", blurb: "Self-hosted relay design" },
      { slug: "competitors", title: "Competitor matrix", blurb: "Living comparison with evidence grades" },
      { slug: "user-demand-research", title: "User demand research", blurb: "Community demand to requirements" },
      { slug: "releases", title: "Release engineering", blurb: "Pipeline and idempotent publishing" },
      { slug: "adr/0001-application-shell", title: "ADR-0001: Application shell", blurb: "Why Tauri 2, and the replaceable-shell rule" },
    ],
  },
];

export function PortalPage() {
  return (
    <main className="docs-layout">
      <aside className="docs-sidebar" aria-label="Documentation">
        {DOC_GROUPS.map((g) => (
          <div key={g.label} className="sidebar-group">
            <p className="sidebar-label">{g.label}</p>
            {g.entries.map((e) => (
              <a key={e.slug} href={`/docs/${e.slug}/`}>
                {e.title}
              </a>
            ))}
          </div>
        ))}
      </aside>
      <article className="docs-content">
        <h1>Documentation</h1>
        <p>
          LinkFYR is one free, open control layer for a machine's Internet
          connections: 108 real tools, tiered bridges, a background service,
          desktop, mobile and CLI clients over one versioned IPC contract.
          These pages are the same files that live in the repository under{" "}
          <code>docs/</code>.
        </p>
        {DOC_GROUPS.map((g) => (
          <section key={g.label}>
            <h2>{g.label}</h2>
            <div className="portal-grid">
              {g.entries.map((e) => (
                <a key={e.slug} className="card portal-card" href={`/docs/${e.slug}/`}>
                  <span className="portal-title">{e.title}</span>
                  <span className="portal-blurb">{e.blurb}</span>
                </a>
              ))}
            </div>
          </section>
        ))}
      </article>
    </main>
  );
}

export function DocsPage({ slug }: { slug: string }) {
  const md = mdFor(slug);
  if (!md) return <NotFoundPage />;
  const title =
    DOC_GROUPS.flatMap((g) => g.entries).find((e) => e.slug === slug)?.title ?? slug;
  return (
    <main className="docs-layout">
      <aside className="docs-sidebar" aria-label="Documentation">
        {DOC_GROUPS.map((g) => (
          <div key={g.label} className="sidebar-group">
            <p className="sidebar-label">{g.label}</p>
            {g.entries.map((e) => (
              <a
                key={e.slug}
                href={`/docs/${e.slug}/`}
                className={e.slug === slug ? "sidebar-active" : undefined}
                aria-current={e.slug === slug ? "page" : undefined}
              >
                {e.title}
              </a>
            ))}
          </div>
        ))}
      </aside>
      <article className="docs-content">
        <p className="crumb">
          <a href="/docs/">Docs</a> / {title}
        </p>
        <div className="md-body">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{md}</ReactMarkdown>
        </div>
      </article>
    </main>
  );
}

export function NotFoundPage() {
  return (
    <main className="notfound">
      <h1>Page not found</h1>
      <p>
        That page does not exist. Start from the{" "}
        <a href="/docs/">documentation portal</a>.
      </p>
    </main>
  );
}
