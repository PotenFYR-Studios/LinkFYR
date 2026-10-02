import { Link, useParams } from "react-router-dom";
import { Block } from "../components/DocBlocks";
import { DOC_CATEGORIES, DOC_SECTIONS, sectionBySlug } from "../docs/content";
import NotFound from "./NotFound";

function Sidebar({ activeSlug }: { activeSlug?: string }) {
  return (
    <aside className="docs-sidebar" aria-label="Documentation">
      {DOC_CATEGORIES.map((cat) => (
        <div key={cat} className="sidebar-group">
          <p className="sidebar-label">{cat}</p>
          {DOC_SECTIONS.filter((s) => s.category === cat).map((s) => (
            <Link
              key={s.slug}
              to={`/docs/${s.slug}`}
              className={s.slug === activeSlug ? "sidebar-active" : undefined}
              aria-current={s.slug === activeSlug ? "page" : undefined}
            >
              {s.title}
            </Link>
          ))}
        </div>
      ))}
    </aside>
  );
}

/** /docs (portal) and /docs/:slug (rendered content blocks). */
export default function Docs() {
  const { slug } = useParams<{ slug?: string }>();

  if (slug) {
    const section = sectionBySlug(slug);
    if (!section) return <NotFound />;
    return (
      <main className="docs-layout">
        <Sidebar activeSlug={slug} />
        <article className="docs-content">
          <p className="crumb">
            <Link to="/docs">Docs</Link> / {section.title}
          </p>
          <h1>{section.title}</h1>
          <p className="md-blurb">{section.blurb}</p>
          <div className="md-body">
            {section.blocks.map((b, i) => (
              <Block key={i} block={b} />
            ))}
          </div>
        </article>
      </main>
    );
  }

  return (
    <main className="docs-layout">
      <Sidebar />
      <article className="docs-content">
        <h1>Documentation</h1>
        <p>
          LinkFYR is one free, open control layer for a machine's Internet
          connections: 108 real tools, tiered bridges, a background service,
          desktop, mobile and CLI clients over one versioned IPC contract.
        </p>
        {DOC_CATEGORIES.map((cat) => (
          <section key={cat}>
            <h2>{cat}</h2>
            <div className="portal-grid">
              {DOC_SECTIONS.filter((s) => s.category === cat).map((s) => (
                <Link key={s.slug} className="card portal-card" to={`/docs/${s.slug}`}>
                  <span className="portal-title">{s.title}</span>
                  <span className="portal-blurb">{s.blurb}</span>
                </Link>
              ))}
            </div>
          </section>
        ))}
      </article>
    </main>
  );
}
