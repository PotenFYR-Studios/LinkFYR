import { fmtRatePlain, ifColor } from "../lib/format";
import type { InterfaceTelemetry } from "@linkfyr/types";

/**
 * Stacked contribution bars: which interface carries the combined load.
 * Widths are the honest share of the current total (no log scaling that
 * could exaggerate idle links).
 */
export function ContributionBars({
  interfaces,
  compact = false,
}: {
  interfaces: InterfaceTelemetry[];
  compact?: boolean;
}) {
  const active = interfaces.filter(
    (i) => i.interface.status === "up" || i.interface.status === "dormant",
  );
  const total = Math.max(active.reduce((s, i) => s + i.rxBps + i.txBps, 0), 1);

  if (active.length === 0) {
    return (
      <div className="text-xs text-ink-faint">No active interfaces.</div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      {active.map((i) => {
        const share = ((i.rxBps + i.txBps) / total) * 100;
        const color = ifColor(i.interface.kind);
        return (
          <div key={i.interface.id} className="flex items-center gap-3">
            <div className="w-28 shrink-0 truncate text-xs text-ink-muted" title={i.interface.friendlyName}>
              {i.interface.friendlyName}
            </div>
            <div
              className="h-2 flex-1 overflow-hidden rounded-full"
              style={{ background: "var(--color-stroke-soft)" }}
              role="meter"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(share)}
              aria-label={`${i.interface.friendlyName} carries ${Math.round(share)} percent of traffic`}
            >
              <div
                className="h-full rounded-full"
                style={{
                  width: `${share}%`,
                  background: color,
                  transition: "width 500ms ease",
                }}
              />
            </div>
            <div className="num w-32 shrink-0 text-right text-xs">
              {fmtRatePlain(i.rxBps)}
              <span className="text-ink-faint"> · {share.toFixed(0)}%</span>
            </div>
            {compact ? null : null}
          </div>
        );
      })}
    </div>
  );
}
