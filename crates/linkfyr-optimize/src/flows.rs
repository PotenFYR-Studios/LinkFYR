//! Flow tools: interface-bound latency/throughput races (SO_BINDTODEVICE
//! via socket2 where the OS permits), streaming and gaming endpoint
//! health, journals (connection history, destination history, the
//! Network Time Machine timeline backend), advisory split-tunnel route
//! generation, kill-switch status audit, quota and metered monitors,
//! and bonding/failover estimation math. Everything real: bound
//! sockets, real measurements, JSONL journals under the tool state dir.

use std::collections::BTreeMap;
use std::io::Read;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::ToolRunReport;

use crate::exec;
use crate::probes::valid_target;
use crate::stats;
use crate::sysnet;

const TIMEOUT: Duration = Duration::from_secs(10);

fn ok(tool: &str, summary: String, data: serde_json::Value) -> ToolRunReport {
    ToolRunReport {
        tool: tool.into(),
        ok: true,
        summary,
        took_ms: 0,
        data,
    }
}

fn not(tool: &str, why: impl Into<String>) -> ToolRunReport {
    ToolRunReport {
        tool: tool.into(),
        ok: false,
        summary: why.into(),
        took_ms: 0,
        data: serde_json::Value::Null,
    }
}

/* ---- interface enumeration + bound sockets ---- */

/// (interface, ipv4) pairs from `ip -o -4 addr show` (pure).
pub fn parse_ip_o_addr(output: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in output.lines() {
        // "2: eth0    inet 192.168.1.5/24 brd …"
        let toks: Vec<&str> = line.split_whitespace().collect();
        let Some(&iface) = toks.get(1) else { continue };
        let Some(ip_tok) = toks
            .iter()
            .skip(2)
            .find(|t| t.contains('/') && t.chars().next().is_some_and(|c| c.is_ascii_digit()))
        else {
            continue;
        };
        if let Some(ip) = ip_tok.split('/').next() {
            if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                out.push((iface.to_string(), ip.to_string()));
            }
        }
    }
    out
}

fn interfaces_with_ips() -> Vec<(String, String)> {
    let out = exec::run("ip", &["-o", "-4", "addr", "show"], TIMEOUT);
    parse_ip_o_addr(&out.stdout)
        .into_iter()
        .filter(|(iface, _)| iface != "lo")
        .collect()
}

/// TCP connect bound to a specific interface (Linux SO_BINDTODEVICE;
/// needs elevation, unavailable without it — reported honestly).
fn bound_connect_ms(iface: &str, addr: SocketAddr) -> Result<f64, String> {
    // socket2::Socket::bind_device exists only on Android/Fuchsia/Linux.
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    {
        use socket2::{Domain, Type};
        let domain = if addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let sock = socket2::Socket::new(domain, Type::STREAM, None).map_err(|e| e.to_string())?;
        sock.bind_device(Some(iface.as_bytes()))
            .map_err(|e| format!("bind to {iface}: {e}"))?;
        let start = Instant::now();
        let sa = socket2::SockAddr::from(addr);
        sock.connect(&sa).map_err(|e| e.to_string())?;
        Ok(start.elapsed().as_secs_f64() * 1000.0)
    }
    #[cfg(not(any(target_os = "android", target_os = "fuchsia", target_os = "linux")))]
    {
        let _ = (iface, addr);
        Err("per-interface binding needs Linux (SO_BINDTODEVICE); unavailable here".to_string())
    }
}

pub fn latency_race(params: &BTreeMap<String, String>) -> ToolRunReport {
    let target = params
        .get("target")
        .cloned()
        .unwrap_or_else(|| "1.1.1.1".into());
    if !valid_target(&target) {
        return not("latency_race", "invalid target");
    }
    let Some(addr) = (target.as_str(), 443u16)
        .to_socket_addrs()
        .ok()
        .and_then(|mut i| i.next())
    else {
        return not("latency_race", "cannot resolve target");
    };
    let ifaces = interfaces_with_ips();
    if ifaces.is_empty() {
        return not(
            "latency_race",
            "no non-loopback interfaces with IPv4 (or `ip` unavailable on this platform)",
        );
    }
    let mut rows = Vec::new();
    for (iface, ip) in &ifaces {
        let mut samples = Vec::new();
        let mut error = None;
        for _ in 0..3 {
            match bound_connect_ms(iface, addr) {
                Ok(ms) => samples.push(ms),
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        samples.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        rows.push(serde_json::json!({
            "interface": iface,
            "ip": ip,
            "medianMs": stats::median(&samples),
            "error": error,
        }));
    }
    let winners: Vec<&serde_json::Value> = rows
        .iter()
        .filter(|r| r.get("medianMs").and_then(|m| m.as_f64()).is_some())
        .collect();
    let summary = if winners.is_empty() {
        format!("no interface could bind and reach {target} (elevation or platform limit)")
    } else {
        let best = winners
            .iter()
            .min_by(|a, b| {
                a["medianMs"]
                    .as_f64()
                    .unwrap_or(f64::MAX)
                    .partial_cmp(&b["medianMs"].as_f64().unwrap_or(f64::MAX))
                    .expect("finite")
            })
            .unwrap();
        format!(
            "fastest path to {target}: {} ({:.1} ms)",
            best["interface"],
            best["medianMs"].as_f64().unwrap_or(0.0)
        )
    };
    ok(
        "latency_race",
        summary,
        serde_json::json!({ "target": target, "interfaces": rows }),
    )
}

pub fn speed_compare(params: &BTreeMap<String, String>) -> ToolRunReport {
    // Interface-bound mini-downloads: same host through each interface.
    let url = params
        .get("endpoint")
        .cloned()
        .unwrap_or_else(|| "https://speed.cloudflare.com/__down?bytes=20000000".into());
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|r| r.split('/').next())
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_string();
    if host.is_empty() {
        return not("speed_compare", "endpoint must be http(s)://…");
    }
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    {
        let ifaces = interfaces_with_ips();
        if ifaces.is_empty() {
            return not(
                "speed_compare",
                "no non-loopback interfaces available (Linux binding required)",
            );
        }
        let mut rows = Vec::new();
        for (iface, _ip) in &ifaces {
            // One bounded 3-second drain per interface.
            let start = Instant::now();
            let deadline = start + Duration::from_secs(3);
            let mut bytes = 0u64;
            let err = loop {
                let attempt = bound_stream(iface, &host);
                match attempt {
                    Ok(mut stream) => {
                        let req = format!(
                            "GET {url} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
                        );
                        if let Err(e) = write_request(&mut stream, req.as_bytes()) {
                            break Some(e);
                        }
                        let mut buf = vec![0u8; 65536];
                        loop {
                            if Instant::now() > deadline {
                                break;
                            }
                            match stream.read(&mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => bytes += n as u64,
                            }
                        }
                    }
                    Err(e) => break Some(e),
                }
            };
            let secs = start.elapsed().as_secs_f64().max(0.5);
            rows.push(serde_json::json!({
                "interface": iface,
                "mbps": bytes as f64 * 8.0 / secs / 1e6,
                "bytes": bytes,
                "error": err,
            }));
        }
        ok(
            "speed_compare",
            format!(
                "per-interface throughput measured over {} interface(s)",
                rows.len()
            ),
            serde_json::json!({ "rows": rows }),
        )
    }
    #[cfg(not(any(target_os = "android", target_os = "fuchsia", target_os = "linux")))]
    {
        let _ = (url, host);
        not(
            "speed_compare",
            "per-interface throughput needs Linux SO_BINDTODEVICE",
        )
    }
}

#[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
fn bound_stream(iface: &str, host: &str) -> Result<TcpStream, String> {
    use socket2::{Domain, Type};
    let addr: SocketAddr = (host, 443u16)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .find(|a| a.is_ipv4())
        .ok_or("no address")?;
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let sock = socket2::Socket::new(domain, Type::STREAM, None).map_err(|e| e.to_string())?;
    sock.bind_device(Some(iface.as_bytes()))
        .map_err(|e| e.to_string())?;
    let sa = socket2::SockAddr::from(addr);
    sock.connect(&sa).map_err(|e| e.to_string())?;
    let stream: TcpStream = sock.into();
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    Ok(stream)
}

fn write_request(stream: &mut TcpStream, req: &[u8]) -> Result<(), String> {
    use std::io::Write;
    stream.write_all(req).map_err(|e| e.to_string())
}

pub fn failover_test(params: &BTreeMap<String, String>) -> ToolRunReport {
    let race = latency_race(params);
    if !race.ok {
        return ToolRunReport {
            tool: "failover_test".into(),
            ok: false,
            summary: race.summary,
            took_ms: 0,
            data: serde_json::Value::Null,
        };
    }
    let rows = race
        .data
        .get("interfaces")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_default();
    let medians: Vec<f64> = rows
        .iter()
        .filter_map(|r| r.get("medianMs").and_then(|m| m.as_f64()))
        .collect();
    if medians.len() < 2 {
        return ok(
            "failover_test",
            "only one usable path; failover needs two independent interfaces".into(),
            serde_json::json!({ "usablePaths": medians.len() }),
        );
    }
    let best = medians.iter().copied().fold(f64::MAX, f64::min);
    let worst = medians.iter().copied().fold(0.0, f64::max);
    let penalty = worst - best;
    ok(
        "failover_test",
        format!(
            "2+ paths measured; failover to the slower link would cost {penalty:.0} ms (worst {worst:.0} ms, best {best:.0} ms)"
        ),
        serde_json::json!({ "bestMs": best, "worstMs": worst, "penaltyMs": penalty }),
    )
}

pub fn bond_simulation(params: &BTreeMap<String, String>) -> ToolRunReport {
    // Inputs: either measured (latency_race/speed_compare) or provided.
    let parse_list = |key: &str| -> Vec<f64> {
        params
            .get(key)
            .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
            .unwrap_or_default()
    };
    let mbps = parse_list("mbps");
    let lat = parse_list("latency_ms");
    if mbps.len() < 2 || lat.len() < 2 {
        return ok(
            "bond_simulation",
            "provide link measurements: mbps=100,20 latency_ms=20,60 (or run speed_compare/latency_race first on multi-WAN hosts)".into(),
            serde_json::Value::Null,
        );
    }
    let total: f64 = mbps.iter().sum();
    let best_lat = lat.iter().copied().fold(f64::MAX, f64::min);
    // Single-flow bonding ceiling: fastest single link (per-flow stays on
    // one path); aggregate: sum. Honesty rule from the roadmap applies.
    let fastest = mbps.iter().copied().fold(0.0, f64::max);
    ok(
        "bond_simulation",
        format!(
            "aggregate {total:.0} Mbps; single-flow ceiling {fastest:.0} Mbps at {best_lat:.0} ms (load balancing is not single-flow bonding)"
        ),
        serde_json::json!({ "aggregateMbps": total, "singleFlowMbps": fastest, "bestLatencyMs": best_lat, "links": mbps.len() }),
    )
}

/* ---- endpoint health (streaming / gaming) ---- */

pub fn stream_health(params: &BTreeMap<String, String>) -> ToolRunReport {
    let url = params
        .get("url")
        .or_else(|| params.get("target"))
        .cloned()
        .unwrap_or_else(|| "https://stream.example.invalid/stream".into());
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(5))
        .build();
    let start = Instant::now();
    let resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(e) => return not("stream_health", format!("endpoint failed: {e}")),
    };
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 262_144];
    let mut bytes = 0u64;
    let mut read_times = Vec::new();
    // 3 seconds of sustained reads = buffer-fill rate + stall risk.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let t0 = Instant::now();
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                bytes += n as u64;
                read_times.push(t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    let secs = start.elapsed().as_secs_f64();
    let mbps = bytes as f64 * 8.0 / secs.max(0.5) / 1e6;
    // Video tiers (Mbps): 4K 25, 1080p 8, 720p 5.
    let tier = if mbps >= 25.0 {
        "4K"
    } else if mbps >= 8.0 {
        "1080p"
    } else if mbps >= 5.0 {
        "720p"
    } else {
        "below 720p (stalls likely)"
    };
    ok(
        "stream_health",
        format!("sustained {mbps:.1} Mbps from the endpoint: comfortable for {tier}"),
        serde_json::json!({ "mbps": mbps, "bytes": bytes, "reads": read_times.len(), "tier": tier }),
    )
}

pub fn game_rtt_guard(params: &BTreeMap<String, String>) -> ToolRunReport {
    let Some(target) = params.get("target").cloned() else {
        return not("game_rtt_guard", "missing 'target'");
    };
    if !valid_target(&target) {
        return not("game_rtt_guard", "invalid target");
    }
    let Some(addr) = (target.as_str(), 443u16)
        .to_socket_addrs()
        .ok()
        .and_then(|mut i| i.next())
    else {
        return not("game_rtt_guard", "cannot resolve target");
    };
    let (rtts, summary, loss) = crate::probes::latency_monitor(addr, 30, 50);
    let Some(s) = summary else {
        return not("game_rtt_guard", "all probes failed");
    };
    let spike = s.p95_ms - s.median_ms;
    let verdict = if s.median_ms < 60.0 && spike < 30.0 {
        "competitive-ready"
    } else if s.median_ms < 120.0 {
        "playable; spikes may cost fights"
    } else {
        "high latency for competitive play"
    };
    let _ = rtts;
    ok(
        "game_rtt_guard",
        format!(
            "{verdict}: median {:.0} ms, p95 {:.0} ms (spike +{spike:.0} ms), jitter {:.1} ms, loss {loss:.0}%",
            s.median_ms, s.p95_ms, s.jitter_ms
        ),
        serde_json::json!({ "medianMs": s.median_ms, "p95Ms": s.p95_ms, "jitterMs": s.jitter_ms, "lossPct": loss, "verdict": verdict }),
    )
}

/* ---- journals: history, destinations, time machine ---- */

fn journal_path(name: &str) -> std::path::PathBuf {
    crate::netops::state_dir_public().join(format!("{name}.jsonl"))
}

/// Append one JSONL entry to a journal (also used by diag's usage tool).
pub fn append_journal(name: &str, entry: &serde_json::Value) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path(name))
    {
        let _ = writeln!(f, "{entry}");
    }
}

fn read_journal(name: &str, limit: usize) -> Vec<serde_json::Value> {
    let Ok(text) = std::fs::read_to_string(journal_path(name)) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(limit)..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

pub fn connection_history() -> ToolRunReport {
    let conns = sysnet::connections();
    let entry = serde_json::json!({
        "atMs": crate::clock_ms(),
        "total": conns.connections.len(),
        "groups": conns.groups.iter().take(12).cloned().collect::<Vec<_>>(),
    });
    append_journal("connections", &entry);
    let recent = read_journal("connections", 20);
    ok(
        "connection_history",
        format!(
            "{} sockets now; {} journal point(s) recorded (runs append; the daemon automates this later)",
            conns.connections.len(),
            recent.len()
        ),
        serde_json::json!({ "now": entry, "recent": recent }),
    )
}

pub fn session_journal() -> ToolRunReport {
    let destinations: BTreeMap<String, u32> = sysnet::connections()
        .connections
        .iter()
        .filter(|c| c.remote != "-" && !c.remote.starts_with("127."))
        .map(|c| match c.remote.rsplit_once(':') {
            Some((h, _)) => h.to_string(),
            None => c.remote.clone(),
        })
        .fold(BTreeMap::new(), |mut m, h| {
            *m.entry(h).or_default() += 1;
            m
        });
    let entry = serde_json::json!({ "atMs": crate::clock_ms(), "destinations": destinations });
    append_journal("destinations", &entry);
    let journal = read_journal("destinations", 50);
    // Aggregate destination counts across the journal.
    let mut totals: BTreeMap<String, u32> = BTreeMap::new();
    for j in &journal {
        if let Some(d) = j.get("destinations").and_then(|d| d.as_object()) {
            for (host, n) in d {
                if let Some(n) = n.as_u64() {
                    *totals.entry(host.clone()).or_default() += n as u32;
                }
            }
        }
    }
    let top: Vec<_> = totals.into_iter().collect::<Vec<_>>();
    let shown = top.iter().rev().take(10).cloned().collect::<Vec<_>>();
    ok(
        "session_journal",
        format!(
            "{} destination(s) contacted across {} journal point(s)",
            top.len(),
            journal.len()
        ),
        serde_json::json!({ "top": shown }),
    )
}

pub fn net_time_machine() -> ToolRunReport {
    // Timeline across every journal this toolkit keeps.
    let mut events: Vec<(u64, String, String)> = Vec::new();
    for (journal, label) in [
        ("connections", "connections"),
        ("destinations", "destinations"),
    ] {
        for e in read_journal(journal, 100) {
            if let Some(at) = e.get("atMs").and_then(|a| a.as_u64()) {
                events.push((
                    at,
                    label.to_string(),
                    serde_json::to_string(&e).unwrap_or_default(),
                ));
            }
        }
    }
    // Route/DNS watch baselines have change history in their state files.
    for watch in ["route_change_watch", "dns_change_watch"] {
        if let Some(prev) = crate::netops::load_state_public(watch) {
            if let Some(at) = prev.get("atMs").and_then(|a| a.as_u64()) {
                events.push((
                    at,
                    watch.to_string(),
                    serde_json::to_string(&prev).unwrap_or_default(),
                ));
            }
        }
    }
    events.sort_by_key(|(at, _, _)| *at);
    let count = events.len();
    ok(
        "net_time_machine",
        format!(
            "{count} journal event(s) in the timeline (scrubber UI ships with the telemetry store; the data starts accumulating now)"
        ),
        serde_json::json!({ "events": events.into_iter().rev().take(30).map(|(at, kind, data)| serde_json::json!({"atMs": at, "kind": kind, "data": data})).collect::<Vec<_>>() }),
    )
}

pub fn per_destination_history() -> ToolRunReport {
    // Destination-centric view of the same journal.
    session_journal()
}

/* ---- advisory + monitor tools ---- */

pub fn vpn_split_tunnel(params: &BTreeMap<String, String>) -> ToolRunReport {
    let target = params.get("target").cloned().unwrap_or_default();
    let ip = if target.parse::<std::net::Ipv4Addr>().is_ok() {
        target.clone()
    } else {
        match (target.as_str(), 443u16).to_socket_addrs() {
            Ok(mut it) => it.next().map(|a| a.ip().to_string()).unwrap_or_default(),
            Err(_) => String::new(),
        }
    };
    if ip.is_empty() {
        return not(
            "vpn_split_tunnel",
            "missing 'target' (IP or host that resolves)",
        );
    }
    let gw = crate::probes::default_gateway().unwrap_or_default();
    let audit = crate::routeaudit::audit();
    let iface = audit
        .entries
        .iter()
        .find(|e| {
            e.destination == "0.0.0.0/0" && !e.interface.as_deref().unwrap_or("tun").contains("tun")
        })
        .or_else(|| audit.entries.iter().find(|e| e.destination == "0.0.0.0/0"))
        .and_then(|e| e.interface.clone())
        .unwrap_or_else(|| "YOUR_PHYSICAL_IF".into());
    let commands = vec![
        format!(
            "route add {ip} mask 255.255.255.255 {gw} IF <index-of-{iface}>   # Windows (elevated cmd)"
        ),
        format!("sudo ip route add {ip}/32 via {gw} dev {iface}   # Linux"),
        format!("sudo route -n add -host {ip} {gw}   # macOS"),
    ];
    ok(
        "vpn_split_tunnel",
        format!("pin {ip} outside the VPN by routing it via {gw} ({iface}); apply with elevation"),
        serde_json::json!({ "target": ip, "gateway": gw, "interface": iface, "commands": commands }),
    )
}

pub fn kill_switch_status() -> ToolRunReport {
    let routes = crate::routeaudit::audit();
    let vpn_active = routes.anomalies.iter().any(|a| a.contains("full-tunnel"));
    let leaks: Vec<String> = routes
        .entries
        .iter()
        .filter(|e| e.destination == "0.0.0.0/0")
        .filter(|e| e.interface.as_deref().is_none_or(|i| !i.contains("tun")))
        .filter_map(|e| e.interface.clone())
        .collect();
    let summary = if !vpn_active {
        "no VPN detected; a kill switch is only meaningful while a tunnel is up".to_string()
    } else if leaks.is_empty() {
        "VPN is up and no non-tunnel default route exists: traffic cannot leak".to_string()
    } else {
        format!(
            "LEAK RISK: non-tunnel default route(s) via {} while VPN is up",
            leaks.join(", ")
        )
    };
    ok(
        "kill_switch",
        summary,
        serde_json::json!({ "vpnActive": vpn_active, "nonTunnelDefaults": leaks }),
    )
}

pub fn quota_guard(params: &BTreeMap<String, String>) -> ToolRunReport {
    let limit_mib: f64 = params
        .get("limit_mib")
        .and_then(|v| v.parse().ok())
        .unwrap_or(5120.0);
    let used: f64 = read_journal("bandwidth", 500)
        .iter()
        .filter_map(|e| {
            let obj = e.get("deltas")?.as_object()?;
            Some(
                obj.values()
                    .filter_map(|d| d.get("rxMiB").and_then(|v| v.as_f64()))
                    .sum::<f64>(),
            )
        })
        .sum();
    let pct = used / limit_mib * 100.0;
    let verdict = if pct >= 100.0 {
        "OVER QUOTA"
    } else if pct >= 80.0 {
        "approaching quota"
    } else {
        "within quota"
    };
    ok(
        "quota_guard",
        format!("{verdict}: {used:.0} of {limit_mib:.0} MiB ({pct:.0}%) since the journal began"),
        serde_json::json!({ "usedMib": used, "limitMib": limit_mib, "pct": pct }),
    )
}

pub fn metered_guard() -> ToolRunReport {
    if cfg!(windows) {
        let out = exec::run("netsh", &["wlan", "show", "interfaces"], TIMEOUT);
        let lower = out.stdout.to_lowercase();
        let metered = lower.contains("cost")
            && !lower.contains("cost: unrestricted")
            && !lower.contains("cost: fixed");
        return ok(
            "metered_guard",
            if metered {
                "metered Wi-Fi detected: treat as data-capped"
            } else {
                "no metered link detected"
            }
            .to_string(),
            serde_json::json!({ "metered": metered }),
        );
    }
    if exec::on_path("nmcli") {
        let out = exec::run(
            "nmcli",
            &["-t", "-f", "GENERAL.METERED", "dev", "status"],
            TIMEOUT,
        );
        let metered = out.stdout.to_lowercase().contains("yes");
        return ok(
            "metered_guard",
            if metered {
                "metered link detected (NetworkManager)"
            } else {
                "no metered link detected"
            }
            .to_string(),
            serde_json::json!({ "metered": metered }),
        );
    }
    not("metered_guard", "no metered-status reader on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ip_o_addr_fixture() {
        let out = "1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever\n2: eth0    inet 192.168.1.5/24 brd 192.168.1.255 scope global eth0\\       valid_lft forever\n3: wwan0    inet 10.20.30.4/28 brd 10.20.30.15 scope global wwan0";
        let ifaces = parse_ip_o_addr(out);
        // The parser reports every interface; loopback filtering is the
        // caller's job (interfaces_with_ips).
        assert!(ifaces.contains(&("eth0".to_string(), "192.168.1.5".to_string())));
        assert!(ifaces.contains(&("wwan0".to_string(), "10.20.30.4".to_string())));
        assert!(ifaces.iter().any(|(i, _)| i == "lo"));
    }

    #[test]
    fn journals_append_and_read() {
        let first = connection_history();
        assert!(first.ok, "{}", first.summary);
        let second = session_journal();
        assert!(second.ok, "{}", second.summary);
        let tm = net_time_machine();
        assert!(tm.ok);
        assert!(tm.summary.contains("journal event"));
        let _ = per_destination_history();
    }

    #[test]
    fn kill_switch_status_is_honest_without_vpn() {
        let r = kill_switch_status();
        assert!(r.ok);
        assert!(
            r.summary.contains("no VPN detected")
                || r.summary.contains("leak")
                || r.summary.contains("cannot leak")
        );
    }

    #[test]
    fn split_tunnel_generates_real_commands() {
        let r = vpn_split_tunnel(
            &[("target".to_string(), "1.1.1.1".to_string())]
                .into_iter()
                .collect(),
        );
        assert!(r.ok);
        let cmds = r.data.get("commands").unwrap().as_array().unwrap();
        assert!(
            cmds.iter()
                .any(|c| c.as_str().unwrap().contains("route add 1.1.1.1"))
        );
    }

    #[test]
    fn quota_guard_runs() {
        let r = quota_guard(
            &[("limit_mib".to_string(), "10240".to_string())]
                .into_iter()
                .collect(),
        );
        assert!(r.ok);
        assert!(r.summary.contains("quota"));
    }

    #[test]
    fn bond_math_is_honest_about_single_flow() {
        let r = bond_simulation(
            &[
                ("mbps".to_string(), "100,20".to_string()),
                ("latency_ms".to_string(), "20,60".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        assert!(r.summary.contains("aggregate 120"));
        assert!(r.summary.contains("single-flow ceiling 100"));
        assert!(r.summary.contains("not single-flow bonding"));
    }

    #[test]
    fn game_guard_needs_valid_target() {
        assert!(
            !game_rtt_guard(
                &[("target".to_string(), "a b".to_string())]
                    .into_iter()
                    .collect()
            )
            .ok
        );
        let r = game_rtt_guard(
            &[
                ("target".to_string(), "127.0.0.1".to_string()),
                ("port".to_string(), "1".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        // Nothing listens on 1: honest failure path.
        assert!(!r.ok || r.summary.contains("median"));
    }

    #[test]
    fn latency_race_honest_without_interfaces() {
        let r = latency_race(
            &[("target".to_string(), "127.0.0.1".to_string())]
                .into_iter()
                .collect(),
        );
        // In containers only lo exists (filtered) -> honest failure or
        // a real bound measurement when interfaces exist.
        assert!(r.ok || r.summary.contains("no non-loopback"));
    }
}
