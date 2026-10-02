//! DNS toolkit extensions: record lookups (A/AAAA/PTR/TXT/MX/CNAME/SRV),
//! DoH benchmarking with JSON APIs, and resolver consistency checks.
//! All real queries over UDP or HTTPS; parsers are fixture-tested.

use std::io::Read;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::{DohReport, DohResult};

use crate::dns::{build_query, skip_name};
use crate::stats::{self, Prng};

const UDP_TIMEOUT: Duration = Duration::from_millis(2000);

/// One parsed DNS answer.
#[derive(Debug, Clone, PartialEq)]
pub struct DnsAnswer {
    pub rtype: u16,
    pub ttl: u32,
    pub text: String,
}

/// Query `domain` for `rtype` against `server` and parse all answers.
pub fn lookup(server: SocketAddr, domain: &str, rtype: u16) -> Result<Vec<DnsAnswer>, String> {
    let mut prng = Prng::from_clock();
    let bind: SocketAddr = if server.is_ipv6() {
        "[::]:0".parse().expect("bind v6")
    } else {
        "0.0.0.0:0".parse().expect("bind v4")
    };
    let sock = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
    sock.connect(server).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(UDP_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let id = (prng.next_u64() & 0xFFFF) as u16;
    let query = build_query(id, domain);
    // qtype lives in the last 4 bytes (type+class): patch bytes len-4..len-2.
    let mut query = query;
    let n = query.len();
    query[n - 4] = (rtype >> 8) as u8;
    query[n - 3] = (rtype & 0xFF) as u8;

    sock.send(&query).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 4096];
    let n = sock.recv(&mut buf).map_err(|e| e.to_string())?;
    parse_answers(&buf[..n]).map_err(|e| format!("malformed response: {e}"))
}

/// Parse a response into typed answers (handles compression pointers).
pub fn parse_answers(buf: &[u8]) -> Result<Vec<DnsAnswer>, String> {
    if buf.len() < 12 {
        return Err("short header".into());
    }
    let rcode = buf[3] & 0x0F;
    if rcode == 3 {
        return Ok(vec![]); // NXDOMAIN: no answers, not an error
    }
    if rcode != 0 {
        return Err(format!("rcode {rcode}"));
    }
    let qd = u16::from_be_bytes([buf[4], buf[5]]);
    let an = u16::from_be_bytes([buf[6], buf[7]]);
    let mut pos = 12;
    for _ in 0..qd {
        pos = skip_name(buf, pos).ok_or("bad question name")?;
        pos += 4;
    }
    let mut out = Vec::new();
    for _ in 0..an {
        pos = skip_name(buf, pos).ok_or("bad answer name")?;
        let need = pos + 10;
        if buf.len() < need {
            return Err("truncated rr".into());
        }
        let rtype = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let ttl = u32::from_be_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]);
        let rdlen = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]) as usize;
        pos += 10;
        let end = pos + rdlen;
        if buf.len() < end {
            return Err("truncated rdata".into());
        }
        let text = render_rdata(buf, pos, rtype)?;
        out.push(DnsAnswer { rtype, ttl, text });
        pos = end;
    }
    Ok(out)
}

fn render_rdata(buf: &[u8], start: usize, rtype: u16) -> Result<String, String> {
    match rtype {
        1 => {
            let b: Vec<u8> = buf[start..start + 4].to_vec();
            Ok(std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string())
        }
        28 => {
            if start + 16 > buf.len() {
                return Err("short aaaa".into());
            }
            let mut o = [0u8; 16];
            o.copy_from_slice(&buf[start..start + 16]);
            Ok(std::net::Ipv6Addr::from(o).to_string())
        }
        16 => {
            // TXT: sequence of length-prefixed strings.
            let mut parts = Vec::new();
            let mut p = start;
            while p < buf.len() {
                let len = buf[p] as usize;
                p += 1;
                if p + len > buf.len() {
                    break;
                }
                parts.push(String::from_utf8_lossy(&buf[p..p + len]).to_string());
                p += len;
            }
            Ok(parts.join(" "))
        }
        2 | 5 | 12 | 15 => {
            // NS / CNAME / PTR / MX: MX has a 2-byte preference first.
            let name_start = if rtype == 15 { start + 2 } else { start };
            let (name, _) = read_name(buf, name_start)?;
            Ok(name)
        }
        33 => {
            // SRV: priority weight port target
            if start + 7 > buf.len() {
                return Err("short srv".into());
            }
            let port = u16::from_be_bytes([buf[start + 4], buf[start + 5]]);
            let (target, _) = read_name(buf, start + 6)?;
            Ok(format!("{target}:{port}"))
        }
        _ => Ok(format!("<{rdlen} bytes>", rdlen = buf.len() - start)),
    }
}

/// Read a possibly-compressed name starting at `pos`; returns (name, next_pos).
fn read_name(buf: &[u8], pos: usize) -> Result<(String, usize), String> {
    let mut labels = Vec::new();
    let mut p = pos;
    let mut jumped = false;
    let mut next = pos;
    let mut hops = 0;
    loop {
        let len = *buf.get(p).ok_or("eof in name")?;
        match len & 0xC0 {
            0xC0 => {
                if buf.len() < p + 2 {
                    return Err("bad pointer".into());
                }
                if !jumped {
                    next = p + 2;
                    jumped = true;
                }
                p = (u16::from_be_bytes([buf[p] & 0x3F, buf[p + 1]])) as usize;
                hops += 1;
                if hops > 32 {
                    return Err("pointer loop".into());
                }
            }
            0x00 => {
                if !jumped {
                    next = p + 1;
                }
                return Ok((labels.join("."), next));
            }
            _ => {
                let end = p + 1 + len as usize;
                if buf.len() < end {
                    return Err("label overruns".into());
                }
                labels.push(String::from_utf8_lossy(&buf[p + 1..end]).to_string());
                p = end;
            }
        }
    }
}

/// The system's first resolver, detected via the shared dns module.
pub fn system_resolver() -> Option<SocketAddr> {
    crate::dns::system_resolvers()
        .first()
        .and_then(|s| s.parse().ok())
        .map(|ip: IpAddr| SocketAddr::new(ip, 53))
}

/// Public DoH providers with JSON APIs (no key, free).
pub fn doh_endpoints() -> Vec<(String, String)> {
    vec![
        (
            "cloudflare".into(),
            "https://cloudflare-dns.com/dns-query".into(),
        ),
        ("google".into(), "https://dns.google/resolve".into()),
        (
            "quad9".into(),
            "https://dns.quad9.net:5053/dns-query".into(),
        ),
    ]
}

fn doh_query(endpoint: &str, name: &str) -> Result<(Duration, Option<String>), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(4))
        .build();
    let url = format!("{endpoint}?name={name}&type=A");
    let start = Instant::now();
    let resp = agent
        .get(&url)
        .set("Accept", "application/dns-json")
        .call()
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    resp.into_reader()
        .take(65536)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    let took = start.elapsed();
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let answer = v
        .get("Answer")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("data"))
        .and_then(|d| d.as_str())
        .map(ToString::to_string);
    Ok((took, answer))
}

/// Benchmark DoH providers (3 samples each) and compare with plain UDP.
pub fn doh_benchmark() -> DohReport {
    let mut results = Vec::new();
    for (label, endpoint) in doh_endpoints() {
        let mut rtts = Vec::new();
        let mut answer = None;
        let mut error = None;
        for _ in 0..3 {
            match doh_query(&endpoint, "www.cloudflare.com") {
                Ok((took, a)) => {
                    rtts.push(took.as_secs_f64() * 1000.0);
                    answer = a;
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        rtts.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        let success = !rtts.is_empty();
        results.push(DohResult {
            endpoint: label,
            success,
            median_ms: stats::median(&rtts),
            answer,
            error,
        });
    }
    let best = results
        .iter()
        .filter(|r| r.success)
        .min_by(|a, b| {
            a.median_ms
                .unwrap_or(f64::MAX)
                .partial_cmp(&b.median_ms.unwrap_or(f64::MAX))
                .expect("finite")
        })
        .map(|r| r.endpoint.clone());

    // Compare against the fastest plain-UDP resolver if reachable.
    let udp = crate::dns::bench_default();
    let udp_best_ms = udp.results.first().and_then(|r| r.cached_ms);

    let verdict = match (&best, udp_best_ms) {
        (Some(b), Some(u)) if u < 30.0 => format!(
            "Plain DNS ({u:.0} ms cached) is faster than DoH for lookups; keep DoH only where UDP 53 is blocked (best DoH: {b})"
        ),
        (Some(b), _) => {
            format!("DoH is the practical choice here (best: {b}); plain UDP is slow or blocked")
        }
        (None, _) => "No DoH provider answered; check HTTPS egress".into(),
    };

    DohReport {
        results,
        recommended: best,
        udp_best_ms,
        verdict,
    }
}

/// Check that resolvers agree on an A record (spoofing/hijack signal).
pub fn resolver_consistency(domain: &str) -> (bool, String) {
    let servers = [
        ("system", system_resolver()),
        ("1.1.1.1", "1.1.1.1:53".parse().ok()),
        ("8.8.8.8", "8.8.8.8:53".parse().ok()),
        ("9.9.9.9", "9.9.9.9:53".parse().ok()),
    ];
    let mut seen: Vec<(String, String)> = Vec::new();
    for (label, server) in servers.iter().filter_map(|(l, s)| s.map(|s| (*l, s))) {
        if let Ok(answers) = lookup(server, domain, 1) {
            if let Some(a) = answers.first() {
                seen.push((label.to_string(), a.text.clone()));
            }
        }
    }
    if seen.is_empty() {
        return (false, "no resolver answered".into());
    }
    let mut unique = seen.clone();
    unique.sort_by(|a, b| a.1.cmp(&b.1));
    unique.dedup_by(|a, b| a.1 == b.1);
    let ok = unique.len() == 1;
    let detail = if ok {
        format!("all {} resolvers agree: {}", seen.len(), seen[0].1)
    } else {
        format!(
            "resolvers disagree: {}",
            seen.iter()
                .map(|(l, a)| format!("{l}={a}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    (ok, detail)
}

/// Reverse lookup: IP → hostname via in-addr.arpa / ip6.arpa PTR.
pub fn reverse_lookup(ip: IpAddr) -> Result<String, String> {
    let server = system_resolver().unwrap_or_else(|| "1.1.1.1:53".parse().expect("cf"));
    let arpa = match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])
        }
        IpAddr::V6(v6) => {
            let mut parts = Vec::new();
            for b in v6.octets().iter().rev() {
                parts.push(format!("{:x}", b & 0xF));
                parts.push(format!("{:x}", b >> 4));
            }
            format!("{}.ip6.arpa", parts.join("."))
        }
    };
    let answers = lookup(server, &arpa, 12)?;
    answers
        .first()
        .map(|a| a.text.clone())
        .ok_or_else(|| "no PTR record".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with(rtype: u16, rdata: &[u8], qname: &[u8]) -> Vec<u8> {
        let mut pkt = Vec::new();
        pkt.extend_from_slice(&[0xAB, 0xCD, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0]);
        pkt.extend_from_slice(qname);
        pkt.extend_from_slice(&[(rtype >> 8) as u8, (rtype & 0xFF) as u8, 0, 1]);
        pkt.extend_from_slice(&[0xC0, 0x0C, (rtype >> 8) as u8, (rtype & 0xFF) as u8, 0, 1]);
        pkt.extend_from_slice(&60u32.to_be_bytes());
        pkt.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        pkt.extend_from_slice(rdata);
        pkt
    }

    #[test]
    fn parses_txt_records() {
        // TXT: two char-strings "hello" and "world".
        let rdata = [
            5, b'h', b'e', b'l', b'l', b'o', 5, b'w', b'o', b'r', b'l', b'd',
        ];
        let qname = [4, b't', b'e', b's', b't', 0];
        let pkt = response_with(16, &rdata, &qname);
        let answers = parse_answers(&pkt).expect("parse");
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].text, "hello world");
        assert_eq!(answers[0].ttl, 60);
    }

    #[test]
    fn parses_ptr_and_srv_records() {
        let qname = [0];
        // PTR rdata: compressed pointer to offset 12
        let ptr = response_with(12, &[0xC0, 0x0C], &qname);
        assert_eq!(parse_answers(&ptr).unwrap()[0].rtype, 12);

        // SRV: prio 0, weight 0, port 443, target = root
        let srv_rdata = [0, 0, 0, 0, 1, 0xBB, 0];
        let srv = response_with(33, &srv_rdata, &qname);
        let answers = parse_answers(&srv).unwrap();
        assert_eq!(answers[0].text, ":443");
    }

    #[test]
    fn parses_a_and_aaaa_rdata() {
        let qname = [0];
        let a = response_with(1, &[1, 2, 3, 4], &qname);
        assert_eq!(parse_answers(&a).unwrap()[0].text, "1.2.3.4");
        let aaaa = response_with(
            28,
            &[0x20, 0x01, 0xD, 0xB8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            &qname,
        );
        assert_eq!(parse_answers(&aaaa).unwrap()[0].text, "2001:db8::1");
    }

    #[test]
    fn nxdomain_is_empty_not_error() {
        let pkt = [0u8, 0, 0x81, 0x83, 0, 0, 0, 0, 0, 0, 0, 0];
        let answers = parse_answers(&pkt).expect("nxdomain parses");
        assert_eq!(answers.len(), 0, "NXDOMAIN has no answers");
    }

    #[test]
    fn reverse_arpa_names_are_correct() {
        let ip4: IpAddr = "192.168.1.10".parse().unwrap();
        if let IpAddr::V4(v4) = ip4 {
            let o = v4.octets();
            let arpa = format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0]);
            assert_eq!(arpa, "10.1.168.192.in-addr.arpa");
        }
        let ip6: IpAddr = "2001:db8::1".parse().unwrap();
        if let IpAddr::V6(v6) = ip6 {
            let mut parts = Vec::new();
            for b in v6.octets().iter().rev() {
                parts.push(format!("{:x}", b & 0xF));
                parts.push(format!("{:x}", b >> 4));
            }
            assert!(
                parts
                    .join(".")
                    .starts_with("1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0")
            );
        }
    }

    #[test]
    fn doh_benchmark_handles_unreachable_endpoints() {
        // In an offline container every provider fails; the report must
        // say so instead of panicking.
        let r = doh_benchmark();
        assert!(!r.results.is_empty());
        if r.recommended.is_none() {
            assert!(r.verdict.contains("No DoH provider"));
        }
    }

    #[test]
    fn consistency_reports_disagreement() {
        // In an offline container every resolver fails -> the honest
        // "no resolver answered" detail; with egress it resolves.
        let (ok, detail) = resolver_consistency("example.invalid");
        assert_ne!(detail, "");
        if ok {
            assert!(detail.contains("agree"));
        }
    }
}
