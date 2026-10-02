//! Latency probe family: TCP ping, sustained latency monitor, jitter
//! burst, ICMP ping through the OS binary (with strict target
//! validation, since it is passed to a subprocess), IPv6 readiness, and
//! the TCP port scanner. All use real sockets or real ping output.

use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::{PortResult, PortScanReport};

use crate::exec;
use crate::stats;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// A host is either a valid IP or a conservative hostname (letters,
/// digits, hyphen, dot). This matters because it is passed to `ping`.
pub fn valid_target(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

pub fn resolve_one(host: &str, port: u16) -> Option<SocketAddr> {
    (host, port).to_socket_addrs().ok()?.next()
}

fn connect_ms(addr: SocketAddr) -> Result<f64, String> {
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

/// Sustained latency: `samples` TCP connects with stats + loss.
pub fn latency_monitor(
    addr: SocketAddr,
    samples: u32,
    interval_ms: u64,
) -> (Vec<f64>, Option<stats::LatencySummary>, f64) {
    let mut rtts = Vec::new();
    for _ in 0..samples.max(1) {
        if let Ok(ms) = connect_ms(addr) {
            rtts.push(ms);
        }
        std::thread::sleep(Duration::from_millis(interval_ms.clamp(20, 5000)));
    }
    let loss_pct = 100.0 * (1.0 - rtts.len() as f64 / f64::from(samples.max(1)));
    let summary = stats::summarize(&rtts);
    (rtts, summary, loss_pct)
}

/// Jitter burst: 20 rapid connects; IQR is the headline number.
pub fn jitter_burst(addr: SocketAddr) -> (f64, Option<f64>) {
    let (rtts, summary, _) = latency_monitor(addr, 20, 25);
    let sorted = {
        let mut s = rtts.clone();
        s.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        s
    };
    let q1 = stats::percentile(&sorted, 25.0).unwrap_or(0.0);
    let q3 = stats::percentile(&sorted, 75.0).unwrap_or(0.0);
    (q3 - q1, summary.map(|s| s.jitter_ms))
}

/// ICMP ping via the OS binary; returns parsed rtt_ms. Target MUST pass
/// `valid_target` (callers enforce; the test asserts the guard).
pub fn icmp_ping(host: &str) -> Result<f64, String> {
    if !valid_target(host) {
        return Err("invalid target".into());
    }
    let args: Vec<String> = if cfg!(windows) {
        ["ping", "-n", "1", "-w", "2000", host]
    } else {
        ["ping", "-c", "1", "-W", "2", host]
    }
    .iter()
    .map(ToString::to_string)
    .collect();
    let refs: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let out = exec::run("ping", &refs, Duration::from_secs(6));
    if !out.success {
        return Err(out.combined.trim().chars().take(200).collect());
    }
    parse_ping_rtt(&out.combined).ok_or_else(|| "no timing line in ping output".to_string())
}

/// Extract "time=X ms" from ping output (pure; fixture-tested).
pub fn parse_ping_rtt(output: &str) -> Option<f64> {
    let lower = output.to_lowercase();
    let idx = lower.find("time=")?;
    let rest = &output[idx + 5..];
    let num: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.parse().ok()
}

/// The default gateway address, parsed per OS.
pub fn default_gateway() -> Option<String> {
    if cfg!(windows) {
        let out = exec::run("route", &["print", "0.0.0.0"], Duration::from_secs(10));
        exec::extract_ipv4(&out.stdout)
            .into_iter()
            .find(|ip| ip != "0.0.0.0")
    } else if cfg!(target_os = "macos") {
        let out = exec::run("route", &["-n", "get", "default"], Duration::from_secs(10));
        out.stdout
            .lines()
            .find(|l| l.trim().starts_with("gateway:"))
            .and_then(|l| l.split_whitespace().last())
            .filter(|s| s.parse::<std::net::Ipv4Addr>().is_ok())
            .map(ToString::to_string)
    } else {
        let out = exec::run("ip", &["route", "show", "default"], Duration::from_secs(10));
        out.stdout.lines().find_map(|l| {
            let toks: Vec<&str> = l.split_whitespace().collect();
            toks.iter()
                .position(|t| *t == "via")
                .and_then(|i| toks.get(i + 1))
                .map(|s| (*s).to_string())
        })
    }
}

/// IPv6 readiness: AAAA resolution + a real v6 TCP connect.
pub fn ipv6_readiness(host: &str) -> (bool, String) {
    let resolved: Vec<IpAddr> = format!("{host}:443")
        .to_socket_addrs()
        .map(|it| it.map(|sa| sa.ip()).collect())
        .unwrap_or_default();
    let has_aaaa = resolved.iter().any(|ip| ip.is_ipv6());
    if !has_aaaa {
        return (
            false,
            "no AAAA record: this destination is IPv4-only".into(),
        );
    }
    let v6 = resolved.iter().find(|ip| ip.is_ipv6()).and_then(|ip| {
        let addr = SocketAddr::new(*ip, 443);
        connect_ms(addr).ok().map(|ms| (addr, ms))
    });
    match v6 {
        Some((addr, ms)) => (true, format!("IPv6 works: {ms:.1} ms to {addr}")),
        None => (
            false,
            "AAAA exists but IPv6 connect failed (no v6 route or blocked)".into(),
        ),
    }
}

/// Common service ports for the scanner default.
pub const COMMON_PORTS: &[u16] = &[
    21, 22, 23, 25, 53, 80, 110, 123, 143, 443, 445, 465, 587, 993, 995, 1433, 1521, 3306, 3389,
    5432, 5900, 6379, 8080, 8443, 9100, 25565, 27017,
];

/// Hard cap per scan: this is a diagnostic tool, not a mass scanner.
pub const MAX_PORTS_PER_SCAN: usize = 256;

/// TCP connect scan of `ports` on `host` (bounded by MAX_PORTS_PER_SCAN).
pub fn port_scan(host: &str, ports: &[u16]) -> PortScanReport {
    let ports = &ports[..ports.len().min(MAX_PORTS_PER_SCAN)];
    let mut results = Vec::new();
    let mut open_count = 0;
    for port in ports {
        let addr = resolve_one(host, *port);
        let r = match addr {
            Some(addr) => match connect_ms(addr) {
                Ok(ms) => {
                    open_count += 1;
                    PortResult {
                        port: *port,
                        open: true,
                        latency_ms: Some(ms),
                    }
                }
                Err(_) => PortResult {
                    port: *port,
                    open: false,
                    latency_ms: None,
                },
            },
            None => PortResult {
                port: *port,
                open: false,
                latency_ms: None,
            },
        };
        results.push(r);
    }
    PortScanReport {
        target: host.into(),
        ports: results,
        open_count,
        scanned: ports.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_validation_blocks_command_injection_shapes() {
        assert!(valid_target("example.com"));
        assert!(valid_target("192.168.1.1"));
        assert!(valid_target("::1"));
        assert!(valid_target("a-b.example.co.uk"));
        assert!(!valid_target(""));
        assert!(!valid_target("example.com;rm -rf"));
        assert!(!valid_target("$(whoami)"));
        assert!(!valid_target("8.8.8.8/24"));
        assert!(!valid_target("a b"));
        assert!(!valid_target("-flag.example"));
    }

    #[test]
    fn parses_ping_time_lines_across_os() {
        assert_eq!(
            parse_ping_rtt("64 bytes from 1.1.1.1: icmp_seq=1 ttl=58 time=12.3 ms"),
            Some(12.3)
        );
        assert_eq!(
            parse_ping_rtt("Reply from 1.1.1.1: bytes=32 time=8ms TTL=57"),
            Some(8.0)
        );
        assert_eq!(parse_ping_rtt("Request timed out."), None);
    }

    #[test]
    fn icmp_ping_loopback_is_real() {
        if !exec::on_path("ping") {
            return;
        }
        let ms = icmp_ping("127.0.0.1").expect("loopback ping");
        assert!(ms < 100.0, "{ms}");
    }

    #[test]
    fn latency_monitor_measures_a_real_listener() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (rtts, summary, loss) = latency_monitor(addr, 4, 10);
        assert_eq!(rtts.len(), 4);
        let s = summary.expect("summary");
        assert!(s.median_ms < 500.0);
        assert_eq!(loss, 0.0);
    }

    #[test]
    fn jitter_burst_returns_iqr() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let (iqr, mad) = jitter_burst(l.local_addr().unwrap());
        assert!(iqr >= 0.0);
        assert!(mad.unwrap_or(0.0) >= 0.0);
    }

    #[test]
    fn port_scan_finds_open_ports() {
        let a = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let b = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let pa = a.local_addr().unwrap().port();
        let pb = b.local_addr().unwrap().port();
        let report = port_scan("127.0.0.1", &[pa, pb, 1]);
        assert_eq!(report.open_count, 2);
        assert_eq!(report.scanned, 3);
        assert!(report.ports.iter().any(|p| p.port == pa && p.open));
        assert!(report.ports.iter().any(|p| p.port == 1 && !p.open));
    }

    #[test]
    fn port_scan_caps_total_ports() {
        let many: Vec<u16> = (1..=1000u32).map(|p| p as u16).collect();
        let report = port_scan("127.0.0.1", &many);
        assert_eq!(report.scanned as usize, MAX_PORTS_PER_SCAN);
    }

    #[test]
    fn ipv6_readiness_is_honest_for_localhost() {
        let (ok, detail) = ipv6_readiness("localhost");
        if ok {
            assert!(detail.contains("IPv6 works"));
        }
        // Either way the explanation is concrete.
        assert_ne!(detail, "");
    }
}
