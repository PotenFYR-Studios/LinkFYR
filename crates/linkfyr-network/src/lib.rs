//! LinkFYR platform abstraction for network interfaces.
//!
//! Phase 1 ships `OsMonitor` (discovery via netdev, counters via sysinfo)
//! and `SimMonitor` (deterministic, for tests). The trait surface is the
//! seam where Windows (IP Helper → WFP later), macOS (sysctl → Network
//! Extension later), and Linux (netlink → nftables/tc later) backends
//! attach without touching callers. See docs/platform-support.md.

pub mod os;
pub mod sim;

pub use os::OsMonitor;

use linkfyr_model::{IfCounters, IfKind, IfStatus, Interface};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MonitorError {
    #[error("failed to enumerate interfaces: {0}")]
    Enumeration(String),
}

/// Discovery of interface identity/state.
pub trait InterfaceMonitor: Send + Sync {
    fn snapshot(&self) -> Result<Vec<Interface>, MonitorError>;
    /// Monotonic counters; callers diff consecutive samples.
    fn counters(&self) -> Result<Vec<IfCounters>, MonitorError>;
}

/// Classify an interface from its OS-reported properties.
/// Kept here so every backend shares one policy.
pub fn classify(name: &str, os_kind: IfKind) -> IfKind {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("loopback") || lowered == "lo" || lowered.starts_with("lo0") {
        return IfKind::Loopback;
    }
    if lowered.contains("tun")
        || lowered.contains("tap")
        || lowered.contains("wg")
        || lowered.contains("ppp")
        || lowered.contains("linkfyr")
    {
        return IfKind::Tunnel;
    }
    if lowered.contains("virtual")
        || lowered.contains("vethernet")
        || lowered.contains("vmware")
        || lowered.contains("virtualbox")
        || lowered.contains("docker")
        || lowered.contains("veth")
        || lowered.contains("hyper-v")
    {
        return IfKind::Virtual;
    }
    if lowered.contains("cellular")
        || lowered.contains("wwan")
        || lowered.contains("mobile")
        || lowered.contains("lte")
        || lowered.contains("5g")
        || lowered.contains("4g")
        || lowered.contains("3g")
    {
        return IfKind::Cellular;
    }
    match os_kind {
        IfKind::Wifi | IfKind::Ethernet | IfKind::Loopback | IfKind::Tunnel => os_kind,
        IfKind::Cellular | IfKind::Virtual | IfKind::Other => {
            if lowered.contains("wi-fi") || lowered.contains("wlan") || lowered.contains("wireless")
            {
                IfKind::Wifi
            } else if lowered.starts_with("en") || lowered.starts_with("eth") {
                IfKind::Ethernet
            } else {
                os_kind
            }
        }
    }
}

/// Map a raw sysinfo-style "is up" flag plus name into a status.
pub fn status_from(is_up: bool, has_traffic_capability: bool) -> IfStatus {
    if is_up {
        IfStatus::Up
    } else if has_traffic_capability {
        IfStatus::Dormant
    } else {
        IfStatus::Down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_names_classify_as_loopback() {
        assert_eq!(classify("lo", IfKind::Other), IfKind::Loopback);
        assert_eq!(classify("lo0", IfKind::Ethernet), IfKind::Loopback);
        assert_eq!(
            classify("Loopback Pseudo-Interface 1", IfKind::Other),
            IfKind::Loopback
        );
    }

    #[test]
    fn tunnel_and_virtual_names_win_over_os_kind() {
        assert_eq!(classify("wg0", IfKind::Ethernet), IfKind::Tunnel);
        assert_eq!(classify("tun-linkfyr", IfKind::Ethernet), IfKind::Tunnel);
        assert_eq!(
            classify("vEthernet (WSL)", IfKind::Ethernet),
            IfKind::Virtual
        );
        assert_eq!(classify("docker0", IfKind::Ethernet), IfKind::Virtual);
    }

    #[test]
    fn cellular_keywords_classify() {
        assert_eq!(classify("Cellular 5G", IfKind::Other), IfKind::Cellular);
        assert_eq!(classify("wwan0", IfKind::Other), IfKind::Cellular);
    }

    #[test]
    fn wifi_and_ethernet_names() {
        assert_eq!(classify("Wi-Fi", IfKind::Other), IfKind::Wifi);
        assert_eq!(classify("wlan0", IfKind::Other), IfKind::Wifi);
        assert_eq!(classify("Ethernet 2", IfKind::Other), IfKind::Ethernet);
        assert_eq!(classify("en0", IfKind::Other), IfKind::Ethernet);
    }

    #[test]
    fn os_kind_passthrough_when_no_name_hint() {
        assert_eq!(classify("nic7", IfKind::Wifi), IfKind::Wifi);
        assert_eq!(classify("nic7", IfKind::Other), IfKind::Other);
    }

    #[test]
    fn status_mapping() {
        assert_eq!(status_from(true, true), IfStatus::Up);
        assert_eq!(status_from(false, true), IfStatus::Dormant);
        assert_eq!(status_from(false, false), IfStatus::Down);
    }
}
