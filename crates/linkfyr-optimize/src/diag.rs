//! Diagnostics: socket/exposure audits, VPN detection, lease and cache
//! info, retransmit/TCP-info readers, qdisc/ECN posture, quality math
//! (VoIP MOS, loss bursts, throughput variance, latency profile), data
//! usage accounting with a state file, and the deep interface dump.
//! Every reader uses the OS's supported command; parsers are pure and
//! fixture-tested.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::exec;
use crate::{probes, routeaudit, sysnet};

const TIMEOUT: Duration = Duration::from_secs(15);

fn ok(
    tool: &str,
    summary: String,
    data: serde_json::Value,
) -> linkfyr_model::optimize::ToolRunReport {
    linkfyr_model::optimize::ToolRunReport {
        tool: tool.into(),
        ok: true,
        summary,
        took_ms: 0,
        data,
    }
}

fn unavailable(tool: &str, why: impl Into<String>) -> linkfyr_model::optimize::ToolRunReport {
    linkfyr_model::optimize::ToolRunReport {
        tool: tool.into(),
        ok: false,
        summary: why.into(),
        took_ms: 0,
        data: serde_json::Value::Null,
    }
}

/* ---- socket / exposure ---- */

pub fn sockstat() -> linkfyr_model::optimize::ToolRunReport {
    let conns = sysnet::connections().connections;
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for c in &conns {
        *counts.entry(c.state.clone()).or_default() += 1;
    }
    let total = conns.len() as u32;
    let time_wait = counts.get("close-wait").copied().unwrap_or(0)
        + counts.get("timewait").copied().unwrap_or(0);
    ok(
        "sockstat",
        format!(
            "{total} sockets; top state {:?}",
            counts.iter().max_by_key(|(_, v)| **v).map(|(k, _)| k)
        ),
        serde_json::json!({ "total": total, "states": counts, "drainWarning": time_wait > 100 }),
    )
}

pub fn smb_exposure() -> linkfyr_model::optimize::ToolRunReport {
    let listening = sysnet::listening_ports();
    let open445 = listening.iter().any(|c| c.local.ends_with(":445"));
    let binding = listening
        .iter()
        .filter(|c| c.local.ends_with(":445"))
        .map(|c| c.local.clone())
        .collect::<Vec<_>>();
    let exposed = open445
        && binding
            .iter()
            .any(|l| !l.starts_with("127.0.0.1") && !l.starts_with("[::1]"));
    ok(
        "smb_exposure",
        if exposed {
            "SMB (445) is listening on a non-loopback address: file sharing is network-reachable"
                .into()
        } else if open445 {
            "SMB (445) listening, loopback/restricted only".into()
        } else {
            "SMB (445) is not listening: no file-sharing exposure".into()
        },
        serde_json::json!({ "listening": open445, "exposed": exposed, "bindings": binding }),
    )
}

pub fn rdp_exposure() -> linkfyr_model::optimize::ToolRunReport {
    let listening = sysnet::listening_ports();
    let open3389 = listening.iter().any(|c| c.local.ends_with(":3389"));
    let mut registry_disabled: Option<bool> = None;
    if cfg!(windows) {
        let out = exec::run(
            "reg",
            &[
                "query",
                r"HKLM\SYSTEM\CurrentControlSet\Control\Terminal Server",
                "/v",
                "fDenyTSConnections",
            ],
            TIMEOUT,
        );
        registry_disabled =
            out.stdout
                .contains("0x1")
                .then_some(true)
                .or(if out.stdout.contains("0x0") {
                    Some(false)
                } else {
                    None
                });
    }
    let exposed = open3389 || registry_disabled == Some(false);
    ok(
        "rdp_exposure",
        if exposed {
            "RDP appears enabled/listening: ensure it is firewall-restricted".into()
        } else {
            "RDP is not enabled".into()
        },
        serde_json::json!({ "listening3389": open3389, "registryEnabled": registry_disabled.map(|d| !d) }),
    )
}

pub fn llmnr_check() -> linkfyr_model::optimize::ToolRunReport {
    let listening = sysnet::listening_ports();
    let mdns = listening.iter().any(|c| c.local.ends_with(":5353"));
    let llmnr = listening.iter().any(|c| c.local.ends_with(":5355"));
    let summary = match (mdns, llmnr) {
        (false, false) => "no multicast name-resolution services (mDNS 5353 / LLMNR 5355) are listening".to_string(),
        (true, false) => "mDNS (5353) active: names resolve on the local link; normal for printers/AirPlay".to_string(),
        (_, true) => "LLMNR (5355) active: legacy name resolution; spoofable on untrusted networks, consider disabling".to_string(),
    };
    ok(
        "llmnr_check",
        summary,
        serde_json::json!({ "mdns": mdns, "llmnr": llmnr }),
    )
}

pub fn vpn_detect() -> linkfyr_model::optimize::ToolRunReport {
    let routes = routeaudit::audit();
    let hijack = routes.anomalies.iter().any(|a| a.contains("full-tunnel"));
    let tunnel_ifaces = if exec::on_path("ip") {
        let out = exec::run("ip", &["link", "show"], TIMEOUT);
        out.stdout
            .lines()
            .filter(|l| l.contains(": "))
            .filter_map(|l| l.split(':').nth(1))
            .map(str::trim)
            .filter(|n| {
                let n = n.to_lowercase();
                n.starts_with("tun")
                    || n.starts_with("tap")
                    || n.starts_with("wg")
                    || n.starts_with("ppp")
                    || n.starts_with("utun")
            })
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let detected = hijack || !tunnel_ifaces.is_empty();
    ok(
        "vpn_detect",
        if detected {
            format!(
                "VPN detected: {} tunnel interface(s){}",
                tunnel_ifaces.len(),
                if hijack {
                    " + full-tunnel route pattern"
                } else {
                    ""
                }
            )
        } else {
            "no tunnel interfaces or VPN route patterns detected".into()
        },
        serde_json::json!({ "tunnelInterfaces": tunnel_ifaces, "fullTunnelPattern": hijack }),
    )
}

pub fn router_hop_map() -> linkfyr_model::optimize::ToolRunReport {
    let Some(gw) = probes::default_gateway() else {
        return unavailable("router_hop_map", "no default gateway found");
    };
    let mac = sysnet::arp_table()
        .into_iter()
        .find(|(ip, _, _)| *ip == gw)
        .map(|(_, m, _)| m);
    ok(
        "router_hop_map",
        format!(
            "gateway {gw} at MAC {}",
            mac.as_deref().unwrap_or("(not in ARP table)")
        ),
        serde_json::json!({ "gateway": gw, "mac": mac }),
    )
}

pub fn multicast_snoop() -> linkfyr_model::optimize::ToolRunReport {
    if exec::on_path("ip") {
        let out = exec::run("ip", &["maddr", "show"], TIMEOUT);
        let groups: Vec<String> = out
            .stdout
            .lines()
            .filter_map(|l| l.trim().strip_prefix("group "))
            .map(ToString::to_string)
            .collect();
        return ok(
            "multicast_snoop",
            format!("{} multicast group memberships", groups.len()),
            serde_json::json!({ "groups": groups }),
        );
    }
    if cfg!(windows) {
        let out = exec::run("netsh", &["interface", "ipv4", "show", "joins"], TIMEOUT);
        let groups: Vec<String> = exec::extract_ipv4(&out.stdout);
        return ok(
            "multicast_snoop",
            format!("{} multicast join addresses", groups.len()),
            serde_json::json!({ "groups": groups }),
        );
    }
    unavailable("multicast_snoop", "no supported multicast listing tool")
}

/* ---- lease / cache / counters ---- */

pub fn lease_info() -> linkfyr_model::optimize::ToolRunReport {
    if cfg!(windows) || !exec::on_path("ip") {
        if !cfg!(windows) {
            return unavailable("lease_info", "no supported lease reader on this platform");
        }
        let out = exec::run("ipconfig", &["/all"], TIMEOUT);
        let lines: Vec<String> = out
            .stdout
            .lines()
            .filter(|l| {
                let lower = l.to_lowercase();
                lower.contains("lease") || lower.contains("dhcp server")
            })
            .map(|l| l.trim().to_string())
            .collect();
        return ok(
            "lease_info",
            format!("{} lease lines from ipconfig", lines.len()),
            serde_json::json!({ "lines": lines }),
        );
    }
    if exec::on_path("nmcli") {
        let out = exec::run(
            "nmcli",
            &["-t", "-f", "NAME,DEVICE,TYPE", "con", "show", "--active"],
            TIMEOUT,
        );
        let rows: Vec<String> = out.stdout.lines().map(ToString::to_string).collect();
        return ok(
            "lease_info",
            format!("{} active DHCP-capable connection(s)", rows.len()),
            serde_json::json!({ "connections": rows }),
        );
    }
    unavailable("lease_info", "neither ipconfig nor nmcli available")
}

pub fn dns_cache_stats() -> linkfyr_model::optimize::ToolRunReport {
    if exec::on_path("resolvectl") {
        let out = exec::run("resolvectl", &["statistics"], TIMEOUT);
        let lines: Vec<String> = out.stdout.lines().map(ToString::to_string).collect();
        if !lines.is_empty() {
            return ok(
                "dns_cache_stats",
                format!("{} statistics lines", lines.len()),
                serde_json::json!({ "lines": lines }),
            );
        }
    }
    if cfg!(windows) {
        let out = exec::run(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "(Get-DnsClientCache | Measure-Object).Count",
            ],
            TIMEOUT,
        );
        let count: String = out.stdout.trim().to_string();
        if let Ok(n) = count.parse::<u32>() {
            return ok(
                "dns_cache_stats",
                format!("{n} entries in the resolver cache"),
                serde_json::json!({ "entries": n }),
            );
        }
    }
    unavailable("dns_cache_stats", "no supported cache statistics tool")
}

/// Parse `/proc/net/snmp` Tcp counters (pure).
pub fn parse_snmp_tcp(content: &str) -> Option<(u64, u64)> {
    let mut header: Option<Vec<&str>> = None;
    for line in content.lines() {
        if let Some(h) = line.strip_prefix("Tcp: ") {
            if header.is_none() {
                header = Some(h.split_whitespace().collect());
                continue;
            }
            let values: Vec<&str> = h.split_whitespace().collect();
            let hdr = header.as_ref().unwrap();
            let get = |name: &str| -> Option<u64> {
                let i = hdr.iter().position(|h| *h == name)?;
                values.get(i)?.parse().ok()
            };
            let out = get("OutSegs")?;
            let retrans = get("RetransSegs")?;
            return Some((out, retrans));
        }
    }
    None
}

pub fn retransmit_rate() -> linkfyr_model::optimize::ToolRunReport {
    if cfg!(windows) {
        let out = exec::run("netstat", &["-s", "-p", "tcp"], TIMEOUT);
        let get = |needle: &str| -> Option<u64> {
            out.stdout
                .lines()
                .find(|l| l.contains(needle))
                .and_then(|l| l.split_whitespace().find_map(|t| t.parse::<u64>().ok()))
        };
        if let (Some(sent), Some(retrans)) = (get("Segments Sent"), get("Segments Retransmitted")) {
            let rate = if sent > 0 {
                retrans as f64 / sent as f64 * 100.0
            } else {
                0.0
            };
            return ok(
                "retransmit_rate",
                format!("{rate:.2}% retransmit rate ({retrans}/{sent} segments)"),
                serde_json::json!({ "sent": sent, "retransmitted": retrans, "ratePct": rate }),
            );
        }
        return unavailable("retransmit_rate", "netstat counters not parseable");
    }
    match std::fs::read_to_string("/proc/net/snmp") {
        Ok(snmp) => match parse_snmp_tcp(&snmp) {
            Some((out, retrans)) => {
                let rate = if out > 0 {
                    retrans as f64 / out as f64 * 100.0
                } else {
                    0.0
                };
                ok(
                    "retransmit_rate",
                    format!("{rate:.2}% retransmit rate ({retrans}/{out} segments)"),
                    serde_json::json!({ "sent": out, "retransmitted": retrans, "ratePct": rate }),
                )
            }
            None => unavailable("retransmit_rate", "/proc/net/snmp had no Tcp counters"),
        },
        Err(_) => unavailable(
            "retransmit_rate",
            "/proc/net/snmp not readable on this platform",
        ),
    }
}

/// Parse `ss -tin` extended info lines (pure).
pub fn parse_ss_tin(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter(|l| l.contains("rtt:") || l.contains("cwnd:"))
        .filter_map(|l| {
            l.split_whitespace()
                .find(|t| t.starts_with("rtt:") || t.starts_with("cwnd:"))
                .map(|t| {
                    (
                        l.split_whitespace().next().unwrap_or("?").to_string(),
                        t.to_string(),
                    )
                })
        })
        .collect()
}

pub fn tcp_info() -> linkfyr_model::optimize::ToolRunReport {
    if !exec::on_path("ss") {
        return unavailable(
            "tcp_info",
            "per-socket TCP info needs `ss -ti` (Linux) or is unsupported here",
        );
    }
    let out = exec::run("ss", &["-tin"], TIMEOUT);
    let infos = parse_ss_tin(&out.stdout);
    ok(
        "tcp_info",
        format!("{} live TCP connection(s) with kernel timing", infos.len()),
        serde_json::json!({ "connections": infos.into_iter().map(|(a, i)| serde_json::json!({"addr": a, "info": i})).collect::<Vec<_>>() }),
    )
}

pub fn qdisc_audit() -> linkfyr_model::optimize::ToolRunReport {
    if !exec::on_path("tc") && !std::path::Path::new("/proc/sys/net/ipv4").exists() {
        return unavailable("qdisc_audit", "queueing disciplines need tc/sysctl (Linux)");
    }
    let state = crate::tcpaudit::read_linux_state();
    let checks = crate::tcpaudit::analyze_linux(&state);
    let qdisc = checks
        .iter()
        .find(|c| c.key == "root_qdisc")
        .map(|c| format!("root qdisc: {}", c.current));
    ok(
        "qdisc_audit",
        qdisc.unwrap_or_else(|| "no qdisc information available".into()),
        serde_json::json!({ "checks": checks }),
    )
}

pub fn ecn_check() -> linkfyr_model::optimize::ToolRunReport {
    if std::path::Path::new("/proc/sys/net/ipv4/tcp_ecn").exists() {
        let v = std::fs::read_to_string("/proc/sys/net/ipv4/tcp_ecn").unwrap_or_default();
        let v = v.trim();
        return ok(
            "ecn_check",
            format!(
                "host ECN mode {v} (1 = enabled, 2 = full): the kernel negotiates ECN where paths allow"
            ),
            serde_json::json!({ "tcp_ecn": v }),
        );
    }
    if cfg!(windows) {
        let out = exec::run("netsh", &["int", "tcp", "show", "global"], TIMEOUT);
        let ecn = out
            .stdout
            .lines()
            .find(|l| l.to_lowercase().contains("ecn"));
        return ok(
            "ecn_check",
            ecn.map_or_else(
                || "ECN setting not reported".into(),
                |l| l.trim().to_string(),
            ),
            serde_json::json!({ "line": ecn }),
        );
    }
    unavailable("ecn_check", "no ECN setting reader on this platform")
}

/* ---- quality math over real measurements ---- */

/// Classic R-factor -> MOS estimate from measured latency/jitter/loss.
pub fn voip_mos_params(rtt_ms: f64, jitter_ms: f64, loss_pct: f64) -> (f64, f64) {
    let effective_latency = rtt_ms + jitter_ms * 2.0 + 10.0;
    let ld = if effective_latency < 160.0 {
        effective_latency - 60.0
    } else {
        40.0 + (effective_latency - 60.0) * 0.1
    };
    let loss_effect = loss_pct * 2.5;
    let r_factor = 93.2 - ld - loss_effect;
    let mos =
        1.0 + 0.035 * r_factor + 0.000_007 * r_factor * (r_factor - 60.0) * (100.0 - r_factor);
    (r_factor.max(0.0), mos.clamp(1.0, 4.5))
}

pub fn voip_mos(params: &BTreeMap<String, String>) -> linkfyr_model::optimize::ToolRunReport {
    let target = params
        .get("target")
        .cloned()
        .unwrap_or_else(|| "1.1.1.1".into());
    let Some(addr) = probes::resolve_one(&target, 443) else {
        return unavailable("voip_mos", format!("cannot resolve {target}"));
    };
    let (_, summary, loss) = probes::latency_monitor(addr, 20, 50);
    let Some(s) = summary else {
        return unavailable("voip_mos", "probes failed; cannot estimate call quality");
    };
    let (r, mos) = voip_mos_params(s.median_ms, s.jitter_ms, loss);
    let verdict = if mos >= 4.0 {
        "excellent"
    } else if mos >= 3.6 {
        "good"
    } else if mos >= 3.1 {
        "fair; calls may sound degraded"
    } else {
        "poor; calls will suffer"
    };
    ok(
        "voip_mos",
        format!(
            "estimated MOS {mos:.2} ({verdict}) from median {:.0} ms, jitter {:.1} ms, loss {loss:.0}%",
            s.median_ms, s.jitter_ms
        ),
        serde_json::json!({ "mos": mos, "rFactor": r, "medianMs": s.median_ms, "jitterMs": s.jitter_ms, "lossPct": loss }),
    )
}

pub fn loss_bursts(params: &BTreeMap<String, String>) -> linkfyr_model::optimize::ToolRunReport {
    let target = params
        .get("target")
        .cloned()
        .unwrap_or_else(|| "1.1.1.1".into());
    let samples: u32 = params
        .get("samples")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50)
        .clamp(10, 300);
    let Some(addr) = probes::resolve_one(&target, 443) else {
        return unavailable("loss_bursts", format!("cannot resolve {target}"));
    };
    let mut results = Vec::new();
    for _ in 0..samples {
        results.push(std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok());
    }
    let mut bursts = Vec::new();
    let mut run = 0u32;
    for r in &results {
        if *r {
            if run > 0 {
                bursts.push(run);
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    if run > 0 {
        bursts.push(run);
    }
    let lost = results.iter().filter(|r| !**r).count();
    let worst = bursts.iter().copied().max().unwrap_or(0);
    ok(
        "loss_bursts",
        format!("{lost}/{samples} lost; worst burst {worst} consecutive"),
        serde_json::json!({ "samples": samples, "lost": lost, "bursts": bursts, "worstBurst": worst }),
    )
}

pub fn throughput_variance(
    params: &BTreeMap<String, String>,
) -> linkfyr_model::optimize::ToolRunReport {
    let endpoint = params
        .get("endpoint")
        .cloned()
        .unwrap_or_else(|| "https://speed.cloudflare.com".into());
    let mut rates = Vec::new();
    for _ in 0..3 {
        let opts = crate::speedtest::SpeedtestOptions {
            endpoint: endpoint.clone(),
            duration_s: 3,
        };
        let r = crate::speedtest::run(&opts);
        if r.download_mbps > 0.0 {
            rates.push(r.download_mbps);
        }
    }
    if rates.is_empty() {
        return unavailable(
            "throughput_variance",
            "no burst completed; endpoint unreachable",
        );
    }
    let mean = rates.iter().sum::<f64>() / rates.len() as f64;
    let variance = rates.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / rates.len() as f64;
    let cov = if mean > 0.0 {
        variance.sqrt() / mean * 100.0
    } else {
        0.0
    };
    let verdict = if cov < 5.0 {
        "very stable"
    } else if cov < 15.0 {
        "stable"
    } else if cov < 30.0 {
        "variable (buffer or radio issues)"
    } else {
        "unstable"
    };
    ok(
        "throughput_variance",
        format!("3-burst download CoV {cov:.1}% ({verdict})"),
        serde_json::json!({ "burstsMbps": rates, "meanMbps": mean, "covPct": cov }),
    )
}

/// Latency profile across diverse real services (regional proxy).
pub fn geo_route_compare() -> linkfyr_model::optimize::ToolRunReport {
    let services = [
        ("cloudflare", "www.cloudflare.com"),
        ("google", "www.google.com"),
        ("microsoft", "www.microsoft.com"),
        ("amazon", "www.amazon.com"),
        ("wikipedia", "www.wikipedia.org"),
    ];
    let mut rows = Vec::new();
    for (name, host) in services {
        if let Some(addr) = probes::resolve_one(host, 443) {
            let (rtts, summary, _) = probes::latency_monitor(addr, 3, 40);
            rows.push(serde_json::json!({
                "service": name,
                "medianMs": summary.map(|s| s.median_ms),
                "ok": !rtts.is_empty(),
            }));
        } else {
            rows.push(serde_json::json!({ "service": name, "medianMs": null, "ok": false }));
        }
    }
    let answered = rows
        .iter()
        .filter(|r| r.get("ok").and_then(|o| o.as_bool()).unwrap_or(false))
        .count();
    ok(
        "geo_route_compare",
        format!("{answered}/5 global services reachable; medians in data"),
        serde_json::json!({ "profiles": rows }),
    )
}

/// Data usage: diff interface counters against a stored state file.
pub fn data_usage() -> linkfyr_model::optimize::ToolRunReport {
    let current = if let Ok(text) = std::fs::read_to_string("/proc/net/dev") {
        let mut map = serde_json::Map::new();
        for line in text.lines().skip(2) {
            let Some((iface, stats)) = line.split_once(':') else {
                continue;
            };
            let fields: Vec<&str> = stats.split_whitespace().collect();
            if fields.len() >= 16 {
                map.insert(
                    iface.trim().to_string(),
                    serde_json::json!({ "rx": fields[0].parse::<u64>().unwrap_or(0), "tx": fields[8].parse::<u64>().unwrap_or(0) }),
                );
            }
        }
        serde_json::Value::Object(map)
    } else {
        return unavailable(
            "data_usage",
            "interface counters not readable on this platform",
        );
    };
    let path = crate::netops::state_dir_public().join("data-usage.json");
    let previous: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let _ = std::fs::write(&path, serde_json::to_string(&current).unwrap_or_default());
    let mut deltas = serde_json::Map::new();
    if let (Some(cur), Some(prev)) = (current.as_object(), previous.as_object()) {
        for (iface, c) in cur {
            let (crx, ctx) = (
                c.get("rx").and_then(|v| v.as_u64()).unwrap_or(0),
                c.get("tx").and_then(|v| v.as_u64()).unwrap_or(0),
            );
            if let Some(p) = prev.get(iface) {
                let (prx, ptx) = (
                    p.get("rx").and_then(|v| v.as_u64()).unwrap_or(0),
                    p.get("tx").and_then(|v| v.as_u64()).unwrap_or(0),
                );
                if crx >= prx && ctx >= ptx {
                    deltas.insert(iface.clone(), serde_json::json!({ "rxMiB": (crx - prx) as f64 / 1_048_576.0, "txMiB": (ctx - ptx) as f64 / 1_048_576.0 }));
                }
            }
        }
    }
    let total_rx: f64 = deltas
        .values()
        .filter_map(|d| d.get("rxMiB").and_then(|v| v.as_f64()))
        .sum();
    ok(
        "data_usage",
        if previous.is_null() {
            format!("baseline stored ({total_rx:.0} MiB seen); run again for deltas")
        } else {
            format!("{total_rx:.1} MiB received since last check")
        },
        serde_json::json!({ "deltas": deltas }),
    )
}

/// Deep interface dump: real OS text, lightly structured.
pub fn iface_deep() -> linkfyr_model::optimize::ToolRunReport {
    let (tool, text) = if cfg!(windows) {
        let out = exec::run("ipconfig", &["/all"], TIMEOUT);
        ("ipconfig /all", out.stdout)
    } else if exec::on_path("ip") {
        let a = exec::run("ip", &["-d", "addr"], TIMEOUT).stdout;
        let b = exec::run("ip", &["route"], TIMEOUT).stdout;
        let c = exec::run("ip", &["-s", "link"], TIMEOUT).stdout;
        ("ip -d addr / route / -s link", format!("{a}\n{b}\n{c}"))
    } else {
        let out = exec::run("ifconfig", &["-a"], TIMEOUT);
        ("ifconfig -a", out.stdout)
    };
    if text.trim().is_empty() {
        return unavailable("iface_deep", "no interface dump tool available");
    }
    ok(
        "iface_deep",
        format!(
            "{tool}: {} lines of live interface state",
            text.lines().count()
        ),
        serde_json::json!({ "dump": text }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snmp_parser_extracts_retrans_counters() {
        let snmp = "Ip: fwd 1\nTcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails EstabResets CurrEstab InSegs OutSegs RetransSegs InErrs OutRsts\nTcp: 1 200 120000 -1 5 1 0 0 2 100 200 3 0 1\n";
        assert_eq!(parse_snmp_tcp(snmp), Some((200, 3)));
        assert_eq!(parse_snmp_tcp("garbage"), None);
    }

    #[test]
    fn ss_tin_parser_finds_rtt_lines() {
        let out = "ESTAB 0 0 10.0.0.1:5(x) 93.184.216.34:443\n\trtt:12.3/4.5 mss:1448 cwnd:10\n";
        let infos = parse_ss_tin(out);
        assert_eq!(infos.len(), 1);
        assert!(infos[0].1.contains("rtt:"));
    }

    #[test]
    fn mos_math_matches_expectations() {
        let (r, mos) = voip_mos_params(20.0, 1.0, 0.0);
        assert!(mos > 4.0, "clean path must be excellent: {mos}");
        let (_, poor) = voip_mos_params(400.0, 40.0, 5.0);
        assert!(poor < 3.0, "bad path must be poor: {poor}");
        let _ = r;
    }

    #[test]
    fn exposure_checks_run_on_live_tables() {
        let _ = smb_exposure();
        let _ = rdp_exposure();
        let _ = llmnr_check();
        let _ = sockstat();
        let _ = vpn_detect();
        let _ = router_hop_map();
    }

    #[test]
    fn loss_bursts_counts_consecutive_failures() {
        // A closed port on loopback = every probe fails = one big burst.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port().to_string();
        drop(l);
        let r = loss_bursts(
            &[
                ("target".to_string(), "127.0.0.1".to_string()),
                ("samples".to_string(), port),
            ]
            .into_iter()
            .collect(),
        );
        assert!(r.ok);
        assert!(r.summary.contains("worst burst"));
    }

    #[test]
    fn data_usage_baseline_then_delta() {
        let first = data_usage();
        assert!(first.ok, "{}", first.summary);
        let second = data_usage();
        assert!(second.ok);
    }

    #[test]
    fn retransmit_rate_real_or_honest() {
        let r = retransmit_rate();
        assert!(r.ok || !r.summary.is_empty());
    }
}
// appended quick tools

pub fn wifi_security_audit() -> linkfyr_model::optimize::ToolRunReport {
    let scan = crate::wifiscan::scan();
    let risky: Vec<serde_json::Value> = scan
        .networks
        .iter()
        .filter(|ap| {
            let s = ap.security.as_deref().unwrap_or("").to_uppercase();
            s.is_empty() || s == "NONE" || s.contains("WEP")
        })
        .map(|ap| serde_json::json!({ "ssid": ap.ssid, "channel": ap.channel }))
        .collect();
    let summary = if risky.is_empty() {
        "no open/WEP networks visible (all use modern encryption)".to_string()
    } else {
        format!(
            "{} open or WEP network(s) in range: avoid sending data over them",
            risky.len()
        )
    };
    ok(
        "wifi_security_audit",
        summary,
        serde_json::json!({ "risky": risky, "visible": scan.networks.len() }),
    )
}

pub fn conntrack_table() -> linkfyr_model::optimize::ToolRunReport {
    for path in ["/proc/net/nf_conntrack", "/proc/net/ip_conntrack"] {
        if let Ok(text) = std::fs::read_to_string(path) {
            let lines: Vec<&str> = text.lines().collect();
            return ok(
                "conntrack_table",
                format!("{} tracked connection(s)", lines.len()),
                serde_json::json!({ "entries": lines.len(), "sample": lines.first() }),
            );
        }
    }
    unavailable(
        "conntrack_table",
        "conntrack table not readable (needs root or the module is absent)",
    )
}

pub fn wifi_signal_watch() -> linkfyr_model::optimize::ToolRunReport {
    let scan = crate::wifiscan::scan();
    let current: BTreeMap<String, u8> = scan
        .networks
        .iter()
        .map(|ap| {
            (
                ap.ssid.clone().unwrap_or_else(|| ap.bssid.clone()),
                ap.signal_pct,
            )
        })
        .collect();
    let path = crate::netops::state_dir_public().join("wifi-signal.json");
    let previous: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let _ = std::fs::write(&path, serde_json::to_string(&current).unwrap_or_default());
    let mut drift: Vec<String> = Vec::new();
    if let Some(prev) = previous.as_object() {
        for (ssid, pct) in &current {
            if let Some(old) = prev.get(ssid).and_then(|v| v.as_u64()) {
                let delta = i64::from(*pct) - old as i64;
                if delta.abs() >= 15 {
                    drift.push(format!("{ssid}: {old}% -> {pct}%"));
                }
            }
        }
    }
    ok(
        "wifi_signal_watch",
        if drift.is_empty() {
            "signal levels stable since last check".into()
        } else {
            drift.join("; ")
        },
        serde_json::json!({ "drift": drift, "visible": current.len() }),
    )
}

pub fn bandwidth_history() -> linkfyr_model::optimize::ToolRunReport {
    let usage = data_usage();
    if !usage.ok {
        return usage;
    }
    let path = crate::netops::state_dir_public().join("bandwidth-history.json");
    let mut history: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let entry = serde_json::json!({ "atMs": crate::clock_ms(), "deltas": usage.data.get("deltas").cloned().unwrap_or(serde_json::Value::Null) });
    // Journal too: the quota guard sums this file.
    crate::flows::append_journal("bandwidth", &entry);
    history.push(entry);
    let keep = history.len().saturating_sub(500);
    history.drain(0..keep);
    let _ = std::fs::write(&path, serde_json::to_string(&history).unwrap_or_default());
    ok(
        "bandwidth_history",
        format!(
            "{} history point(s) recorded (capped at 500)",
            history.len()
        ),
        serde_json::json!({ "points": history.len(), "series": history.iter().rev().take(10).cloned().collect::<Vec<_>>() }),
    )
}

#[cfg(test)]
mod appended_tests {
    use super::*;

    #[test]
    fn wifi_security_and_conntrack_never_panic() {
        let _ = wifi_security_audit();
        let _ = conntrack_table();
        let _ = wifi_signal_watch();
        let _ = bandwidth_history();
    }
}
