// Static generation for every route (discord-botlists docs architecture):
// each page is rendered to real HTML so direct refreshes, crawlers and
// no-JS visitors get content on first response. Runs after `vite build`.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { renderToString } from "react-dom/server";
import React from "react";
import { MemoryRouter } from "react-router-dom";

const __dir = dirname(fileURLToPath(import.meta.url));
const dist = resolve(__dir, "../dist");
const CANON = "https://linkfyr.docs.potenfyr.in";
const e = React.createElement;

interface Page {
  path: string;
  file: string;
  title: string;
  description: string;
  noindex?: boolean;
}

const vite = await createServer({
  server: { middlewareMode: true },
  appType: "custom",
  logLevel: "error",
});

const app = (await vite.ssrLoadModule("/src/App.tsx")) as { default: React.ComponentType };
const content = (await vite.ssrLoadModule("/src/docs/content.ts")) as {
  DOC_SECTIONS: { slug: string; title: string; blurb: string }[];
};
const seo = (await vite.ssrLoadModule("/src/SeoManager.tsx")) as {
  metaForPath: (path: string) => { title: string; description: string };
};

const pages: Page[] = [];
const push = (path: string, file: string) => {
  const meta = seo.metaForPath(path);
  pages.push({ path, file, ...meta });
};
push("/", "index.html");
push("/docs", "docs/index.html");
for (const s of content.DOC_SECTIONS) {
  push(`/docs/${s.slug}`, `docs/${s.slug}/index.html`);
}
pages.push({
  path: "/404",
  file: "404.html",
  title: "Not Found - LinkFYR Docs",
  description: "Page not found.",
  noindex: true,
});

const shell = readFileSync(resolve(dist, "index.html"), "utf8");
const esc = (s: string) => s.replace(/"/g, "&quot;");

for (const page of pages) {
  const html = renderToString(
    e(MemoryRouter, { initialEntries: [page.path] }, e(app.default)),
  );
  const canon = `${CANON}${page.path === "/" ? "/" : `${page.path}/`}`;
  const out = shell
    .replace('<div id="root"></div>', `<div id="root">${html}</div>`)
    .replace(/<title>.*?<\/title>/, `<title>${page.title}</title>`)
    .replace(
      "</head>",
      `  <meta name="description" content="${esc(page.description)}">\n` +
        `  ${page.noindex ? '<meta name="robots" content="noindex">\n' : ""}` +
        `  <link rel="canonical" href="${canon}">\n` +
        `  <meta property="og:type" content="website">\n` +
        `  <meta property="og:site_name" content="LinkFYR Docs">\n` +
        `  <meta property="og:title" content="${esc(page.title)}">\n` +
        `  <meta property="og:description" content="${esc(page.description)}">\n` +
        `  <meta property="og:url" content="${canon}">\n` +
        `  <meta property="og:image" content="${CANON}/og.png">\n` +
        `  <meta name="twitter:card" content="summary_large_image">\n` +
        `</head>`,
    );
  const target = resolve(dist, page.file);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, out);
}

const urls = pages
  .filter((p) => !p.noindex)
  .map((p) => `  <url><loc>${CANON}${p.path === "/" ? "/" : `${p.path}/`}</loc></url>`)
  .join("\n");
writeFileSync(
  resolve(dist, "sitemap.xml"),
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`,
);

await vite.close();
console.log(`prerendered ${pages.length} routes`);
