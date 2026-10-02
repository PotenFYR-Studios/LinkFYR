//! Deterministic network simulator implementing [`InterfaceMonitor`].
//!
//! Used by tests and (later) scheduler benchmarks to replay scenarios:
//! high latency, loss, flap, cap exhaustion, asymmetric bandwidth.
//! Counters advance at scripted rates per synthetic tick â€” no real time
//! or real NICs involved, so assertions stay exact.

use std::sync::atomic::{AtomicU64, Ordering};

use linkfyr_model::now_ms;
use linkfyr_model::{IfCounters, IfKind, IfStatus, Interface};

use crate::InterfaceMonitor;

/// One simulated interface with scripted behavior.
#[derive(Debug, Clone)]
pub struct SimInterface {
    pub interface: Interface,
    /// Bytes per tick while Up.
    pub rx_bytes_per_tick: u64,
    pub tx_bytes_per_tick: u64,
    /// Errors added per tick while Up.
    pub rx_errors_per_tick: u64,
    pub tx_errors_per_tick: u64,
    /// Optional flap window (inclusive tick range during which the link is Down).
    pub down_ticks: Option<(u64, u64)>,
}

impl SimInterface {
    pub fn new(id: &str, name: &str, kind: IfKind) -> Self {
        Self {
            interface: Interface {
                id: id.to_string(),
                name: name.to_string(),
                friendly_name: name.to_string(),
                kind,
                status: IfStatus::Up,
                mac: None,
                ipv4: vec!["10.0.0.1/24".to_string()],
                ipv6: vec![],
                gateway: Some("10.0.0.254".to_string()),
                mtu: Some(1500),
                speed_bps: None,
                metered: false,
            },
            rx_bytes_per_tick: 0,
            tx_bytes_per_tick: 0,
            rx_errors_per_tick: 0,
            tx_errors_per_tick: 0,
            down_ticks: None,
        }
    }

    #[must_use]
    pub fn with_rates(mut self, rx: u64, tx: u64) -> Self {
        self.rx_bytes_per_tick = rx;
        self.tx_bytes_per_tick = tx;
        self
    }

    #[must_use]
    pub fn with_errors(mut self, rx: u64, tx: u64) -> Self {
        self.rx_errors_per_tick = rx;
        self.tx_errors_per_tick = tx;
        self
    }

    #[must_use]
    pub fn down_during(mut self, from: u64, to: u64) -> Self {
        self.down_ticks = Some((from, to));
        self
    }

    fn is_down_at(&self, tick: u64) -> bool {
        matches!(self.down_ticks, Some((a, b)) if tick >= a && tick <= b)
    }
}

/// A deterministic monitor advancing fixed-rate counters per call.
#[derive(Debug)]
pub struct SimMonitor {
    interfaces: Vec<SimInterface>,
    tick: AtomicU64,
    state: std::sync::Mutex<SimState>,
    fixed_timestamp: bool,
}

#[derive(Debug, Default)]
struct SimState {
    rx_bytes: Vec<u64>,
    tx_bytes: Vec<u64>,
    rx_errors: Vec<u64>,
    tx_errors: Vec<u64>,
}

impl SimMonitor {
    pub fn new(interfaces: Vec<SimInterface>) -> Self {
        let n = interfaces.len();
        Self {
            interfaces,
            tick: AtomicU64::new(0),
            state: std::sync::Mutex::new(SimState {
                rx_bytes: vec![0; n],
                tx_bytes: vec![0; n],
                rx_errors: vec![0; n],
                tx_errors: vec![0; n],
            }),
            fixed_timestamp: true,
        }
    }

    /// Use the wall clock for counter timestamps instead of a fixed base.
    /// Needed because the telemetry engine diffs timestamps, not ticks.
    #[must_use]
    pub fn wall_clock(mut self) -> Self {
        self.fixed_timestamp = false;
        self
    }

    pub fn tick(&self) -> u64 {
        self.tick.load(Ordering::SeqCst)
    }

    fn base_ms() -> u64 {
        1_700_000_000_000
    }
}

impl InterfaceMonitor for SimMonitor {
    fn snapshot(&self) -> Result<Vec<Interface>, crate::MonitorError> {
        let t = self.tick();
        Ok(self
            .interfaces
            .iter()
            .map(|s| {
                let mut ifc = s.interface.clone();
                ifc.status = if s.is_down_at(t) {
                    IfStatus::Down
                } else {
                    IfStatus::Up
                };
                ifc
            })
            .collect())
    }

    fn counters(&self) -> Result<Vec<IfCounters>, crate::MonitorError> {
        let t = self.tick.fetch_add(1, Ordering::SeqCst);
        let mut st = self.state.lock().expect("sim state poisoned");
        let mut out = Vec::with_capacity(self.interfaces.len());
        for (i, s) in self.interfaces.iter().enumerate() {
            let down = s.is_down_at(t);
            if !down {
                st.rx_bytes[i] = st.rx_bytes[i].saturating_add(s.rx_bytes_per_tick);
                st.tx_bytes[i] = st.tx_bytes[i].saturating_add(s.tx_bytes_per_tick);
                st.rx_errors[i] = st.rx_errors[i].saturating_add(s.rx_errors_per_tick);
                st.tx_errors[i] = st.tx_errors[i].saturating_add(s.tx_errors_per_tick);
            }
            out.push(IfCounters {
                id: s.interface.id.clone(),
                rx_bytes: st.rx_bytes[i],
                tx_bytes: st.tx_bytes[i],
                rx_packets: 0,
                tx_packets: 0,
                rx_errors: st.rx_errors[i],
                tx_errors: st.tx_errors[i],
                timestamp_ms: if self.fixed_timestamp {
                    Self::base_ms() + t * 1000
                } else {
                    now_ms()
                },
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> SimMonitor {
        SimMonitor::new(vec![
            SimInterface::new("eth0", "Ethernet", IfKind::Ethernet).with_rates(1_000_000, 500_000),
            SimInterface::new("wifi0", "Wi-Fi", IfKind::Wifi)
                .with_rates(250_000, 125_000)
                .with_errors(3, 1)
                .down_during(2, 3),
        ])
    }

    #[test]
    fn counters_advance_by_scripted_rates() {
        let m = monitor();
        let first = m.counters().unwrap();
        assert_eq!(first[0].rx_bytes, 1_000_000);
        assert_eq!(first[1].rx_errors, 3);

        let second = m.counters().unwrap();
        assert_eq!(second[0].rx_bytes, 2_000_000);
        assert_eq!(second[1].rx_bytes, 500_000);
    }

    #[test]
    fn flap_window_reports_down_and_freezes_counters() {
        let m = monitor();
        // ticks 0, 1: up
        assert_eq!(m.snapshot().unwrap()[1].status, IfStatus::Up);
        m.counters().unwrap();
        m.counters().unwrap();
        // tick 2: inside the inclusive down window [2, 3]
        assert_eq!(m.snapshot().unwrap()[1].status, IfStatus::Down);
        let during_flap = m.counters().unwrap();
        assert_eq!(
            during_flap[1].rx_bytes, 500_000,
            "counters frozen while down"
        );
        // tick 3 still down
        assert_eq!(m.snapshot().unwrap()[1].status, IfStatus::Down);
        m.counters().unwrap();
        // tick 4: up again
        assert_eq!(m.snapshot().unwrap()[1].status, IfStatus::Up);
        let after = m.counters().unwrap();
        assert_eq!(after[1].rx_bytes, 750_000, "rates resume after flap");
    }

    #[test]
    fn timestamps_are_deterministic_in_fixed_mode() {
        let m = monitor();
        let a = m.counters().unwrap();
        let b = m.counters().unwrap();
        assert_eq!(b[0].timestamp_ms - a[0].timestamp_ms, 1000);
    }

    #[test]
    fn wall_clock_mode_uses_now() {
        let m = monitor().wall_clock();
        let a = m.counters().unwrap();
        assert!(a[0].timestamp_ms > 1_700_000_000_000);
    }
}
