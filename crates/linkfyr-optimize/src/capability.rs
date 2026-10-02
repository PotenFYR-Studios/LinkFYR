//! Capability detection: which tools can actually run on this machine
//! right now (available / needs elevation / missing). The UI renders
//! these states instead of pretending every button works everywhere.

use linkfyr_model::optimize::{CapabilitiesReport, CapabilityState, ToolCapability};

use crate::exec;

const AIRPORT: &str =
    "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport";

/// Detect every toolkit capability on this machine. Enumerated from the
/// registry so the count can never drift from the implemented catalog.
pub fn detect() -> CapabilitiesReport {
    let registry = crate::registry::catalog();
    let state_for = |tool: &str| -> (CapabilityState, String) {
        match tool {
            // Elevated mutations.
            "dns_apply" | "dhcp_renew" | "dhcp_release" | "adapter_reset" | "winsock_reset"
            | "tcp_tuning_apply" | "hotspot_share" | "ntp_sync" => (
                if exec::is_elevated() {
                    CapabilityState::Available
                } else {
                    CapabilityState::Elevated
                },
                "changes system network settings; elevation required".into(),
            ),
            // External-egress (opt-in) tools.
            "public_ip"
            | "doh_benchmark"
            | "whois_lookup"
            | "ip_geolocation"
            | "captive_portal_probe" => (
                CapabilityState::Available,
                "contacts an external service when you run it".into(),
            ),
            // ping-backed tools.
            "icmp_ping" | "gateway_latency" | "mtu" => (
                if exec::on_path("ping") {
                    CapabilityState::Available
                } else {
                    CapabilityState::Unavailable
                },
                "OS ping binary with DF/echo support".into(),
            ),
            "wifi_scan" => {
                if cfg!(windows) {
                    (
                        if exec::on_path("netsh") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "netsh wlan show networks".into(),
                    )
                } else if cfg!(target_os = "macos") {
                    (
                        if std::path::Path::new(AIRPORT).exists() {
                            CapabilityState::Available
                        } else {
                            CapabilityState::PlatformLimited
                        },
                        "airport -s".into(),
                    )
                } else {
                    (
                        if exec::on_path("nmcli") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "NetworkManager (nmcli) required".into(),
                    )
                }
            }
            "tcp_audit" => {
                if cfg!(windows) {
                    (
                        if exec::on_path("netsh") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "netsh int tcp show global".into(),
                    )
                } else if cfg!(target_os = "macos") {
                    (
                        if exec::on_path("sysctl") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "sysctl net.inet.tcp".into(),
                    )
                } else {
                    (
                        if std::path::Path::new("/proc/sys/net/ipv4").exists()
                            || exec::on_path("sysctl")
                        {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "/proc/sys/net/ipv4 + tc".into(),
                    )
                }
            }
            "route_audit" => {
                let bin = if cfg!(windows) {
                    "route"
                } else if cfg!(target_os = "macos") {
                    "netstat"
                } else {
                    "ip"
                };
                (
                    if exec::on_path(bin) {
                        CapabilityState::Available
                    } else {
                        CapabilityState::Unavailable
                    },
                    format!("{bin} (read-only)"),
                )
            }
            "connections" | "listening_ports" => {
                if cfg!(windows) {
                    (
                        if exec::on_path("netstat") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "netstat -ano".into(),
                    )
                } else if cfg!(target_os = "macos") {
                    (
                        if exec::on_path("lsof") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "lsof -F".into(),
                    )
                } else {
                    (
                        if exec::on_path("ss") || exec::on_path("netstat") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "ss/netstat socket tables".into(),
                    )
                }
            }
            "arp_table" => {
                if cfg!(windows) {
                    (
                        if exec::on_path("arp") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "arp -a".into(),
                    )
                } else {
                    (
                        if exec::on_path("ip") || exec::on_path("arp") {
                            CapabilityState::Available
                        } else {
                            CapabilityState::Unavailable
                        },
                        "ip neigh / arp".into(),
                    )
                }
            }
            "flush_dns" => repair_flush_dns_state(),
            // Pure-socket tools run everywhere.
            _ => (
                CapabilityState::Available,
                "pure sockets, no privileges".into(),
            ),
        }
    };

    let tools = registry
        .into_iter()
        .map(|t| {
            let (state, detail) = state_for(&t.id);
            ToolCapability {
                tool: t.id.clone(),
                state,
                detail,
                name: t.name,
                group: t.group,
                takes_target: t.takes_target,
            }
        })
        .collect();

    CapabilitiesReport {
        elevated: exec::is_elevated(),
        platform: std::env::consts::OS.to_string(),
        tools,
    }
}

fn repair_flush_dns_state() -> (CapabilityState, String) {
    let present = if cfg!(windows) {
        exec::on_path("ipconfig")
    } else if cfg!(target_os = "macos") {
        exec::on_path("dscacheutil")
    } else {
        exec::on_path("resolvectl") || exec::on_path("systemd-resolve")
    };
    (
        if present {
            CapabilityState::Available
        } else {
            CapabilityState::Unavailable
        },
        "flushes the OS resolver cache".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_reports_every_tool_with_a_state() {
        let r = detect();
        assert!(r.tools.len() >= 10);
        assert_ne!(r.platform, "");
        for t in &r.tools {
            assert_ne!(t.tool, "");
            assert_ne!(t.detail, "");
        }
        // wifi_scan on Linux CI containers is honestly "unavailable".
        let wifi = r.tools.iter().find(|t| t.tool == "wifi_scan").unwrap();
        assert!(matches!(
            wifi.state,
            CapabilityState::Available
                | CapabilityState::Unavailable
                | CapabilityState::PlatformLimited
                | CapabilityState::Elevated
        ));
    }
}
