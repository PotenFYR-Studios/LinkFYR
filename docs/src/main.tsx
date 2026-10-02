import { createRoot } from "react-dom/client";
import { DocsPage, NotFoundPage, PortalPage } from "./pages";
import "./styles.css";

/**
 * LinkFYR docs site (Vite + React, org template).
 *
 * Plain-anchor navigation: every route is emitted as its own
 * index.html by the vite plugin, so links work on GitHub Pages with
 * zero client router. Dev gets the same routes via a rewrite in
 * vite.config.ts.
 */

const CANON = "https://linkfyr.docs.potenfyr.in";
const GH = "https://github.com/PotenFYR-Studios/LinkFYR";

function currentPath(): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  let p = window.location.pathname;
  if (base && p.startsWith(base)) p = p.slice(base.length);
  if (!p.startsWith("/")) p = `/${p}`;
  return p.replace(/\/+$/, "") || "/";
}

function Header() {
  return (
    <header className="site-header">
      <div className="site-header-inner">
        <a className="brand" href={import.meta.env.BASE_URL}>
          <span className="brand-mark" aria-hidden />
          <span className="brand-name">LinkFYR</span>
          <span className="brand-tag">docs</span>
        </a>
        <nav className="site-nav" aria-label="Primary">
          <a href="/docs/">Documentation</a>
          <a href={`${GH}/releases`}>Releases</a>
          <a href={`${GH}/blob/main/docs/roadmap.md`}>Roadmap</a>
          <a href={GH} className="nav-strong">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}

function Footer() {
  return (
    <footer className="site-footer">
      <div className="site-footer-inner">
        <span>
          PotenFYR Studios · Apache-2.0 with the Commons Clause · free to use,
          not for resale
        </span>
        <span className="footer-links">
          <a href={`${GH}/blob/main/LICENSE`}>License</a>
          <a href={`${GH}/blob/main/SECURITY.md`}>Security</a>
          <a href={`${GH}/blob/main/CONTRIBUTING.md`}>Contributing</a>
        </span>
      </div>
    </footer>
  );
}

function Landing() {
  const tools = 108;
  return (
    <main className="landing">
      <section className="hero">
        <p className="hero-kicker">PotenFYR Studios</p>
        <h1 className="hero-title">
          The operating system for your Internet connections
        </h1>
        <p className="hero-sub">
          One free, open control layer for what your apps do on the network.
          Monitor with {tools} real tools, optimize ping and jitter, bridge
          connections where Windows removed it, and keep everything running in
          the background. Local-only by default.
        </p>
        <div className="hero-actions">
          <a className="btn btn-primary" href="/docs/">
            Read the docs
          </a>
          <a className="btn" href={`${GH}/releases`}>
            Download
          </a>
        </div>
      </section>

      <section className="cards" aria-label="What LinkFYR does">
        <article className="card">
          <h2>Measure everything, honestly</h2>
          <p>
            DNS benchmarking with DoH fallback, IPv6 penalty detection,
            bufferbloat grading, MTU black holes, per-app socket tables, TLS
            certificate expiry, DNS leak and interception tests, VoIP MOS,
            Wi-Fi channel congestion and more. Failed probes stay visible and
            capability states are never faked.
          </p>
        </article>
        <article className="card">
          <h2>Fix what the OS allows</h2>
          <p>
            Tiered network bridging (Hyper-V teaming, clearly labeled NetNat
            fallback, Linux and macOS L2), per-app firewall rules, kill-switch
            holds, split-tunnel routing, Linux traffic shaping, DNS apply with
            one-click restore. Unelevated runs list the exact commands.
          </p>
        </article>
        <article className="card">
          <h2>Run it your way</h2>
          <p>
            Desktop app with tray and background mode, a first-class CLI, a
            background service for exam-safe operation, and a mobile companion
            that talks to your own machine. One versioned IPC contract, zero
            drift between clients.
          </p>
        </article>
      </section>

      <section className="quickstart" aria-label="Quick start">
        <h2>Quick start</h2>
        <pre className="code">
          <code>{`git clone ${GH}
cd LinkFYR
pnpm install
cargo run -p linkfyr-cli -- status
# desktop app
cargo tauri dev`}</code>
        </pre>
      </section>

      <section className="portal-cta">
        <a className="btn btn-primary" href="/docs/">
          Browse all documentation
        </a>
        <p>
          Architecture, roadmap, platform support, daemon and mobile guides,
          security model, and every design note.
        </p>
      </section>
    </main>
  );
}

function App() {
  const path = currentPath();
  let page = <Landing />;
  if (path === "/docs") page = <PortalPage />;
  else if (path.startsWith("/docs/")) page = <DocsPage slug={path.slice("/docs/".length)} />;
  else if (path !== "/") page = <NotFoundPage />;
  return (
    <div className="site">
      <Header />
      {page}
      <Footer />
    </div>
  );
}

const rootEl = document.getElementById("root");
if (rootEl) {
  createRoot(rootEl).render(<App />);
}

export { CANON };
