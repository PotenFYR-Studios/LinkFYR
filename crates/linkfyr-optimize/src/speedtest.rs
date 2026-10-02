//! Real throughput test (download/upload/latency) against any
//! compatible endpoint. Default is Cloudflare's free speed endpoint
//! (`https://speed.cloudflare.com`); self-hosted servers work too, so
//! nothing depends on a vendor.

use std::io::Read;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::SpeedtestReport;

use crate::stats;

#[derive(Debug, Clone)]
pub struct SpeedtestOptions {
    /// Base URL, e.g. "https://speed.cloudflare.com".
    pub endpoint: String,
    /// Seconds per direction.
    pub duration_s: u32,
}

impl Default for SpeedtestOptions {
    fn default() -> Self {
        Self {
            endpoint: "https://speed.cloudflare.com".into(),
            duration_s: 8,
        }
    }
}

/// Parsed endpoint: (scheme, host, port).
pub fn parse_endpoint(base: &str) -> Result<(String, String, u16), String> {
    let (scheme, rest) = base
        .strip_prefix("https://")
        .map(|r| ("https", r))
        .or_else(|| base.strip_prefix("http://").map(|r| ("http", r)))
        .ok_or_else(|| "endpoint must start with http:// or https://".to_string())?;
    let hostport = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>().map_err(|_| "bad port".to_string())?,
        ),
        None => (
            hostport.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    if host.is_empty() {
        return Err("empty host".into());
    }
    Ok((scheme.into(), host, port))
}

fn connect_rtt_ms(addr: SocketAddr) -> Result<f64, String> {
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

fn drain(reader: &mut dyn Read) -> u64 {
    let mut buf = vec![0u8; 65536];
    let mut total = 0;
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => return total,
            Ok(n) => total += n as u64,
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(5))
        .build()
}

/// Measure download, upload, and connect latency. All values measured;
/// zero throughput with a `None` latency means the endpoint failed.
pub fn run(opts: &SpeedtestOptions) -> SpeedtestReport {
    let Ok((scheme, host, port)) = parse_endpoint(&opts.endpoint) else {
        return SpeedtestReport {
            endpoint: opts.endpoint.clone(),
            latency_ms: None,
            download_mbps: 0.0,
            upload_mbps: 0.0,
            duration_s: 0,
        };
    };

    // Latency: three real TCP connects, median.
    let mut latencies = Vec::new();
    if let Ok(mut addrs) = (host.as_str(), port).to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            for _ in 0..3 {
                if let Ok(ms) = connect_rtt_ms(addr) {
                    latencies.push(ms);
                }
            }
        }
    }
    let mut sorted_lat = latencies.clone();
    sorted_lat.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let latency_ms = stats::median(&sorted_lat);

    let a = agent();
    let body = crate::bloat::pseudo_random_mib();
    let deadline = Duration::from_secs(u64::from(opts.duration_s.max(1)));

    // Download phase.
    let down_url = format!("{scheme}://{host}:{port}/__down?bytes=50000000");
    let mut down_bytes: u64 = 0;
    let start = Instant::now();
    while start.elapsed() < deadline {
        let resp = match a.get(&down_url).call() {
            Ok(r) => r,
            Err(_) if down_bytes > 0 => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(_) => break, // endpoint unusable; report honestly
        };
        down_bytes += drain(&mut resp.into_reader());
    }

    // Upload phase.
    let up_url = format!("{scheme}://{host}:{port}/__up");
    let mut up_bytes: u64 = 0;
    let start = Instant::now();
    while start.elapsed() < deadline {
        let resp = match a
            .post(&up_url)
            .set("Content-Type", "application/octet-stream")
            .send_bytes(&body)
        {
            Ok(r) => r,
            Err(_) if up_bytes > 0 => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(_) => break,
        };
        let _ = drain(&mut resp.into_reader());
        up_bytes += body.len() as u64;
    }

    let secs = deadline.as_secs_f64();
    SpeedtestReport {
        endpoint: opts.endpoint.clone(),
        latency_ms,
        download_mbps: down_bytes as f64 * 8.0 / secs / 1e6,
        upload_mbps: up_bytes as f64 * 8.0 / secs / 1e6,
        duration_s: opts.duration_s.max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn parses_endpoints_with_and_without_port() {
        assert_eq!(
            parse_endpoint("https://speed.cloudflare.com").unwrap(),
            ("https".into(), "speed.cloudflare.com".into(), 443)
        );
        assert_eq!(
            parse_endpoint("http://127.0.0.1:8080/base").unwrap(),
            ("http".into(), "127.0.0.1".into(), 8080)
        );
        assert!(parse_endpoint("ftp://x").is_err());
        assert!(parse_endpoint("https://").is_err());
    }

    #[test]
    fn measures_against_real_local_http_server() {
        let (addr, _server) = testutil::spawn_http_server();
        let opts = SpeedtestOptions {
            endpoint: format!("http://{addr}"),
            duration_s: 1,
        };
        let report = run(&opts);
        let latency = report.latency_ms.expect("latency measured");
        assert!(latency < 200.0, "loopback latency: {latency}");
        assert!(report.download_mbps > 0.0, "real bytes down: {report:?}");
        assert!(report.upload_mbps > 0.0, "real bytes up: {report:?}");
        assert_eq!(report.duration_s, 1);
    }

    #[test]
    fn bad_endpoint_reports_zeros_without_panic() {
        let report = run(&SpeedtestOptions {
            endpoint: "notaurl".into(),
            duration_s: 1,
        });
        assert_eq!(report.download_mbps, 0.0);
        assert!(report.latency_ms.is_none());
    }
}
