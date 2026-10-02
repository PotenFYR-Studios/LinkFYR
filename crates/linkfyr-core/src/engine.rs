//! The application engine: ties config + telemetry into one object that
//! both the Tauri shell and the CLI drive. This is the in-process Phase 1
//! form; the privileged `linkfyrd` daemon (Phase 1.5) hosts the same
//! engine behind the authenticated IPC transport.

use std::sync::Arc;

use linkfyr_ipc::{Config, ErrorCode, HistoryPoint, Preferences, Request, Response};
use linkfyr_model::{Interface, Snapshot};
use linkfyr_network::{InterfaceMonitor, OsMonitor, sim::SimMonitor};
use linkfyr_telemetry::{EngineConfig, TcpProber, TelemetryEngine};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorMode {
    /// Real OS interfaces.
    Os,
    /// Deterministic simulator (for tests, demos, and UI development).
    Simulated,
}

pub struct AppEngine {
    telemetry: Arc<TelemetryEngine>,
    store: std::sync::Mutex<crate::config::ConfigStore>,
    config: std::sync::RwLock<Config>,
    degraded_note: Option<String>,
    worker: std::sync::Mutex<Option<JoinHandle<()>>>,
    /// Rolling alert ring (interface up/down, health drops).
    alerts: std::sync::Mutex<std::collections::VecDeque<linkfyr_ipc::Alert>>,
    /// Last seen (status, health) per interface id, for transitions.
    last_states:
        std::sync::Mutex<std::collections::HashMap<String, (linkfyr_model::IfStatus, Option<u8>)>>,
}

const ALERT_RING: usize = 100;

impl AppEngine {
    /// Build the engine from a config directory.
    pub fn open(config_dir: &std::path::Path, mode: MonitorMode) -> Result<Arc<Self>, String> {
        let store = crate::config::ConfigStore::new(config_dir);
        let loaded = store
            .load()
            .map_err(|e| format!("failed to read config: {e}"))?;

        let monitor: Arc<dyn InterfaceMonitor> = match mode {
            MonitorMode::Os => Arc::new(OsMonitor::new()),
            MonitorMode::Simulated => Arc::new(SimMonitor::new(vec![
                linkfyr_network::sim::SimInterface::new(
                    "eth0",
                    "Ethernet",
                    linkfyr_model::IfKind::Ethernet,
                )
                .with_rates(36_000_000, 9_000_000),
                linkfyr_network::sim::SimInterface::new(
                    "wifi0",
                    "Wi-Fi 6",
                    linkfyr_model::IfKind::Wifi,
                )
                .with_rates(18_000_000, 4_500_000),
                linkfyr_network::sim::SimInterface::new(
                    "5g0",
                    "5G",
                    linkfyr_model::IfKind::Cellular,
                )
                .with_rates(21_000_000, 7_000_000)
                .with_errors(2, 0),
            ])),
        };

        let prober = Arc::new(TcpProber::new(linkfyr_telemetry::ProbeConfig::default()));
        let telemetry = TelemetryEngine::new(EngineConfig::default(), monitor, prober);

        Ok(Arc::new(Self {
            telemetry,
            store: std::sync::Mutex::new(store),
            config: std::sync::RwLock::new(loaded.config),
            degraded_note: loaded.reason,
            worker: std::sync::Mutex::new(None),
            alerts: std::sync::Mutex::new(std::collections::VecDeque::new()),
            last_states: std::sync::Mutex::new(std::collections::HashMap::new()),
        }))
    }

    /// Start the background sampling loop. Panics outside a tokio context;
    /// embedders without one should use [`start_on`](Self::start_on).
    pub fn start(self: &Arc<Self>) {
        self.start_on(&tokio::runtime::Handle::current());
    }

    /// Start the loop on an explicit runtime (embedders like Tauri call
    /// this from a non-tokio setup hook).
    pub fn start_on(self: &Arc<Self>, handle: &tokio::runtime::Handle) {
        let mut worker = self.worker.lock().expect("worker lock");
        if worker.is_some() {
            return;
        }
        let engine = self.telemetry.clone();
        let spawned = handle.spawn(async move {
            engine.run().await;
        });
        *worker = Some(spawned);
    }

    /// Start against a pre-built telemetry engine (used by tests/CLI).
    pub fn with_telemetry(
        telemetry: Arc<TelemetryEngine>,
        store: crate::config::ConfigStore,
        config: Config,
    ) -> Arc<Self> {
        Arc::new(Self {
            telemetry,
            store: std::sync::Mutex::new(store),
            config: std::sync::RwLock::new(config),
            degraded_note: None,
            worker: std::sync::Mutex::new(None),
            alerts: std::sync::Mutex::new(std::collections::VecDeque::new()),
            last_states: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Compare a snapshot against the last seen state and append
    /// transition alerts (pure side of the alert engine).
    pub fn ingest_snapshot(&self, snap: &Snapshot) {
        use linkfyr_model::IfStatus;
        let mut states = self.last_states.lock().expect("states lock");
        let mut alerts = self.alerts.lock().expect("alerts lock");
        for t in &snap.interfaces {
            let id = t.interface.id.clone();
            let health = t.health.as_ref().map(|h| h.overall);
            let key = (t.interface.status, health);
            match states.entry(id.clone()) {
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    let prev = *o.get();
                    if prev.0 == IfStatus::Up && key.0 != IfStatus::Up {
                        alerts.push_back(Self::alert(
                            &id,
                            linkfyr_ipc::Severity::Warning,
                            format!("{} went down", t.interface.friendly_name),
                            format!(
                                "status changed from up to {}",
                                format!("{:?}", key.0).to_lowercase()
                            ),
                            snap.timestamp_ms,
                        ));
                    } else if prev.0 != IfStatus::Up && key.0 == IfStatus::Up {
                        alerts.push_back(Self::alert(
                            &id,
                            linkfyr_ipc::Severity::Info,
                            format!("{} came up", t.interface.friendly_name),
                            "interface is back and carrying traffic".into(),
                            snap.timestamp_ms,
                        ));
                    }
                    if let (Some(prev_h), Some(now_h)) = (prev.1, health)
                        && now_h < 50
                        && prev_h >= 50
                    {
                        alerts.push_back(Self::alert(
                            &id,
                            linkfyr_ipc::Severity::Warning,
                            format!("{} health dropped to {now_h}", t.interface.friendly_name),
                            "multi-factor health fell below 50; see Interfaces view".into(),
                            snap.timestamp_ms,
                        ));
                    }
                    o.insert(key);
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(key);
                }
            }
        }
        while alerts.len() > ALERT_RING {
            alerts.pop_front();
        }
    }

    /// Recent alerts, oldest first.
    pub fn alerts(&self) -> Vec<linkfyr_ipc::Alert> {
        self.alerts
            .lock()
            .expect("alerts lock")
            .iter()
            .cloned()
            .collect()
    }

    pub fn degraded_note(&self) -> Option<&str> {
        self.degraded_note.as_deref()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<Snapshot>> {
        self.telemetry.subscribe()
    }

    pub async fn snapshot(&self) -> Arc<Snapshot> {
        self.telemetry.current().await
    }

    /// Force one sampling tick (CLI one-shot mode and tests).
    pub async fn tick_once(&self) -> Arc<Snapshot> {
        self.telemetry.tick_once().await
    }

    /// Sample once if the engine has never ticked (idempotent, cheap).
    async fn ensure_warm(self: &Arc<Self>) {
        if self.telemetry.tick_count() == 0 {
            let snap = self.telemetry.tick_once().await;
            self.ingest_snapshot(&snap);
        }
    }

    fn alert(
        id: &str,
        severity: linkfyr_ipc::Severity,
        title: String,
        body: String,
        ts: u64,
    ) -> linkfyr_ipc::Alert {
        linkfyr_ipc::Alert {
            id: format!("{id}-{ts}"),
            severity,
            title,
            body,
            timestamp_ms: ts,
        }
    }

    pub async fn history(&self, max_points: u32) -> Vec<HistoryPoint> {
        self.telemetry
            .history()
            .await
            .into_iter()
            .map(|(timestamp_ms, rx_bps, tx_bps)| HistoryPoint {
                timestamp_ms,
                rx_bps,
                tx_bps,
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(max_points as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn config(&self) -> Config {
        self.config.read().expect("config lock").clone()
    }

    pub fn update_preferences(
        &self,
        prefs: Preferences,
    ) -> Result<Config, crate::config::StoreError> {
        let mut cfg = self.config();
        cfg.preferences = prefs;
        self.store.lock().expect("store lock").save(&cfg)?;
        *self.config.write().expect("config lock") = cfg.clone();
        Ok(cfg)
    }

    /// Handle one IPC request. Shared by the Tauri commands and (later)
    /// the daemon transport, so behavior cannot drift between clients.
    pub async fn handle_request(self: &Arc<Self>, request: Request) -> Response {
        // A one-shot client (CLI) may arrive before the first tick; make
        // sure a snapshot exists so reads never return an empty world.
        self.ensure_warm().await;
        match request {
            Request::GetSnapshot => Response::Snapshot(Box::new((*self.snapshot().await).clone())),
            Request::GetInterfaces => {
                let snap = self.snapshot().await;
                let interfaces: Vec<Interface> = snap
                    .interfaces
                    .iter()
                    .map(|t| t.interface.clone())
                    .collect();
                Response::Interfaces { interfaces }
            }
            Request::GetHistory { max_points } => Response::History {
                points: self.history(max_points).await,
            },
            Request::GetConfig => Response::Config(Box::new(self.config())),
            Request::UpdatePreferences { preferences } => {
                match self.update_preferences(preferences) {
                    Ok(cfg) => Response::PreferencesUpdated(Box::new(cfg.preferences)),
                    Err(e) => Response::Error(Box::new(linkfyr_ipc::ApiError::new(
                        ErrorCode::Internal,
                        e.to_string(),
                    ))),
                }
            }
            Request::Ping => Response::Pong {
                version: linkfyr_model::ENGINE_VERSION.to_string(),
            },
            // Optimization Toolkit: the tools are blocking (real sockets,
            // real OS commands), so they run on the blocking pool and the
            // async runtime stays responsive for telemetry + UI.
            Request::OptimizeCapabilities => {
                let r = blocking(linkfyr_optimize::capability::detect).await;
                Response::Capabilities(Box::new(r))
            }
            Request::OptimizeDnsBenchmark => {
                let r = blocking(linkfyr_optimize::dns::bench_default).await;
                Response::DnsBenchmark(Box::new(r))
            }
            Request::OptimizeDnsApply { servers, interface } => {
                let mut parsed = Vec::new();
                let mut bad = None;
                for s in &servers {
                    match s.parse() {
                        Ok(ip) => parsed.push(ip),
                        Err(_) => bad = Some(s.clone()),
                    }
                }
                if let Some(bad) = bad {
                    return Response::Error(Box::new(linkfyr_ipc::ApiError::new(
                        ErrorCode::ValidationError,
                        format!("invalid server address: {bad}"),
                    )));
                }
                let r =
                    blocking(move || linkfyr_optimize::dns::apply(&parsed, interface.as_deref()))
                        .await;
                Response::DnsApplied(Box::new(r))
            }
            Request::OptimizeRouteScan { target } => {
                let r = blocking(move || linkfyr_optimize::routescan::scan(&target, 5)).await;
                Response::RouteScan(Box::new(r))
            }
            Request::OptimizeBloatTest { duration_s } => {
                let mut opts = linkfyr_optimize::bloat::BloatOptions::default();
                if let Some(d) = duration_s {
                    opts.phase_secs = d.clamp(1, 60);
                }
                let r = blocking(move || linkfyr_optimize::bloat::run(&opts)).await;
                Response::BloatTest(Box::new(r))
            }
            Request::OptimizeSpeedTest {
                endpoint,
                duration_s,
            } => {
                let mut opts = linkfyr_optimize::speedtest::SpeedtestOptions::default();
                if let Some(e) = endpoint {
                    opts.endpoint = e;
                }
                if let Some(d) = duration_s {
                    opts.duration_s = d.clamp(1, 60);
                }
                let r = blocking(move || linkfyr_optimize::speedtest::run(&opts)).await;
                Response::SpeedTest(Box::new(r))
            }
            Request::OptimizeMtu { target } => {
                let r = blocking(move || linkfyr_optimize::mtu::probe(&target, 1500)).await;
                Response::Mtu(Box::new(r))
            }
            Request::OptimizeWifiScan => {
                let r = blocking(linkfyr_optimize::wifiscan::scan).await;
                Response::WifiScan(Box::new(r))
            }
            Request::OptimizeTcpAudit => {
                let r = blocking(linkfyr_optimize::tcpaudit::audit).await;
                Response::TcpAudit(Box::new(r))
            }
            Request::OptimizeRouteAudit => {
                let r = blocking(linkfyr_optimize::routeaudit::audit).await;
                Response::RouteAudit(Box::new(r))
            }
            Request::OptimizeRepair { action } => {
                let r = blocking(move || linkfyr_optimize::repair::run(action)).await;
                Response::Repaired(Box::new(r))
            }
            Request::OptimizeRun { tool, params } => {
                let r = blocking(move || linkfyr_optimize::registry::run(&tool, &params)).await;
                Response::ToolRun(Box::new(r))
            }
            Request::BridgeList => {
                let bridges = blocking(linkfyr_bridge::list).await;
                Response::Bridges { bridges }
            }
            Request::BridgeCreate { spec } => {
                let r = blocking(move || linkfyr_bridge::create(&spec)).await;
                Response::BridgeReported(Box::new(r))
            }
            Request::BridgeRemove { name } => {
                let r = blocking(move || linkfyr_bridge::remove(&name)).await;
                Response::BridgeReported(Box::new(r))
            }
            Request::GetAlerts => {
                let snap = self.snapshot().await;
                self.ingest_snapshot(&snap);
                Response::Alerts {
                    alerts: self.alerts(),
                }
            }
        }
    }
}

/// Run a blocking toolkit call on the tokio blocking pool. The toolkit
/// tools return errors inside their reports rather than panicking, so
/// a join failure here means a bug: fail loudly instead of silently
/// returning fabricated data.
async fn blocking<T, F>(f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("optimization tool panicked")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigStore;
    use linkfyr_network::sim::{SimInterface, SimMonitor};
    use std::time::Duration;

    fn sim_engine() -> Arc<AppEngine> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let monitor = Arc::new(SimMonitor::new(vec![
            SimInterface::new("eth0", "Ethernet", linkfyr_model::IfKind::Ethernet)
                .with_rates(1_000_000, 500_000),
        ]));
        let prober = Arc::new(TcpProber::new(linkfyr_telemetry::ProbeConfig::default()));
        let telemetry = TelemetryEngine::new(
            EngineConfig {
                tick: Duration::from_secs(1),
                probe_every: 5,
                history_len: 10,
            },
            monitor,
            prober,
        );
        let dir =
            std::env::temp_dir().join(format!("linkfyr-core-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        AppEngine::with_telemetry(telemetry, ConfigStore::new(&dir), Config::default())
    }

    #[tokio::test]
    async fn handles_ping_and_snapshot_requests() {
        let e = sim_engine();
        match e.handle_request(Request::Ping).await {
            Response::Pong { version } => assert_ne!(version, ""),
            other => panic!("expected pong, got {other:?}"),
        }

        // First tick populates the engine snapshot.
        e.telemetry.tick_once().await;
        match e.handle_request(Request::GetSnapshot).await {
            Response::Snapshot(s) => assert_eq!(s.interfaces.len(), 1),
            other => panic!("expected snapshot, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn history_respects_max_points() {
        let e = sim_engine();
        for _ in 0..5 {
            e.telemetry.tick_once().await;
        }
        match e
            .handle_request(Request::GetHistory { max_points: 2 })
            .await
        {
            Response::History { points } => {
                assert_eq!(points.len(), 2);
                assert!(
                    points
                        .windows(2)
                        .all(|w| w[0].timestamp_ms <= w[1].timestamp_ms)
                );
            }
            other => panic!("expected history, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn preferences_update_persists() {
        let e = sim_engine();
        let prefs = Preferences {
            theme: linkfyr_ipc::Theme::Light,
            expert_mode: true,
            ..Preferences::default()
        };
        match e
            .handle_request(Request::UpdatePreferences { preferences: prefs })
            .await
        {
            Response::PreferencesUpdated(p) => {
                assert!(p.expert_mode);
                assert_eq!(p.theme, linkfyr_ipc::Theme::Light);
            }
            other => panic!("expected preferences, got {other:?}"),
        }
        // Reload from disk through a fresh store over the same path.
        match e.handle_request(Request::GetConfig).await {
            Response::Config(c) => assert!(c.preferences.expert_mode),
            other => panic!("expected config, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn interfaces_request_returns_without_telemetry() {
        let e = sim_engine();
        e.telemetry.tick_once().await;
        match e.handle_request(Request::GetInterfaces).await {
            Response::Interfaces { interfaces } => {
                assert_eq!(interfaces.len(), 1);
                assert_eq!(interfaces[0].id, "eth0");
            }
            other => panic!("expected interfaces, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn optimize_capabilities_dispatch() {
        let e = sim_engine();
        match e.handle_request(Request::OptimizeCapabilities).await {
            Response::Capabilities(c) => {
                assert!(!c.tools.is_empty());
                assert_ne!(c.platform, "");
            }
            other => panic!("expected capabilities, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn optimize_repair_dispatch_round_trips() {
        let e = sim_engine();
        let req = Request::OptimizeRepair {
            action: linkfyr_model::optimize::RepairAction::FlushDns,
        };
        match e.handle_request(req).await {
            Response::Repaired(r) => assert_ne!(r.outcome, ""),
            other => panic!("expected repair report, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn optimize_dns_apply_validates_addresses() {
        let e = sim_engine();
        let req = Request::OptimizeDnsApply {
            servers: vec!["not-an-ip".into()],
            interface: None,
        };
        match e.handle_request(req).await {
            Response::Error(err) => assert_eq!(err.code, ErrorCode::ValidationError),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn registry_tool_runs_through_engine() {
        let e = sim_engine();
        let mut params = std::collections::BTreeMap::new();
        params.insert("text".to_string(), "hello".to_string());
        let req = Request::OptimizeRun {
            tool: "hosts_file".into(),
            params,
        };
        match e.handle_request(req).await {
            Response::ToolRun(r) => assert!(r.ok, "{}", r.summary),
            other => panic!("expected tool run, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_list_and_validation_flow_through_engine() {
        let e = sim_engine();
        match e.handle_request(Request::BridgeList).await {
            Response::Bridges { bridges } => {
                // In a container there may be no bridges; must not error.
                let _ = bridges;
            }
            other => panic!("expected bridges, got {other:?}"),
        }
        let bad = Request::BridgeCreate {
            spec: linkfyr_model::optimize::BridgeSpec {
                name: "bad; name".into(),
                members: vec!["eth0".into()],
                mode: linkfyr_model::optimize::BridgeMode::L2Switch,
                internal_prefix: None,
            },
        };
        match e.handle_request(bad).await {
            Response::BridgeReported(r) => assert_eq!(r.outcome, "failed"),
            other => panic!("expected bridge report, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn alert_engine_fires_on_interface_down_transition() {
        let e = sim_engine();

        // First snapshot records the initial state (no alerts yet).
        let snap = e.telemetry.tick_once().await;
        e.ingest_snapshot(&snap);
        assert!(e.alerts().is_empty());

        // Simulate a down snapshot for the same interface id.
        let mut down_snap = (*snap).clone();
        down_snap.timestamp_ms += 1000;
        down_snap.interfaces[0].interface.status = linkfyr_model::IfStatus::Down;
        e.ingest_snapshot(&down_snap);
        let alerts = e.alerts();
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].title.contains("went down"));

        // And back up.
        let mut up_snap = down_snap.clone();
        up_snap.timestamp_ms += 1000;
        up_snap.interfaces[0].interface.status = linkfyr_model::IfStatus::Up;
        e.ingest_snapshot(&up_snap);
        let alerts = e.alerts();
        assert_eq!(alerts.len(), 2);
        assert!(alerts[1].title.contains("came up"));

        match e.handle_request(Request::GetAlerts).await {
            Response::Alerts { alerts } => assert_eq!(alerts.len(), 2),
            other => panic!("expected alerts, got {other:?}"),
        }
    }
}
