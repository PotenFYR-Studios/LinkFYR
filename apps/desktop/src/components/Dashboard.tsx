import { useEffect, useState } from "react";
import { useStore } from "../state/store";
import { LiveChart } from "./LiveChart";
import { ContributionBars } from "./ContributionBars";
import { HealthGauge } from "./HealthGauge";
import { AlertsCard } from "./OptimizeView";
import { fmtMs, fmtPct, fmtRate, fmtRatePlain, ifColor, kindLabel } from "../lib/format";
import { bridge } from "../lib/bridge";
import { useSmoothNumber } from "../lib/smooth";
import { IconUp } from "./icons";
import type { Alert, HistoryPoint } from "@linkfyr/types";

/**
 * Overview: the at-a-glance control center. Every number here is live
 * engine data; when the engine is absent the view says so instead of
 * inventing values.
 */
export function Dashboard() {
  const { snapshot, interfaces } = useStore();
  const [history, setHistory] = useState<HistoryPoint[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);

  useEffect(() => {
    let stop = false;
    const pull = async () => {
      const res = await bridge.request({ type: "get_history", maxPoints: 120 });
      if (!stop && res.type === "history") setHistory(res.points);
    };
    void pull();
    const t = setInterval(pull, 1000);
    return () => {
      stop = true;
      clearInterval(t);
    };
  }, []);

  useEffect(() => {
    let stop = false;
    const pull = async () => {
      const res = await bridge.request({ type: "get_alerts" });
      if (!stop && res.type === "alerts") setAlerts(res.alerts);
    };
    void pull();
    const t = setInterval(pull, 5000);
    return () => {
      stop = true;
      clearInterval(t);
    };
  }, []);

  const animation = useStore((s) => s.config?.preferences.animation ?? "full");
  const smoothRx = useSmoothNumber(snapshot?.totals.rxBps ?? 0, animation === "full");
  const smoothTx = useSmoothNumber(snapshot?.totals.txBps ?? 0, animation === "full");

  if (!snapshot) return <EngineOffline />;

  const down = fmtRate(smoothRx);
  const up = fmtRate(smoothTx);
  const upIfs = interfaces.filter((i) => i.interface.status === "up");

  return (
    <div className="mx-auto flex max-w-[1200px] flex-col gap-4 p-6">
      {/* Hero: combined throughput — the single focal point of the screen. */}
      <section
        className="lf-card lf-hero-beat relative overflow-hidden p-6"
        aria-label="Combined throughput"
        style={{
          background:
            "linear-gradient(180deg, color-mix(in srgb, var(--color-accent) 6%, var(--color-surface)), var(--color-surface) 62%)",
        }}
      >
        <div
          aria-hidden
          className="absolute inset-x-0 top-0 h-px"
          style={{
            background:
              "linear-gradient(90deg, transparent, color-mix(in srgb, var(--color-accent) 55%, transparent), transparent)",
          }}
        />
        <div className="flex items-start justify-between">
          <div>
            <div className="text-2xs font-medium tracking-wide text-ink-muted uppercase">
              Combined throughput
            </div>
            <div className="mt-2 flex items-baseline gap-3">
              <span className="num text-[52px] leading-none font-semibold tracking-tight" data-metric>
                {down.value}
              </span>
              <span className="text-base text-ink-muted">{down.unit}</span>
              <span className="flex items-center gap-1.5 border-l border-stroke pl-3 text-sm" style={{ color: "var(--color-ok)" }}>
                <IconUp size={14} />
                <span className="num text-base">{up.value}</span>
                <span className="text-ink-muted">{up.unit}</span>
              </span>
            </div>
          </div>
          <div className="flex items-center gap-1.5 text-2xs text-ink-faint">
            <span className="lf-live-dot inline-block h-1.5 w-1.5 rounded-full bg-accent" />
            live
          </div>
        </div>
        <div className="mt-4">
          <LiveChart points={history} height={170} ariaLabel="Throughput history, download and upload" />
        </div>
        <div className="mt-2 flex gap-5 text-2xs text-ink-muted">
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-[2px] w-4 rounded-full" style={{ background: "var(--color-accent)" }} />
            download
          </span>
          <span className="flex items-center gap-1.5">
            <span
              className="inline-block h-0 w-4"
              style={{ borderTop: "2px dashed var(--color-ok)" }}
            />
            upload
          </span>
        </div>
      </section>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {/* Interface contribution */}
        <section className="lf-card p-5 lg:col-span-2" aria-label="Interface contribution">
          <h2 className="text-xs font-semibold tracking-wide text-ink-muted">
            Interface contribution
          </h2>
          <div className="mt-4">
            <ContributionBars interfaces={interfaces} />
          </div>
        </section>

        {/* Internet path quality */}
        <section className="lf-card p-5" aria-label="Internet path quality">
          <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Internet path</h2>
          <div className="mt-4 grid grid-cols-3 gap-2 text-center">
            <Metric label="Latency" value={fmtMs(snapshot.internet.rttAvgMs)} />
            <Metric label="Jitter" value={fmtMs(snapshot.internet.jitterMs)} />
            <Metric label="Loss" value={fmtPct(snapshot.internet.lossPct)} />
          </div>
          <div className="mt-3 text-2xs text-ink-faint">
            {snapshot.internet.sampleCount > 0
              ? `${snapshot.internet.sampleCount} probes in window`
              : "probing…"}
          </div>
        </section>

        {/* Alerts (interface transitions + health drops) */}
        <AlertsCard alerts={alerts} />
      </div>

      {/* Interfaces strip */}
      <section className="lf-card p-5" aria-label="Interfaces">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Interfaces</h2>
        {upIfs.length === 0 ? (
          <div className="mt-3 text-xs text-ink-faint">No interfaces are up right now.</div>
        ) : (
          <div className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
            {upIfs.slice(0, 8).map((i) => (
              <div
                key={i.interface.id}
                className="rounded-[10px] border border-stroke-soft p-3"
                style={{ borderLeft: `2px solid ${ifColor(i.interface.kind)}` }}
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="truncate text-xs font-medium" title={i.interface.friendlyName}>
                    {i.interface.friendlyName}
                  </div>
                  <HealthGauge health={i.health} size={36} />
                </div>
                <div className="num mt-2 text-2xs text-ink-muted">
                  ↓ {fmtRatePlain(i.rxBps)} · ↑ {fmtRatePlain(i.txBps)}
                </div>
                <div className="text-2xs text-ink-faint">{kindLabel(i.interface.kind)}</div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-stroke-soft py-2.5">
      <div className="num text-sm font-medium" data-metric>
        {value}
      </div>
      <div className="mt-0.5 text-2xs text-ink-faint">{label}</div>
    </div>
  );
}

function EngineOffline() {
  const error = useStore((s) => s.error);
  return (
    <div className="flex h-full items-center justify-center">
      <div className="lf-card max-w-sm p-8 text-center">
        <h2 className="text-sm font-semibold">Engine not attached</h2>
        <p className="mt-2 text-xs leading-relaxed text-ink-muted">
          This preview is running outside the LinkFYR shell, so there is no live
          engine to sample. Launch the desktop app to see real interface telemetry.
        </p>
        {error ? (
          <p className="mt-3 rounded-md border border-stroke-soft p-2 text-left text-2xs text-ink-faint">
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}
