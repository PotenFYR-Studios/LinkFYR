import { healthColor } from "../lib/format";
import type { HealthScore } from "@linkfyr/types";

/**
 * Radial health gauge with the factor breakdown visible on demand.
 * The score is never a black box: factors are rendered in the tooltip
 * and (in expert mode) inline.
 */
export function HealthGauge({
  health,
  size = 56,
  expert = false,
}: {
  health: HealthScore | null;
  size?: number;
  expert?: boolean;
}) {
  const stroke = 5;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const value = health?.overall ?? 0;
  const color = healthColor(health?.overall);

  return (
    <div
      className="relative inline-flex items-center justify-center"
      style={{ width: size, height: size }}
      role="meter"
      aria-valuenow={health?.overall ?? undefined}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={
        health ? `Health ${health.overall} of 100` : "Health unknown"
      }
    >
      <svg width={size} height={size} style={{ transform: "rotate(-90deg)" }} aria-hidden>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--color-stroke-soft)"
          strokeWidth={stroke}
        />
        {health ? (
          <circle
            cx={size / 2}
            cy={size / 2}
            r={r}
            fill="none"
            stroke={color}
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeDasharray={c}
            strokeDashoffset={c - (value / 100) * c}
            style={{ transition: "stroke-dashoffset 600ms ease, stroke 400ms ease" }}
          />
        ) : null}
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">
        <span className="num text-xs font-medium" style={{ color: health ? color : "var(--color-ink-faint)" }}>
          {health ? value : "–"}
        </span>
      </div>
      {expert && health ? (
        <div className="pointer-events-none absolute top-full left-1/2 z-10 mt-1 hidden -translate-x-1/2 group-hover:block">
          <div className="lf-card p-2 text-2xs">
            {health.factors.map(([name, v]) => (
              <div key={name} className="flex justify-between gap-3">
                <span className="text-ink-muted">{name}</span>
                <span className="num">{v}</span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
