import { useMemo, useRef, useState } from "react";
import type { HistoryPoint } from "@linkfyr/types";
import { fmtRatePlain, timeLabel } from "../lib/format";
import { useEasedScale, useSmoothedSeries } from "../lib/smooth";
import { useStore } from "../state/store";

interface LiveChartProps {
  points: HistoryPoint[];
  height?: number;
  showAxis?: boolean;
  ariaLabel: string;
}

/**
 * The live throughput chart. SVG path from a sliding window; values are
 * rAF-eased between samples (premium flow, disabled by reduced motion) and
 * hover readouts snap to real samples — no fabricated interpolation.
 */
export function LiveChart({ points, height = 180, showAxis = true, ariaLabel }: LiveChartProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const animation = useStore((s) => s.config?.preferences.animation ?? "full");
  const smoothPoints = useSmoothedSeries(points, animation === "full");
  const width = 600; // viewBox units; scales to container

  // Instantaneous window max (drives the quantized scale target).
  const windowMax = useMemo(() => {
    if (points.length < 2) return 1;
    return Math.max(...points.map((p) => Math.max(p.rxBps, p.txBps)), 1);
  }, [points]);
  const scaleMax = useEasedScale(windowMax, animation === "full");

  const model = useMemo(() => {
    if (smoothPoints.length < 2) return null;
    const t0 = smoothPoints[0]?.timestampMs ?? 0;
    const t1 = smoothPoints[smoothPoints.length - 1]?.timestampMs ?? 1;
    const span = Math.max(t1 - t0, 1);
    const x = (p: HistoryPoint) => ((p.timestampMs - t0) / span) * width;
    const y = (v: number) => height - (v / scaleMax) * (height - 8) - 4;
    const path = (key: "rxBps" | "txBps") =>
      smoothPoints.map((p, i) => `${i === 0 ? "M" : "L"}${x(p).toFixed(1)},${y(p[key]).toFixed(1)}`).join(" ");
    const area = (key: "rxBps" | "txBps") =>
      `${path(key)} L${x(smoothPoints[smoothPoints.length - 1] as HistoryPoint).toFixed(1)},${height} L${x(smoothPoints[0] as HistoryPoint).toFixed(1)},${height} Z`;
    return { x, y, rx: path("rxBps"), rxArea: area("rxBps"), tx: path("txBps") };
  }, [smoothPoints, height, scaleMax]);

  const onMove = (e: React.MouseEvent) => {
    const rect = wrapRef.current?.getBoundingClientRect();
    if (!rect || !model) return;
    const frac = (e.clientX - rect.left) / rect.width;
    const idx = Math.round(frac * (points.length - 1));
    setHover(Math.min(Math.max(idx, 0), points.length - 1));
  };

  // Hover reads the REAL sample, never the smoothed one.
  const hoverPoint = hover != null ? points[hover] : undefined;

  return (
    <div ref={wrapRef} className="relative" onMouseMove={onMove} onMouseLeave={() => setHover(null)}>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        style={{ width: "100%", height }}
        role="img"
        aria-label={ariaLabel}
      >
        <defs>
          <linearGradient id="lf-rx-fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.20" />
            <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0.02" />
          </linearGradient>
        </defs>
        {[0.25, 0.5, 0.75].map((f) => (
          <line
            key={f}
            x1={0}
            x2={width}
            y1={height - f * (height - 8) - 4}
            y2={height - f * (height - 8) - 4}
            stroke="var(--color-stroke-soft)"
            strokeWidth={1}
          />
        ))}
        {model ? (
          <>
            <path d={model.rxArea} fill="url(#lf-rx-fill)" />
            <path d={model.rx} fill="none" stroke="var(--color-accent)" strokeWidth={2} vectorEffect="non-scaling-stroke" />
            <path
              d={model.tx}
              fill="none"
              stroke="var(--color-ok)"
              strokeWidth={1.6}
              strokeDasharray="4 3"
              vectorEffect="non-scaling-stroke"
              opacity={0.9}
            />
          </>
        ) : null}
      </svg>

      {model && showAxis ? (
        <div className="pointer-events-none absolute right-2 top-1 text-2xs text-ink-muted num">
          {fmtRatePlain(scaleMax)}
        </div>
      ) : null}

      {hoverPoint ? (
        <div
          className="pointer-events-none absolute -top-1 z-10 rounded-lg border border-stroke bg-overlay px-2.5 py-1.5 text-2xs shadow-xl"
          style={{ left: `calc(${((hover ?? 0) / Math.max(points.length - 1, 1)) * 100}% - 44px)` }}
        >
          <div className="num text-ink-muted">{timeLabel(hoverPoint.timestampMs)}</div>
          <div className="num" style={{ color: "var(--color-accent-soft)" }}>
            ↓ {fmtRatePlain(hoverPoint.rxBps)}
          </div>
          <div className="num" style={{ color: "var(--color-ok)" }}>
            ↑ {fmtRatePlain(hoverPoint.txBps)}
          </div>
        </div>
      ) : null}

      {points.length < 2 ? (
        <div className="absolute inset-0 flex items-center justify-center text-xs text-ink-muted">
          Collecting samples…
        </div>
      ) : null}
    </div>
  );
}
