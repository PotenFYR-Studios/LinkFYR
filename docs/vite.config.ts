import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dir = dirname(fileURLToPath(import.meta.url));
const CANON = "https://linkfyr.docs.potenfyr.in";

type RouteMeta = { path: string; title: string; description: string };

const DOC_SLUGS: { slug: string; title: string; description: string }[] = [
  { slug: "architecture", title: "Architecture", description: "Process topology, crate map, data flow, and performance budgets for LinkFYR." },
  { slug: "roadmap", title: "Roadmap", description: "Every LinkFYR feature classified Now/Next/Later; nothing is ever dropped." },
  { slug: "platform-support", title: "Platform support", description: "Per-OS capability matrix for LinkFYR that never fakes parity." },
  { slug: "daemon", title: "linkfyrd service", description: "Install LinkFYR's background service on Windows, Linux, and macOS." },
  { slug: "mobile", title: "Mobile companion", description: "Build the LinkFYR Tauri 2 app for Android and iOS." },
  { slug: "threat-model", title: "Threat model", description: "STRIDE-annotated threat model and fail-open/fail-closed policy for LinkFYR." },
  { slug: "security", title: "Security engineering", description: "IPC hardening, supply chain, signing, and secrets handling in LinkFYR." },
  { slug: "ux", title: "UX and design system", description: "Motion dials, design tokens, and the accessibility standard." },
  { slug: "flow-rules", title: "Flow Rules", description: "The LinkFYR Flow Rules language: predicates, actions, and priorities." },
  { slug: "protocol", title: "Fusion protocol", description: "The LinkFYR Fusion multipath bonding protocol design." },
  { slug: "edge", title: "Edge node", description: "Self-hosted LinkFYR Edge relay design." },
  { slug: "competitors", title: "Competitor matrix", description: "Living competitor comparison for LinkFYR with evidence grades." },
  { slug: "user-demand-research", title: "User demand research", description: "Community demand turned into evidence-graded LinkFYR requirements." },
  { slug: "releases", title: "Release engineering", description: "LinkFYR release pipeline and idempotent publishing behavior." },
  { slug: "adr/0001-application-shell", title: "ADR-0001: Application shell", description: "Why LinkFYR uses Tauri 2, and the replaceable-shell rule." },
];

const ROUTES: RouteMeta[] = [
  {
    path: "/",
    title: "LinkFYR - the operating system for your Internet connections",
    description:
      "Free, open control layer for your connections: 108 real network tools, tiered bridging, traffic control, background service, desktop, mobile and CLI. Local-only by default.",
  },
  {
    path: "/docs",
    title: "Documentation - LinkFYR",
    description:
      "Architecture, roadmap, platform support, daemon and mobile guides, threat model, and every LinkFYR design note.",
  },
  ...DOC_SLUGS.map((d) => ({
    path: `/docs/${d.slug}`,
    title: `${d.title} - LinkFYR Docs`,
    description: d.description,
  })),
];

/** Emit one real index.html per route with SEO meta (org template). */
function multiPageEmit(): Plugin {
  return {
    name: "linkfyr-multi-page",
    apply: "build",
    writeBundle() {
      const outDir = resolve(__dir, "dist");
      const shell = readFileSync(resolve(outDir, "index.html"), "utf8");
      const emit = (meta: RouteMeta, file: string, robots = "") => {
        const canon = `${CANON}${meta.path === "/" ? "/" : `${meta.path}/`}`;
        const esc = (s: string) => s.replace(/"/g, "&quot;");
        const html = shell
          .replace(/<title>.*?<\/title>/, `<title>${meta.title}</title>`)
          .replace(
            "</head>",
            `  <meta name="description" content="${esc(meta.description)}">\n` +
              `  <link rel="canonical" href="${canon}">\n` +
              `  ${robots}` +
              `  <meta property="og:type" content="website">\n` +
              `  <meta property="og:site_name" content="LinkFYR Docs">\n` +
              `  <meta property="og:title" content="${esc(meta.title)}">\n` +
              `  <meta property="og:description" content="${esc(meta.description)}">\n` +
              `  <meta property="og:url" content="${canon}">\n` +
              `  <meta property="og:image" content="${CANON}/og.png">\n` +
              `  <meta name="twitter:card" content="summary_large_image">\n` +
              `  <meta name="twitter:title" content="${esc(meta.title)}">\n` +
              `  <meta name="twitter:description" content="${esc(meta.description)}">\n` +
              `  <meta name="twitter:image" content="${CANON}/og.png">\n` +
              `</head>`,
          );
        const dir = resolve(outDir, file, "..");
        mkdirSync(dir, { recursive: true });
        writeFileSync(resolve(outDir, file), html);
      };
      for (const r of ROUTES) {
        const file =
          r.path === "/" ? "index.html" : `${r.path.replace(/^\//, "")}/index.html`;
        emit(r, file);
      }
      emit({ path: "/404", title: "Not Found - LinkFYR", description: "Page not found." }, "404.html", '<meta name="robots" content="noindex">\n');
    },
  };
}

/** Dev fallback: serve the shell for /docs/... so plain URLs work. */
function docsFallback(): Plugin {
  return {
    name: "linkfyr-docs-fallback",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((req, _res, next) => {
        const url = req.url?.split("?")[0] ?? "/";
        if (url.startsWith("/docs/") && !url.includes(".")) {
          req.url = "/index.html";
        }
        next();
      });
    },
  };
}

export default defineConfig({
  root: __dir,
  base: process.env.VITE_BASE ?? "/",
  plugins: [react(), tailwindcss(), multiPageEmit(), docsFallback()],
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
  server: { port: 4179 },
});
