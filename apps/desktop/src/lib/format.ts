/** Formatting helpers. All metric text uses tabular numerals (see app.css). */

export function fmtRate(bps: number): { value: string; unit: string } {
  if (!Number.isFinite(bps) || bps <= 0) return { value: "0", unit: "Mbps" };
  if (bps >= 1_000_000_000) return { value: (bps / 1_000_000_000).toFixed(2), unit: "Gbps" };
  if (bps >= 100_000_000) return { value: (bps / 1_000_000).toFixed(0), unit: "Mbps" };
  if (bps >= 10_000_000) return { value: (bps / 1_000_000).toFixed(1), unit: "Mbps" };
  if (bps >= 1_000_000) return { value: (bps / 1_000_000).toFixed(2), unit: "Mbps" };
  return { value: (bps / 1_000).toFixed(0), unit: "Kbps" };
}

export function fmtRatePlain(bps: number): string {
  const { value, unit } = fmtRate(bps);
  return `${value} ${unit}`;
}

export function fmtMs(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return "—";
  return ms >= 100 ? `${ms.toFixed(0)} ms` : `${ms.toFixed(1)} ms`;
}

export function fmtPct(pct: number | null | undefined): string {
  if (pct == null || !Number.isFinite(pct)) return "—";
  return `${pct.toFixed(1)}%`;
}

/** Health color mapping is semantic, never decorative. */
export function healthColor(overall: number | null | undefined): string {
  if (overall == null) return "var(--color-ink-faint)";
  if (overall >= 80) return "var(--color-ok)";
  if (overall >= 50) return "var(--color-warn)";
  return "var(--color-danger)";
}

export function ifColor(kind: string): string {
  return `var(--color-if-${kind})`;
}

/** Stable per-interface hue for charts, from the fixed kind palette. */
export function kindLabel(kind: string): string {
  switch (kind) {
    case "ethernet":
      return "Ethernet";
    case "wifi":
      return "Wi-Fi";
    case "cellular":
      return "Cellular";
    case "tunnel":
      return "Tunnel";
    case "virtual":
      return "Virtual";
    case "loopback":
      return "Loopback";
    default:
      return "Other";
  }
}

export function timeLabel(tsMs: number): string {
  const d = new Date(tsMs);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}
