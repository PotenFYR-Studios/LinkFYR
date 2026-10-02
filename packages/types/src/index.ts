/**
 * Mirror of crates/linkfyr-model (the wire contract).
 * Serialized with serde camelCase / snake_case enums on the Rust side.
 * Keep in lockstep; a fixture round-trip test guards this (see src/__tests__).
 */

export type IfKind =
  | "ethernet"
  | "wifi"
  | "cellular"
  | "virtual"
  | "tunnel"
  | "loopback"
  | "other";

export type IfStatus = "up" | "down" | "dormant" | "unknown";

export type HealthStatus = "healthy" | "degraded" | "poor" | "down";

export interface Interface {
  id: string;
  name: string;
  friendlyName: string;
  kind: IfKind;
  status: IfStatus;
  mac: string | null;
  ipv4: string[];
  ipv6: string[];
  gateway: string | null;
  mtu: number | null;
  speedBps: number | null;
  metered: boolean;
}

export interface IfCounters {
  id: string;
  rxBytes: number;
  txBytes: number;
  rxPackets: number;
  txPackets: number;
  rxErrors: number;
  txErrors: number;
  timestampMs: number;
}

export interface HealthScore {
  overall: number;
  status: HealthStatus;
  factors: [string, number][];
}

export interface InterfaceTelemetry {
  interface: Interface;
  rxBps: number;
  txBps: number;
  health: HealthScore | null;
  errors: IfCounters;
}

export interface ProbeStats {
  rttAvgMs: number | null;
  rttMinMs: number | null;
  rttMaxMs: number | null;
  jitterMs: number | null;
  lossPct: number | null;
  sampleCount: number;
}

export interface Totals {
  rxBps: number;
  txBps: number;
}

export interface Snapshot {
  timestampMs: number;
  engineVersion: string;
  interfaces: InterfaceTelemetry[];
  totals: Totals;
  internet: ProbeStats;
}

export interface HistoryPoint {
  timestampMs: number;
  rxBps: number;
  txBps: number;
}

/* ---- linkfyr-ipc ---- */

export type Theme = "system" | "dark" | "light";
export type AnimationLevel = "full" | "reduced" | "off";

export interface Preferences {
  theme: Theme;
  expertMode: boolean;
  animation: AnimationLevel;
  localOnly: boolean;
  closeToTray: boolean;
}

export interface Config {
  schemaVersion: number;
  preferences: Preferences;
}

export type Request =
  | { type: "get_snapshot" }
  | { type: "get_interfaces" }
  | { type: "get_history"; maxPoints: number }
  | { type: "get_config" }
  | { type: "update_preferences"; preferences: Preferences }
  | { type: "ping" }
  | { type: "optimize_capabilities" }
  | { type: "optimize_dns_benchmark" }
  | { type: "optimize_dns_apply"; servers: string[]; interface: string | null }
  | { type: "optimize_route_scan"; target: string }
  | { type: "optimize_bloat_test"; durationS: number | null }
  | { type: "optimize_speed_test"; endpoint: string | null; durationS: number | null }
  | { type: "optimize_mtu"; target: string }
  | { type: "optimize_wifi_scan" }
  | { type: "optimize_tcp_audit" }
  | { type: "optimize_route_audit" }
  | { type: "optimize_repair"; action: RepairAction }
  | { type: "optimize_run"; tool: string; params: Record<string, string> }
  | { type: "bridge_list" }
  | { type: "bridge_create"; spec: BridgeSpec }
  | { type: "bridge_remove"; name: string }
  | { type: "get_alerts" };

export type Response =
  | ({ type: "snapshot" } & Snapshot)
  | { type: "interfaces"; interfaces: Interface[] }
  | { type: "history"; points: HistoryPoint[] }
  | ({ type: "config" } & Config)
  | ({ type: "preferences_updated" } & Preferences)
  | { type: "pong"; version: string }
  | ({ type: "capabilities" } & CapabilitiesReport)
  | ({ type: "dns_benchmark" } & DnsBenchmarkReport)
  | ({ type: "dns_applied" } & DnsApplyReport)
  | ({ type: "route_scan" } & RouteScanReport)
  | ({ type: "bloat_test" } & BloatReport)
  | ({ type: "speed_test" } & SpeedtestReport)
  | ({ type: "mtu" } & MtuReport)
  | ({ type: "wifi_scan" } & WifiAnalysis)
  | ({ type: "tcp_audit" } & TcpAuditReport)
  | ({ type: "route_audit" } & RouteAuditReport)
  | ({ type: "repaired" } & RepairReport)
  | ({ type: "tool_run" } & ToolRunReport)
  | { type: "bridges"; bridges: BridgeInfo[] }
  | ({ type: "bridge_reported" } & BridgeReport)
  | { type: "alerts"; alerts: Alert[] }
  | { type: "error"; code: string; message: string };

/* ---- Optimization toolkit (linkfyr-model optimize) ---- */

export type Grade = "a_plus" | "a" | "b" | "c" | "d" | "f";

export type CapabilityState = "available" | "elevated" | "unavailable" | "platform_limited";

export interface ToolCapability {
  tool: string;
  state: CapabilityState;
  detail: string;
  /** Registry metadata, kept in the same record so catalog and
   * capability can never drift. */
  name?: string;
  group?: string;
  takesTarget?: boolean;
}

export interface CapabilitiesReport {
  elevated: boolean;
  platform: string;
  tools: ToolCapability[];
}

export interface DnsResolverResult {
  server: string;
  label: string;
  success: boolean;
  cachedMs: number | null;
  uncachedMs: number | null;
  score: number | null;
  error: string | null;
}

export interface DnsBenchmarkReport {
  results: DnsResolverResult[];
  recommended: string | null;
  systemResolvers: string[];
}

export interface DnsApplyReport {
  interface: string | null;
  servers: string[];
  previous: string[];
  outcome: string;
  detail: string;
}

export interface LatencyPath {
  label: string;
  addr: string;
  samples: number;
  success: number;
  minMs: number | null;
  medianMs: number | null;
  p95Ms: number | null;
  jitterMs: number | null;
}

export interface RouteScanReport {
  target: string;
  paths: LatencyPath[];
  verdict: string;
  recommendation: string;
}

export interface BloatReport {
  baselineMs: number;
  downAddedMs: number;
  upAddedMs: number;
  downGrade: Grade;
  upGrade: Grade;
  downMbps: number;
  upMbps: number;
  durationS: number;
  probeTarget: string;
}

export interface SpeedtestReport {
  endpoint: string;
  latencyMs: number | null;
  downloadMbps: number;
  uploadMbps: number;
  durationS: number;
}

export interface MtuReport {
  target: string;
  pathMtu: number;
  probes: number;
  verdict: string;
}

export interface WifiAp {
  ssid: string | null;
  bssid: string;
  channel: number;
  frequencyMhz: number;
  band: string;
  signalPct: number;
  security: string | null;
}

export interface ChannelLoad {
  channel: number;
  networks: number;
}

export interface WifiAnalysis {
  networks: WifiAp[];
  channelLoad2g: ChannelLoad[];
  channelLoad5g: ChannelLoad[];
  recommendation2g: number | null;
  recommendation5g: number | null;
  explanation: string;
}

export type CheckStatus = "ok" | "suboptimal" | "unknown";

export interface TcpCheck {
  key: string;
  current: string;
  recommended: string;
  status: CheckStatus;
  fix: string | null;
}

export interface TcpAuditReport {
  platform: string;
  elevated: boolean;
  checks: TcpCheck[];
  summary: string;
}

export interface RouteEntry {
  destination: string;
  gateway: string | null;
  interface: string | null;
  metric: number | null;
}

export interface RouteAuditReport {
  entries: RouteEntry[];
  defaultCount: number;
  anomalies: string[];
}

export type RepairAction = "flush_dns";

export interface RepairReport {
  action: RepairAction;
  outcome: string;
  detail: string;
}

export type BridgeMode = "l2_switch" | "nat_share";

export interface BridgeSpec {
  name: string;
  members: string[];
  mode: BridgeMode;
  internalPrefix: string | null;
}

export interface BridgeInfo {
  name: string;
  mode: BridgeMode;
  members: string[];
  state: string;
}

export interface BridgeReport {
  action: string;
  outcome: string;
  detail: string;
  commands: string[];
}

export interface ToolRunReport {
  tool: string;
  ok: boolean;
  summary: string;
  tookMs: number;
  data: unknown;
}

export type Severity = "info" | "warning" | "critical";

export interface Alert {
  id: string;
  severity: Severity;
  title: string;
  body: string;
  timestampMs: number;
}
