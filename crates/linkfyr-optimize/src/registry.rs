//! The tool registry: the single source of truth for every module in
//! the Optimization Toolkit. The catalog below is what capabilities,
//! the CLI, and the UI enumerate; `run` dispatches implemented tools
//! through the generic `ToolRunReport` envelope so adding tool #101
//! never touches the wire contract again.
//!
//! Catalog rule (honesty): every entry in `catalog()` runs for real
//! today on at least one supported platform. Entries in
//! `planned_catalog()` are named, designed modules with their real
//! mechanism chosen; they are listed so the 100+ roadmap is explicit
//! and preserved, and they are *never* reported as available.

use std::collections::BTreeMap;

use linkfyr_model::optimize::{TextReport, ToolDescriptor, ToolRunReport};

use crate::{bloat, dns, dnskit, probes, routescan, speedtest, sysnet, web};

/// Every implemented tool: (id, name, group, blurb, takes_target).
pub fn catalog() -> Vec<ToolDescriptor> {
    let d = |id: &str, name: &str, group: &str, blurb: &str, target: bool| ToolDescriptor {
        id: id.into(),
        name: name.into(),
        group: group.into(),
        blurb: blurb.into(),
        takes_target: target,
    };
    vec![
        d(
            "dns_benchmark",
            "DNS benchmark",
            "Latency",
            "Rank resolvers by measured lookup time",
            false,
        ),
        d(
            "dns_apply",
            "Apply DNS",
            "Control",
            "Set the OS resolver list (captured restore)",
            false,
        ),
        d(
            "doh_benchmark",
            "DoH benchmark",
            "Latency",
            "Rank DNS-over-HTTPS providers; UDP-53-blocked fallback",
            false,
        ),
        d(
            "dns_lookup",
            "DNS lookup",
            "Latency",
            "Resolve A/AAAA/PTR/TXT/MX/CNAME/SRV records",
            true,
        ),
        d(
            "reverse_lookup",
            "Reverse DNS",
            "Latency",
            "IP address to hostname via PTR",
            true,
        ),
        d(
            "resolver_consistency",
            "Resolver agreement",
            "Security",
            "Detect DNS spoofing via cross-resolver comparison",
            true,
        ),
        d(
            "route_scan",
            "Route scan",
            "Latency",
            "IPv4 vs IPv6 path comparison with controls",
            true,
        ),
        d(
            "latency_monitor",
            "Latency monitor",
            "Latency",
            "Sustained probe series with p50/p95/jitter/loss",
            true,
        ),
        d(
            "jitter_burst",
            "Jitter burst",
            "Latency",
            "20 rapid probes; IQR jitter measurement",
            true,
        ),
        d(
            "icmp_ping",
            "Ping (ICMP)",
            "Latency",
            "One real ICMP echo via the OS ping binary",
            true,
        ),
        d(
            "gateway_latency",
            "Gateway latency",
            "Latency",
            "Ping the default gateway; first-hop health",
            false,
        ),
        d(
            "ipv6_readiness",
            "IPv6 readiness",
            "Latency",
            "AAAA resolution plus a real IPv6 connect",
            true,
        ),
        d(
            "bloat",
            "Bufferbloat",
            "Latency",
            "Added lag under load, graded A+ to F",
            false,
        ),
        d(
            "http_ttfb",
            "HTTP time-to-first-byte",
            "Latency",
            "Full request path timing for a URL",
            true,
        ),
        d(
            "tls_check",
            "TLS check",
            "Security",
            "Handshake and certificate presence for a URL",
            true,
        ),
        d(
            "speedtest",
            "Speed test",
            "Throughput",
            "Real download/upload/latency measurement",
            false,
        ),
        d(
            "port_scan",
            "Port scan",
            "Security",
            "Bounded TCP connect scan (diagnostics, max 256)",
            true,
        ),
        d(
            "connections",
            "Per-app connections",
            "Security",
            "Which process talks where, from the OS socket table",
            false,
        ),
        d(
            "listening_ports",
            "Listening ports",
            "Security",
            "Every local listening socket and its owner",
            false,
        ),
        d(
            "mtu",
            "MTU discovery",
            "Throughput",
            "Path MTU via DF pings; black-hole finder",
            true,
        ),
        d(
            "wifi_scan",
            "Wi-Fi channels",
            "Environment",
            "Visible networks, congestion, best channel",
            false,
        ),
        d(
            "tcp_audit",
            "TCP audit",
            "Environment",
            "Stack settings vs latency-friendly values",
            false,
        ),
        d(
            "route_audit",
            "Route audit",
            "Environment",
            "Multi-WAN and VPN full-tunnel detection",
            false,
        ),
        d(
            "arp_table",
            "ARP table",
            "Environment",
            "Layer-2 neighbors with MAC addresses",
            false,
        ),
        d(
            "proxy_config",
            "Proxy configuration",
            "Environment",
            "System and environment proxy settings",
            false,
        ),
        d(
            "hosts_file",
            "Hosts file",
            "Environment",
            "Parsed custom name mappings",
            false,
        ),
        d(
            "flush_dns",
            "Flush DNS",
            "Repair",
            "Clear the system resolver cache",
            false,
        ),
        d(
            "public_ip",
            "Public IP (opt-in)",
            "Security",
            "Your egress IP as the Internet sees it",
            false,
        ),
        // Session 4: netops, svcprobe, diag module families.
        d(
            "dhcp_renew",
            "DHCP renew",
            "Repair",
            "Renew leases (ipconfig/nmcli/dhclient, elevated)",
            false,
        ),
        d(
            "dhcp_release",
            "DHCP release",
            "Repair",
            "Release a lease before switching networks (elevated)",
            false,
        ),
        d(
            "adapter_reset",
            "Adapter reset",
            "Repair",
            "Disable/enable a NIC (elevated)",
            false,
        ),
        d(
            "winsock_reset",
            "Winsock reset",
            "Repair",
            "Windows LSP reset, reboot-completes (elevated)",
            false,
        ),
        d(
            "tcp_tuning_apply",
            "TCP tuning apply",
            "Control",
            "Apply the TCP audit's fixes (elevated, explicit)",
            false,
        ),
        d(
            "hotspot_share",
            "Hotspot (ICS)",
            "Control",
            "List/enable Windows connection sharing (elevated)",
            false,
        ),
        d(
            "route_change_watch",
            "Route watch",
            "Security",
            "Diff routing table vs stored baseline",
            false,
        ),
        d(
            "dns_change_watch",
            "DNS watch",
            "Security",
            "Alert on resolver setting changes",
            false,
        ),
        d(
            "arp_spoof_check",
            "ARP watch",
            "Security",
            "Gateway MAC change alarm",
            false,
        ),
        d(
            "channel_history",
            "Channel history",
            "Environment",
            "Wi-Fi environment diff",
            false,
        ),
        d(
            "pmtud_watch",
            "PMTUD watch",
            "Throughput",
            "Repeat MTU probes; catch path changes",
            true,
        ),
        d(
            "clock_skew",
            "Clock skew",
            "Latency",
            "NTP offset vs system clock",
            false,
        ),
        d(
            "ntp_sync",
            "NTP sync",
            "Repair",
            "Force OS time resync (elevated)",
            false,
        ),
        d(
            "wake_on_lan",
            "Wake-on-LAN",
            "Control",
            "Magic packet via broadcast UDP",
            false,
        ),
        d(
            "upnp_map",
            "UPnP map",
            "Security",
            "Router port mappings via SSDP + SOAP",
            false,
        ),
        d(
            "whois_lookup",
            "Whois (RDAP)",
            "Security",
            "Domain registration data",
            true,
        ),
        d(
            "asn_route_lookup",
            "ASN lookup",
            "Security",
            "Who announces this IP (Team Cymru DNS)",
            true,
        ),
        d(
            "ip_geolocation",
            "IP geolocation (opt-in)",
            "Security",
            "Approximate IP location (external query)",
            true,
        ),
        d(
            "dnssec_check",
            "DNSSEC check",
            "Security",
            "Resolver validation posture (AD bit)",
            true,
        ),
        d(
            "dns_leak",
            "DNS leak test",
            "Security",
            "UDP-53 interception detector",
            false,
        ),
        d(
            "http_redirect_trace",
            "Redirect trace",
            "Security",
            "Follow a URL's redirect chain",
            true,
        ),
        d(
            "headers_audit",
            "Security headers",
            "Security",
            "HSTS/CSP presence for a URL",
            true,
        ),
        d(
            "stack_snapshot",
            "Stack snapshot",
            "Environment",
            "Whole-stack JSON dump",
            false,
        ),
        d(
            "diff_snapshot",
            "Stack diff",
            "Environment",
            "Diff against stored snapshot",
            false,
        ),
        d(
            "export_report",
            "Export report",
            "Environment",
            "Write diagnostics bundle to disk",
            false,
        ),
        d(
            "self_test",
            "Self test",
            "Repair",
            "Verify environment assumptions",
            false,
        ),
        d(
            "iperf_endpoint",
            "Throughput server",
            "Throughput",
            "Local transfer endpoint for LAN tests",
            false,
        ),
        d(
            "sockstat",
            "Socket stats",
            "Environment",
            "TCP state histogram",
            false,
        ),
        d(
            "smb_exposure",
            "SMB exposure",
            "Security",
            "Port 445 reachability check",
            false,
        ),
        d(
            "rdp_exposure",
            "RDP exposure",
            "Security",
            "Remote-desktop exposure check",
            false,
        ),
        d(
            "llmnr_check",
            "LLMNR/mDNS",
            "Security",
            "Local name-resolution exposure",
            false,
        ),
        d(
            "vpn_detect",
            "VPN detection",
            "Security",
            "Tunnels + full-tunnel patterns",
            false,
        ),
        d(
            "router_hop_map",
            "Gateway map",
            "Environment",
            "Gateway with MAC from ARP",
            false,
        ),
        d(
            "multicast_snoop",
            "Multicast",
            "Environment",
            "Group memberships",
            false,
        ),
        d(
            "lease_info",
            "DHCP lease info",
            "Environment",
            "Lease times per platform",
            false,
        ),
        d(
            "dns_cache_stats",
            "DNS cache stats",
            "Environment",
            "Resolver cache statistics",
            false,
        ),
        d(
            "retransmit_rate",
            "Retransmit rate",
            "Latency",
            "Loss from OS TCP counters",
            false,
        ),
        d(
            "tcp_info",
            "TCP diagnostics",
            "Latency",
            "Per-socket rtt/cwnd (Linux)",
            false,
        ),
        d(
            "qdisc_audit",
            "qdisc audit",
            "Environment",
            "Queueing discipline review",
            false,
        ),
        d(
            "ecn_check",
            "ECN check",
            "Latency",
            "Host ECN posture",
            false,
        ),
        d(
            "voip_mos",
            "VoIP MOS",
            "Latency",
            "Call-quality estimate from live probes",
            true,
        ),
        d(
            "loss_bursts",
            "Loss bursts",
            "Latency",
            "Consecutive-loss patterns",
            true,
        ),
        d(
            "throughput_variance",
            "Throughput variance",
            "Throughput",
            "3-burst stability (CoV)",
            false,
        ),
        d(
            "geo_route_compare",
            "Service latency map",
            "Latency",
            "Medians across global services",
            false,
        ),
        d(
            "data_usage",
            "Data usage",
            "Throughput",
            "Byte deltas since last check",
            false,
        ),
        d(
            "bandwidth_history",
            "Bandwidth history",
            "Throughput",
            "Rolling usage journal",
            false,
        ),
        d(
            "iface_deep",
            "Interface deep dive",
            "Environment",
            "Live interface state dump",
            false,
        ),
        d(
            "wifi_security_audit",
            "Wi-Fi security audit",
            "Security",
            "Open/WEP networks in range",
            false,
        ),
        d(
            "wifi_signal_watch",
            "Wi-Fi signal watch",
            "Environment",
            "Signal drift alerts",
            false,
        ),
        d(
            "conntrack_table",
            "Conntrack",
            "Security",
            "Connection tracking table (best-effort)",
            false,
        ),
        d(
            "captive_portal_probe",
            "Captive portal",
            "Latency",
            "Hotel/cafe login interception detector",
            false,
        ),
        d(
            "metric_audit",
            "Route metric audit",
            "Environment",
            "Default-route priority review (VPN metric steal)",
            false,
        ),
        // Session 5: flows + cert module families.
        d(
            "cert_expiry",
            "Certificate expiry",
            "Security",
            "TLS cert lifetime via a real rustls handshake",
            true,
        ),
        d(
            "connection_history",
            "Connection history",
            "Security",
            "Append + view the per-app socket journal",
            false,
        ),
        d(
            "session_journal",
            "Session journal",
            "Security",
            "Long-run destination ledger (JSONL)",
            false,
        ),
        d(
            "per_destination_history",
            "Destination history",
            "Security",
            "Top contacted hosts over the journal",
            false,
        ),
        d(
            "net_time_machine",
            "Network Time Machine",
            "Environment",
            "Timeline across every journal (data backend)",
            false,
        ),
        d(
            "kill_switch",
            "Kill-switch status",
            "Security",
            "Leak audit while a VPN is up",
            false,
        ),
        d(
            "vpn_split_tunnel",
            "Split tunnel (advisory)",
            "Control",
            "Exact route commands to pin a host outside the VPN",
            true,
        ),
        d(
            "quota_guard",
            "Quota guard",
            "Throughput",
            "Usage vs your MiB budget from the journal",
            false,
        ),
        d(
            "metered_guard",
            "Metered guard",
            "Control",
            "Metered-link detection (status + advisory)",
            false,
        ),
        d(
            "latency_race",
            "Latency race",
            "Latency",
            "Same target per interface, SO_BINDTODEVICE (Linux)",
            true,
        ),
        d(
            "speed_compare",
            "Interface race",
            "Throughput",
            "Per-interface mini-download compare (Linux)",
            false,
        ),
        d(
            "failover_test",
            "Failover readiness",
            "Latency",
            "Path penalty if the best link drops",
            true,
        ),
        d(
            "bond_simulation",
            "Bond simulation",
            "Throughput",
            "Aggregate vs single-flow math (honest)",
            false,
        ),
        d(
            "stream_health",
            "Stream health",
            "Latency",
            "Sustained read rate; buffer-stall risk",
            true,
        ),
        d(
            "game_rtt_guard",
            "Game RTT guard",
            "Latency",
            "30-probe spike analysis with verdict",
            true,
        ),
        // Enforcement v0 (linkfyr-enforce; dispatched by the engine).
        d(
            "app_block",
            "App block",
            "Control",
            "Per-program firewall block (Windows netsh; Linux per-owner, labeled)",
            true,
        ),
        d(
            "app_allow",
            "App allow",
            "Control",
            "Remove a LinkFYR-owned block rule",
            true,
        ),
        d(
            "app_rule_list",
            "App rules list",
            "Control",
            "Every firewall rule this tool created",
            false,
        ),
        d(
            "app_rule_remove",
            "Rules remove",
            "Control",
            "Remove all LinkFYR-owned rules (owner-tag scoped)",
            false,
        ),
        d(
            "kill_switch_arm",
            "Kill switch arm",
            "Control",
            "Fail-closed outbound hold (established + loopback pass)",
            false,
        ),
        d(
            "kill_switch_disarm",
            "Kill switch disarm",
            "Control",
            "Restore normal outbound policy",
            false,
        ),
        d(
            "vpn_split_enforce",
            "Split tunnel apply",
            "Control",
            "Execute the advisory route: pin a host outside the VPN",
            true,
        ),
        d(
            "shaping",
            "Shaping apply",
            "Control",
            "fq_codel root queue (Linux tc; latency-first)",
            false,
        ),
        d(
            "shaping_remove",
            "Shaping remove",
            "Control",
            "Remove the LinkFYR qdisc",
            false,
        ),
        d(
            "traffic_priority",
            "Priority",
            "Control",
            "prio qdisc + DSCP EF filter (Linux tc)",
            false,
        ),
        d(
            "per_app_limits",
            "Per-app rate limits",
            "Control",
            "Rate ceilings per owner via nftables meters (Linux); Windows needs WFP (honestly reported)",
            false,
        ),
        d(
            "night_shift",
            "Night shift",
            "Control",
            "Time-window enforcement with midnight wrap: check, then apply throttle/block",
            false,
        ),
        d(
            "multiwan_plan",
            "Multi-WAN plan",
            "Control",
            "Rank interfaces by health + activity; ordered failover plan",
            false,
        ),
    ]
}

/// Planned modules. Empty as of this release: every named module in the
/// catalog has a real implementation on at least one platform.
pub fn planned_catalog() -> Vec<ToolDescriptor> {
    vec![]
}
pub fn catalog_size() -> (usize, usize) {
    (catalog().len(), catalog().len() + planned_catalog().len())
}

fn report(
    tool: &str,
    ok: bool,
    summary: String,
    data: serde_json::Value,
    took_ms: u64,
) -> ToolRunReport {
    ToolRunReport {
        tool: tool.into(),
        ok,
        summary,
        took_ms,
        data,
    }
}

fn target_addr(
    params: &BTreeMap<String, String>,
    default_port: u16,
) -> Result<std::net::SocketAddr, String> {
    let target = params.get("target").ok_or("missing 'target' parameter")?;
    if !probes::valid_target(target) {
        return Err(format!("invalid target: {target}"));
    }
    let port: u16 = params
        .get("port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(default_port);
    probes::resolve_one(target, port).ok_or_else(|| format!("could not resolve {target}:{port}"))
}

/// Run one registry tool. Unknown ids and bad parameters produce an
/// `ok=false` report (never a panic, never a fake success).
pub fn run(tool: &str, params: &BTreeMap<String, String>) -> ToolRunReport {
    let started = std::time::Instant::now();
    let elapsed = || started.elapsed().as_millis() as u64;
    let simple = |text: String, ms: Option<f64>| {
        serde_json::to_value(TextReport { text, ms }).expect("serialize")
    };

    // The closure isolates `?` and early `return Ok(..)` to a Result
    // context without infecting the outer report-wrapping signature.
    let result: Result<ToolRunReport, String> = (|| match tool {
        "dns_benchmark" => {
            let r = dns::bench_default();
            let rec = r.recommended.clone().unwrap_or_default();
            Ok(report(
                tool,
                r.results.iter().any(|x| x.success),
                format!(
                    "{} resolvers measured; fastest {}",
                    r.results.len(),
                    if rec.is_empty() {
                        "none answered".into()
                    } else {
                        rec
                    }
                ),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "doh_benchmark" => {
            let r = dnskit::doh_benchmark();
            Ok(report(
                tool,
                r.recommended.is_some(),
                r.verdict.clone(),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "dns_lookup" => {
            let domain = params
                .get("domain")
                .or_else(|| params.get("target"))
                .cloned()
                .ok_or("missing 'domain' parameter")?;
            let record = params.get("record").map_or("A", String::as_str);
            let rtype = match record.to_uppercase().as_str() {
                "A" => 1,
                "CNAME" => 5,
                "PTR" => 12,
                "MX" => 15,
                "TXT" => 16,
                "AAAA" => 28,
                "SRV" => 33,
                other => {
                    return Ok(report(
                        tool,
                        false,
                        format!("unsupported record {other}"),
                        serde_json::Value::Null,
                        elapsed(),
                    ));
                }
            };
            let server =
                dnskit::system_resolver().unwrap_or_else(|| "1.1.1.1:53".parse().expect("cf"));
            let answers = dnskit::lookup(server, &domain, rtype)?;
            let texts: Vec<String> = answers.iter().map(|a| a.text.clone()).collect();
            Ok(report(
                tool,
                !texts.is_empty(),
                format!(
                    "{domain} {record}: {}",
                    if texts.is_empty() {
                        "no records".into()
                    } else {
                        texts.join(", ")
                    }
                ),
                serde_json::to_value(&texts).expect("serialize"),
                elapsed(),
            ))
        }
        "reverse_lookup" => {
            let ip = params
                .get("target")
                .cloned()
                .ok_or("missing 'target' (an IP)")?;
            let ip: std::net::IpAddr = ip.parse().map_err(|_| "target must be an IP")?;
            let name = dnskit::reverse_lookup(ip)?;
            Ok(report(
                tool,
                true,
                format!("{ip} -> {name}"),
                simple(name, None),
                elapsed(),
            ))
        }
        "resolver_consistency" => {
            let domain = params.get("target").cloned().ok_or("missing 'target'")?;
            let (ok, detail) = dnskit::resolver_consistency(&domain);
            Ok(report(
                tool,
                ok,
                detail.clone(),
                simple(detail, None),
                elapsed(),
            ))
        }
        "route_scan" => {
            let target = params.get("target").cloned().ok_or("missing 'target'")?;
            let r = routescan::scan(&target, 5);
            Ok(report(
                tool,
                r.paths.iter().any(|p| p.success > 0),
                r.verdict.clone(),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "latency_monitor" => {
            let addr = target_addr(params, 443)?;
            let samples: u32 = params
                .get("samples")
                .and_then(|s| s.parse().ok())
                .unwrap_or(20)
                .clamp(1, 600);
            let (_, summary, loss) = probes::latency_monitor(addr, samples, 100);
            match summary {
                Some(s) => Ok(report(
                    tool,
                    true,
                    format!(
                        "median {:.1} ms, p95 {:.1} ms, jitter {:.1} ms, loss {loss:.0}%",
                        s.median_ms, s.p95_ms, s.jitter_ms
                    ),
                    serde_json::json!({ "medianMs": s.median_ms, "minMs": s.min_ms, "p95Ms": s.p95_ms, "jitterMs": s.jitter_ms, "lossPct": loss }),
                    elapsed(),
                )),
                None => Ok(report(
                    tool,
                    false,
                    "every probe failed".into(),
                    serde_json::Value::Null,
                    elapsed(),
                )),
            }
        }
        "jitter_burst" => {
            let addr = target_addr(params, 443)?;
            let (iqr, mad) = probes::jitter_burst(addr);
            Ok(report(
                tool,
                iqr.is_finite(),
                format!(
                    "IQR jitter {iqr:.2} ms (mean-adj {mad:.2} ms)",
                    mad = mad.unwrap_or(0.0)
                ),
                serde_json::json!({ "iqrMs": iqr, "meanAbsDeltaMs": mad }),
                elapsed(),
            ))
        }
        "icmp_ping" => {
            let target = params.get("target").cloned().ok_or("missing 'target'")?;
            let ms = probes::icmp_ping(&target)?;
            Ok(report(
                tool,
                true,
                format!("{ms:.1} ms to {target}"),
                simple(format!("{ms:.1} ms"), Some(ms)),
                elapsed(),
            ))
        }
        "gateway_latency" => {
            let gw = probes::default_gateway().ok_or("no default gateway found")?;
            let ms = probes::icmp_ping(&gw)?;
            Ok(report(
                tool,
                true,
                format!("{ms:.1} ms to gateway {gw}"),
                simple(format!("{gw}: {ms:.1} ms"), Some(ms)),
                elapsed(),
            ))
        }
        "ipv6_readiness" => {
            let target = params.get("target").cloned().ok_or("missing 'target'")?;
            let (ok, detail) = probes::ipv6_readiness(&target);
            Ok(report(
                tool,
                ok,
                detail.clone(),
                simple(detail, None),
                elapsed(),
            ))
        }
        "bloat" => {
            let mut opts = bloat::BloatOptions::default();
            if let Some(secs) = params.get("seconds").and_then(|s| s.parse::<u32>().ok()) {
                opts.phase_secs = secs.clamp(1, 60);
            }
            let r = bloat::run(&opts);
            Ok(report(
                tool,
                r.down_mbps > 0.0 || r.up_mbps > 0.0,
                format!(
                    "down +{:.0} ms ({}), up +{:.0} ms ({})",
                    r.down_added_ms,
                    r.down_grade.label(),
                    r.up_added_ms,
                    r.up_grade.label()
                ),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "http_ttfb" => {
            let url = params
                .get("url")
                .or_else(|| params.get("target"))
                .cloned()
                .ok_or("missing 'url'")?;
            let ms = web::http_ttfb(&url)?;
            Ok(report(
                tool,
                true,
                format!("{ms:.0} ms to first byte"),
                simple(format!("{ms:.0} ms"), Some(ms)),
                elapsed(),
            ))
        }
        "tls_check" => {
            let url = params
                .get("url")
                .or_else(|| params.get("target"))
                .cloned()
                .ok_or("missing 'url'")?;
            let out = web::tls_check(&url);
            Ok(report(
                tool,
                out.ok,
                out.detail.clone(),
                serde_json::json!({ "ok": out.ok, "status": out.status, "certPresent": out.cert_present, "detail": out.detail }),
                elapsed(),
            ))
        }
        "public_ip" => {
            let (ip, loc) = web::public_ip()?;
            Ok(report(
                tool,
                true,
                format!("public IP {ip} ({loc})"),
                simple(format!("{ip} ({loc})"), None),
                elapsed(),
            ))
        }
        "speedtest" => {
            let mut opts = speedtest::SpeedtestOptions::default();
            if let Some(e) = params.get("endpoint") {
                opts.endpoint.clone_from(e);
            }
            let r = speedtest::run(&opts);
            Ok(report(
                tool,
                r.latency_ms.is_some(),
                format!(
                    "down {:.1} Mbps, up {:.1} Mbps",
                    r.download_mbps, r.upload_mbps
                ),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "port_scan" => {
            let target = params.get("target").cloned().ok_or("missing 'target'")?;
            let ports: Vec<u16> = match params.get("ports") {
                Some(spec) => spec
                    .split(',')
                    .filter_map(|p| p.trim().parse().ok())
                    .collect(),
                None => probes::COMMON_PORTS.to_vec(),
            };
            if ports.is_empty() {
                return Ok(report(
                    tool,
                    false,
                    "no valid ports in 'ports'".into(),
                    serde_json::Value::Null,
                    elapsed(),
                ));
            }
            let r = probes::port_scan(&target, &ports);
            Ok(report(
                tool,
                true,
                format!("{} open of {} scanned", r.open_count, r.scanned),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "connections" => {
            let r = sysnet::connections();
            Ok(report(
                tool,
                !r.connections.is_empty(),
                format!(
                    "{} sockets, {} processes",
                    r.connections.len(),
                    r.groups.len()
                ),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "listening_ports" => {
            let l = sysnet::listening_ports();
            Ok(report(
                tool,
                !l.is_empty(),
                format!("{} sockets listening", l.len()),
                serde_json::to_value(l).expect("serialize"),
                elapsed(),
            ))
        }
        "mtu" => {
            let target = params.get("target").cloned().ok_or("missing 'target'")?;
            let r = crate::mtu::probe(&target, 1500);
            Ok(report(
                tool,
                r.path_mtu > 0,
                format!("path MTU {}: {}", r.path_mtu, r.verdict),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "wifi_scan" => {
            let r = crate::wifiscan::scan();
            Ok(report(
                tool,
                !r.networks.is_empty(),
                r.explanation.clone(),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "tcp_audit" => {
            let r = crate::tcpaudit::audit();
            Ok(report(
                tool,
                !r.checks.is_empty(),
                r.summary.clone(),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "route_audit" => {
            let r = crate::routeaudit::audit();
            let findings = r.anomalies.len();
            Ok(report(
                tool,
                !r.entries.is_empty() || findings > 0,
                format!("{} routes, {} findings", r.entries.len(), findings),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        "arp_table" => {
            let t = sysnet::arp_table();
            let rendered = t
                .iter()
                .map(|(ip, mac, s)| format!("{ip}  {mac}  {s}"))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(report(
                tool,
                !t.is_empty(),
                format!("{} neighbors", t.len()),
                simple(
                    if rendered.is_empty() {
                        "(empty)".into()
                    } else {
                        rendered
                    },
                    None,
                ),
                elapsed(),
            ))
        }
        "proxy_config" => {
            let text = sysnet::proxy_configuration();
            Ok(report(
                tool,
                true,
                text.clone(),
                simple(text, None),
                elapsed(),
            ))
        }
        "hosts_file" => {
            let content =
                std::fs::read_to_string(sysnet::hosts_path()).map_err(|e| e.to_string())?;
            let entries = sysnet::parse_hosts(&content);
            let rendered = entries
                .iter()
                .map(|(ip, h)| format!("{ip}  {h}"))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(report(
                tool,
                true,
                format!("{} custom mappings", entries.len()),
                simple(rendered, None),
                elapsed(),
            ))
        }
        "flush_dns" => {
            let r = crate::repair::flush_dns();
            Ok(report(
                tool,
                r.outcome == "applied",
                format!("{}: {}", r.outcome, r.detail),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        // ---- netops family ----
        "dhcp_renew" => Ok(crate::netops::dhcp_renew(params)),
        "dhcp_release" => Ok(crate::netops::dhcp_release(params)),
        "adapter_reset" => Ok(crate::netops::adapter_reset(params)),
        "winsock_reset" => Ok(crate::netops::winsock_reset()),
        "tcp_tuning_apply" => Ok(crate::netops::tcp_tuning_apply()),
        "hotspot_share" => Ok(crate::netops::hotspot_share(params)),
        "route_change_watch" => Ok(crate::netops::route_change_watch()),
        "dns_change_watch" => Ok(crate::netops::dns_change_watch()),
        "arp_spoof_check" => Ok(crate::netops::arp_spoof_check()),
        "channel_history" => Ok(crate::netops::channel_history()),
        "pmtud_watch" | "mtu_monitor" => Ok(crate::netops::pmtud_watch(params)),
        // ---- svcprobe family ----
        "clock_skew" => Ok(crate::svcprobe::clock_skew()),
        "ntp_sync" => Ok(crate::svcprobe::ntp_sync()),
        "wake_on_lan" => {
            let mac = params
                .get("target")
                .or_else(|| params.get("mac"))
                .cloned()
                .ok_or("missing 'target' (a MAC address)")?;
            Ok(crate::svcprobe::wake_on_lan(&mac))
        }
        "upnp_map" => Ok(crate::svcprobe::upnp_map()),
        "whois_lookup" => {
            let target = params
                .get("target")
                .cloned()
                .ok_or("missing 'target' domain")?;
            Ok(crate::svcprobe::whois_lookup(&target))
        }
        "asn_route_lookup" => {
            let ip = params
                .get("target")
                .cloned()
                .ok_or("missing 'target' (an IPv4)")?;
            Ok(crate::svcprobe::asn_route_lookup(&ip))
        }
        "ip_geolocation" => {
            let ip = params.get("target").cloned().unwrap_or_default();
            Ok(crate::svcprobe::ip_geolocation(&ip))
        }
        "dnssec_check" => {
            let domain = params
                .get("target")
                .cloned()
                .unwrap_or_else(|| "cloudflare.com".into());
            Ok(crate::svcprobe::dnssec_check(&domain))
        }
        "dns_leak" => Ok(crate::svcprobe::dns_leak()),
        "http_redirect_trace" => {
            let url = params
                .get("url")
                .or_else(|| params.get("target"))
                .cloned()
                .ok_or("missing 'url'")?;
            Ok(crate::svcprobe::http_redirect_trace(&url))
        }
        "headers_audit" => {
            let url = params
                .get("url")
                .or_else(|| params.get("target"))
                .cloned()
                .ok_or("missing 'url'")?;
            Ok(crate::svcprobe::headers_audit(&url))
        }
        "stack_snapshot" => Ok(crate::svcprobe::stack_snapshot()),
        "diff_snapshot" => Ok(crate::svcprobe::diff_snapshot()),
        "export_report" => Ok(crate::svcprobe::export_report()),
        "self_test" => Ok(crate::svcprobe::self_test()),
        "iperf_endpoint" => Ok(crate::svcprobe::iperf_endpoint(params)),
        "captive_portal_probe" => Ok(crate::svcprobe::captive_portal_probe()),
        // ---- diag family ----
        "sockstat" => Ok(crate::diag::sockstat()),
        "smb_exposure" => Ok(crate::diag::smb_exposure()),
        "rdp_exposure" => Ok(crate::diag::rdp_exposure()),
        "llmnr_check" => Ok(crate::diag::llmnr_check()),
        "vpn_detect" => Ok(crate::diag::vpn_detect()),
        "router_hop_map" => Ok(crate::diag::router_hop_map()),
        "multicast_snoop" => Ok(crate::diag::multicast_snoop()),
        "lease_info" => Ok(crate::diag::lease_info()),
        "dns_cache_stats" => Ok(crate::diag::dns_cache_stats()),
        "retransmit_rate" => Ok(crate::diag::retransmit_rate()),
        "tcp_info" => Ok(crate::diag::tcp_info()),
        "qdisc_audit" => Ok(crate::diag::qdisc_audit()),
        "ecn_check" => Ok(crate::diag::ecn_check()),
        "voip_mos" => Ok(crate::diag::voip_mos(params)),
        "loss_bursts" => Ok(crate::diag::loss_bursts(params)),
        "throughput_variance" => Ok(crate::diag::throughput_variance(params)),
        "geo_route_compare" => Ok(crate::diag::geo_route_compare()),
        "data_usage" => Ok(crate::diag::data_usage()),
        "bandwidth_history" => Ok(crate::diag::bandwidth_history()),
        "iface_deep" => Ok(crate::diag::iface_deep()),
        "wifi_security_audit" => Ok(crate::diag::wifi_security_audit()),
        "wifi_signal_watch" => Ok(crate::diag::wifi_signal_watch()),
        "conntrack_table" => Ok(crate::diag::conntrack_table()),
        "metric_audit" => {
            let r = crate::routeaudit::audit();
            let defaults: Vec<&linkfyr_model::optimize::RouteEntry> = r
                .entries
                .iter()
                .filter(|e| e.destination == "0.0.0.0/0")
                .collect();
            let mut verdict = "single default route; priority order fine".to_string();
            if defaults.len() > 1 {
                let named: Vec<String> = defaults
                    .iter()
                    .map(|e| {
                        format!(
                            "{} via {} (metric {})",
                            e.interface.clone().unwrap_or_default(),
                            e.gateway.clone().unwrap_or_default(),
                            e.metric.unwrap_or(0)
                        )
                    })
                    .collect();
                verdict = format!(
                    "{} default routes; lowest metric wins: {}",
                    defaults.len(),
                    named.join(" | ")
                );
            }
            Ok(report(
                tool,
                !r.entries.is_empty(),
                verdict,
                serde_json::json!({ "defaults": defaults.len(), "routes": r.entries.len() }),
                elapsed(),
            ))
        }
        "dns_apply" => {
            let servers: Vec<std::net::IpAddr> = params
                .get("servers")
                .map(|s| s.split(',').filter_map(|p| p.trim().parse().ok()).collect())
                .unwrap_or_default();
            if servers.is_empty() {
                return Ok(report(
                    tool,
                    false,
                    "missing 'servers' parameter (comma-separated IPs)".into(),
                    serde_json::Value::Null,
                    elapsed(),
                ));
            }
            let iface = params.get("interface").cloned();
            let r = dns::apply(&servers, iface.as_deref());
            Ok(report(
                tool,
                r.outcome == "applied",
                format!("{}: {}", r.outcome, r.detail),
                serde_json::to_value(r).expect("serialize"),
                elapsed(),
            ))
        }
        // ---- cert + flows families ----
        "cert_expiry" => {
            let host = params
                .get("target")
                .cloned()
                .ok_or("missing 'target' host")?;
            Ok(crate::cert::cert_expiry(&host))
        }
        "connection_history" => Ok(crate::flows::connection_history()),
        "session_journal" | "per_destination_history" => Ok(crate::flows::session_journal()),
        "net_time_machine" => Ok(crate::flows::net_time_machine()),
        "kill_switch" => Ok(crate::flows::kill_switch_status()),
        "vpn_split_tunnel" => Ok(crate::flows::vpn_split_tunnel(params)),
        "quota_guard" => Ok(crate::flows::quota_guard(params)),
        "metered_guard" => Ok(crate::flows::metered_guard()),
        "latency_race" => Ok(crate::flows::latency_race(params)),
        "speed_compare" => Ok(crate::flows::speed_compare(params)),
        "failover_test" => Ok(crate::flows::failover_test(params)),
        "bond_simulation" => Ok(crate::flows::bond_simulation(params)),
        "stream_health" => Ok(crate::flows::stream_health(params)),
        "game_rtt_guard" => Ok(crate::flows::game_rtt_guard(params)),
        other => Ok(report(
            other,
            false,
            format!("unknown tool '{other}'; see `linkfyr optimize list`"),
            serde_json::Value::Null,
            0,
        )),
    })();

    match result {
        Ok(r) => r,
        Err(e) => report(tool, false, e, serde_json::Value::Null, elapsed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn catalog_is_large_and_well_formed() {
        let (implemented, total) = catalog_size();
        assert!(
            implemented >= 25,
            "implemented modules this session: {implemented}"
        );
        assert!(total >= 100, "named modules must exceed 100: {total}");
        let mut ids: Vec<String> = catalog().into_iter().map(|t| t.id).collect();
        ids.extend(planned_catalog().into_iter().map(|t| t.id));
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "tool ids must be unique");
        for t in catalog() {
            assert!(!t.name.is_empty() && !t.blurb.is_empty());
        }
    }

    #[test]
    fn unknown_tool_is_an_honest_failure() {
        let r = run("definitely_not_a_tool", &p(&[]));
        assert!(!r.ok);
        assert!(r.summary.contains("unknown tool"));
    }

    #[test]
    fn invalid_targets_and_params_fail_cleanly() {
        assert!(!run("icmp_ping", &p(&[("target", "; rm -rf")])).ok);
        assert!(!run("dns_lookup", &p(&[])).ok);
        assert!(!run("port_scan", &p(&[("target", "x"), ("ports", "not,ports")])).ok);
        assert!(!run("dns_apply", &p(&[])).ok);
    }

    #[test]
    fn simple_tools_run_for_real() {
        let hosts = run("hosts_file", &p(&[]));
        assert!(hosts.ok, "{}", hosts.summary);
        let proxy = run("proxy_config", &p(&[]));
        assert!(proxy.ok);

        // Real listener for latency-family tools.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port().to_string();
        let mon = run(
            "latency_monitor",
            &p(&[("target", "127.0.0.1"), ("port", &port), ("samples", "3")]),
        );
        assert!(mon.ok, "{}", mon.summary);
        assert!(mon.summary.contains("median"));

        let burst = run(
            "jitter_burst",
            &p(&[("target", "127.0.0.1"), ("port", &port)]),
        );
        assert!(burst.ok, "{}", burst.summary);
    }

    #[test]
    fn icmp_ping_tool_runs_when_ping_exists() {
        if !crate::exec::on_path("ping") {
            return;
        }
        let r = run("icmp_ping", &p(&[("target", "127.0.0.1")]));
        assert!(r.ok, "{}", r.summary);
    }

    #[test]
    fn port_scan_tool_reports_open_ports() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port().to_string();
        let r = run(
            "port_scan",
            &p(&[("target", "127.0.0.1"), ("ports", &format!("{port},1"))]),
        );
        assert!(r.ok);
        assert!(r.summary.contains("1 open"));
    }
}
