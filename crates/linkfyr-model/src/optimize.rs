//! Wire types for the Optimization Toolkit.
//!
//! These are measurements and recommendations produced by real network
//! tools (DNS benchmark, route scan, bufferbloat test, speed test, MTU
//! discovery, TCP audit, Wi-Fi analysis, route audit, repair actions).
//! All values are measured, never fabricated; when a tool cannot run the
//! corresponding report says so via its capability/outcome fields.

use serde::{Deserialize, Serialize};

/// Letter grade for latency-under-load (bufferbloat) results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    APlus,
    A,
    B,
    C,
    D,
    F,
}

impl Grade {
    pub fn label(self) -> &'static str {
        match self {
            Grade::APlus => "A+",
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
            Grade::D => "D",
            Grade::F => "F",
        }
    }
}

/// Honest capability state for a tool on this machine right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    /// The tool can run fully right now.
    Available,
    /// The tool is real but needs elevation (admin/root) to run.
    Elevated,
    /// A required OS facility or binary is missing.
    Unavailable,
    /// The OS supports the concept but not through any supported API here.
    PlatformLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCapability {
    pub tool: String,
    pub state: CapabilityState,
    pub detail: String,
    /// Registry metadata (name, group, blurb, target hint), kept in the
    /// same record so catalog and capability can never drift.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub takes_target: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesReport {
    pub elevated: bool,
    pub platform: String,
    pub tools: Vec<ToolCapability>,
}

/// One measured resolver from a DNS benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsResolverResult {
    pub server: String,
    pub label: String,
    pub success: bool,
    /// Median RTT in ms for a warm-cache lookup, when measured.
    pub cached_ms: Option<f64>,
    /// RTT in ms for a fresh (uncached-path) lookup, when measured.
    pub uncached_ms: Option<f64>,
    /// Lower is better; blends cached and uncached medians.
    pub score: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsBenchmarkReport {
    pub results: Vec<DnsResolverResult>,
    /// Best resolver by score among successful ones.
    pub recommended: Option<String>,
    /// Resolver(s) the OS currently uses, as detected.
    pub system_resolvers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsApplyReport {
    pub interface: Option<String>,
    pub servers: Vec<String>,
    /// Values captured before the change, for one-click restore.
    pub previous: Vec<String>,
    /// applied | needs_elevation | failed | unsupported.
    pub outcome: String,
    pub detail: String,
}

/// Latency statistics for one probed network path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyPath {
    pub label: String,
    pub addr: String,
    pub samples: u32,
    pub success: u32,
    pub min_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteScanReport {
    pub target: String,
    pub paths: Vec<LatencyPath>,
    pub verdict: String,
    pub recommendation: String,
}

/// Measured latency increase while the link is saturated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BloatReport {
    pub baseline_ms: f64,
    pub down_added_ms: f64,
    pub up_added_ms: f64,
    pub down_grade: Grade,
    pub up_grade: Grade,
    pub down_mbps: f64,
    pub up_mbps: f64,
    pub duration_s: u32,
    pub probe_target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedtestReport {
    pub endpoint: String,
    pub latency_ms: Option<f64>,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub duration_s: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtuReport {
    pub target: String,
    /// Largest IPv4 total size that traversed the path without fragmentation.
    pub path_mtu: u16,
    pub probes: u16,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiAp {
    pub ssid: Option<String>,
    pub bssid: String,
    pub channel: u16,
    pub frequency_mhz: u16,
    /// "2.4" | "5" | "6" | "unknown".
    pub band: String,
    /// Normalized 0-100, higher is stronger.
    pub signal_pct: u8,
    pub security: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelLoad {
    pub channel: u16,
    pub networks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiAnalysis {
    pub networks: Vec<WifiAp>,
    pub channel_load_2g: Vec<ChannelLoad>,
    pub channel_load_5g: Vec<ChannelLoad>,
    pub recommendation_2g: Option<u16>,
    pub recommendation_5g: Option<u16>,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Suboptimal,
    Unknown,
}

/// One TCP stack setting: current value, recommended value, and the real
/// command that changes it (needs elevation unless marked otherwise).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpCheck {
    pub key: String,
    pub current: String,
    pub recommended: String,
    pub status: CheckStatus,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpAuditReport {
    pub platform: String,
    pub elevated: bool,
    pub checks: Vec<TcpCheck>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteEntry {
    /// Destination network, normalized dotted form ("0.0.0.0/0" for default).
    pub destination: String,
    pub gateway: Option<String>,
    pub interface: Option<String>,
    pub metric: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteAuditReport {
    pub entries: Vec<RouteEntry>,
    pub default_count: u32,
    pub anomalies: Vec<String>,
}

/// One-shot real repair actions (DNS cache flush is the safe universal one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    FlushDns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    pub action: RepairAction,
    /// applied | needs_elevation | unavailable | failed.
    pub outcome: String,
    pub detail: String,
}

/// How a bridge is realized. `L2Switch` is a true learning bridge
/// (Linux/macOS, Windows with Hyper-V teaming); `NatShare` is the
/// supported Windows fallback that shares an uplink via NAT/forwarding
/// and is always labeled as such in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeMode {
    L2Switch,
    NatShare,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeSpec {
    pub name: String,
    /// Interface names/aliases to bridge, at least two for L2.
    pub members: Vec<String>,
    pub mode: BridgeMode,
    /// NatShare only: subnet served on the internal side (e.g. 192.168.137.0/24).
    pub internal_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeInfo {
    pub name: String,
    pub mode: BridgeMode,
    pub members: Vec<String>,
    /// up | down | unknown, as the OS reports it.
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeReport {
    pub action: String,
    /// applied | needs_elevation | unavailable | failed.
    pub outcome: String,
    pub detail: String,
    /// The exact commands (executed or to-run-as-admin).
    pub commands: Vec<String>,
}

/// One DNS-over-HTTPS provider measurement (JSON DoH API).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DohResult {
    pub endpoint: String,
    pub success: bool,
    pub median_ms: Option<f64>,
    pub answer: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DohReport {
    pub results: Vec<DohResult>,
    /// Best DoH endpoint when any answered.
    pub recommended: Option<String>,
    /// Best plain-UDP resolver from the same run, for comparison.
    pub udp_best_ms: Option<f64>,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortResult {
    pub port: u16,
    pub open: bool,
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortScanReport {
    pub target: String,
    pub ports: Vec<PortResult>,
    pub open_count: u32,
    pub scanned: u32,
}

/// One live socket attributed to a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConnection {
    pub proto: String,
    pub local: String,
    pub remote: String,
    pub state: String,
    pub pid: Option<u32>,
    pub process: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGroup {
    pub process: String,
    pub pid: Option<u32>,
    pub connections: u32,
    /// remote endpoints, deduplicated, capped for display.
    pub remotes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsReport {
    pub connections: Vec<AppConnection>,
    pub groups: Vec<AppGroup>,
    /// Why attribution may be partial (e.g. other users' sockets).
    pub detail: String,
}

/// Generic result envelope for registry tools. `data` always carries a
/// typed model struct serialized to JSON (never ad-hoc shapes), so the
/// wire stays inspectable and versioned even as the catalog grows to
/// 100+ tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunReport {
    pub tool: String,
    pub ok: bool,
    /// One-line human summary of what was measured.
    pub summary: String,
    pub took_ms: u64,
    /// The tool's typed report, serialized with the model's casing rules.
    pub data: serde_json::Value,
}

/// Registry metadata for one tool. The registry is the single source of
/// truth for enumeration (capabilities, CLI help, UI catalog).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub id: String,
    pub name: String,
    /// Latency | Throughput | Environment | Security | Repair | Control.
    pub group: String,
    pub blurb: String,
    /// The tool needs a target host/IP parameter.
    pub takes_target: bool,
}

/// Simple text-valued tool result (ping, proxy, hosts, and friends).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextReport {
    pub text: String,
    /// Optional measured latency in ms.
    pub ms: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimize_types_are_camel_case_on_the_wire() {
        let r = DnsResolverResult {
            server: "1.1.1.1".into(),
            label: "Cloudflare".into(),
            success: true,
            cached_ms: Some(8.0),
            uncached_ms: Some(21.0),
            score: Some(11.9),
            error: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"cachedMs\""));
        assert!(json.contains("\"uncachedMs\""));
        let back: DnsResolverResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.server, "1.1.1.1");
        assert_eq!(back.cached_ms, Some(8.0));
    }

    #[test]
    fn bloat_report_roundtrips_with_grades() {
        let r = BloatReport {
            baseline_ms: 12.0,
            down_added_ms: 180.0,
            up_added_ms: 640.0,
            down_grade: Grade::D,
            up_grade: Grade::F,
            down_mbps: 210.5,
            up_mbps: 22.1,
            duration_s: 20,
            probe_target: "1.1.1.1:443".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"downGrade\":\"d\""));
        assert!(json.contains("\"baselineMs\""));
        let back: BloatReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.up_grade, Grade::F);
        assert_eq!(Grade::APlus.label(), "A+");
    }

    #[test]
    fn capability_states_are_snake_case() {
        let t = ToolCapability {
            tool: "wifi_scan".into(),
            state: CapabilityState::PlatformLimited,
            detail: "airport utility not present".into(),
            name: "Wi-Fi channels".into(),
            group: "Environment".into(),
            takes_target: false,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"state\":\"platform_limited\""));
        assert!(json.contains("\"tool\":\"wifi_scan\""));
    }

    #[test]
    fn repair_action_and_report_roundtrip() {
        let r = RepairReport {
            action: RepairAction::FlushDns,
            outcome: "applied".into(),
            detail: "ipconfig /flushdns succeeded".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"action\":\"flush_dns\""));
        let back: RepairReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.action, RepairAction::FlushDns);
    }

    #[test]
    fn wifi_and_route_types_roundtrip() {
        let ap = WifiAp {
            ssid: Some("HomeNet".into()),
            bssid: "aa:bb:cc:dd:ee:ff".into(),
            channel: 6,
            frequency_mhz: 2437,
            band: "2.4".into(),
            signal_pct: 82,
            security: Some("WPA2".into()),
        };
        let json = serde_json::to_string(&ap).unwrap();
        assert!(json.contains("\"frequencyMhz\""));
        let e = RouteEntry {
            destination: "0.0.0.0/0".into(),
            gateway: Some("192.168.1.1".into()),
            interface: Some("eth0".into()),
            metric: Some(100),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: RouteEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.destination, "0.0.0.0/0");
        assert_eq!(back.metric, Some(100));
    }
}
