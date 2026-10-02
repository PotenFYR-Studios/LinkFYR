import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * WCAG AA contrast enforcement on the actual design tokens (apps/desktop/
 * src/styles/app.css). Fails the build if a future palette edit makes text
 * invisible — this codifies the "nothing disappears into the background"
 * rule with numbers instead of eyeballs.
 */

interface Palette {
  base: string;
  surface: string;
  raised: string;
  ink: string;
  inkMuted: string;
  inkFaint: string;
  accent: string;
  accentSoft: string;
  ok: string;
  warn: string;
  danger: string;
}

function parseThemes(css: string): Record<string, Palette> {
  const extract = (block: string): Palette => {
    const get = (name: string): string => {
      const m = block.match(new RegExp(`--color-${name}:\\s*(#[0-9a-fA-F]{6})`));
      if (!m) throw new Error(`token --color-${name} not found`);
      return m[1] as string;    };
    return {
      base: get("base"),
      surface: get("surface"),
      raised: get("raised"),
      ink: get("ink"),
      inkMuted: get("ink-muted"),
      inkFaint: get("ink-faint"),
      accent: get("accent"),
      accentSoft: get("accent-soft"),
      ok: get("ok"),
      warn: get("warn"),
      danger: get("danger"),
    };
  };

  const darkBlock = css.slice(css.indexOf("@theme"), css.indexOf(':root[data-theme="light"]'));
  const lightBlock = css.slice(css.indexOf(':root[data-theme="light"]'), css.indexOf("html,"));
  return { dark: extract(darkBlock), light: extract(lightBlock) };
}

function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const l1 = luminance(a);
  const l2 = luminance(b);
  const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

/** Approximate color-mix(in srgb, fg X%, bg) as used by pills/active states. */
function mix(fg: string, bg: string, alpha: number): string {
  const f = hexToRgb(fg);
  const b = hexToRgb(bg);
  const c = f.map((v, i) => Math.round(v * alpha + (b[i] as number) * (1 - alpha)));
  return `#${c.map((v) => (v as number).toString(16).padStart(2, "0")).join("")}`;
}

const css = readFileSync(join(__dirname, "..", "styles", "app.css"), "utf8");
const themes = parseThemes(css);

describe.each(["dark", "light"] as const)("contrast (%s theme)", (theme) => {
  const p = themes[theme] as Palette;

  it("body text on surface meets AA (4.5:1)", () => {
    expect(contrast(p.ink, p.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(p.ink, p.base)).toBeGreaterThanOrEqual(4.5);
  });

  it("muted text on surface and raised meets AA (4.5:1)", () => {
    expect(contrast(p.inkMuted, p.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(p.inkMuted, p.raised)).toBeGreaterThanOrEqual(4.5);
  });

  it("faint text on surface meets AA (4.5:1) — no invisible hints", () => {
    expect(contrast(p.inkFaint, p.surface)).toBeGreaterThanOrEqual(4.5);
  });

  it("accent text on surface is readable (4.5:1)", () => {
    expect(contrast(p.accent, p.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(p.accentSoft, p.raised)).toBeGreaterThanOrEqual(4.5);
  });

  it("status colors on their 12% tint pills meet AA for small text", () => {
    for (const c of [p.ok, p.warn, p.danger]) {
      const tinted = mix(c, p.surface, 0.12);
      expect(contrast(c, tinted)).toBeGreaterThanOrEqual(4.5);
    }
  });
});
