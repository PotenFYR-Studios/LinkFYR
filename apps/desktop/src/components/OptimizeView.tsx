import { useEffect, useMemo, useState } from "react";
import { bridge } from "../lib/bridge";
import { useSmoothNumber } from "../lib/smooth";
import { useStore } from "../state/store";
import type {
  Alert,
  BloatReport,
  BridgeInfo,
  BridgeMode,
  CapabilitiesReport,
  CapabilityState,
  DnsBenchmarkReport,
  Grade,
  MtuReport,
  RepairReport,
  Response,
  RouteAuditReport,
  RouteScanReport,
  SpeedtestReport,
  TcpAuditReport,
  ToolCapability,
  ToolRunReport,
  WifiAnalysis,
} from "@linkfyr/types";

/**
 * Optimize: every registry tool, one measured result at a time.
 * The tool list comes from the engine's capability report (registry-
 * driven), so new modules appear here without UI changes. Bridges get
 * their own control card below. No fabricated numbers; when the engine
 * is not attached the view says so.
 */

const GROUP_ORDER = ["Latency", "Throughput", "Environment", "Security", "Repair", "Control"];

const RUNNING_NOTE: Record<string, string> = {
  dns_benchmark: "Querying resolvers with real DNS lookups",
  doh_benchmark: "Measuring DNS-over-HTTPS providers",
  dns_lookup: "Sending the query to your resolver",
  reverse_lookup: "Asking for the PTR record",
  resolver_consistency: "Comparing answers across resolvers",
  routescan: "Opening TCP connections along each path",
  route_scan: "Opening TCP connections along each path",
  latency_monitor: "Running the sustained probe series",
  jitter_burst: "Firing 20 rapid probes",
  icmp_ping: "Sending one real ICMP echo",
  gateway_latency: "Pinging the default gateway",
  ipv6_readiness: "Resolving AAAA and connecting over IPv6",
  bloat: "Saturating the link while probing latency",
  http_ttfb: "Timing the full request path",
  tls_check: "Establishing the TLS session",
  speedtest: "Moving real bytes both directions",
  port_scan: "Probing ports with TCP connects",
  connections: "Reading the OS socket table",
  listening_ports: "Reading the OS socket table",
  mtu: "Sending DF probes and binary-searching the MTU",
  wifi_scan: "Asking the OS for every visible network",
  tcp_audit: "Reading kernel TCP settings",
  route_audit: "Parsing the routing table",
  arp_table: "Reading the neighbor table",
  proxy_config: "Reading proxy configuration",
  hosts_file: "Parsing the hosts file",
  flush_dns: "Flushing the resolver cache",
  public_ip: "Asking the Internet how it sees you (opt-in)",
  dns_apply: "Applying the resolver list",
};

type RunState =
  | { phase: "idle" }
  | { phase: "running"; note: string }
  | { phase: "done"; report: ToolRunReport }
  | { phase: "failed"; message: string };

export function OptimizeView() {
  const [capabilities, setCapabilities] = useState<CapabilitiesReport | null>(null);
  const [toolId, setToolId] = useState<string | null>(null);
  const [run, setRun] = useState<RunState>({ phase: "idle" });
  const [target, setTarget] = useState("");
  const error = useStore((s) => s.error);

  useEffect(() => {
    let stop = false;
    void (async () => {
      const res = await bridge.request({ type: "optimize_capabilities" });
      if (!stop && res.type === "capabilities") {
        setCapabilities(res);
        setToolId((current) => current ?? res.tools[0]?.tool ?? null);
      }
    })();
    return () => {
      stop = true;
    };
  }, []);

  const groups = useMemo(() => {
    if (!capabilities) return [] as [string, ToolCapability[]][];
    const map = new Map<string, ToolCapability[]>();
    for (const t of capabilities.tools) {
      const key = t.group ?? "Other";
      const list = map.get(key) ?? [];
      list.push(t);
      map.set(key, list);
    }
    return [...map.entries()].sort(
      (a, b) => GROUP_ORDER.indexOf(a[0]) - GROUP_ORDER.indexOf(b[0]),
    );
  }, [capabilities]);

  const selected = capabilities?.tools.find((t) => t.tool === toolId) ?? null;

  const runTool = async () => {
    if (!selected) return;
    if (selected.takesTarget && !target.trim()) {
      setRun({ phase: "failed", message: "Enter a target host or IP first." });
      return;
    }
    const params: Record<string, string> = {};
    if (selected.takesTarget) params.target = target.trim();
    if (selected.tool === "http_ttfb" || selected.tool === "tls_check") {
      params.url = target.trim().startsWith("http") ? target.trim() : `https://${target.trim()}`;
    }
    if (selected.tool === "dns_lookup") params.domain = target.trim();
    setRun({
      phase: "running",
      note: RUNNING_NOTE[selected.tool] ?? "Running real measurements",
    });
    const res = await bridge.request({ type: "optimize_run", tool: selected.tool, params });
    if (res.type === "tool_run") {
      setRun({ phase: "done", report: res });
    } else if (res.type === "error") {
      setRun({ phase: "failed", message: res.message });
    } else {
      setRun({ phase: "failed", message: "Unexpected engine response." });
    }
  };

  if (!bridge.available) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="lf-card max-w-sm p-8 text-center">
          <h2 className="text-sm font-semibold">Engine not attached</h2>
          <p className="mt-2 text-xs leading-relaxed text-ink-muted">
            Optimization tools run against the real network through the
            engine. Launch the desktop app to use them.
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

  return (
    <div className="mx-auto flex max-w-[1200px] flex-col gap-4 p-6">
      <header className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">Optimize</h1>
          <p className="mt-0.5 text-xs text-ink-muted">
            {capabilities
              ? `${capabilities.tools.length} tools on this machine. Measured, not estimated.`
              : "Checking what this machine supports…"}
          </p>
        </div>
        {capabilities ? (
          <div className="lf-pill border border-stroke-soft text-ink-muted">
            {capabilities.platform}
            {capabilities.elevated ? " · elevated" : " · user"}
          </div>
        ) : null}
      </header>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[300px_1fr]">
        {/* Registry index */}
        <nav aria-label="Optimization tools" className="flex flex-col gap-4">
          {groups.map(([group, tools]) => (
            <section key={group} className="lf-card p-3">
              <div className="px-1 pb-1.5 text-2xs font-semibold tracking-wide text-ink-faint uppercase">
                {group}
              </div>
              {tools.map((t) => (
                <button
                  key={t.tool}
                  className="lf-rail-item"
                  data-active={toolId === t.tool}
                  onClick={() => {
                    setToolId(t.tool);
                    setRun({ phase: "idle" });
                  }}
                  aria-current={toolId === t.tool ? "true" : undefined}
                  title={t.name}
                >
                  <span className="flex-1 truncate">{t.name}</span>
                  <CapabilityDot state={t.state} />
                </button>
              ))}
            </section>
          ))}
          {!capabilities ? (
            <section className="lf-card p-4 text-xs text-ink-faint">Loading catalog…</section>
          ) : null}
        </nav>

        {/* Result panel */}
        <section className="lf-card min-h-[420px] p-6" aria-live="polite">
          {selected ? (
            <>
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h2 className="text-sm font-semibold">{selected.name}</h2>
                  <p className="mt-0.5 text-xs text-ink-muted">{selected.detail}</p>
                </div>
                <button
                  className="lf-btn shrink-0"
                  data-variant="primary"
                  onClick={() => void runTool()}
                  disabled={run.phase === "running" || selected.state === "unavailable"}
                >
                  {run.phase === "running" ? "Running" : "Run"}
                </button>
              </div>

              {selected.takesTarget ? (
                <label className="mt-4 block text-2xs text-ink-faint">
                  Target host
                  <input
                    value={target}
                    onChange={(e) => setTarget(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void runTool();
                    }}
                    placeholder="example.com or 1.1.1.1"
                    className="num mt-1 w-full rounded-[8px] border border-stroke bg-transparent px-3 py-2 text-sm outline-none focus:border-accent"
                    aria-label="Target host"
                  />
                </label>
              ) : null}

              <div key={`${toolId}-${run.phase}`} className="lf-enter mt-5">
                {run.phase === "idle" ? (
                  <IdlePanel tool={selected} />
                ) : null}
                {run.phase === "running" ? (
                  <div className="flex items-center gap-2.5 py-10 text-sm text-ink-muted">
                    <span className="lf-live-dot inline-block h-2 w-2 rounded-full bg-accent" />
                    {run.note}
                  </div>
                ) : null}
                {run.phase === "failed" ? (
                  <div
                    className="rounded-[10px] border p-4 text-xs text-ink-muted"
                    style={{ borderColor: "var(--color-danger)" }}
                  >
                    {run.message}
                  </div>
                ) : null}
                {run.phase === "done" ? (
                  <ToolRunPanel tool={selected.tool} report={run.report} />
                ) : null}
              </div>
            </>
          ) : (
            <div className="py-10 text-center text-xs text-ink-faint">Select a tool to begin.</div>
          )}
        </section>
      </div>

      <BridgesSection />
    </div>
  );
}

function CapabilityDot({ state }: { state: CapabilityState }) {
  const color =
    state === "available"
      ? "var(--color-ok)"
      : state === "elevated"
        ? "var(--color-warn)"
        : "var(--color-ink-faint)";
  return (
    <span
      title={state.replace("_", " ")}
      aria-label={state.replace("_", " ")}
      className="inline-block h-1.5 w-1.5 shrink-0 rounded-full"
      style={{ background: color }}
    />
  );
}

function IdlePanel({ tool }: { tool: ToolCapability }) {
  const note =
    tool.state === "available"
      ? "Ready to run on this machine."
      : tool.state === "elevated"
        ? "Runs, but changing the system needs an elevated app."
        : tool.state === "platform_limited"
          ? "Partially supported on this platform."
          : tool.state === "unavailable"
            ? "Not available on this machine (missing OS tool or adapter)."
            : "Checking support…";
  return (
    <div className="rounded-[10px] border border-dashed border-stroke p-5 text-xs text-ink-faint">
      <div className="font-medium text-ink-muted">{tool.name}</div>
      <p className="mt-1 leading-relaxed">{note}</p>
    </div>
  );
}

/**
 * Registry result renderer: typed panels where a rich renderer exists,
 * an honest summary + structured JSON for everything else. The JSON is
 * the tool's typed report serialized, not ad-hoc UI state.
 */
export function ToolRunPanel({ tool, report }: { tool: string; report: ToolRunReport }) {
  const d = report.data;
  switch (tool) {
    case "dns_benchmark":
      return <DnsResult report={d as unknown as DnsBenchmarkReport} />;
    case "route_scan":
      return <RouteScanResult report={d as unknown as RouteScanReport} />;
    case "bloat":
      return <BloatResult report={d as unknown as BloatReport} />;
    case "speedtest":
      return <SpeedtestResult report={d as unknown as SpeedtestReport} />;
    case "mtu":
      return <MtuResult report={d as unknown as MtuReport} />;
    case "wifi_scan":
      return <WifiResult report={d as unknown as WifiAnalysis} />;
    case "tcp_audit":
      return <TcpResult report={d as unknown as TcpAuditReport} />;
    case "route_audit":
      return <RouteAuditResult report={d as unknown as RouteAuditReport} />;
    case "flush_dns":
      return <RepairResult report={d as unknown as RepairReport} />;
    default:
      return <GenericResult report={report} />;
  }
}

function GenericResult({ report }: { report: ToolRunReport }) {
  const json = useMemo(() => {
    if (report.data === null || report.data === undefined) return "";
    try {
      return JSON.stringify(report.data, null, 2);
    } catch {
      return String(report.data);
    }
  }, [report.data]);

  return (
    <div className="flex flex-col gap-3">
      <p
        className="text-sm font-medium"
        style={{ color: report.ok ? "var(--color-ink)" : "var(--color-warn)" }}
      >
        {report.summary}
      </p>
      {json ? (
        <pre
          className="num max-h-[320px] overflow-auto rounded-[10px] border border-stroke-soft p-3 text-2xs leading-relaxed text-ink-muted"
          aria-label="Raw measurement data"
        >
          {json}
        </pre>
      ) : null}
      <p className="text-2xs text-ink-faint">measured in {report.tookMs} ms</p>
    </div>
  );
}

/* ---- Bridges: tiered control with honest labels ---- */

export function BridgesPanel({
  bridges,
  busy,
  onCreate,
  onRemove,
}: {
  bridges: BridgeInfo[];
  busy: boolean;
  onCreate: (spec: { name: string; members: string[]; mode: BridgeMode }) => void;
  onRemove: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const [members, setMembers] = useState("");
  const [mode, setMode] = useState<BridgeMode>("l2_switch");
  const parsedMembers = members
    .split(",")
    .map((m) => m.trim())
    .filter(Boolean);
  const valid =
    name.length > 0 && /^[A-Za-z0-9_-]+$/.test(name) && parsedMembers.length >= 2;

  return (
    <div className="flex flex-col gap-4">
      <ul className="flex flex-col gap-2">
        {bridges.length === 0 ? (
          <li className="text-xs text-ink-faint">
            No bridges on this machine. Creating one needs an elevated app; the exact
            commands are shown before anything runs.
          </li>
        ) : (
          bridges.map((b) => (
            <li
              key={b.name}
              className="flex items-center gap-3 rounded-[8px] border border-stroke-soft p-3"
            >
              <span className="num min-w-0 flex-1 truncate text-xs font-medium">{b.name}</span>
              <span
                className="lf-pill border border-stroke-soft"
                style={{ color: b.mode === "nat_share" ? "var(--color-warn)" : "var(--color-ok)" }}
                title={
                  b.mode === "nat_share"
                    ? "NAT sharing: different subnets, not a true bridge"
                    : "True layer-2 switch"
                }
              >
                {b.mode === "nat_share" ? "NAT share" : "L2 switch"}
              </span>
              <span className="text-2xs text-ink-faint">{b.state}</span>
              <button className="lf-btn px-2 py-1 text-2xs" onClick={() => onRemove(b.name)} disabled={busy}>
                Remove
              </button>
            </li>
          ))
        )}
      </ul>

      <div className="flex flex-wrap items-end gap-2 border-t border-stroke-soft pt-4">
        <label className="text-2xs text-ink-faint">
          Name
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="br0"
            className="num mt-1 w-28 rounded-[8px] border border-stroke bg-transparent px-2 py-1.5 text-sm outline-none focus:border-accent"
            aria-label="Bridge name"
          />
        </label>
        <label className="min-w-[220px] flex-1 text-2xs text-ink-faint">
          Member interfaces (comma-separated)
          <input
            value={members}
            onChange={(e) => setMembers(e.target.value)}
            placeholder="Ethernet, Ethernet 2"
            className="mt-1 w-full rounded-[8px] border border-stroke bg-transparent px-2 py-1.5 text-sm outline-none focus:border-accent"
            aria-label="Member interfaces"
          />
        </label>
        <div
          role="radiogroup"
          aria-label="Bridge mode"
          className="flex overflow-hidden rounded-lg border border-stroke"
          style={{ background: "var(--color-raised)" }}
        >
          {(
            [
              { value: "l2_switch", label: "L2 switch" },
              { value: "nat_share", label: "NAT fallback" },
            ] as { value: BridgeMode; label: string }[]
          ).map((o) => (
            <button
              key={o.value}
              role="radio"
              aria-checked={mode === o.value}
              onClick={() => setMode(o.value)}
              className="px-3 py-1.5 text-2xs font-medium"
              style={{
                background:
                  mode === o.value
                    ? "color-mix(in srgb, var(--color-accent) 18%, transparent)"
                    : "transparent",
                color: mode === o.value ? "var(--color-accent-soft)" : "var(--color-ink-muted)",
              }}
            >
              {o.label}
            </button>
          ))}
        </div>
        <button
          className="lf-btn"
          disabled={!valid || busy}
          onClick={() => {
            onCreate({ name, members: parsedMembers, mode });
            setName("");
            setMembers("");
          }}
        >
          Create bridge
        </button>
      </div>
      <p className="text-2xs text-ink-faint">
        {mode === "nat_share"
          ? "NAT fallback shares one uplink through the others; the two sides stay separate subnets. Windows: Hyper-V L2 switch is the true bridge; NAT works everywhere."
          : "Windows: Hyper-V virtual switch with teaming (true bridge, needs Hyper-V). Linux/macOS: real layer-2 bridge."}
      </p>
    </div>
  );
}

function BridgesSection() {
  const [bridges, setBridges] = useState<BridgeInfo[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const refresh = async () => {
    const res = await bridge.request({ type: "bridge_list" });
    if (res.type === "bridges") setBridges(res.bridges);
  };

  useEffect(() => {
    void refresh();
  }, []);

  const create = async (spec: { name: string; members: string[]; mode: BridgeMode }) => {
    setBusy(true);
    setNote(null);
    const res = await bridge.request({
      type: "bridge_create",
      spec: { name: spec.name, members: spec.members, mode: spec.mode, internalPrefix: null },
    });
    if (res.type === "bridge_reported") {
      setNote(
        res.outcome === "applied"
          ? "Bridge created."
          : res.outcome === "needs_elevation"
            ? "Needs an elevated app session. Run LinkFYR as administrator/root, or use the commands:"
            : `${res.outcome}: ${res.detail}`,
      );
      if (res.outcome !== "applied" && res.commands.length > 0) {
        setNote((n) => `${n ?? ""}\n${res.commands.map((c) => `$ ${c}`).join("\n")}`);
      }
    }
    await refresh();
    setBusy(false);
  };

  const remove = async (name: string) => {
    setBusy(true);
    const res = await bridge.request({ type: "bridge_remove", name });
    if (res.type === "bridge_reported" && res.outcome !== "applied") {
      setNote(`${res.outcome}: ${res.detail}`);
    }
    await refresh();
    setBusy(false);
  };

  return (
    <section className="lf-card p-6" aria-label="Network bridges">
      <h2 className="text-sm font-semibold">Network bridges</h2>
      <p className="mt-0.5 text-xs text-ink-muted">
        Combine two or more connections. Windows removed its bridge UI; these are the
        supported alternatives.
      </p>
      <div className="mt-4">
        <BridgesPanel
          bridges={bridges ?? []}
          busy={busy}
          onCreate={(spec) => void create(spec)}
          onRemove={(name) => void remove(name)}
        />
      </div>
      {note ? (
        <pre className="num mt-3 whitespace-pre-wrap rounded-[8px] border border-stroke-soft p-3 text-2xs text-ink-muted">
          {note}
        </pre>
      ) : null}
    </section>
  );
}

/* ---- typed result renderers (unchanged behavior, reused by registry) ---- */

function DnsResult({ report }: { report: DnsBenchmarkReport }) {
  const [apply, setApply] = useState<{ busy: boolean; outcome: string | null }>({
    busy: false,
    outcome: null,
  });
  const winners = report.results.filter((r) => r.success);
  return (
    <div className="flex flex-col gap-4">
      <table className="w-full text-xs">
        <thead>
          <tr className="text-left text-2xs text-ink-faint">
            <th className="pb-2 font-medium">Resolver</th>
            <th className="pb-2 font-medium">Cached</th>
            <th className="pb-2 font-medium">Fresh</th>
            <th className="pb-2 text-right font-medium">Score</th>
          </tr>
        </thead>
        <tbody>
          {winners.map((r) => (
            <tr key={r.server} className="border-t border-stroke-soft">
              <td className="py-2">
                <span className="num">{r.server}</span>
                <span className="ml-2 text-2xs text-ink-faint">{r.label}</span>
                {report.recommended === r.server ? (
                  <span className="lf-pill ml-2 border border-stroke-soft text-accent-soft">
                    fastest
                  </span>
                ) : null}
              </td>
              <td className="num py-2 text-ink-muted">{r.cachedMs?.toFixed(1)} ms</td>
              <td className="num py-2 text-ink-muted">{r.uncachedMs?.toFixed(1)} ms</td>
              <td className="num py-2 text-right">{r.score?.toFixed(1)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {winners.length === 0 ? (
        <p className="text-xs text-ink-faint">No resolver answered. Check connectivity.</p>
      ) : null}
      {report.recommended ? (
        <div className="flex flex-wrap items-center gap-3">
          <button
            className="lf-btn"
            disabled={apply.busy}
            onClick={async () => {
              setApply({ busy: true, outcome: null });
              const res = await bridge.request({
                type: "optimize_dns_apply",
                servers: [report.recommended!],
                interface: null,
              });
              setApply({
                busy: false,
                outcome: res.type === "dns_applied" ? res.outcome : "failed",
              });
            }}
          >
            Set {report.recommended} as system DNS
          </button>
          {apply.outcome ? (
            <span
              className="text-2xs"
              style={{
                color: apply.outcome === "applied" ? "var(--color-ok)" : "var(--color-warn)",
              }}
            >
              {apply.outcome === "applied"
                ? "Applied. Previous servers were recorded for restore."
                : apply.outcome === "needs_elevation"
                  ? "Needs an elevated app session to change system DNS."
                  : `Could not apply: ${apply.outcome}`}
            </span>
          ) : (
            <span className="text-2xs text-ink-faint">
              Uses the OS resolver setting; reversible from Settings later.
            </span>
          )}
        </div>
      ) : null}
      <p className="text-2xs text-ink-faint">
        Score blends warm-cache and fresh lookups. Your system currently uses:{" "}
        {report.systemResolvers.length ? report.systemResolvers.join(", ") : "(not detected)"}
      </p>
    </div>
  );
}

function RouteScanResult({ report }: { report: RouteScanReport }) {
  const medians = report.paths.map((p) => p.medianMs ?? 0).filter((m) => m > 0);
  const max = Math.max(...medians, 1);
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm font-medium">{report.verdict}</p>
      <div className="flex flex-col gap-2.5">
        {report.paths.map((p) => (
          <div key={`${p.label}-${p.addr}`} className="flex items-center gap-3 text-xs">
            <span className="w-40 shrink-0 truncate text-ink-muted" title={p.addr}>
              {p.label}
            </span>
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-raised">
              <div
                className="h-full rounded-full"
                style={{
                  width: `${Math.max(((p.medianMs ?? 0) / max) * 100, p.medianMs ? 3 : 0)}%`,
                  background: "var(--color-accent)",
                }}
              />
            </div>
            <span className="num w-16 text-right text-ink-muted">
              {p.medianMs !== null ? `${p.medianMs.toFixed(1)} ms` : "no answer"}
            </span>
          </div>
        ))}
      </div>
      <p className="text-xs text-ink-muted">{report.recommendation}</p>
    </div>
  );
}

const GRADE_TEXT: Record<Grade, string> = {
  a_plus: "A+",
  a: "A",
  b: "B",
  c: "C",
  d: "D",
  f: "F",
};
const GRADE_COLOR: Record<Grade, string> = {
  a_plus: "var(--color-ok)",
  a: "var(--color-ok)",
  b: "var(--color-ok)",
  c: "var(--color-warn)",
  d: "var(--color-warn)",
  f: "var(--color-danger)",
};

function BloatResult({ report }: { report: BloatReport }) {
  const animation = useStore((s) => s.config?.preferences.animation ?? "full");
  const down = useSmoothNumber(report.downAddedMs, animation === "full");
  const up = useSmoothNumber(report.upAddedMs, animation === "full");
  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-2 gap-4">
        <BloatGrade label="Download" grade={report.downGrade} added={down} mbps={report.downMbps} />
        <BloatGrade label="Upload" grade={report.upGrade} added={up} mbps={report.upMbps} />
      </div>
      <p className="text-2xs text-ink-faint">
        Baseline latency {report.baselineMs.toFixed(1)} ms to {report.probeTarget}. Grades follow
        the added latency while the link is busy: A+ under 5 ms, F over 200 ms.
      </p>
    </div>
  );
}

function BloatGrade({
  label,
  grade,
  added,
  mbps,
}: {
  label: string;
  grade: Grade;
  added: number;
  mbps: number;
}) {
  return (
    <div className="rounded-[10px] border border-stroke-soft p-4">
      <div className="text-2xs font-semibold tracking-wide text-ink-faint uppercase">{label}</div>
      <div className="mt-2 flex items-baseline gap-3">
        <span className="num text-4xl font-semibold" style={{ color: GRADE_COLOR[grade] }} data-metric>
          {GRADE_TEXT[grade]}
        </span>
        <span className="num text-sm text-ink-muted">+{Math.round(added)} ms when busy</span>
      </div>
      <div className="num mt-2 text-2xs text-ink-faint">load {mbps.toFixed(1)} Mbps</div>
    </div>
  );
}

function SpeedtestResult({ report }: { report: SpeedtestReport }) {
  const animation = useStore((s) => s.config?.preferences.animation ?? "full");
  const down = useSmoothNumber(report.downloadMbps, animation === "full");
  const up = useSmoothNumber(report.uploadMbps, animation === "full");
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-3 gap-3 text-center">
        <BigMetric
          label="Latency"
          value={report.latencyMs !== null ? `${report.latencyMs.toFixed(1)} ms` : "unreachable"}
        />
        <BigMetric label="Download" value={`${down.toFixed(1)} Mbps`} />
        <BigMetric label="Upload" value={`${up.toFixed(1)} Mbps`} />
      </div>
      <p className="text-2xs text-ink-faint">
        Endpoint {report.endpoint}.{" "}
        {report.latencyMs === null
          ? "The endpoint did not answer; nothing was measured."
          : `${report.durationS}s per direction, real transfers.`}
      </p>
    </div>
  );
}

function BigMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[10px] border border-stroke-soft py-3">
      <div className="num text-lg font-semibold" data-metric>
        {value}
      </div>
      <div className="mt-0.5 text-2xs text-ink-faint">{label}</div>
    </div>
  );
}

function MtuResult({ report }: { report: MtuReport }) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-baseline gap-3">
        <span className="num text-4xl font-semibold" data-metric>
          {report.pathMtu > 0 ? report.pathMtu : "?"}
        </span>
        <span className="text-xs text-ink-muted">bytes path MTU</span>
      </div>
      <p className="text-xs text-ink-muted">{report.verdict}</p>
      <p className="text-2xs text-ink-faint">
        {report.probes} DF probes to {report.target}
      </p>
    </div>
  );
}

function WifiResult({ report }: { report: WifiAnalysis }) {
  const loads = [...report.channelLoad2g, ...report.channelLoad5g];
  const max = Math.max(...loads.map((l) => l.networks), 1);
  return (
    <div className="flex flex-col gap-4">
      {report.networks.length === 0 ? (
        <p className="text-xs text-ink-faint">{report.explanation}</p>
      ) : (
        <>
          <p className="text-xs text-ink-muted">{report.explanation}</p>
          <div className="grid grid-cols-2 gap-x-6 gap-y-2">
            {loads.map((l) => (
              <div key={l.channel} className="flex items-center gap-2 text-2xs">
                <span className="num w-14 text-ink-muted">ch {l.channel}</span>
                <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-raised">
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${(l.networks / max) * 100}%`,
                      background:
                        report.recommendation2g === l.channel || report.recommendation5g === l.channel
                          ? "var(--color-ok)"
                          : "var(--color-accent)",
                    }}
                  />
                </div>
                <span className="num w-6 text-right text-ink-faint">{l.networks}</span>
              </div>
            ))}
          </div>
          <p className="text-2xs text-ink-faint">
            {report.networks.length} networks visible. Recommended channels shown in green.
          </p>
        </>
      )}
    </div>
  );
}

function TcpResult({ report }: { report: TcpAuditReport }) {
  return (
    <div className="flex flex-col gap-3">
      {report.checks.length === 0 ? (
        <p className="text-xs text-ink-faint">No TCP settings could be read on this platform.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {report.checks.map((c) => (
            <li
              key={c.key}
              className="flex items-start gap-2.5 rounded-[8px] border border-stroke-soft p-2.5"
            >
              <span
                className="mt-1 inline-block h-1.5 w-1.5 shrink-0 rounded-full"
                style={{
                  background:
                    c.status === "ok"
                      ? "var(--color-ok)"
                      : c.status === "suboptimal"
                        ? "var(--color-warn)"
                        : "var(--color-ink-faint)",
                }}
              />
              <div className="min-w-0 flex-1">
                <div className="num truncate text-xs font-medium" title={c.key}>
                  {c.key}
                </div>
                <div className="text-2xs text-ink-muted">
                  now <span className="num">{c.current}</span>
                  {c.status !== "ok" ? (
                    <>
                      {" "}· suggested <span className="num">{c.recommended}</span>
                    </>
                  ) : null}
                </div>
                {c.fix ? (
                  <code className="num mt-1 block truncate text-2xs text-ink-faint" title={c.fix}>
                    {c.fix}
                  </code>
                ) : null}
              </div>
            </li>
          ))}
        </ul>
      )}
      <p className="text-xs text-ink-muted">{report.summary}</p>
    </div>
  );
}

function RouteAuditResult({ report }: { report: RouteAuditReport }) {
  return (
    <div className="flex flex-col gap-4">
      <div className="text-sm">
        <span className="num font-semibold">{report.defaultCount}</span>{" "}
        <span className="text-ink-muted">
          default {report.defaultCount === 1 ? "route" : "routes"}
        </span>
      </div>
      {report.anomalies.length === 0 ? (
        <p className="text-xs text-ink-muted">Nothing suspicious in the routing table.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {report.anomalies.map((a) => (
            <li
              key={a}
              className="rounded-[8px] border border-stroke-soft p-3 text-xs leading-relaxed text-ink-muted"
              style={{ borderLeft: "2px solid var(--color-warn)" }}
            >
              {a}
            </li>
          ))}
        </ul>
      )}
      <details className="text-2xs text-ink-faint">
        <summary className="cursor-pointer">Full table ({report.entries.length} entries)</summary>
        <table className="num mt-2 w-full text-left">
          <tbody>
            {report.entries.map((e, i) => (
              <tr key={`${e.destination}-${i}`} className="border-t border-stroke-soft">
                <td className="py-1 pr-4">{e.destination}</td>
                <td className="py-1 pr-4 text-ink-muted">{e.gateway ?? "on-link"}</td>
                <td className="py-1 text-ink-faint">{e.interface ?? "-"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}

function RepairResult({ report }: { report: RepairReport }) {
  return (
    <div className="flex flex-col gap-2">
      <p
        className="text-sm font-medium"
        style={{ color: report.outcome === "applied" ? "var(--color-ok)" : "var(--color-warn)" }}
      >
        {report.outcome === "applied" ? "DNS cache flushed" : report.outcome}
      </p>
      <p className="text-xs text-ink-muted">{report.detail}</p>
    </div>
  );
}

/** Kept for the direct typed responses (used by tests and older callers). */
export function ResultPanel({ response }: { response: Response }) {
  switch (response.type) {
    case "dns_benchmark":
      return <DnsResult report={response} />;
    case "route_scan":
      return <RouteScanResult report={response} />;
    case "bloat_test":
      return <BloatResult report={response} />;
    case "speed_test":
      return <SpeedtestResult report={response} />;
    case "mtu":
      return <MtuResult report={response} />;
    case "wifi_scan":
      return <WifiResult report={response} />;
    case "tcp_audit":
      return <TcpResult report={response} />;
    case "route_audit":
      return <RouteAuditResult report={response} />;
    case "repaired":
      return <RepairResult report={response} />;
    default:
      return null;
  }
}

/* ---- Alerts card (Optimize view keeps everything tool-related together) ---- */

export function AlertsCard({ alerts }: { alerts: Alert[] }) {
  return (
    <section className="lf-card p-5" aria-label="Recent alerts">
      <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Recent alerts</h2>
      {alerts.length === 0 ? (
        <p className="mt-3 text-xs text-ink-faint">
          Nothing yet. Interface up/down events and health drops appear here.
        </p>
      ) : (
        <ul className="mt-3 flex flex-col gap-2">
          {alerts.slice(-5).reverse().map((a) => (
            <li key={a.id} className="flex items-start gap-2.5 text-xs">
              <span
                className="mt-1.5 inline-block h-1.5 w-1.5 shrink-0 rounded-full"
                style={{
                  background:
                    a.severity === "info"
                      ? "var(--color-ok)"
                      : a.severity === "warning"
                        ? "var(--color-warn)"
                        : "var(--color-danger)",
                }}
              />
              <div className="min-w-0">
                <span className="font-medium">{a.title}</span>
                <span className="ml-2 text-2xs text-ink-faint">{a.body}</span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
