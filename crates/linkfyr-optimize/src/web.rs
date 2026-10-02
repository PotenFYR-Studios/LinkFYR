//! Web-facing tools: HTTP time-to-first-byte, TLS reachability, and
//! opt-in public-IP discovery. The public-IP tool contacts an external
//! service by explicit user action only (privacy policy: local-only
//! unless you run it).

use std::io::Read;
use std::time::{Duration, Instant};

use ureq::Response;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(8))
        .build()
}

/// Time to first byte for a GET: DNS + connect + TLS + server think.
pub fn http_ttfb(url: &str) -> Result<f64, String> {
    let start = Instant::now();
    let resp = agent().get(url).call().map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 512];
    let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("empty body".into());
    }
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

/// TLS handshake check: reachable + status + https semantics.
pub struct TlsOutcome {
    pub ok: bool,
    pub status: u16,
    pub cert_present: bool,
    pub detail: String,
}

pub fn tls_check(url: &str) -> TlsOutcome {
    let is_https = url.starts_with("https://");
    match agent().get(url).call() {
        Ok(resp) => TlsOutcome {
            ok: true,
            status: resp.status(),
            // A successful https GET implies a completed handshake; ureq
            // does not expose the peer certificate in this version.
            cert_present: is_https,
            detail: format!(
                "{}HTTP {}",
                if is_https {
                    "TLS established, "
                } else {
                    "plain HTTP, "
                },
                resp.status()
            ),
        },
        Err(e) => TlsOutcome {
            ok: false,
            status: 0,
            cert_present: false,
            detail: e.to_string(),
        },
    }
}

/// Public IP via Cloudflare trace (plain text, no key). Opt-in tool.
pub fn public_ip() -> Result<(String, String), String> {
    let resp: Response = agent()
        .get("https://www.cloudflare.com/cdn-cgi/trace")
        .call()
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    resp.into_reader()
        .take(8192)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    let mut ip = String::new();
    let mut loc = String::new();
    for line in body.lines() {
        if let Some(v) = line.strip_prefix("ip=") {
            ip = v.to_string();
        }
        if let Some(v) = line.strip_prefix("loc=") {
            loc = v.to_string();
        }
    }
    if ip.is_empty() {
        return Err("trace response had no ip line".into());
    }
    Ok((ip, loc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn ttfb_against_local_server() {
        let (addr, _s) = testutil::spawn_http_server();
        let ms = http_ttfb(&format!("http://{addr}/x")).expect("ttfb");
        assert!(ms > 0.0 && ms < 500.0, "{ms}");
    }

    #[test]
    fn ttfb_bad_url_errors_honestly() {
        assert!(http_ttfb("not-a-url").is_err());
    }

    #[test]
    fn tls_check_local_plain_http_reports_no_cert() {
        let (addr, _s) = testutil::spawn_http_server();
        let out = tls_check(&format!("http://{addr}/"));
        // Plain http: request succeeds, no certificate to present.
        assert!(out.ok);
        assert!(!out.cert_present);
        assert!(out.detail.contains("HTTP 200"));
    }

    #[test]
    fn public_ip_never_panics_regardless_of_egress() {
        // Result depends on container egress; the contract is: real
        // answer or honest error, never a panic or fake value.
        match public_ip() {
            Ok((ip, loc)) => {
                assert_ne!(ip, "");
                assert_ne!(loc, "");
            }
            Err(e) => assert_ne!(e, ""),
        }
    }
}
