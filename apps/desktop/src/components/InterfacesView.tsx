import { useStore } from "../state/store";
import { HealthGauge } from "./HealthGauge";
import { fmtMs, fmtRatePlain, healthColor, ifColor, kindLabel } from "../lib/format";

/** Full interface inventory with health factors inline (expert value). */
export function InterfacesView() {
  const { interfaces, snapshot } = useStore();

  return (
    <div className="mx-auto flex max-w-[1200px] flex-col gap-4 p-6">
      <header>
        <h1 className="text-base font-semibold tracking-tight">Interfaces</h1>
        <p className="mt-0.5 text-xs text-ink-muted">
          {interfaces.length} discovered ·{" "}
          {interfaces.filter((i) => i.interface.status === "up").length} up
        </p>
      </header>

      {interfaces.length === 0 ? (
        <Empty />
      ) : (
        <div className="flex flex-col gap-3">
          {interfaces.map((t) => {
            const i = t.interface;
            const health = t.health;
            return (
              <section key={i.id} className="lf-card p-5" aria-label={i.friendlyName}>
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2.5">
                      <span
                        className="inline-block h-2.5 w-2.5 rounded-full"
                        style={{ background: ifColor(i.kind) }}
                        aria-hidden
                      />
                      <h2 className="truncate text-sm font-semibold">{i.friendlyName}</h2>
                      <StatusPill status={i.status} />
                      {i.metered ? (
                        <span
                          className="lf-pill"
                          style={{
                            color: "var(--color-warn)",
                            background: "color-mix(in srgb, var(--color-warn) 12%, transparent)",
                          }}
                        >
                          metered
                        </span>
                      ) : null}
                    </div>
                    <div className="mt-1.5 flex flex-wrap gap-x-5 gap-y-1 text-2xs text-ink-muted">
                      <span>{kindLabel(i.kind)}</span>
                      <span className="num">{i.ipv4[0] ?? i.ipv6[0] ?? "no address"}</span>
                      {i.gateway ? <span>gw {i.gateway}</span> : null}
                      {i.mac ? <span className="num">{i.mac}</span> : null}
                      {i.mtu ? <span>MTU {i.mtu}</span> : null}
                      {i.speedBps ? <span>{Math.round(i.speedBps / 1_000_000)} Mb/s link</span> : null}
                    </div>
                  </div>
                  <HealthGauge health={health} size={52} />
                </div>

                <div className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
                  <Stat label="Download" value={fmtRatePlain(t.rxBps)} />
                  <Stat label="Upload" value={fmtRatePlain(t.txBps)} />
                  <Stat label="Latency" value={fmtMs(snapshot?.internet.rttAvgMs)} />
                  <Stat
                    label="Errors (total)"
                    value={String(t.errors.rxErrors + t.errors.txErrors)}
                  />
                </div>

                {health ? (
                  <div className="mt-4 flex flex-wrap gap-x-6 gap-y-1 text-2xs text-ink-faint">
                    {health.factors.map(([name, v]) => (
                      <span key={name} className="flex items-center gap-1.5">
                        {name}
                        <span className="num" style={{ color: healthColor(v) }}>
                          {v}
                        </span>
                      </span>
                    ))}
                  </div>
                ) : null}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-stroke-soft px-3 py-2.5">
      <div className="num text-sm font-medium" data-metric>
        {value}
      </div>
      <div className="mt-0.5 text-2xs text-ink-faint">{label}</div>
    </div>
  );
}

function StatusPill({ status }: { status: string }) {
  const map: Record<string, { c: string; label: string }> = {
    up: { c: "var(--color-ok)", label: "up" },
    down: { c: "var(--color-ink-faint)", label: "down" },
    dormant: { c: "var(--color-warn)", label: "dormant" },
    unknown: { c: "var(--color-ink-faint)", label: "unknown" },
  };
  const s = map[status] ?? map["unknown"]!;
  return (
    <span
      className="lf-pill"
      style={{ color: s.c, background: `color-mix(in srgb, ${s.c} 12%, transparent)` }}
    >
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: s.c }} />
      {s.label}
    </span>
  );
}

function Empty() {
  return (
    <div className="lf-card p-10 text-center text-xs text-ink-muted">
      No interfaces discovered yet. The engine samples once per second; this view
      fills in as soon as the first snapshot lands.
    </div>
  );
}
