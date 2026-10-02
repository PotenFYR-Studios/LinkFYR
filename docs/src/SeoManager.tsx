import { useEffect } from "react";
import { useLocation } from "react-router-dom";
import { DOC_SECTIONS } from "./docs/content";

const CANON = "https://linkfyr.docs.potenfyr.in";
const SITE = "LinkFYR Docs";

/**
 * Route-aware document head: title, description, canonical and OG tags
 * are set client-side on navigation; scripts/prerender.ts bakes the
 * same values into the static HTML per route at build time.
 */
export function metaForPath(path: string): { title: string; description: string } {
  if (path === "/") {
    return {
      title: "LinkFYR - the operating system for your Internet connections",
      description:
        "Free, open control layer for your connections: 108 real network tools, tiered bridging, traffic control, background service, desktop, mobile and CLI. Local-only by default.",
    };
  }
  if (path === "/docs") {
    return {
      title: `Documentation - ${SITE}`,
      description:
        "Architecture, roadmap, platform support, daemon and mobile guides, threat model, and every LinkFYR design note.",
    };
  }
  const slug = path.replace(/^\/docs\//, "").replace(/\/$/, "");
  const section = DOC_SECTIONS.find((s) => s.slug === slug);
  if (section) {
    return { title: `${section.title} - ${SITE}`, description: section.blurb };
  }
  return { title: `Not Found - ${SITE}`, description: "Page not found." };
}

function setMeta(name: string, content: string, property = false) {
  const attr = property ? "property" : "name";
  let el = document.head.querySelector(`meta[${attr}="${name}"]`);
  if (!el) {
    el = document.createElement("meta");
    el.setAttribute(attr, name);
    document.head.appendChild(el);
  }
  el.setAttribute("content", content);
}

function setCanonical(href: string) {
  let el = document.head.querySelector('link[rel="canonical"]');
  if (!el) {
    el = document.createElement("link");
    el.setAttribute("rel", "canonical");
    document.head.appendChild(el);
  }
  el.setAttribute("href", href);
}

export default function SeoManager() {
  const location = useLocation();
  useEffect(() => {
    const { title, description } = metaForPath(location.pathname);
    document.title = title;
    setMeta("description", description);
    setMeta("og:title", title, true);
    setMeta("og:description", description, true);
    setCanonical(`${CANON}${location.pathname === "/" ? "/" : `${location.pathname.replace(/\/$/, "")}/`}`);
  }, [location.pathname]);
  return null;
}
