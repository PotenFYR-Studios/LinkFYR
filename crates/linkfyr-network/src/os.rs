//! OS-backed implementation of [`InterfaceMonitor`].
//!
//! Discovery: `netdev` (cross-platform: IP Helper on Windows, getifaddrs
//! on macOS, netlink on Linux). Counters: `sysinfo::Networks`, matched to
//! interfaces by MAC address because the two libraries use different
//! naming (GUIDs vs friendly names on Windows).
//!
//! Platform upgrades (WFP, NetworkExtension, direct netlink) replace this
//! module per-OS without changing the trait — see docs/platform-support.md.

use std::collections::HashMap;
use std::sync::Mutex;

use linkfyr_model::{IfCounters, IfKind, IfStatus, Interface};
use sysinfo::Networks;

use crate::{InterfaceMonitor, MonitorError, classify};

const ZERO_MAC: &str = "00:00:00:00:00:00";

pub struct OsMonitor {
    networks: Mutex<Networks>,
}

impl OsMonitor {
    pub fn new() -> Self {
        Self {
            networks: Mutex::new(Networks::new_with_refreshed_list()),
        }
    }

    /// One consistent enumeration pass: netdev identities + sysinfo
    /// counters joined by MAC. `counters()` re-enumerates identities to
    /// derive stable ids; 1 Hz makes this cost irrelevant for now.
    fn collect(&self) -> Result<(Vec<netdev::Interface>, Vec<IfCounters>), MonitorError> {
        let raw = netdev::get_interfaces();

        let mut networks = self
            .networks
            .lock()
            .map_err(|e| MonitorError::Enumeration(e.to_string()))?;
        networks.refresh(true);

        // sysinfo keyed by MAC (loopback falls back to its name).
        let mut by_key: HashMap<String, (u64, u64, u64, u64, u64, u64)> = HashMap::new();
        for (name, data) in networks.iter() {
            let mac = data.mac_address().to_string();
            let key = if mac == ZERO_MAC { name.clone() } else { mac };
            by_key.insert(
                key,
                (
                    data.total_received(),
                    data.total_transmitted(),
                    data.total_packets_received(),
                    data.total_packets_transmitted(),
                    data.total_errors_on_received(),
                    data.total_errors_on_transmitted(),
                ),
            );
        }

        let ts = linkfyr_model::now_ms();
        let counters = raw
            .iter()
            .map(|i| {
                let mac = i
                    .mac_addr
                    .as_ref()
                    .map_or_else(|| ZERO_MAC.to_string(), |m| m.to_string());
                let key = if mac == ZERO_MAC || is_loopback_name(&i.name) {
                    i.name.clone()
                } else {
                    mac
                };
                let (rx_bytes, tx_bytes, rx_packets, tx_packets, rx_errors, tx_errors) =
                    by_key.get(&key).copied().unwrap_or((0, 0, 0, 0, 0, 0));
                IfCounters {
                    id: i.name.clone(),
                    rx_bytes,
                    tx_bytes,
                    rx_packets,
                    tx_packets,
                    rx_errors,
                    tx_errors,
                    timestamp_ms: ts,
                }
            })
            .collect();

        Ok((raw, counters))
    }
}

impl Default for OsMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn is_loopback_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered == "lo" || lowered.starts_with("lo0") || lowered.contains("loopback")
}

impl InterfaceMonitor for OsMonitor {
    fn snapshot(&self) -> Result<Vec<Interface>, MonitorError> {
        let (raw, counters) = self.collect()?;
        let has_traffic: Vec<bool> = counters
            .iter()
            .map(|c| c.rx_bytes > 0 || c.tx_bytes > 0)
            .collect();

        let out: Vec<Interface> = raw
            .iter()
            .map(|i| Interface {
                id: i.name.clone(),
                name: i.name.clone(),
                friendly_name: i.friendly_name.clone().unwrap_or_else(|| i.name.clone()),
                kind: classify(&i.name, if_kind_from(i)),
                status: status_from_oper(i.oper_state),
                mac: i.mac_addr.as_ref().map(|m| m.to_string()),
                ipv4: i.ipv4.iter().map(|net| net.to_string()).collect(),
                ipv6: i.ipv6.iter().map(|net| net.to_string()).collect(),
                gateway: gateway_ip(i),
                mtu: i.mtu,
                speed_bps: i
                    .transmit_speed
                    .max(i.receive_speed)
                    .map(|s| s.saturating_mul(8)),
                metered: false,
            })
            .collect();

        // Rank physical/active links first (UI order).
        let rank = |idx: usize, i: &Interface| -> u8 {
            let active = has_traffic[idx];
            match (active, i.kind) {
                (true, IfKind::Ethernet | IfKind::Wifi | IfKind::Cellular) => 0,
                (true, _) => 1,
                (false, IfKind::Ethernet | IfKind::Wifi | IfKind::Cellular) => 2,
                (false, IfKind::Loopback) => 4,
                (false, _) => 3,
            }
        };
        let mut order: Vec<usize> = (0..out.len()).collect();
        order.sort_by(|&a, &b| {
            rank(a, &out[a])
                .cmp(&rank(b, &out[b]))
                .then_with(|| out[a].name.cmp(&out[b].name))
        });
        Ok(order.into_iter().map(|idx| out[idx].clone()).collect())
    }

    fn counters(&self) -> Result<Vec<IfCounters>, MonitorError> {
        self.collect().map(|(_, counters)| counters)
    }
}

fn if_kind_from(i: &netdev::Interface) -> IfKind {
    use netdev::prelude::InterfaceType as It;
    match i.if_type {
        It::Ethernet | It::TokenRing | It::Fddi | It::Isdn => IfKind::Ethernet,
        It::Wireless80211 => IfKind::Wifi,
        It::Ppp | It::Slip | It::Atm | It::GenericModem => IfKind::Tunnel,
        It::Loopback => IfKind::Loopback,
        It::ProprietaryVirtual => IfKind::Virtual,
        It::Unknown => {
            if i.is_loopback() {
                IfKind::Loopback
            } else {
                IfKind::Other
            }
        }
        _ => IfKind::Other,
    }
}

/// Map netdev IETF-style oper state onto our display status.
fn status_from_oper(state: netdev::prelude::OperState) -> IfStatus {
    use netdev::prelude::OperState as Os;
    match state {
        Os::Up => IfStatus::Up,
        Os::Dormant | Os::Testing => IfStatus::Dormant,
        Os::Down | Os::LowerLayerDown | Os::NotPresent => IfStatus::Down,
        Os::Unknown => IfStatus::Unknown,
    }
}

fn gateway_ip(i: &netdev::Interface) -> Option<String> {
    let g = i.gateway.as_ref()?;
    let ip = g
        .ipv4
        .first()
        .map(|v| std::net::IpAddr::V4(*v))
        .or_else(|| g.ipv6.first().map(|v| std::net::IpAddr::V6(*v)))?;
    Some(ip.to_string())
}
