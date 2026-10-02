//! Routing table audit: parses the OS routing table via its native tool
//! (`route print` / `ip route` / `netstat -rn`) and flags real anomalies:
//! multiple default routes (multi-WAN/VPN overlay), the classic VPN
//! 0.0.0.0/1 + 128.0.0.0/1 full-tunnel hijack, and missing default.

use std::net::Ipv4Addr;
use std::time::Duration;

use linkfyr_model::optimize::{RouteAuditReport, RouteEntry};

use crate::exec;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Count leading 1-bits of a dotted netmask; rejects non-contiguous
/// masks (pure).
pub fn mask_to_prefix(mask: &str) -> Option<u8> {
    let ip: Ipv4Addr = mask.parse().ok()?;
    let bits = u32::from(ip);
    let ones = bits.count_ones();
    if ones == 0 {
        return Some(0);
    }
    // Contiguous mask: exactly the top `ones` bits set.
    let contiguous = bits == u32::MAX << (32 - ones);
    contiguous.then_some(ones as u8)
}

/// Parse Windows `route print -4` (pure).
pub fn parse_windows(output: &str) -> Vec<RouteEntry> {
    let mut entries = Vec::new();
    let mut active = false;
    for line in output.lines() {
        let t = line.trim();
        if t.starts_with("Active Routes") {
            active = true;
            continue;
        }
        if t.starts_with("Persistent Routes") || (active && t.starts_with("===")) {
            break;
        }
        if !active {
            continue;
        }
        let cols: Vec<&str> = t.split_whitespace().collect();
        if cols.len() != 5 || cols[0].eq_ignore_ascii_case("network") {
            continue;
        }
        let (Ok(dest), Some(prefix)) = (cols[0].parse::<Ipv4Addr>(), mask_to_prefix(cols[1]))
        else {
            continue;
        };
        entries.push(RouteEntry {
            destination: format!("{dest}/{prefix}"),
            gateway: if cols[2].eq_ignore_ascii_case("on-link") {
                None
            } else {
                Some(cols[2].to_string())
            },
            interface: Some(cols[3].to_string()),
            metric: cols[4].parse().ok(),
        });
    }
    entries
}

/// Parse Linux `ip route show` (pure).
pub fn parse_linux(output: &str) -> Vec<RouteEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() {
            continue;
        }
        let dest = if toks[0] == "default" {
            "0.0.0.0/0".to_string()
        } else {
            toks[0].to_string()
        };
        let gw = toks.iter().position(|t| *t == "via").map(|i| toks[i + 1]);
        let dev = toks.iter().position(|t| *t == "dev").map(|i| toks[i + 1]);
        let metric = toks
            .iter()
            .position(|t| *t == "metric")
            .and_then(|i| toks[i + 1].parse().ok());
        entries.push(RouteEntry {
            destination: dest,
            gateway: gw.map(ToString::to_string),
            interface: dev.map(ToString::to_string),
            metric,
        });
    }
    entries
}

/// Parse macOS `netstat -rn -f inet` (pure). Bare destinations like
/// "128.0/1" (VPN shorthands) are expanded.
pub fn parse_macos(output: &str) -> Vec<RouteEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let t = line.trim();
        if t.starts_with("Routing tables") || t.starts_with("Destination") || t.is_empty() {
            continue;
        }
        let cols: Vec<&str> = t.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        let dest_tok = cols[0];
        let known = dest_tok == "default"
            || dest_tok.parse::<Ipv4Addr>().is_ok()
            || (dest_tok.contains('/')
                && looks_bare_network(dest_tok.split('/').next().unwrap_or("")))
            || looks_bare_network(dest_tok);
        if !known {
            continue;
        }
        let dest = normalize_macos_dest(cols[0]);
        if dest.is_none() {
            continue;
        }
        entries.push(RouteEntry {
            destination: dest.expect("checked"),
            gateway: if cols[1].eq_ignore_ascii_case("link#1") {
                None
            } else {
                Some(cols[1].to_string())
            },
            interface: cols.get(3).map(ToString::to_string),
            metric: None,
        });
    }
    entries
}

fn looks_bare_network(tok: &str) -> bool {
    tok.chars().all(|c| c.is_ascii_digit() || c == '.')
        && tok.split('.').count() <= 4
        && !tok.is_empty()
}

/// Expand macOS shorthand: "default" → 0.0.0.0/0; "128.0/1" →
/// 128.0.0.0/1; "192.168.1" → 192.168.1.0/24 (legacy classful rows).
pub fn normalize_macos_dest(tok: &str) -> Option<String> {
    if tok == "default" {
        return Some("0.0.0.0/0".into());
    }
    let (lhs, plen) = tok
        .split_once('/')
        .map_or((tok, None), |(l, p)| (l, Some(p)));
    let octets: Vec<&str> = lhs.split('.').collect();
    if octets.len() > 4 || octets.iter().any(|o| o.parse::<u8>().is_err()) {
        return None;
    }
    let mut full = octets.clone();
    while full.len() < 4 {
        full.push("0");
    }
    let ip: Ipv4Addr = full.join(".").parse().ok()?;
    let plen = match plen {
        Some(p) => p.parse::<u8>().ok()?,
        None => match octets.len() {
            1 => 8,
            2 => 16,
            3 => 24,
            _ => 32,
        },
    };
    Some(format!("{ip}/{plen}"))
}

/// Canonical (network, prefix) for anomaly checks; IPv6 and unparsable
/// entries return None (they are displayed but not flagged).
pub fn canonical(dest: &str) -> Option<(u32, u8)> {
    let (lhs, plen) = dest.split_once('/')?;
    let ip: Ipv4Addr = lhs.parse().ok()?;
    let plen: u8 = plen.parse().ok()?;
    let bits = u32::from(ip);
    if plen == 0 {
        return Some((0, 0));
    }
    if bits & !(u32::MAX << (32 - plen)) != 0 {
        return None; // host bits set: not a network
    }
    Some((bits, plen))
}

/// Anomaly detection over parsed entries (pure).
pub fn detect_anomalies(entries: &[RouteEntry]) -> (u32, Vec<String>) {
    let mut anomalies = Vec::new();
    let defaults = entries
        .iter()
        .filter(|e| canonical(&e.destination) == Some((0, 0)))
        .count() as u32;

    if defaults == 0 {
        anomalies.push("No default route: this host cannot reach the Internet as-is".into());
    } else if defaults > 1 {
        anomalies.push(format!(
            "{defaults} default routes: multi-WAN or VPN overlay detected; LinkFYR multi-WAN scheduling (Phase 5) will manage failover order"
        ));
    }

    let half1 = entries
        .iter()
        .any(|e| canonical(&e.destination) == Some((0, 1)));
    let half2 = entries
        .iter()
        .any(|e| canonical(&e.destination) == Some((0x8000_0000, 1)));
    if half1 && half2 {
        anomalies.push(
            "Classic VPN full-tunnel pattern (0.0.0.0/1 + 128.0.0.0/1): every destination is pinned through the VPN; split tunneling arrives with Flow Rules (Phase 4)".into(),
        );
    }

    (defaults, anomalies)
}

/// Capture the routing table with the platform tool.
pub fn capture() -> Result<String, String> {
    if cfg!(windows) {
        let out = exec::run("route", &["print", "-4"], TIMEOUT);
        out.success
            .then_some(out.stdout)
            .ok_or_else(|| "route print failed".into())
    } else if cfg!(target_os = "macos") {
        let out = exec::run("netstat", &["-rn", "-f", "inet"], TIMEOUT);
        out.success
            .then_some(out.stdout)
            .ok_or_else(|| "netstat failed".into())
    } else {
        let out = exec::run("ip", &["route", "show"], TIMEOUT);
        out.success
            .then_some(out.stdout)
            .ok_or_else(|| "ip route failed".into())
    }
}

/// Live audit: capture + parse + analyze. Capture failure yields an
/// empty report with the error as its only anomaly line.
pub fn audit() -> RouteAuditReport {
    match capture() {
        Ok(text) => {
            let entries = if cfg!(windows) {
                parse_windows(&text)
            } else if cfg!(target_os = "macos") {
                parse_macos(&text)
            } else {
                parse_linux(&text)
            };
            let (default_count, anomalies) = detect_anomalies(&entries);
            RouteAuditReport {
                entries,
                default_count,
                anomalies,
            }
        }
        Err(e) => RouteAuditReport {
            entries: vec![],
            default_count: 0,
            anomalies: vec![format!("routing table unavailable: {e}")],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN_FIXTURE: &str = "===========================================================================\nInterface List\n  12...00 ff aa bb cc dd ......Intel(R) Ethernet\n===========================================================================\n\nActive Routes:\nNetwork Destination        Netmask          Gateway       Interface  Metric\n          0.0.0.0          0.0.0.0      192.168.1.1    192.168.1.50     25\n          0.0.0.0          0.0.0.0     10.212.54.1    10.212.54.20     5\n        0.0.0.0/1  255.255.255.255?      10.8.0.1     10.8.0.23       1\n      128.0.0.0/1? nope\n        127.0.0.0        255.0.0.0         On-link        127.0.0.1    331\n      192.168.1.0    255.255.255.0         On-link     192.168.1.50    281\n===========================================================================\nPersistent Routes:\n  None\n";

    const LINUX_FIXTURE: &str = "default via 192.168.1.1 dev eth0 proto dhcp metric 100\ndefault via 10.8.0.1 dev tun0 metric 50\n192.168.1.0/24 dev eth0 proto kernel scope link src 192.168.1.50 metric 100\n10.8.0.0/24 dev tun0 proto kernel scope link src 10.8.0.23\n128.0.0.0/1 via 10.8.0.1 dev tun0\n0.0.0.0/1 via 10.8.0.1 dev tun0\n";

    const MACOS_FIXTURE: &str = "Routing tables\n\nInternet:\nDestination        Gateway            Flags           Netif Expire\ndefault            192.168.1.1        UGScg             en0\n127                127.0.0.1          UCS               lo0\n128.0/1            10.8.0.1           UGSc            utun3\n0/1                10.8.0.1           UGSc            utun3\n192.168.1          link#1             UCS               en0      !\n";

    #[test]
    fn mask_to_prefix_handles_common_netmasks() {
        assert_eq!(mask_to_prefix("0.0.0.0"), Some(0));
        assert_eq!(mask_to_prefix("255.0.0.0"), Some(8));
        assert_eq!(mask_to_prefix("255.255.255.0"), Some(24));
        assert_eq!(mask_to_prefix("255.255.255.255"), Some(32));
        assert_eq!(mask_to_prefix("255.0.255.0"), None, "non-contiguous");
    }

    #[test]
    fn parses_windows_fixture_and_flags_double_default() {
        let entries = parse_windows(WIN_FIXTURE);
        assert!(entries.len() >= 4);
        let defaults = entries
            .iter()
            .filter(|e| e.destination == "0.0.0.0/0")
            .count();
        assert_eq!(defaults, 2);
        let first = entries
            .iter()
            .find(|e| e.destination == "0.0.0.0/0")
            .unwrap();
        assert_eq!(first.gateway.as_deref(), Some("192.168.1.1"));
        assert!(
            entries
                .iter()
                .any(|e| e.destination == "127.0.0.0/8" && e.gateway.is_none())
        );
        let (count, anomalies) = detect_anomalies(&entries);
        assert_eq!(count, 2);
        assert!(anomalies.iter().any(|a| a.contains("multi-WAN")));
    }

    #[test]
    fn parses_linux_fixture_and_flags_vpn_hijack() {
        let entries = parse_linux(LINUX_FIXTURE);
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.destination == "0.0.0.0/0")
                .count(),
            2
        );
        let (count, anomalies) = detect_anomalies(&entries);
        assert_eq!(count, 2);
        assert!(
            anomalies
                .iter()
                .any(|a| a.contains("0.0.0.0/1 + 128.0.0.0/1"))
        );
        let tun_default = entries
            .iter()
            .find(|e| e.destination == "0.0.0.0/0" && e.interface.as_deref() == Some("tun0"))
            .unwrap();
        assert_eq!(tun_default.metric, Some(50));
    }

    #[test]
    fn parses_macos_fixture_shorthands() {
        let entries = parse_macos(MACOS_FIXTURE);
        assert!(
            entries
                .iter()
                .any(|e| e.destination == "0.0.0.0/0" && e.interface.as_deref() == Some("en0"))
        );
        assert!(entries.iter().any(|e| e.destination == "128.0.0.0/1"));
        assert!(entries.iter().any(|e| e.destination == "0.0.0.0/1"));
        assert!(entries.iter().any(|e| e.destination == "127.0.0.0/8"));
        assert!(entries.iter().any(|e| e.destination == "192.168.1.0/24"));
        let (_, anomalies) = detect_anomalies(&entries);
        assert!(anomalies.iter().any(|a| a.contains("full-tunnel")));
    }

    #[test]
    fn normalizes_macos_destinations() {
        assert_eq!(
            normalize_macos_dest("default").as_deref(),
            Some("0.0.0.0/0")
        );
        assert_eq!(
            normalize_macos_dest("128.0/1").as_deref(),
            Some("128.0.0.0/1")
        );
        assert_eq!(
            normalize_macos_dest("192.168.1").as_deref(),
            Some("192.168.1.0/24")
        );
        assert_eq!(
            normalize_macos_dest("10.0.0.0/8").as_deref(),
            Some("10.0.0.0/8")
        );
        assert_eq!(normalize_macos_dest("fe80::"), None);
    }

    #[test]
    fn canonical_rejects_host_bits_and_v6() {
        assert_eq!(canonical("0.0.0.0/0"), Some((0, 0)));
        assert_eq!(canonical("128.0.0.0/1"), Some((0x8000_0000, 1)));
        assert_eq!(canonical("10.1.2.3/8"), None, "host bits set");
        assert_eq!(canonical("fe80::/64"), None);
    }

    #[test]
    fn missing_default_is_flagged() {
        let entries = vec![RouteEntry {
            destination: "192.168.1.0/24".into(),
            gateway: None,
            interface: Some("eth0".into()),
            metric: Some(100),
        }];
        let (count, anomalies) = detect_anomalies(&entries);
        assert_eq!(count, 0);
        assert!(anomalies.iter().any(|a| a.contains("No default route")));
    }

    #[test]
    fn live_audit_or_honest_failure() {
        let report = audit();
        if report.entries.is_empty() {
            assert!(!report.anomalies.is_empty(), "failure needs a reason");
        } else {
            // Parsing itself must have produced sane networks.
            assert!(
                report
                    .entries
                    .iter()
                    .all(|e| canonical(&e.destination).is_some() || e.destination.contains(':'))
            );
        }
    }
}
