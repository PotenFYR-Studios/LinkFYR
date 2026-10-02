import { useEffect, useState } from "react";
import { useStore } from "../state/store";
import { LiveChart } from "./LiveChart";
import { bridge } from "../lib/bridge";
import { fmtMs, fmtPct } from "../lib/format";
import type { HistoryPoint } from "@linkfyr/types";

/** The Internet path: live quality probes over time. Real data only. */
export function InternetView() {
  const { snapshot } = useStore();
  const [history, setHistory] = useState<HistoryPoint[]>([]);

  useEffect(() => {
    let stop = false;
    const pull = async () => {
      const res = await bridge.request({ type: "get_history", maxPoints: 240 });
      if (!stop && res.type === "history") setHistory(res.points);
    };
    void pull();
    const t = setInterval(pull, 1000);
    return () => {
      stop = true;
      clearInterval(t);
    };
  }, []);

  const inet = snapshot?.internet;

  return (
    <div className="mx-auto flex max-w-[1200px] flex-col gap-4 p-6">
      <header>
        <h1 className="text-base font-semibold tracking-tight">Internet</h1>
        <p className="mt-0.5 text-xs text-ink-muted">
          Path quality measured by timing real TCP connects to public resolvers
          (unprivileged, no raw sockets required).
        </p>
      </header>

      <section className="lf-card p-5" aria-label="Throughput">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted">
          Throughput (last 4 minutes)
        </h2>
        <div className="mt-3">
          <LiveChart points={history} height={220} ariaLabel="Throughput over the last minutes" />
        </div>
      </section>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <section className="lf-card p-5" aria-label="Latency">
          <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Latency</h2>
          <div className="num mt-3 text-2xl font-semibold" data-metric>
            {fmtMs(inet?.rttAvgMs)}
          </div>
          <div className="mt-1 text-2xs text-ink-muted">
            min {fmtMs(inet?.rttMinMs)} · max {fmtMs(inet?.rttMaxMs)}
          </div>
        </section>

        <section className="lf-card p-5" aria-label="Jitter">
          <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Jitter</h2>
          <div className="num mt-3 text-2xl font-semibold" data-metric>
            {fmtMs(inet?.jitterMs)}
          </div>
          <div className="mt-1 text-2xs text-ink-muted">mean successive difference</div>
        </section>

        <section className="lf-card p-5" aria-label="Loss">
          <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Loss</h2>
          <div className="num mt-3 text-2xl font-semibold" data-metric>
            {fmtPct(inet?.lossPct)}
          </div>
          <div className="mt-1 text-2xs text-ink-muted">
            {inet?.sampleCount ?? 0} probes in window
          </div>
        </section>
      </div>
    </div>
  );
}
