//! DNS resolver benchmark and OS DNS application.
//!
//! All measurement is real: a hand-rolled minimal DNS client sends A
//! queries over UDP and times the responses. "Cached" probes use a
//! popular domain; "uncached" probes use a fresh random label under a
//! stable zone so the resolver must leave its cache. Applying the winner
//! uses the platform's supported command (`netsh` / `networksetup` /
//! `nmcli`/`resolvectl`), capturing previous values for one-click
//! restore. Never touches loopback interfaces.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::{DnsApplyReport, DnsBenchmarkReport, DnsResolverResult};

use crate::exec;
use crate::stats::{self, Prng};

const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);
const CACHED_DOMAIN: &str = "www.cloudflare.com";
const UNCACHED_ZONE: &str = "net";

/// Public recursive resolvers used when the user has no custom list.
pub fn default_servers() -> Vec<(String, String)> {
    vec![
        ("1.1.1.1".into(), "Cloudflare".into()),
        ("1.0.0.1".into(), "Cloudflare".into()),
        ("8.8.8.8".into(), "Google".into()),
        ("8.8.4.4".into(), "Google".into()),
        ("9.9.9.9".into(), "Quad9".into()),
        ("149.112.112.112".into(), "Quad9".into()),
        ("208.67.222.222".into(), "OpenDNS".into()),
        ("208.67.220.220".into(), "OpenDNS".into()),
    ]
}

/// Build a minimal A-query packet for `domain` with the given id.
pub fn build_query(id: u16, domain: &str) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(domain.len() + 18);
    pkt.extend_from_slice(&id.to_be_bytes());
    pkt.extend_from_slice(&[0x01, 0x00]); // recursion desired
    pkt.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // qd=1, an/ns/ar=0
    for label in domain.split('.') {
        pkt.push(u8::try_from(label.len()).unwrap_or(0));
        pkt.extend_from_slice(label.as_bytes());
    }
    pkt.push(0);
    pkt.extend_from_slice(&[0, 1, 0, 1]); // A, IN
    pkt
}

/// Skip a (possibly compressed) domain name starting at `off`; returns
/// the offset just past the name.
pub(crate) fn skip_name(buf: &[u8], off: usize) -> Option<usize> {
    let mut pos = off;
    let mut jumps = 0;
    loop {
        let len = *buf.get(pos)?;
        if len & 0xC0 == 0xC0 {
            // compression pointer: name ends here
            return Some(pos + 2);
        }
        if len & 0xC0 != 0 {
            return None; // unsupported label type
        }
        pos += 1 + len as usize;
        if len == 0 {
            return Some(pos);
        }
        jumps += 1;
        if jumps > 128 {
            return None; // compression loop guard
        }
    }
}

/// Parse a DNS response: returns (rcode, answers, first_ttl).
pub fn parse_response(buf: &[u8]) -> Option<(u8, u16, u32)> {
    if buf.len() < 12 {
        return None;
    }
    let qd = u16::from_be_bytes([buf[4], buf[5]]);
    let an = u16::from_be_bytes([buf[6], buf[7]]);
    let rcode = buf[3] & 0x0F;
    let mut pos = 12;
    for _ in 0..qd {
        pos = skip_name(buf, pos)?;
        pos += 4; // qtype + qclass
    }
    let mut first_ttl = None;
    for _ in 0..an {
        pos = skip_name(buf, pos)?;
        let rtype = u16::from_be_bytes([*buf.get(pos)?, *buf.get(pos + 1)?]);
        pos += 2; // TYPE
        pos += 2; // CLASS
        let ttl = u32::from_be_bytes([
            *buf.get(pos)?,
            *buf.get(pos + 1)?,
            *buf.get(pos + 2)?,
            *buf.get(pos + 3)?,
        ]);
        pos += 4; // TTL
        let rdlen = u16::from_be_bytes([*buf.get(pos)?, *buf.get(pos + 1)?]) as usize;
        pos += 2 + rdlen; // RDLENGTH + RDATA
        if rtype == 1 && first_ttl.is_none() {
            first_ttl = Some(ttl);
        }
    }
    Some((rcode, an, first_ttl.unwrap_or(0)))
}

/// One query attempt with a single retry (UDP resolvers occasionally
/// drop a datagram; that must not fail the whole benchmark).
pub fn measure_once(server: SocketAddr, domain: &str, id: u16) -> Result<Duration, String> {
    let bind: SocketAddr = if server.is_ipv6() {
        "[::]:0".parse().expect("valid bind")
    } else {
        "0.0.0.0:0".parse().expect("valid bind")
    };
    let sock = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
    sock.connect(server).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(QUERY_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let query = build_query(id, domain);
    let start = Instant::now();
    let send_result = sock.send(&query).map_err(|e| e.to_string())?;
    if send_result == 0 {
        return Err("empty send".into());
    }
    let mut buf = [0u8; 1500];
    loop {
        let n = sock.recv(&mut buf).map_err(|e| e.to_string())?;
        if n < 12 {
            return Err("short response".into());
        }
        let resp_id = u16::from_be_bytes([buf[0], buf[1]]);
        if resp_id != id {
            continue; // stale datagram from a previous query
        }
        let _ = parse_response(&buf[..n]).ok_or("malformed response")?;
        return Ok(start.elapsed());
    }
}

fn try_measure(server: SocketAddr, domain: &str, id: u16) -> Result<Duration, String> {
    let mut last_err = String::new();
    for _ in 0..2 {
        match measure_once(server, domain, id) {
            Ok(d) => return Ok(d),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn median_rtt(
    server: SocketAddr,
    domain: &str,
    samples: u32,
    prng: &mut Prng,
) -> Result<f64, String> {
    let mut rtts = Vec::new();
    for _ in 0..samples {
        let id = (prng.next_u64() & 0xFFFF) as u16;
        let d = try_measure(server, domain, id)?;
        rtts.push(d.as_secs_f64() * 1000.0);
    }
    rtts.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    stats::median(&rtts).ok_or_else(|| "no samples".to_string())
}

/// Resolve a server spec: "ip" (port 53) or "ip:port".
fn parse_server(addr: &str) -> Result<SocketAddr, String> {
    if let Ok(sa) = addr.parse::<SocketAddr>() {
        return Ok(sa);
    }
    let ip: std::net::IpAddr = addr.parse().map_err(|e| format!("invalid address: {e}"))?;
    Ok(SocketAddr::new(ip, 53))
}

/// Benchmark one resolver: cached + uncached-path medians and a score.
pub fn benchmark_server(addr: &str, label: &str) -> DnsResolverResult {
    let server = match parse_server(addr) {
        Ok(s) => s,
        Err(e) => {
            return DnsResolverResult {
                server: addr.into(),
                label: label.into(),
                success: false,
                cached_ms: None,
                uncached_ms: None,
                score: None,
                error: Some(e),
            };
        }
    };
    let mut prng = Prng::from_clock();

    let cached = median_rtt(server, CACHED_DOMAIN, 3, &mut prng);
    let uncached_domain = format!("{}.{UNCACHED_ZONE}", prng.hex(16));
    let uncached = median_rtt(server, &uncached_domain, 2, &mut prng);

    match (cached, uncached) {
        (Ok(c), Ok(u)) => DnsResolverResult {
            server: addr.into(),
            label: label.into(),
            success: true,
            cached_ms: Some(c),
            uncached_ms: Some(u),
            score: Some(0.6 * c + 0.4 * u),
            error: None,
        },
        (Err(e), _) | (_, Err(e)) => DnsResolverResult {
            server: addr.into(),
            label: label.into(),
            success: false,
            cached_ms: None,
            uncached_ms: None,
            score: None,
            error: Some(e),
        },
    }
}

/// Resolver(s) the OS currently uses, detected (never hardcoded).
pub fn system_resolvers() -> Vec<String> {
    if cfg!(windows) {
        let out = exec::run(
            "netsh",
            &["interface", "ipv4", "show", "dnsservers"],
            Duration::from_secs(10),
        );
        exec::extract_ipv4(&out.combined)
    } else if cfg!(target_os = "macos") {
        let out = exec::run("scutil", &["--dns"], Duration::from_secs(10));
        exec::extract_ipv4(&out.combined)
    } else {
        match std::fs::read_to_string("/etc/resolv.conf") {
            Ok(text) => exec::extract_ipv4(&text),
            Err(_) => Vec::new(),
        }
    }
}

/// Benchmark defaults + system resolvers and rank them.
pub fn bench_default() -> DnsBenchmarkReport {
    let system = system_resolvers();
    let mut servers = default_servers();
    for s in &system {
        if !servers.iter().any(|(ip, _)| ip == s) {
            servers.push((s.clone(), "system".into()));
        }
    }

    let mut results: Vec<DnsResolverResult> = servers
        .into_iter()
        .map(|(ip, label)| benchmark_server(&ip, &label))
        .collect();

    results.sort_by(|a, b| match (&a.score, &b.score) {
        (Some(x), Some(y)) => x.partial_cmp(y).expect("finite scores"),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    let recommended = results
        .first()
        .filter(|r| r.success)
        .map(|r| r.server.clone());

    DnsBenchmarkReport {
        results,
        recommended,
        system_resolvers: system,
    }
}

fn capture_previous_windows(iface: &str) -> Vec<String> {
    let name_arg = format!("name={iface}");
    let out = exec::run(
        "netsh",
        &["interface", "ipv4", "show", "dnsservers", &name_arg],
        Duration::from_secs(10),
    );
    exec::extract_ipv4(&out.stdout)
        .into_iter()
        .filter(|ip| ip != "127.0.0.1")
        .collect()
}

fn windows_connected_interfaces() -> Vec<String> {
    let out = exec::run(
        "netsh",
        &["interface", "ipv4", "show", "interfaces"],
        Duration::from_secs(10),
    );
    let mut names = Vec::new();
    for line in out.stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // Idx Met MTU State Name…  → connected rows have 5+ columns.
        if cols.len() >= 5 && cols[3].eq_ignore_ascii_case("connected") {
            let name = cols[4..].join(" ");
            if !name.eq_ignore_ascii_case("loopback")
                && !name.eq_ignore_ascii_case("loopback pseudo-interface 1")
            {
                names.push(name);
            }
        }
    }
    names
}

fn apply_windows(servers: &[IpAddr], iface: Option<&str>) -> DnsApplyReport {
    let interface = match iface {
        Some(i) => i.to_string(),
        None => match windows_connected_interfaces().first() {
            Some(n) => n.clone(),
            None => {
                return DnsApplyReport {
                    interface: None,
                    servers: servers.iter().map(ToString::to_string).collect(),
                    previous: vec![],
                    outcome: "failed".into(),
                    detail: "no connected non-loopback interface found; specify one".into(),
                };
            }
        },
    };

    let previous = capture_previous_windows(&interface);
    // netsh parses its own parameter string; embedded quotes handle
    // interface names with spaces ("Ethernet 2").
    let quoted = format!("name=\"{interface}\"");
    let mut commands: Vec<Vec<String>> = Vec::new();
    for (n, ip) in servers.iter().enumerate() {
        let (verb, index): (&str, String) = if n == 0 {
            ("set", String::new())
        } else {
            ("add", format!("index={}", n + 1))
        };
        let mut cmd: Vec<String> = ["netsh", "interface", "ipv4", verb, "dns"]
            .iter()
            .map(ToString::to_string)
            .collect();
        cmd.push(quoted.clone());
        if n == 0 {
            cmd.push("static".into());
        }
        cmd.push(ip.to_string());
        if !index.is_empty() {
            cmd.push(index);
        }
        commands.push(cmd);
    }

    if !exec::is_elevated() {
        return DnsApplyReport {
            interface: Some(interface),
            servers: servers.iter().map(ToString::to_string).collect(),
            previous,
            outcome: "needs_elevation".into(),
            detail: format!(
                "run as administrator: {}",
                commands.first().map(|c| c.join(" ")).unwrap_or_default()
            ),
        };
    }

    for args in &commands {
        let refs: Vec<&str> = args[1..].iter().map(String::as_str).collect();
        let out = exec::run(&args[0], &refs, Duration::from_secs(15));
        if !out.success {
            return DnsApplyReport {
                interface: Some(interface),
                servers: servers.iter().map(ToString::to_string).collect(),
                previous,
                outcome: "failed".into(),
                detail: out.combined.trim().chars().take(300).collect(),
            };
        }
    }

    DnsApplyReport {
        interface: Some(interface.clone()),
        servers: servers.iter().map(ToString::to_string).collect(),
        previous,
        outcome: "applied".into(),
        detail: format!("DNS set on {interface}; previous servers recorded for restore"),
    }
}

fn apply_macos(servers: &[IpAddr], iface: Option<&str>) -> DnsApplyReport {
    let service = if let Some(s) = iface {
        s.to_string()
    } else {
        let out = exec::run(
            "networksetup",
            &["-listallnetworkservices"],
            Duration::from_secs(10),
        );
        out.stdout
            .lines()
            .skip(1) // header
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('*'))
            .unwrap_or("Wi-Fi")
            .to_string()
    };

    let prev_out = exec::run(
        "networksetup",
        &["-getdnsservers", &service],
        Duration::from_secs(10),
    );
    let previous = exec::extract_ipv4(&prev_out.stdout);

    let joined = servers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let out = exec::run(
        "networksetup",
        &["-setdnsservers", &service, &joined],
        Duration::from_secs(15),
    );
    DnsApplyReport {
        interface: Some(service),
        servers: servers.iter().map(ToString::to_string).collect(),
        previous,
        outcome: if out.success { "applied" } else { "failed" }.into(),
        detail: if out.success {
            "networksetup updated resolver list".into()
        } else {
            out.combined.trim().chars().take(300).collect()
        },
    }
}

fn default_linux_interface() -> Option<String> {
    let out = exec::run("ip", &["route", "show", "default"], Duration::from_secs(10));
    out.stdout
        .lines()
        .find_map(|l| {
            l.split_whitespace().position(|t| t == "dev").map(|i| {
                l.split_whitespace()
                    .nth(i + 1)
                    .unwrap_or_default()
                    .to_string()
            })
        })
        .filter(|d| !d.is_empty())
}

fn apply_linux(servers: &[IpAddr], iface: Option<&str>) -> DnsApplyReport {
    let interface = iface
        .map(ToString::to_string)
        .or_else(default_linux_interface);
    let Some(interface) = interface else {
        return DnsApplyReport {
            interface: None,
            servers: servers.iter().map(ToString::to_string).collect(),
            previous: vec![],
            outcome: "failed".into(),
            detail: "no default-route interface found; specify one".into(),
        };
    };

    let joined = servers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");

    if exec::on_path("resolvectl") {
        let out = exec::run(
            "resolvectl",
            &["dns", &interface, &joined],
            Duration::from_secs(15),
        );
        let previous = exec::extract_ipv4(
            &exec::run(
                "resolvectl",
                &["status", &interface],
                Duration::from_secs(10),
            )
            .stdout,
        )
        .into_iter()
        .filter(|ip| ip != "127.0.0.53")
        .collect();
        return DnsApplyReport {
            interface: Some(interface),
            servers: servers.iter().map(ToString::to_string).collect(),
            previous,
            outcome: if out.success { "applied" } else { "failed" }.into(),
            detail: if out.success {
                "resolvectl per-link DNS set".into()
            } else {
                out.combined.trim().chars().take(300).collect()
            },
        };
    }

    if exec::on_path("nmcli") {
        let active = exec::run(
            "nmcli",
            &["-t", "-f", "NAME", "con", "show", "--active"],
            Duration::from_secs(10),
        );
        let Some(con) = active.stdout.lines().next().map(str::to_string) else {
            return DnsApplyReport {
                interface: Some(interface),
                servers: servers.iter().map(ToString::to_string).collect(),
                previous: vec![],
                outcome: "failed".into(),
                detail: "no active NetworkManager connection".into(),
            };
        };
        let prev = exec::run(
            "nmcli",
            &["-g", "ipv4.dns", "con", "show", &con],
            Duration::from_secs(10),
        )
        .stdout;
        let previous = exec::extract_ipv4(&prev);
        let out = exec::run(
            "nmcli",
            &[
                "con",
                "mod",
                &con,
                "ipv4.dns",
                &joined,
                "ipv4.ignore-auto-dns",
                "yes",
            ],
            Duration::from_secs(15),
        );
        let _ = exec::run("nmcli", &["con", "up", &con], Duration::from_secs(20));
        return DnsApplyReport {
            interface: Some(interface),
            servers: servers.iter().map(ToString::to_string).collect(),
            previous,
            outcome: if out.success { "applied" } else { "failed" }.into(),
            detail: if out.success {
                format!("NetworkManager connection {con} updated")
            } else {
                out.combined.trim().chars().take(300).collect()
            },
        };
    }

    DnsApplyReport {
        interface: Some(interface),
        servers: servers.iter().map(ToString::to_string).collect(),
        previous: vec![],
        outcome: "unavailable".into(),
        detail: "neither resolvectl nor nmcli present; edit /etc/resolv.conf manually".into(),
    }
}

/// Apply `servers` as the system resolver list. Fails safe: loopback is
/// never selected as target, previous values are captured first, and the
/// report distinguishes applied / needs_elevation / failed / unavailable.
pub fn apply(servers: &[IpAddr], interface: Option<&str>) -> DnsApplyReport {
    let servers: Vec<IpAddr> = servers
        .iter()
        .copied()
        .filter(|ip| !ip.is_loopback())
        .collect();
    if servers.is_empty() {
        return DnsApplyReport {
            interface: interface.map(ToString::to_string),
            servers: vec![],
            previous: vec![],
            outcome: "failed".into(),
            detail: "no non-loopback server addresses given".into(),
        };
    }
    if cfg!(windows) {
        apply_windows(&servers, interface)
    } else if cfg!(target_os = "macos") {
        apply_macos(&servers, interface)
    } else {
        apply_linux(&servers, interface)
    }
}

/// Restore helper used by the UI's undo affordance.
pub fn restore(previous: &[String], interface: Option<&str>) -> DnsApplyReport {
    let ips: Vec<IpAddr> = previous.iter().filter_map(|s| s.parse().ok()).collect();
    if ips.is_empty() {
        // "Empty DNS" means DHCP/auto on all platforms.
        if let Some(iface) = interface {
            if cfg!(windows) {
                let out = exec::run(
                    "netsh",
                    &[
                        "interface",
                        "ipv4",
                        "set",
                        "dns",
                        &format!("name={iface}"),
                        "dhcp",
                    ],
                    Duration::from_secs(15),
                );
                return DnsApplyReport {
                    interface: Some(iface.into()),
                    servers: vec!["dhcp".into()],
                    previous: vec![],
                    outcome: if out.success { "applied" } else { "failed" }.into(),
                    detail: "reverted to DHCP DNS".into(),
                };
            }
            if cfg!(target_os = "macos") {
                let out = exec::run(
                    "networksetup",
                    &["-setdnsservers", iface, "Empty"],
                    Duration::from_secs(15),
                );
                return DnsApplyReport {
                    interface: Some(iface.into()),
                    servers: vec!["automatic".into()],
                    previous: vec![],
                    outcome: if out.success { "applied" } else { "failed" }.into(),
                    detail: "reverted to automatic DNS".into(),
                };
            }
        }
    }
    apply(&ips, interface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_packet_shape_is_valid_dns() {
        let q = build_query(0xABCD, "example.com");
        assert_eq!(&q[0..2], &[0xAB, 0xCD]);
        let expect: Vec<u8> = vec![
            0xAB, 0xCD, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0, 7, b'e', b'x', b'a', b'm', b'p', b'l',
            b'e', 3, b'c', b'o', b'm', 0, 0, 1, 0, 1,
        ];
        assert_eq!(q, expect);
    }

    #[test]
    fn parses_response_with_compressed_answer_name() {
        let q = build_query(7, "a.com");
        let mut resp = q.clone();
        resp[2] = 0x81; // response, recursion ok
        resp[3] = 0x80; // rcode 0
        resp[7] = 1; // 1 answer
        let mut answer: Vec<u8> = Vec::new();
        answer.extend_from_slice(&[0xC0, 0x0C]); // pointer to question name
        answer.extend_from_slice(&[0, 1, 0, 1]); // A IN
        answer.extend_from_slice(&60u32.to_be_bytes()); // ttl
        answer.extend_from_slice(&[0, 4, 1, 2, 3, 4]); // rdlen + rdata
        resp.extend_from_slice(&answer);
        let (rcode, an, ttl) = parse_response(&resp).expect("parse");
        assert_eq!(rcode, 0);
        assert_eq!(an, 1);
        assert_eq!(ttl, 60);
    }

    #[test]
    fn malformed_response_is_rejected() {
        assert!(parse_response(&[0u8; 5]).is_none());
    }

    #[test]
    fn resolver_ranking_orders_by_score_then_failures() {
        // Pure ranking check: no network involved.
        let results = vec![
            DnsResolverResult {
                server: "8.8.8.8".into(),
                label: "Google".into(),
                success: true,
                cached_ms: Some(20.0),
                uncached_ms: Some(40.0),
                score: Some(28.0),
                error: None,
            },
            DnsResolverResult {
                server: "1.1.1.1".into(),
                label: "Cloudflare".into(),
                success: true,
                cached_ms: Some(8.0),
                uncached_ms: Some(30.0),
                score: Some(16.8),
                error: None,
            },
            DnsResolverResult {
                server: "203.0.113.9".into(),
                label: "dead".into(),
                success: false,
                cached_ms: None,
                uncached_ms: None,
                score: None,
                error: Some("timed out".into()),
            },
        ];
        let mut report = DnsBenchmarkReport {
            results,
            recommended: None,
            system_resolvers: vec![],
        };
        report.results.sort_by(|a, b| match (&a.score, &b.score) {
            (Some(x), Some(y)) => x.partial_cmp(y).expect("finite"),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        report.recommended = report
            .results
            .first()
            .filter(|r| r.success)
            .map(|r| r.server.clone());
        assert_eq!(report.results[0].server, "1.1.1.1");
        assert_eq!(report.recommended.as_deref(), Some("1.1.1.1"));
        assert!(!report.results.last().unwrap().success);
    }

    #[test]
    fn invalid_server_address_fails_fast() {
        let bad = benchmark_server("999.999.1.1", "bogus");
        assert!(!bad.success);
        assert!(bad.error.is_some());
        assert!(bad.error.as_deref().unwrap().contains("invalid address"));
    }

    #[test]
    fn apply_refuses_loopback_only_lists() {
        let r = apply(&["127.0.0.1".parse().unwrap()], None);
        assert_eq!(r.outcome, "failed");
        assert!(r.detail.contains("loopback"));
    }

    #[test]
    fn live_udp_bench_against_local_dns_server() {
        // A real UDP DNS responder on localhost; the client code path is
        // exactly what runs against 1.1.1.1 in production.
        let sock = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let port = sock.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            // Serve the samples the benchmark sends, then exit on a
            // short read timeout so the thread never blocks the join.
            sock.set_read_timeout(Some(Duration::from_millis(1500)))
                .expect("timeout");
            loop {
                let Ok((n, peer)) = sock.recv_from(&mut buf) else {
                    return;
                };
                let mut resp = buf[..n].to_vec();
                resp[2] = 0x81;
                resp[3] = 0x80;
                resp[7] = 1;
                resp.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1]);
                resp.extend_from_slice(&60u32.to_be_bytes());
                resp.extend_from_slice(&[0, 4, 93, 184, 216, 34]);
                let _ = sock.send_to(&resp, peer);
            }
        });
        let r = benchmark_server(&format!("127.0.0.1:{port}"), "local-test");
        assert!(r.success, "error: {:?}", r.error);
        let cached = r.cached_ms.expect("cached measured");
        assert!(cached < 1000.0, "localhost RTT must be tiny, got {cached}");
        server.join().expect("server thread");
    }
}
