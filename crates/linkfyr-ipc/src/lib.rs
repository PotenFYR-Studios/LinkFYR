//! LinkFYR stable IPC API.
//!
//! This crate is the contract between clients (GUI, CLI, remote) and the
//! engine. Rules that keep it stable:
//! - Every message carries `v` (API version). Unknown payload variants are
//!   rejected with `UNSUPPORTED`, never guessed.
//! - Errors use one shape everywhere: `{ code, message }` with
//!   machine-readable codes.
//! - Additive evolution only: new variants may be added, existing ones
//!   never change meaning. See docs/adr + api-design rules.

use linkfyr_model::optimize::{
    BloatReport, BridgeInfo, BridgeReport, BridgeSpec, CapabilitiesReport, DnsApplyReport,
    DnsBenchmarkReport, MtuReport, RepairAction, RepairReport, RouteAuditReport, RouteScanReport,
    SpeedtestReport, TcpAuditReport, ToolRunReport, WifiAnalysis,
};
use linkfyr_model::{Interface, ProbeStats, Snapshot};
use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<T> {
    pub v: u32,
    pub id: u64,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(id: u64, payload: T) -> Self {
        Self {
            v: API_VERSION,
            id,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ValidationError,
    NotFound,
    EngineUnavailable,
    Unsupported,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Client → engine requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Request {
    /// Latest engine snapshot.
    GetSnapshot,
    /// Interface list without telemetry.
    GetInterfaces,
    /// Recent 1 s-resolution history, capped by `max_points`.
    GetHistory { max_points: u32 },
    /// Current configuration document.
    GetConfig,
    /// Replace mutable preferences (theme, expert mode, …). Whole-document.
    UpdatePreferences { preferences: Preferences },
    /// Liveness + version handshake.
    Ping,
    /// Which optimization tools can run on this machine right now.
    OptimizeCapabilities,
    /// Benchmark DNS resolvers (real UDP queries).
    OptimizeDnsBenchmark,
    /// Apply a resolver list to the OS (needs elevation on all
    /// platforms; previous values are captured for restore).
    OptimizeDnsApply {
        servers: Vec<String>,
        interface: Option<String>,
    },
    /// Multi-path latency scan of one target (IPv4/IPv6/controls).
    OptimizeRouteScan { target: String },
    /// Latency-under-load (bufferbloat) measurement.
    OptimizeBloatTest { duration_s: Option<u32> },
    /// Real throughput test against a compatible endpoint.
    OptimizeSpeedTest {
        endpoint: Option<String>,
        duration_s: Option<u32>,
    },
    /// Path MTU discovery with DF pings.
    OptimizeMtu { target: String },
    /// Wi-Fi environment scan + channel recommendation.
    OptimizeWifiScan,
    /// TCP stack settings audit.
    OptimizeTcpAudit,
    /// Routing table audit + anomaly detection.
    OptimizeRouteAudit,
    /// Run a one-shot repair action (e.g. DNS cache flush).
    OptimizeRepair { action: RepairAction },
    /// Run any registry tool by id (the extensible path to 100+).
    OptimizeRun {
        tool: String,
        /// String parameters (target, port, url, domain, …).
        params: std::collections::BTreeMap<String, String>,
    },
    /// List network bridges (Hyper-V switches/NetNat, ip link, ifconfig).
    BridgeList,
    /// Create a bridge from a validated spec.
    BridgeCreate { spec: BridgeSpec },
    /// Remove a bridge by name.
    BridgeRemove { name: String },
    /// Recent engine alerts (interface up/down, health drops).
    GetAlerts,
}

/// Engine → client responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Response {
    Snapshot(Box<Snapshot>),
    Interfaces { interfaces: Vec<Interface> },
    History { points: Vec<HistoryPoint> },
    Config(Box<Config>),
    PreferencesUpdated(Box<Preferences>),
    Pong { version: String },
    Capabilities(Box<CapabilitiesReport>),
    DnsBenchmark(Box<DnsBenchmarkReport>),
    DnsApplied(Box<DnsApplyReport>),
    RouteScan(Box<RouteScanReport>),
    BloatTest(Box<BloatReport>),
    SpeedTest(Box<SpeedtestReport>),
    Mtu(Box<MtuReport>),
    WifiScan(Box<WifiAnalysis>),
    TcpAudit(Box<TcpAuditReport>),
    RouteAudit(Box<RouteAuditReport>),
    Repaired(Box<RepairReport>),
    ToolRun(Box<ToolRunReport>),
    Bridges { bridges: Vec<BridgeInfo> },
    BridgeReported(Box<BridgeReport>),
    Alerts { alerts: Vec<Alert> },
    Error(Box<ApiError>),
}

/// Engine → client async events (subscribe once, receive many).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Event {
    SnapshotUpdated(Box<Snapshot>),
    InternetChanged(Box<ProbeStats>),
    Alert(Box<Alert>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// One point of the 1 s-resolution history series.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub timestamp_ms: u64,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

/// User preferences: the mutable, non-policy part of configuration.
/// Policy (rules, profiles) has its own documents in later phases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub theme: Theme,
    pub expert_mode: bool,
    pub animation: AnimationLevel,
    /// Local-only is the default and the promise; the flag exists so a
    /// future opt-in feature cannot silently flip behavior.
    pub local_only: bool,
    /// Keep the engine running in the tray when the window closes.
    /// False = closing the window quits the app. The `linkfyrd` service
    /// (if installed) is unaffected either way.
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            expert_mode: false,
            animation: AnimationLevel::Full,
            local_only: true,
            close_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationLevel {
    Full,
    Reduced,
    Off,
}

/// The full configuration document persisted by the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub schema_version: u32,
    pub preferences: Preferences,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            preferences: Preferences::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkfyr_model::IfKind;

    #[test]
    fn envelope_carries_version() {
        let e = Envelope::new(7, Request::Ping);
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"v\":1"));
        assert!(json.contains("\"id\":7"));
    }

    #[test]
    fn request_response_roundtrip() {
        let req = Envelope::new(1, Request::GetHistory { max_points: 60 });
        let json = serde_json::to_string(&req).unwrap();
        let back: Envelope<Request> = serde_json::from_str(&json).unwrap();
        match back.payload {
            Request::GetHistory { max_points } => assert_eq!(max_points, 60),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn error_shape_is_consistent() {
        let err = ApiError::new(ErrorCode::NotFound, "no such profile");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"not_found\""));
        let back: ApiError = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, ErrorCode::NotFound);
    }

    #[test]
    fn preferences_defaults_are_local_only_and_dark() {
        let p = Preferences::default();
        assert_eq!(p.theme, Theme::Dark);
        assert!(p.local_only);
        assert!(p.close_to_tray);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"theme\":\"dark\""));
        assert!(json.contains("\"localOnly\":true"));
        assert!(json.contains("\"closeToTray\":true"));
        // Old payloads without the field still parse (additive wire).
        let back: Preferences = serde_json::from_str(
            r#"{"theme":"dark","expertMode":false,"animation":"full","localOnly":true}"#,
        )
        .expect("missing closeToTray must default");
        assert!(back.close_to_tray);
    }

    #[test]
    fn config_roundtrip() {
        let c = Config::default();
        let json = serde_json::to_string_pretty(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn request_fields_are_camel_case_on_the_wire() {
        // Regression: the TS client sends camelCase fields; serde used to
        // expect snake_case here and silently rejected GetHistory.
        let json = r#"{"v":1,"id":3,"payload":{"type":"get_history","maxPoints":120}}"#;
        let env: Envelope<Request> = serde_json::from_str(json).expect("camelCase must parse");
        match env.payload {
            Request::GetHistory { max_points } => assert_eq!(max_points, 120),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn optimize_requests_are_camel_case_on_the_wire() {
        // Same regression class as request_fields_are_camel_case_on_the_wire:
        // new toolkit requests carry multi-word fields.
        let cases: Vec<(Envelope<Request>, Vec<&'static str>)> = vec![
            (
                Envelope::new(
                    1,
                    Request::OptimizeDnsApply {
                        servers: vec!["1.1.1.1".into()],
                        interface: Some("Wi-Fi".into()),
                    },
                ),
                vec!["\"servers\"", "\"interface\""],
            ),
            (
                Envelope::new(2, Request::OptimizeRouteScan { target: "x".into() }),
                vec!["\"target\""],
            ),
            (
                Envelope::new(
                    3,
                    Request::OptimizeSpeedTest {
                        endpoint: None,
                        duration_s: Some(5),
                    },
                ),
                vec!["\"endpoint\"", "\"durationS\""],
            ),
            (
                Envelope::new(4, Request::OptimizeBloatTest { duration_s: None }),
                vec!["\"durationS\""],
            ),
            (
                Envelope::new(5, Request::OptimizeMtu { target: "y".into() }),
                vec!["\"target\""],
            ),
            (
                Envelope::new(
                    6,
                    Request::OptimizeRepair {
                        action: RepairAction::FlushDns,
                    },
                ),
                vec!["\"action\""],
            ),
        ];
        for (env, needles) in cases {
            let json = serde_json::to_string(&env).unwrap();
            for needle in needles {
                assert!(json.contains(needle), "missing {needle} in {json}");
            }
            let back: Envelope<Request> = serde_json::from_str(&json).expect("roundtrip");
            assert_eq!(back.v, API_VERSION);
        }
    }

    #[test]
    fn optimize_response_variants_tag_correctly() {
        let r = Response::Repaired(Box::new(RepairReport {
            action: RepairAction::FlushDns,
            outcome: "applied".into(),
            detail: "ok".into(),
        }));
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"type\":\"repaired\""));
        assert!(json.contains("\"outcome\":\"applied\""));
        let back: Response = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Response::Repaired(_)));

        let c = Response::Capabilities(Box::new(CapabilitiesReport {
            elevated: false,
            platform: "linux".into(),
            tools: vec![],
        }));
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"type\":\"capabilities\""));
    }

    #[test]
    fn generic_tool_run_request_round_trips() {
        let mut params = std::collections::BTreeMap::new();
        params.insert("target".to_string(), "example.com".to_string());
        params.insert("samples".to_string(), "30".to_string());
        let req = Envelope::new(
            9,
            Request::OptimizeRun {
                tool: "latency_monitor".into(),
                params,
            },
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"tool\":\"latency_monitor\""));
        assert!(json.contains("\"target\":\"example.com\""));
        let back: Envelope<Request> = serde_json::from_str(&json).unwrap();
        match back.payload {
            Request::OptimizeRun { tool, params } => {
                assert_eq!(tool, "latency_monitor");
                assert_eq!(params.get("samples").map(String::as_str), Some("30"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn bridge_and_alert_variants_wire_correctly() {
        let spec = BridgeSpec {
            name: "br0".into(),
            members: vec!["eth0".into(), "eth1".into()],
            mode: linkfyr_model::optimize::BridgeMode::L2Switch,
            internal_prefix: None,
        };
        let req = Envelope::new(1, Request::BridgeCreate { spec });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"bridge_create\""));
        assert!(json.contains("\"internalPrefix\""));
        assert!(json.contains("\"l2_switch\""));

        let resp = Response::Bridges {
            bridges: vec![BridgeInfo {
                name: "br0".into(),
                mode: linkfyr_model::optimize::BridgeMode::NatShare,
                members: vec![],
                state: "active".into(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"bridges\""));
        assert!(json.contains("\"nat_share\""));

        let alerts = Response::Alerts {
            alerts: vec![Alert {
                id: "a1".into(),
                severity: Severity::Warning,
                title: "Wi-Fi down".into(),
                body: "interface went down".into(),
                timestamp_ms: 1,
            }],
        };
        let json = serde_json::to_string(&alerts).unwrap();
        assert!(json.contains("\"type\":\"alerts\""));
    }

    #[test]
    fn event_variants_serialize() {
        let snap = Snapshot::default();
        let e = Event::SnapshotUpdated(Box::new(snap));
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"type\":\"snapshot_updated\""));

        let ifc = Interface {
            id: "eth0".into(),
            name: "eth0".into(),
            friendly_name: "Ethernet".into(),
            kind: IfKind::Ethernet,
            status: linkfyr_model::IfStatus::Up,
            mac: None,
            ipv4: vec![],
            ipv6: vec![],
            gateway: None,
            mtu: None,
            speed_bps: None,
            metered: false,
        };
        let r = Response::Interfaces {
            interfaces: vec![ifc],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"type\":\"interfaces\""));
        assert!(json.contains("\"interfaces\""));
    }
}
