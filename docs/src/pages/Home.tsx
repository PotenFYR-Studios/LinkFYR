import { Link } from "react-router-dom";
import { DOC_CATEGORIES, DOC_SECTIONS } from "../docs/content";

/** Landing page: what LinkFYR is, and where to go next. */
export default function Home() {
  return (
    <main className="landing">
      <section className="hero">
        <p className="hero-kicker">PotenFYR Studios</p>
        <h1 className="hero-title">
          The operating system for your Internet connections
        </h1>
        <p className="hero-sub">
          One free, open control layer for what your apps do on the network.
          Monitor with 108 real tools, optimize ping and jitter, bridge
          connections where Windows removed it, and keep everything running in
          the background. Local-only by default.
        </p>
        <div className="hero-actions">
          <Link className="btn btn-primary" to="/docs">
            Read the docs
          </Link>
          <a className="btn" href="https://github.com/PotenFYR-Studios/LinkFYR/releases">
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
          <code>{`git clone https://github.com/PotenFYR-Studios/LinkFYR
cd LinkFYR
pnpm install
cargo run -p linkfyr-cli -- status
# desktop app
cargo tauri dev`}</code>
        </pre>
      </section>

      <section className="portal-cta">
        <Link className="btn btn-primary" to="/docs">
          Browse all documentation
        </Link>
        <p>
          {DOC_SECTIONS.length} pages across {DOC_CATEGORIES.length} categories:
          architecture, roadmap, platform support, daemon and mobile guides,
          security model, and every design note.
        </p>
      </section>
    </main>
  );
}
