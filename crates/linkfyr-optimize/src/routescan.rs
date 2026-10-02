//! Multi-path latency scanner: measures real TCP-connect RTT to a
//! destination over IPv4 vs IPv6 and against control endpoints, then
//! detects the classic "IPv6 slower than IPv4" happy-eyeballs penalty
//! that makes one site crawl while others are fine.

use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::{LatencyPath, RouteScanReport};

use crate::stats;

/// Default ports probed for the target.
pub const DEFAULT_PORTS: &[u16] = &[443, 80];
/// Control endpoints used to separate "this path is slow" from "your
/// whole Internet is slow".
pub const CONTROLS: &[(&str, &str)] = &[
    ("1.1.1.1:443", "Cloudflare"),
    ("8.8.8.8:443", "Google DNS"),
    ("9.9.9.9:443", "Quad9"),
];

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const DELTA_MS_THRESHOLD: f64 = 15.0;

/// Resolve a host name to unique IPs using the OS resolver.
pub fn resolve_host(host: &str, port: u16) -> Vec<IpAddr> {
    let mut ips: Vec<IpAddr> = format!("{host}:{port}")
        .to_socket_addrs()
        .map(|iter| iter.map(|sa| sa.ip()).collect())
        .unwrap_or_default();
    ips.sort();
    ips.dedup();
    ips
}

fn connect_rtt(addr: SocketAddr) -> Result<f64, String> {
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

/// Probe one address `samples` times; returns honest stats or None when
/// every attempt failed.
pub fn probe_path(label: &str, addr: SocketAddr, samples: u32) -> LatencyPath {
    let mut rtts = Vec::new();
    for _ in 0..samples {
        if let Ok(ms) = connect_rtt(addr) {
            rtts.push(ms);
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    let summary = stats::summarize(&rtts);
    LatencyPath {
        label: label.into(),
        addr: addr.to_string(),
        samples,
        success: rtts.len() as u32,
        min_ms: summary.map(|s| s.min_ms),
        median_ms: summary.map(|s| s.median_ms),
        p95_ms: summary.map(|s| s.p95_ms),
        jitter_ms: summary.map(|s| s.jitter_ms),
    }
}

/// Verdict + recommendation from measured target paths (pure).
pub fn verdict(paths: &[LatencyPath]) -> (String, String) {
    let target_median = |v4: bool| -> Option<f64> {
        paths
            .iter()
            .filter(|p| p.label.contains("target") && is_family(&p.addr, v4) == Some(true))
            .filter_map(|p| p.median_ms)
            .reduce(f64::min)
    };
    let v4 = target_median(true);
    let v6 = target_median(false);

    match (v4, v6) {
        (Some(four), Some(six)) => {
            let delta = six - four;
            if delta > DELTA_MS_THRESHOLD {
                (
                    format!("IPv6 penalty: this destination is {delta:.0} ms slower over IPv6"),
                    "Prefer IPv4 for this destination until your IPv6 path improves; per-app pinning arrives with Flow Rules (Phase 4)".into(),
                )
            } else if -delta > DELTA_MS_THRESHOLD {
                (
                    format!(
                        "IPv4 penalty: this destination is {:.0} ms slower over IPv4",
                        -delta
                    ),
                    "Prefer IPv6 for this destination".into(),
                )
            } else {
                (
                    format!("Balanced: IPv4 and IPv6 within {delta:.0} ms"),
                    "No address-family action needed; both paths are usable".into(),
                )
            }
        }
        (Some(_), None) => (
            "IPv4 only: no reachable IPv6 address for this destination".into(),
            "Normal for many sites; nothing to fix".into(),
        ),
        (None, Some(_)) => (
            "IPv6 only".into(),
            "Unusual; verify the IPv4 route if you expected dual-stack".into(),
        ),
        (None, None) => (
            "Unreachable: every probe failed".into(),
            "Check DNS, firewall, or whether the host is online".into(),
        ),
    }
}

fn is_family(addr: &str, v4: bool) -> Option<bool> {
    addr.parse::<SocketAddr>().ok().map(|sa| sa.is_ipv4() == v4)
}

/// Full scan of one target plus control endpoints.
pub fn scan(target: &str, samples: u32) -> RouteScanReport {
    let mut paths = Vec::new();

    let ips = resolve_host(target, 443);
    for ip in &ips {
        for port in DEFAULT_PORTS {
            let addr = SocketAddr::new(*ip, *port);
            let family = if addr.is_ipv4() { "IPv4" } else { "IPv6" };
            paths.push(probe_path(
                &format!("target {family} :{port}"),
                addr,
                samples,
            ));
        }
    }

    for (addr, label) in CONTROLS {
        if let Ok(sa) = addr.parse::<SocketAddr>() {
            paths.push(probe_path(&format!("control {label}"), sa, samples));
        }
    }

    // Failed paths stay visible: an unreachable family is a finding, not
    // noise. The verdict reads them all.

    let (v, rec) = verdict(&paths);
    RouteScanReport {
        target: target.into(),
        paths,
        verdict: v,
        recommendation: rec,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_localhost_both_stacks() {
        let ips = resolve_host("localhost", 443);
        assert!(ips.iter().any(|ip| ip.is_loopback()));
    }

    #[test]
    fn probing_real_listener_succeeds() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let p = probe_path("target IPv4 test", addr, 3);
        assert_eq!(p.success, 3);
        let median = p.median_ms.expect("median");
        let min = p.min_ms.expect("min");
        assert!(median < 500.0, "loopback connect must be fast: {median}");
        assert!(min <= median);
    }

    #[test]
    fn failed_probes_report_zero_success() {
        // Nothing listens on this port on 127.0.0.1: connect must fail.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        drop(listener);
        let p = probe_path("target dead", addr, 1);
        assert_eq!(p.success, 0);
        assert!(p.median_ms.is_none());
    }

    fn path(label: &str, addr: &str, median: Option<f64>) -> LatencyPath {
        LatencyPath {
            label: label.into(),
            addr: addr.into(),
            samples: 5,
            success: if median.is_some() { 5 } else { 0 },
            min_ms: median,
            median_ms: median,
            p95_ms: median.map(|m| m * 1.2),
            jitter_ms: median.map(|_| 1.0),
        }
    }

    #[test]
    fn verdict_flags_ipv6_penalty() {
        let paths = vec![
            path("target IPv4 :443", "192.0.2.10:443", Some(20.0)),
            path("target IPv6 :443", "[2001:db8::10]:443", Some(90.0)),
        ];
        let (v, r) = verdict(&paths);
        assert!(v.contains("IPv6 penalty"), "got: {v}");
        assert!(r.contains("IPv4"));
    }

    #[test]
    fn verdict_balanced_when_close() {
        let paths = vec![
            path("target IPv4 :443", "192.0.2.10:443", Some(30.0)),
            path("target IPv6 :443", "[2001:db8::10]:443", Some(33.0)),
        ];
        let (v, _) = verdict(&paths);
        assert!(v.contains("Balanced"), "got: {v}");
    }

    #[test]
    fn verdict_single_stack_and_unreachable() {
        let v4_only = vec![path("target IPv4 :443", "192.0.2.10:443", Some(12.0))];
        assert!(verdict(&v4_only).0.contains("IPv4 only"));

        let none = vec![
            path("target IPv4 :443", "192.0.2.10:443", None),
            path("target IPv6 :443", "[2001:db8::10]:443", None),
        ];
        assert!(verdict(&none).0.contains("Unreachable"));
    }

    #[test]
    fn scan_reports_controls() {
        // localhost as target: resolves and connects are all local.
        let report = scan("localhost", 1);
        assert!(report.paths.iter().any(|p| p.label.contains("control")));
        assert_ne!(report.verdict, "");
        assert_ne!(report.recommendation, "");
    }
}
