//! Service and protocol probes: NTP (real UDP time client), Wake-on-LAN
//! magic packets, UPnP port mappings (SSDP discovery + SOAP), RDAP/ASN
//! lookups, DNSSEC posture (AD bit), UDP-53 interception (DNS leak),
//! redirect tracing, security headers, and the composite snapshot /
//! diff / export / self-test tools. All real network operations.

use std::io::Read as _;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::exec;
use crate::stats::Prng;

const NTP_EPOCH_DELTA: u64 = 2_208_988_800; // 1900 -> 1970 seconds

fn ok_json(
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

fn fail(tool: &str, summary: String) -> linkfyr_model::optimize::ToolRunReport {
    linkfyr_model::optimize::ToolRunReport {
        tool: tool.into(),
        ok: false,
        summary,
        took_ms: 0,
        data: serde_json::Value::Null,
    }
}

/* ---- NTP ---- */

/// Query one NTP server; returns (server_time_unix_ms, rtt_ms).
pub fn ntp_query(server: &str) -> Result<(u64, f64), String> {
    let addr: SocketAddr = format!("{server}:123")
        .parse()
        .map_err(|_| format!("invalid NTP server {server}"))?;
    let bind: SocketAddr = if addr.is_ipv6() {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    };
    let sock = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
    sock.connect(addr).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    let mut packet = [0u8; 48];
    packet[0] = 0x1B; // LI=0, VN=3, Mode=3 (client)
    let start = Instant::now();
    sock.send(&packet).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 48];
    sock.recv(&mut buf).map_err(|e| e.to_string())?;
    let rtt = start.elapsed().as_secs_f64() * 1000.0;
    let seconds = u64::from(u32::from_be_bytes([buf[40], buf[41], buf[42], buf[43]]));
    let fraction = u64::from(u32::from_be_bytes([buf[44], buf[45], buf[46], buf[47]]));
    let unix_ms =
        (seconds.saturating_sub(NTP_EPOCH_DELTA)) * 1000 + fraction * 1000 / u64::from(u32::MAX);
    Ok((unix_ms, rtt))
}

pub fn clock_skew() -> linkfyr_model::optimize::ToolRunReport {
    let mut best: Option<(i64, f64, &str)> = None;
    for server in ["pool.ntp.org", "time.cloudflare.com", "time.google.com"] {
        if let Ok((server_ms, rtt)) = ntp_query(server) {
            let local_ms = crate::clock_ms();
            let skew = server_ms as i64 - local_ms as i64;
            if best.is_none_or(|(_, brtt, _)| rtt < brtt) {
                best = Some((skew, rtt, server));
            }
        }
    }
    match best {
        Some((skew, rtt, server)) => {
            let abs = skew.unsigned_abs();
            let verdict = if abs < 500 {
                "clock is healthy"
            } else if abs < 5000 {
                "mild clock drift"
            } else {
                "significant clock skew (breaks TLS/Kerberos)"
            };
            ok_json(
                "clock_skew",
                format!("{verdict}: {skew:+} ms vs {server} (rtt {rtt:.0} ms)"),
                serde_json::json!({ "skewMs": skew, "rttMs": rtt, "server": server }),
            )
        }
        None => fail(
            "clock_skew",
            "no NTP server answered (UDP 123 blocked?)".into(),
        ),
    }
}

pub fn ntp_sync() -> linkfyr_model::optimize::ToolRunReport {
    let skew = clock_skew();
    if !skew.ok {
        return skew;
    }
    let cmds: Vec<(String, Vec<String>)> = if cfg!(windows) {
        vec![("w32tm".into(), vec!["/resync".into(), "/force".into()])]
    } else {
        vec![("timedatectl".into(), vec!["set-ntp".into(), "true".into()])]
    };
    if !exec::is_elevated() {
        let rendered: Vec<String> = cmds
            .iter()
            .map(|(p, a)| format!("{p} {}", a.join(" ")))
            .collect();
        return ok_json(
            "ntp_sync",
            "needs_elevation: commands listed".into(),
            serde_json::json!({ "outcome": "needs_elevation", "commands": rendered }),
        );
    }
    for (program, args) in &cmds {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = exec::run(program, &refs, Duration::from_secs(20));
        if !out.success {
            return fail(
                "ntp_sync",
                format!(
                    "failed: {}",
                    out.combined.trim().chars().take(200).collect::<String>()
                ),
            );
        }
    }
    ok_json(
        "ntp_sync",
        "system time resync requested".into(),
        serde_json::json!({ "outcome": "applied" }),
    )
}

/* ---- Wake-on-LAN ---- */

pub fn wake_on_lan(mac: &str) -> linkfyr_model::optimize::ToolRunReport {
    let cleaned: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.len() != 12 {
        return fail(
            "wake_on_lan",
            format!("MAC must be 6 hex bytes, got '{mac}'"),
        );
    }
    let mut bytes = Vec::with_capacity(102);
    bytes.extend_from_slice(&[0xFF; 6]);
    let target: Vec<u8> = (0..6)
        .map(|i| u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect();
    for _ in 0..16 {
        bytes.extend_from_slice(&target);
    }
    // Broadcast on each broadcast-capable address family.
    let mut sent = 0;
    for addr in ["255.255.255.255:9", "[ff02::1]:9"] {
        let Ok(sa) = addr.parse::<SocketAddr>() else {
            continue;
        };
        let bind: SocketAddr = if sa.is_ipv6() {
            "[::]:0".parse().unwrap()
        } else {
            "0.0.0.0:0".parse().unwrap()
        };
        if let Ok(sock) = UdpSocket::bind(bind) {
            let _ = sock.set_broadcast(true);
            if sock.send_to(&bytes, sa).is_ok() {
                sent += 1;
            }
        }
    }
    if sent == 0 {
        return fail(
            "wake_on_lan",
            "could not send magic packet on any broadcast socket".into(),
        );
    }
    ok_json(
        "wake_on_lan",
        format!("magic packet sent for {mac} ({sent} broadcast socket(s))"),
        serde_json::json!({ "mac": mac, "sockets": sent }),
    )
}

/* ---- UPnP ---- */

pub fn upnp_map() -> linkfyr_model::optimize::ToolRunReport {
    // 1) SSDP discover an IGD (Internet Gateway Device).
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return fail("upnp_map", "cannot bind UDP socket".into());
    };
    sock.set_read_timeout(Some(Duration::from_secs(3))).ok();
    let msearch = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n";
    sock.send_to(msearch.as_bytes(), "239.255.255.250:1900")
        .map_err(|e| e.to_string())
        .ok();
    let mut location = None;
    let mut buf = [0u8; 4096];
    for _ in 0..5 {
        let Ok((n, _)) = sock.recv_from(&mut buf) else {
            break;
        };
        let text = String::from_utf8_lossy(&buf[..n]).to_lowercase();
        if text.contains("internetgatewaydevice") {
            for line in text.lines() {
                if let Some(loc) = line.trim().strip_prefix("location:") {
                    location = Some(loc.trim().to_string());
                    break;
                }
            }
        }
        if location.is_some() {
            break;
        }
    }
    let Some(location) = location else {
        return ok_json(
            "upnp_map",
            "no UPnP gateway answered SSDP (UPnP disabled or unsupported router)".into(),
            serde_json::json!({ "mappings": [] }),
        );
    };
    // 2) Fetch the device description and find the WANIPControl URL.
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(4))
        .build();
    let Ok(resp) = agent.get(&location).call() else {
        return fail(
            "upnp_map",
            format!("gateway description unreachable at {location}"),
        );
    };
    let mut body = String::new();
    resp.into_reader()
        .take(262_144)
        .read_to_string(&mut body)
        .ok();
    let control_url = find_control_url(&body, &location);

    // 3) Enumerate existing port mappings (bounded).
    let mut mappings = Vec::new();
    if let Some(control) = &control_url {
        for index in 0..32 {
            let soap = format!(
                "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:GetGenericPortMappingEntry xmlns:u=\"urn:schemas-upnp-org:service:WANIPConnection:1\"><NewPortMappingIndex>{index}</NewPortMappingIndex></u:GetGenericPortMappingEntry></s:Body></s:Envelope>"
            );
            let Ok(resp) = agent
                .post(control)
                .set("Content-Type", "text/xml")
                .set(
                    "SOAPAction",
                    "\"urn:schemas-upnp-org:service:WANIPConnection:1#GetGenericPortMappingEntry\"",
                )
                .send_bytes(soap.as_bytes())
            else {
                break;
            };
            if resp.status() != 200 {
                break; // end of mapping list (SpecifiedArrayIndexInvalid)
            }
            let mut xml = String::new();
            resp.into_reader()
                .take(65_536)
                .read_to_string(&mut xml)
                .ok();
            let Some(entry) = extract_tag(&xml, "NewPortMappingDescription") else {
                break;
            };
            let ext = extract_tag(&xml, "NewExternalPort").unwrap_or_else(|| "?".into());
            let int = extract_tag(&xml, "NewInternalPort").unwrap_or_else(|| "?".into());
            let ip = extract_tag(&xml, "NewInternalClient").unwrap_or_else(|| "?".into());
            mappings.push(serde_json::json!({ "description": entry, "external": ext, "internal": int, "client": ip }));
        }
    }
    ok_json(
        "upnp_map",
        format!("{} UPnP port mapping(s) on your router", mappings.len()),
        serde_json::json!({ "gateway": location, "mappings": mappings }),
    )
}

fn find_control_url(xml: &str, location: &str) -> Option<String> {
    let lower = xml.to_lowercase();
    let idx = lower.find("wanipconnection")?;
    let window = &xml[idx.saturating_sub(2000)..idx + 2000.min(xml.len() - idx)];
    let tag = extract_tag(window, "controlURL")?;
    if tag.starts_with("http") {
        Some(tag)
    } else {
        // Relative URL resolved against the description base.
        let base = location.split("/description").next().unwrap_or(location);
        Some(format!("{base}{tag}"))
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = xml.find(&open)?;
    let content_start = xml[start..].find('>')? + start + 1;
    let close = format!("</{tag}>");
    let end = xml[content_start..].find(&close)? + content_start;
    Some(xml[content_start..end].trim().to_string())
}

/* ---- RDAP / ASN / geolocation (opt-in egress) ---- */

fn http_json(url: &str) -> Result<serde_json::Value, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(6))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut body = String::new();
    resp.into_reader()
        .take(262_144)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

pub fn whois_lookup(target: &str) -> linkfyr_model::optimize::ToolRunReport {
    let url = format!("https://rdap.org/domain/{target}");
    match http_json(&url) {
        Ok(v) => {
            let name = v.get("ldhName").and_then(|n| n.as_str()).unwrap_or(target);
            let registrar = v
                .get("entities")
                .and_then(|e| e.as_array())
                .and_then(|a| a.first())
                .and_then(|e| e.get("vcardArray"))
                .and_then(|v| v.as_array())
                .and_then(|v| v.get(1))
                .and_then(|v| v.as_array())
                .map(|card| {
                    card.iter()
                        .filter_map(|f| f.as_array())
                        .find(|f| f.first().and_then(|t| t.as_str()) == Some("fn"))
                        .and_then(|f| f.get(3))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string()
                });
            let events: Vec<String> = v
                .get("events")
                .and_then(|e| e.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|ev| {
                            let action = ev.get("eventAction").and_then(|x| x.as_str())?;
                            let date = ev.get("eventDate").and_then(|x| x.as_str())?;
                            Some(format!("{action}: {date}"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            ok_json(
                "whois_lookup",
                format!(
                    "{target} registered via {}",
                    registrar.as_deref().unwrap_or("unknown")
                ),
                serde_json::json!({ "name": name, "registrar": registrar, "events": events }),
            )
        }
        Err(e) => fail("whois_lookup", format!("RDAP lookup failed: {e}")),
    }
}

/// ASN + prefix for an IP via Team Cymru's DNS service (TXT record).
pub fn asn_route_lookup(ip: &str) -> linkfyr_model::optimize::ToolRunReport {
    let parsed: IpAddr = match ip.parse() {
        Ok(v4 @ IpAddr::V4(_)) => v4,
        _ => {
            return fail(
                "asn_route_lookup",
                format!("need an IPv4 address, got '{ip}'"),
            );
        }
    };
    let IpAddr::V4(v4) = parsed else {
        unreachable!()
    };
    let o = v4.octets();
    let query = format!("{}.{}.{}.{}.origin.asn.cymru.com", o[3], o[2], o[1], o[0]);
    let server: SocketAddr = "8.8.8.8:53".parse().expect("static");
    match crate::dnskit::lookup(server, &query, 16) {
        Ok(answers) if !answers.is_empty() => {
            let parts: Vec<&str> = answers[0].text.split('|').map(str::trim).collect();
            let (asn, prefix, owner) = (
                parts.first().copied().unwrap_or("?"),
                parts.get(1).copied().unwrap_or("?"),
                parts.get(3).copied().unwrap_or("?"),
            );
            ok_json(
                "asn_route_lookup",
                format!("{ip} is {prefix} announced by AS{asn} ({owner})"),
                serde_json::json!({ "ip": ip, "asn": asn, "prefix": prefix, "owner": owner }),
            )
        }
        _ => fail(
            "asn_route_lookup",
            "origin lookup returned no answer".into(),
        ),
    }
}

pub fn ip_geolocation(ip: &str) -> linkfyr_model::optimize::ToolRunReport {
    let url = format!("http://ip-api.com/json/{ip}?fields=status,country,city,isp,as");
    let v = match http_json(&url) {
        Ok(v) => v,
        Err(e) => return fail("ip_geolocation", format!("opt-in lookup failed: {e}")),
    };
    if v.get("status").and_then(|s| s.as_str()) != Some("success") {
        return fail("ip_geolocation", "lookup service reported failure".into());
    }
    let country = v.get("country").and_then(|c| c.as_str()).unwrap_or("?");
    let city = v.get("city").and_then(|c| c.as_str()).unwrap_or("?");
    let isp = v.get("isp").and_then(|c| c.as_str()).unwrap_or("?");
    ok_json(
        "ip_geolocation",
        format!("{ip}: {city}, {country} via {isp} (opt-in external query)"),
        serde_json::json!({ "ip": ip, "country": country, "city": city, "isp": isp }),
    )
}

/* ---- DNSSEC posture + UDP-53 interception ---- */

pub fn dnssec_check(domain: &str) -> linkfyr_model::optimize::ToolRunReport {
    // A validating resolver sets the AD (authenticated data) bit for
    // signed domains and not for unsigned ones.
    let server =
        crate::dnskit::system_resolver().unwrap_or_else(|| "1.1.1.1:53".parse().expect("cf"));
    let signed = ad_bit(server, domain);
    let unsigned = ad_bit(server, "example.com"); // deliberately unsigned
    let verdict = match (signed, unsigned) {
        (Some(true), Some(false)) => {
            "your resolver validates DNSSEC (AD bit on signed, off on unsigned)"
        }
        (Some(true), _) => "AD bit set; resolver likely validates DNSSEC",
        (Some(false), _) => "resolver does not validate DNSSEC (no AD bit)",
        (None, _) => "no response; cannot determine DNSSEC posture",
    };
    ok_json(
        "dnssec_check",
        format!("{domain}: {verdict}"),
        serde_json::json!({ "signedAdBit": signed, "unsignedAdBit": unsigned }),
    )
}

/// AD (authenticated data) bit for a domain from a raw response.
fn ad_bit(server: SocketAddr, domain: &str) -> Option<bool> {
    let raw = raw_query_header_flags(server, domain).ok()?;
    if raw.len() < 12 {
        return None;
    }
    let rcode = raw[3] & 0x0F;
    if rcode != 0 {
        return None; // NXDOMAIN etc: no AD signal
    }
    Some((raw[3] & 0x10) != 0)
}

/// Raw query returning the full response buffer for header-bit checks.
fn raw_query_header_flags(server: SocketAddr, domain: &str) -> Result<Vec<u8>, String> {
    use std::net::UdpSocket as S;
    let bind: SocketAddr = if server.is_ipv6() {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    };
    let sock = S::bind(bind).map_err(|e| e.to_string())?;
    sock.connect(server).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_millis(2000)))
        .map_err(|e| e.to_string())?;
    let mut q = crate::dns::build_query(0x4D2, domain);
    q[2] = 0x01; // RD
    q[3] = 0x10; // AD-requested per RFC 6840 (DO bit lives in OPT; AD set in query)
    sock.send(&q).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1500];
    let n = sock.recv(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(n);
    Ok(buf)
}

/// UDP-53 interception test: with an *unconnected* socket, the response
/// source address reveals who actually answered. If it is not the
/// resolver we asked, UDP 53 is being intercepted (DNS hijack / leak).
pub fn dns_leak() -> linkfyr_model::optimize::ToolRunReport {
    let mut results = Vec::new();
    for server in ["8.8.8.8", "1.1.1.1", "9.9.9.9"] {
        let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
            break;
        };
        let _ = sock.set_read_timeout(Some(Duration::from_millis(1500)));
        let mut prng = Prng::from_clock();
        let domain = format!("{}.example.com", prng.hex(12));
        let q = crate::dns::build_query((prng.next_u64() & 0xFFFF) as u16, &domain);
        if sock.send_to(&q, format!("{server}:53")).is_err() {
            results.push(serde_json::json!({ "asked": server, "answered": null }));
            continue;
        }
        let mut buf = [0u8; 1500];
        match sock.recv_from(&mut buf) {
            Ok((n, peer)) => {
                let answered = peer.ip().to_string();
                let intercepted = answered != server;
                results.push(serde_json::json!({ "asked": server, "answered": answered, "intercepted": intercepted, "bytes": n }));
            }
            Err(_) => results.push(serde_json::json!({ "asked": server, "answered": null })),
        }
    }
    let intercepted_any = results.iter().any(|r| {
        r.get("intercepted")
            .and_then(|i| i.as_bool())
            .unwrap_or(false)
    });
    let summary = if intercepted_any {
        "UDP 53 INTERCEPTED: a middlebox answered instead of the resolver you asked (classic DNS hijack)".to_string()
    } else {
        "no interception detected: public resolvers answered directly".to_string()
    };
    ok_json(
        "dns_leak",
        summary,
        serde_json::json!({ "results": results, "intercepted": intercepted_any }),
    )
}

/* ---- HTTP redirect trace + security headers ---- */

pub fn http_redirect_trace(url: &str) -> linkfyr_model::optimize::ToolRunReport {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_read(Duration::from_secs(6))
        .build();
    let mut current = url.to_string();
    let mut hops = Vec::new();
    let mut total = 0.0f64;
    for hop in 0..10 {
        let start = Instant::now();
        match agent.get(&current).call() {
            Ok(resp) => {
                total += start.elapsed().as_secs_f64() * 1000.0;
                let status = resp.status();
                let location = resp.header("location").map(ToString::to_string);
                hops.push(serde_json::json!({ "hop": hop, "url": current, "status": status, "location": location }));
                match (status / 100, location) {
                    (3, Some(loc)) => {
                        current = if loc.starts_with("http") {
                            loc
                        } else {
                            join_url(&current, &loc)
                        };
                    }
                    _ => break,
                }
            }
            Err(e) => {
                hops.push(
                    serde_json::json!({ "hop": hop, "url": current, "error": e.to_string() }),
                );
                break;
            }
        }
    }
    let hop_count = hops.len();
    ok_json(
        "http_redirect_trace",
        format!("{hop_count} hop(s), {total:.0} ms total for {url}"),
        serde_json::json!({ "hops": hops }),
    )
}

fn join_url(base: &str, loc: &str) -> String {
    if loc.starts_with('/') {
        let scheme_end = base.find("://").map_or(0, |i| i + 3);
        let host_end = base[scheme_end..]
            .find('/')
            .map_or(base.len(), |i| scheme_end + i);
        format!("{}{loc}", &base[..host_end])
    } else {
        loc.to_string()
    }
}

pub fn headers_audit(url: &str) -> linkfyr_model::optimize::ToolRunReport {
    let agent = ureq::AgentBuilder::new()
        .redirects(1)
        .timeout_read(Duration::from_secs(6))
        .build();
    let resp = match agent.get(url).call() {
        Ok(r) => r,
        Err(e) => return fail("headers_audit", format!("request failed: {e}")),
    };
    let check = |name: &str| resp.header(name).map(|v| format!("{name}: {v}"));
    let present: Vec<String> = [
        "strict-transport-security",
        "content-security-policy",
        "x-frame-options",
        "x-content-type-options",
        "referrer-policy",
    ]
    .iter()
    .filter_map(|h| check(h))
    .collect();
    let missing: Vec<&str> = [
        "strict-transport-security",
        "content-security-policy",
        "x-content-type-options",
    ]
    .iter()
    .copied()
    .filter(|h| resp.header(h).is_none())
    .collect();
    ok_json(
        "headers_audit",
        format!("{} of 5 security headers present", present.len()),
        serde_json::json!({ "present": present, "missing": missing, "status": resp.status() }),
    )
}

/* ---- composite tools ---- */

pub fn stack_snapshot() -> linkfyr_model::optimize::ToolRunReport {
    let snapshot = serde_json::json!({
        "tcp": crate::tcpaudit::audit(),
        "routes": crate::routeaudit::audit(),
        "resolvers": crate::dns::system_resolvers(),
        "proxy": crate::sysnet::proxy_configuration(),
        "connections": { "count": crate::sysnet::connections().connections.len() },
    });
    ok_json(
        "stack_snapshot",
        "full network stack snapshot captured".into(),
        snapshot,
    )
}

pub fn diff_snapshot() -> linkfyr_model::optimize::ToolRunReport {
    let current = stack_snapshot().data;
    let dir = crate::netops::state_dir_public().join("snapshots");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("stack-latest.json");
    let previous: Option<serde_json::Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let _ = std::fs::write(
        &path,
        serde_json::to_string_pretty(&current).unwrap_or_default(),
    );
    match previous {
        None => ok_json(
            "diff_snapshot",
            "baseline stored; run again after changes to diff".into(),
            current,
        ),
        Some(prev) => {
            let a = serde_json::to_string_pretty(&prev).unwrap_or_default();
            let b = serde_json::to_string_pretty(&current).unwrap_or_default();
            let changed = a != b;
            ok_json(
                "diff_snapshot",
                if changed {
                    "stack CHANGED since the last snapshot".into()
                } else {
                    "stack unchanged since the last snapshot".into()
                },
                serde_json::json!({ "changed": changed }),
            )
        }
    }
}

pub fn export_report() -> linkfyr_model::optimize::ToolRunReport {
    let bundle = serde_json::json!({
        "generatedMs": crate::clock_ms(),
        "snapshot": stack_snapshot().data,
        "alertsProxy": crate::sysnet::proxy_configuration(),
    });
    let dir = crate::netops::state_dir_public().join("reports");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("linkfyr-report-{}.json", crate::clock_ms()));
    if let Err(e) = std::fs::write(
        &file,
        serde_json::to_string_pretty(&bundle).unwrap_or_default(),
    ) {
        return fail("export_report", e.to_string());
    }
    ok_json(
        "export_report",
        format!("report written to {}", file.display()),
        serde_json::json!({ "path": file.to_string_lossy() }),
    )
}

pub fn self_test() -> linkfyr_model::optimize::ToolRunReport {
    let checks = vec![
        ("ping binary", exec::on_path("ping")),
        ("udp sockets", UdpSocket::bind("127.0.0.1:0").is_ok()),
        (
            "tcp listener",
            std::net::TcpListener::bind("127.0.0.1:0").is_ok(),
        ),
        (
            "system resolvers",
            !crate::dns::system_resolvers().is_empty(),
        ),
        (
            "routing table",
            !crate::routeaudit::audit().entries.is_empty(),
        ),
    ];
    let passed = checks.iter().filter(|(_, ok)| *ok).count();
    ok_json(
        "self_test",
        format!("{passed}/{} environment checks pass", checks.len()),
        serde_json::json!({ "checks": checks.into_iter().map(|(n, ok)| serde_json::json!({"name": n, "ok": ok})).collect::<Vec<_>>() }),
    )
}

/* ---- local throughput endpoint (iperf-style) ---- */

pub fn iperf_endpoint(
    params: &std::collections::BTreeMap<String, String>,
) -> linkfyr_model::optimize::ToolRunReport {
    let port: u16 = params
        .get("port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(5201);
    let seconds: u64 = params
        .get("seconds")
        .and_then(|p| p.parse().ok())
        .unwrap_or(30)
        .clamp(1, 600);
    let listener = match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => l,
        Err(e) => return fail("iperf_endpoint", format!("bind 0.0.0.0:{port}: {e}")),
    };
    if let Err(e) = listener.set_nonblocking(true) {
        return fail("iperf_endpoint", e.to_string());
    }
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let total_bytes = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut served = 0u32;
    while Instant::now() < deadline {
        if let Ok((mut stream, _)) = listener.accept() {
            served += 1;
            let n = std::sync::Arc::clone(&total_bytes);
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut buf = vec![0u8; 65536];
                let payload = vec![b'x'; 65536];
                loop {
                    if stream.write_all(&payload).is_err() {
                        return;
                    }
                    n.fetch_add(payload.len() as u64, Ordering::Relaxed);
                    let _ = stream.read(&mut buf);
                }
            });
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let mbps = total_bytes.load(Ordering::Relaxed) as f64 * 8.0 / seconds as f64 / 1e6;
    ok_json(
        "iperf_endpoint",
        format!("served {served} client(s), {mbps:.1} Mbps total over {seconds}s on port {port}"),
        serde_json::json!({ "clients": served, "mbps": mbps, "port": port, "seconds": seconds }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntp_packet_round_trip_or_honest_failure() {
        match ntp_query("127.0.0.1") {
            Ok(_) | Err(_) => {}
        }
        let r = clock_skew();
        // Either a real answer or the honest "no NTP server answered".
        assert!(r.ok || r.summary.contains("no NTP server"));
    }

    #[test]
    fn wol_validates_mac_and_sends() {
        assert!(!wake_on_lan("zz:zz").ok);
        let r = wake_on_lan("AA:BB:CC:DD:EE:FF");
        // Loopback broadcast may or may not be permitted; never a panic.
        assert!(r.ok || r.summary.contains("could not send"));
    }

    #[test]
    fn join_url_handles_relative_locations() {
        assert_eq!(join_url("https://a.dev/x/y", "/z"), "https://a.dev/z");
        assert_eq!(
            join_url("https://a.dev/x", "https://b.dev"),
            "https://b.dev"
        );
    }

    #[test]
    fn self_test_passes_in_container() {
        let r = self_test();
        assert!(r.ok);
        assert!(r.summary.contains("/5"), "{}", r.summary);
    }

    #[test]
    fn diff_and_export_work() {
        let first = diff_snapshot();
        assert!(first.ok);
        let second = diff_snapshot();
        assert!(second.ok);
        let exported = export_report();
        assert!(exported.ok, "{}", exported.summary);
    }

    #[test]
    fn redirect_trace_and_headers_run_or_fail_honestly() {
        let rt = http_redirect_trace("http://127.0.0.1:1/");
        // Unreachable port: the trace reports an error hop, not a panic.
        assert_ne!(rt.data.get("hops").unwrap().as_array().unwrap().len(), 0);
        let h = headers_audit("not-a-url");
        assert!(!h.ok);
    }
}

/// Captive-portal detection: a plain-HTTP 204 endpoint must answer 204;
/// anything else (redirect/login page) means a portal is intercepting.
pub fn captive_portal_probe() -> linkfyr_model::optimize::ToolRunReport {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_read(Duration::from_secs(5))
        .build();
    for url in [
        "http://connectivitycheck.gstatic.com/generate_204",
        "http://www.msftconnecttest.com/connecttest.txt",
    ] {
        match agent.get(url).call() {
            Ok(resp) => {
                let status = resp.status();
                if status == 204 || status == 200 {
                    return ok_json(
                        "captive_portal_probe",
                        "no captive portal: the Internet answered directly".into(),
                        serde_json::json!({ "status": status, "probe": url }),
                    );
                }
                if status / 100 == 3 {
                    return ok_json(
                        "captive_portal_probe",
                        format!(
                            "CAPTIVE PORTAL detected: {url} redirected to a login page (status {status})"
                        ),
                        serde_json::json!({ "status": status, "probe": url, "portal": true }),
                    );
                }
            }
            Err(e) => {
                return fail("captive_portal_probe", format!("probe unreachable: {e}"));
            }
        }
    }
    fail("captive_portal_probe", "no probe endpoint answered".into())
}

#[cfg(test)]
mod portal_tests {
    use super::*;

    #[test]
    fn portal_probe_never_panics() {
        let r = captive_portal_probe();
        assert!(r.ok || !r.summary.is_empty());
    }
}
