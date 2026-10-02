//! The live telemetry engine: polls the monitor, derives per-interface
//! rates + health, aggregates probe stats, and publishes snapshots.
//!
//! Design notes:
//! - Rates come from timestamp-differenced counter totals (robust to
//!   irregular tick spacing), never from assumed fixed periods.
//! - Snapshots are published through a `tokio::sync::watch` channel:
//!   latest state only, natural coalescing, no unbounded queues.
//! - A bounded ring keeps the recent 1 s-resolution series for charts.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use linkfyr_model::{IfCounters, InterfaceTelemetry, ProbeSample, ProbeStats, Snapshot};
use linkfyr_network::InterfaceMonitor;
use tokio::sync::{RwLock, watch};

use crate::health::{HealthInputs, errors_per_s, score_interface};
use crate::probe::{TcpProber, aggregate};
use crate::ring::RingBuffer;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Telemetry tick interval.
    pub tick: Duration,
    /// Probe every N ticks (probing is slower than sampling).
    pub probe_every: u32,
    /// Ring length for the 1 s chart series.
    pub history_len: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(1),
            probe_every: 5,
            history_len: 300,
        }
    }
}

#[derive(Debug, Clone)]
struct PrevCounters {
    counters: IfCounters,
}

pub struct TelemetryEngine {
    config: EngineConfig,
    monitor: Arc<dyn InterfaceMonitor>,
    prober: Arc<TcpProber>,
    prev: RwLock<HashMap<String, PrevCounters>>,
    history: RwLock<RingBuffer<(u64, f64, f64)>>,
    probe_window: RwLock<Vec<ProbeSample>>,
    tx: watch::Sender<Arc<Snapshot>>,
    /// Held so the watch channel never has zero receivers (a `send()` to
    /// a receiver-less channel does not update the observable value).
    _rx: watch::Receiver<Arc<Snapshot>>,
    ticks: AtomicU64,
}

impl TelemetryEngine {
    pub fn new(
        config: EngineConfig,
        monitor: Arc<dyn InterfaceMonitor>,
        prober: Arc<TcpProber>,
    ) -> Arc<Self> {
        let (tx, rx) = watch::channel(Arc::new(Snapshot::default()));
        let history_len = config.history_len.max(1);
        Arc::new(Self {
            config,
            monitor,
            prober,
            prev: RwLock::new(HashMap::new()),
            history: RwLock::new(RingBuffer::new(
                NonZeroUsize::new(history_len).expect("nonzero"),
            )),
            probe_window: RwLock::new(Vec::new()),
            tx,
            _rx: rx,
            ticks: AtomicU64::new(0),
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.tx.subscribe()
    }

    pub async fn current(&self) -> Arc<Snapshot> {
        self.tx.borrow().clone()
    }
    pub async fn history(&self) -> Vec<(u64, f64, f64)> {
        self.history.read().await.iter().copied().collect()
    }

    fn tick_index(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }

    /// How many ticks have run (0 = no data yet).
    pub fn tick_count(&self) -> u64 {
        self.tick_index()
    }

    /// Run one sampling tick. Public for deterministic tests.
    pub async fn tick_once(&self) -> Arc<Snapshot> {
        self.ticks.fetch_add(1, Ordering::Relaxed);
        let interfaces = match self.monitor.snapshot() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("monitor snapshot failed: {e}");
                return self.current().await;
            }
        };
        let counters_list = match self.monitor.counters() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("monitor counters failed: {e}");
                return self.current().await;
            }
        };
        let by_id: HashMap<String, IfCounters> = counters_list
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();

        // Probe cadence: first tick plus every probe_every ticks.
        let tick_idx = self.tick_index();
        let mut probe_window = self.probe_window.write().await;
        let every = u64::from(self.config.probe_every);
        if tick_idx == 1 || tick_idx.is_multiple_of(every) {
            let samples = self.prober.probe_once().await;
            probe_window.extend(samples);
            let max = self.config.probe_every as usize * 4;
            if probe_window.len() > max {
                let overflow = probe_window.len() - max;
                probe_window.drain(0..overflow);
            }
        }
        let internet: ProbeStats = aggregate(&probe_window);
        let loss_fraction = internet.loss_pct.unwrap_or(0.0) / 100.0;
        drop(probe_window);

        let mut prev_map = self.prev.write().await;
        let mut interfaces_out: Vec<InterfaceTelemetry> = Vec::with_capacity(interfaces.len());
        let mut total_rx = 0.0_f64;
        let mut total_tx = 0.0_f64;

        for interface in interfaces {
            let Some(curr) = by_id.get(&interface.id) else {
                continue;
            };
            let (rx_bps, tx_bps, health) = match prev_map.get(&interface.id) {
                Some(prev) => {
                    let dt_s = (curr.timestamp_ms.saturating_sub(prev.counters.timestamp_ms))
                        as f64
                        / 1000.0;
                    let (rx_bps, tx_bps) = if dt_s > 0.0 {
                        (
                            curr.rx_bytes.saturating_sub(prev.counters.rx_bytes) as f64 * 8.0
                                / dt_s,
                            curr.tx_bytes.saturating_sub(prev.counters.tx_bytes) as f64 * 8.0
                                / dt_s,
                        )
                    } else {
                        (0.0, 0.0)
                    };
                    let (rx_eps, tx_eps) = errors_per_s(&prev.counters, curr);
                    let health = score_interface(&HealthInputs {
                        status: interface.status,
                        rx_errors_per_s: rx_eps,
                        tx_errors_per_s: tx_eps,
                        probe_loss_fraction: loss_fraction,
                    });
                    (rx_bps, tx_bps, Some(health))
                }
                None => (0.0, 0.0, None),
            };
            prev_map.insert(
                interface.id.clone(),
                PrevCounters {
                    counters: curr.clone(),
                },
            );
            total_rx += rx_bps;
            total_tx += tx_bps;
            interfaces_out.push(InterfaceTelemetry {
                interface,
                rx_bps,
                tx_bps,
                health,
                errors: curr.clone(),
            });
        }
        drop(prev_map);

        let snap = Arc::new(Snapshot {
            timestamp_ms: linkfyr_model::now_ms(),
            engine_version: linkfyr_model::ENGINE_VERSION.to_string(),
            interfaces: interfaces_out,
            totals: linkfyr_model::Totals {
                rx_bps: total_rx,
                tx_bps: total_tx,
            },
            internet,
        });

        self.history.write().await.push((
            snap.timestamp_ms,
            snap.totals.rx_bps,
            snap.totals.tx_bps,
        ));
        let _ = self.tx.send(snap.clone());
        snap
    }

    /// Run the engine loop forever (caller aborts to stop).
    pub async fn run(self: Arc<Self>) {
        let mut interval = tokio::time::interval(self.config.tick);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let n = self.ticks.load(Ordering::Relaxed) + 1;
            tracing::debug!(tick = n, "engine tick start");
            self.tick_once().await;
            tracing::debug!(tick = n, "engine tick done");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::ProbeConfig;
    use linkfyr_model::{IfKind, ProbeSample};
    use linkfyr_network::sim::{SimInterface, SimMonitor};

    fn sim() -> Arc<SimMonitor> {
        Arc::new(SimMonitor::new(vec![
            SimInterface::new("eth0", "Ethernet", IfKind::Ethernet).with_rates(1_000_000, 500_000),
            SimInterface::new("wifi0", "Wi-Fi", IfKind::Wifi)
                .with_rates(250_000, 125_000)
                .with_errors(3, 1),
        ]))
    }

    fn engine(monitor: Arc<SimMonitor>) -> Arc<TelemetryEngine> {
        let prober = Arc::new(TcpProber::new(ProbeConfig {
            targets: vec![],
            per_attempt_timeout: Duration::from_millis(10),
        }));
        TelemetryEngine::new(
            EngineConfig {
                tick: Duration::from_secs(1),
                probe_every: 5,
                history_len: 8,
            },
            monitor,
            prober,
        )
    }

    #[tokio::test]
    async fn first_tick_has_no_rates_but_second_does() {
        let m = sim();
        let e = engine(m.clone());

        let s1 = e.tick_once().await;
        assert_eq!(s1.interfaces.len(), 2);
        assert_eq!(s1.totals.rx_bps, 0.0, "no baseline yet");
        assert!(s1.interfaces.iter().all(|i| i.health.is_none()));

        let s2 = e.tick_once().await;
        // eth: 1 MB/tick → 8 Mbit/s; wifi: 250 KB/tick → 2 Mbit/s. Total 10 Mbit/s.
        assert!(
            (s2.totals.rx_bps - 10_000_000.0).abs() < 1.0,
            "got {}",
            s2.totals.rx_bps
        );
        assert!(
            (s2.totals.tx_bps - 5_000_000.0).abs() < 1.0,
            "got {}",
            s2.totals.tx_bps
        );
        assert!(s2.interfaces.iter().all(|i| i.health.is_some()));
    }

    #[tokio::test]
    async fn health_reflects_error_rates() {
        let m = sim();
        let e = engine(m.clone());
        e.tick_once().await;
        let s = e.tick_once().await;

        let eth = s
            .interfaces
            .iter()
            .find(|i| i.interface.id == "eth0")
            .unwrap();
        let wifi = s
            .interfaces
            .iter()
            .find(|i| i.interface.id == "wifi0")
            .unwrap();
        assert_eq!(eth.health.as_ref().unwrap().overall, 100);
        // wifi: 4 err/s over the window: errors factor ≈ 92; no probes → reachability 100.
        let wscore = wifi.health.as_ref().unwrap();
        assert!(
            wscore.overall < 100 && wscore.overall >= 80,
            "wifi scored {wscore:?}"
        );
        assert_eq!(wscore.status, linkfyr_model::HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn history_ring_is_bounded_and_appends() {
        let m = sim();
        let e = engine(m.clone());
        for _ in 0..12 {
            e.tick_once().await;
        }
        let hist = e.history().await;
        assert_eq!(hist.len(), 8, "history respects configured bound");
        assert!(
            hist.windows(2).all(|w| w[1].0 >= w[0].0),
            "timestamps ascend"
        );
    }

    #[tokio::test]
    async fn snapshot_watch_coalesces_to_latest() {
        let m = sim();
        let e = engine(m.clone());
        let mut rx = e.subscribe();
        assert!(
            rx.borrow().interfaces.is_empty(),
            "starts with default snapshot"
        );
        e.tick_once().await;
        rx.mark_changed();
        let latest = rx.borrow().clone();
        assert!(!latest.interfaces.is_empty());
    }

    #[tokio::test]
    async fn probe_stats_aggregate_into_snapshot() {
        // Engine with no targets: aggregate of empty window = defaults.
        let m = sim();
        let e = engine(m.clone());
        e.tick_once().await;
        let s = e.tick_once().await;
        assert_eq!(s.internet.sample_count, 0);
        assert!(s.internet.loss_pct.is_none() || s.internet.loss_pct == Some(0.0));
    }

    #[test]
    fn probe_sample_type_is_wire_compatible() {
        let s = ProbeSample {
            target: "x".into(),
            rtt_ms: Some(1.0),
            ok: true,
            timestamp_ms: 0,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"rttMs\""));
    }
}
