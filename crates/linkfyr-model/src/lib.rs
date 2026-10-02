//! LinkFYR shared data model.
//!
//! Every type in this crate is serializable and forms the wire contract
//! between the engine, the GUI, the CLI, and future remote clients.
//! Types here must stay dependency-light and backward-compatible:
//! additive fields only (see docs/adr, api-and-interface-design rules).

use serde::{Deserialize, Serialize};

pub mod optimize;

/// Stable engine version reported in snapshots for client compatibility checks.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Coarse interface classification used for icons, colors, and policy defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IfKind {
    Ethernet,
    Wifi,
    Cellular,
    Virtual,
    Tunnel,
    Loopback,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IfStatus {
    Up,
    Down,
    Dormant,
    Unknown,
}

/// A discovered network interface. Identity (`id`) is stable across polls
/// for the same adapter (OS index/name based).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub id: String,
    pub name: String,
    pub friendly_name: String,
    pub kind: IfKind,
    pub status: IfStatus,
    pub mac: Option<String>,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub gateway: Option<String>,
    pub mtu: Option<u32>,
    /// Negotiated link speed in bits/s, when the OS reports it.
    pub speed_bps: Option<u64>,
    /// User/OS flagged as metered (data-capped or costly).
    pub metered: bool,
}

/// Monotonic byte/packet/error counters for one interface at one instant.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IfCounters {
    pub id: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub timestamp_ms: u64,
}

/// One latency probe attempt against a probe endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeSample {
    pub target: String,
    pub rtt_ms: Option<f64>,
    pub ok: bool,
    pub timestamp_ms: u64,
}

/// Aggregated Internet latency/loss statistics over a rolling window.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStats {
    pub rtt_avg_ms: Option<f64>,
    pub rtt_min_ms: Option<f64>,
    pub rtt_max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: Option<f64>,
    pub sample_count: u32,
}

/// Multi-factor health score. `factors` exposes every input so the UI can
/// explain the score (never a black box).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthScore {
    /// 0-100.
    pub overall: u8,
    pub status: HealthStatus,
    /// (factor name, contribution 0-100)
    pub factors: Vec<(String, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Poor,
    Down,
}

/// Per-interface telemetry derived for one engine tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceTelemetry {
    pub interface: Interface,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub health: Option<HealthScore>,
    pub errors: IfCounters,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub rx_bps: f64,
    pub tx_bps: f64,
}

/// The full engine snapshot pushed to clients on every tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub timestamp_ms: u64,
    pub engine_version: String,
    pub interfaces: Vec<InterfaceTelemetry>,
    pub totals: Totals,
    pub internet: ProbeStats,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            timestamp_ms: now_ms(),
            engine_version: ENGINE_VERSION.to_string(),
            interfaces: Vec::new(),
            totals: Totals::default(),
            internet: ProbeStats::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_json_roundtrip_uses_camel_case() {
        let mut snap = Snapshot::default();
        snap.interfaces.push(InterfaceTelemetry {
            interface: Interface {
                id: "eth0".into(),
                name: "eth0".into(),
                friendly_name: "Ethernet".into(),
                kind: IfKind::Ethernet,
                status: IfStatus::Up,
                mac: Some("aa:bb:cc:dd:ee:ff".into()),
                ipv4: vec!["192.168.1.10/24".into()],
                ipv6: vec![],
                gateway: Some("192.168.1.1".into()),
                mtu: Some(1500),
                speed_bps: Some(1_000_000_000),
                metered: false,
            },
            rx_bps: 1_000_000.0,
            tx_bps: 512_000.0,
            health: Some(HealthScore {
                overall: 94,
                status: HealthStatus::Healthy,
                factors: vec![("loss".into(), 99)],
            }),
            errors: IfCounters::default(),
        });

        let json = serde_json::to_string(&snap).expect("serialize");
        assert!(
            json.contains("\"friendlyName\""),
            "wire format is camelCase: {json}"
        );
        assert!(
            json.contains("\"rxBps\""),
            "wire format is camelCase: {json}"
        );

        let back: Snapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.interfaces.len(), 1);
        assert_eq!(back.interfaces[0].interface.kind, IfKind::Ethernet);
        assert_eq!(
            back.interfaces[0].health.as_ref().expect("health").overall,
            94
        );
    }

    #[test]
    fn enums_serialize_as_snake_case() {
        assert_eq!(serde_json::to_string(&IfKind::Wifi).unwrap(), r#""wifi""#);
        assert_eq!(
            serde_json::to_string(&HealthStatus::Degraded).unwrap(),
            r#""degraded""#
        );
    }
}
