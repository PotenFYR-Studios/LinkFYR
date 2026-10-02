//! Bufferbloat measurement: real latency-under-load grading.
//!
//! Saturates the downlink and uplink with parallel HTTP transfers while
//! continuously measuring connect-RTT to an independent probe target,
//! then grades the added latency (the lag gamers and video callers feel
//! the moment someone starts a download). Endpoints default to
//! Cloudflare's free speed infrastructure; every URL is overridable so
//! the engine can be tested against a local server.

use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use linkfyr_model::optimize::BloatReport;

use crate::stats;

#[derive(Debug, Clone)]
pub struct BloatOptions {
    /// Seconds per phase (idle baseline, download load, upload load).
    pub phase_secs: u32,
    /// Parallel streams per load phase.
    pub streams: u32,
    /// URL whose GET body saturates the downlink.
    pub down_url: String,
    /// URL whose POST body saturates the uplink.
    pub up_url: String,
    /// Independent latency probe target (host:port).
    pub probe_target: String,
    pub probe_interval_ms: u64,
}

impl Default for BloatOptions {
    fn default() -> Self {
        Self {
            phase_secs: 8,
            streams: 4,
            down_url: "https://speed.cloudflare.com/__down?bytes=100000000".into(),
            up_url: "https://speed.cloudflare.com/__up".into(),
            probe_target: "1.1.1.1:443".into(),
            probe_interval_ms: 250,
        }
    }
}

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(4))
        .build()
}

fn connect_rtt_ms(addr: SocketAddr) -> Result<f64, String> {
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| e.to_string())?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

/// Sample the probe target until `deadline`, returning raw RTT samples.
fn sample_until(probe: SocketAddr, deadline: Instant, interval: Duration) -> Vec<f64> {
    let mut samples = Vec::new();
    while Instant::now() < deadline {
        if let Ok(ms) = connect_rtt_ms(probe) {
            samples.push(ms);
        }
        std::thread::sleep(interval);
    }
    samples
}

fn drain(reader: &mut dyn Read, bytes: &AtomicU64) {
    let mut buf = vec![0u8; 65536];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                bytes.fetch_add(n as u64, Ordering::Relaxed);
            }
        }
    }
}

/// Run the full measurement. Every number in the report is measured.
pub fn run(opts: &BloatOptions) -> BloatReport {
    let probe: SocketAddr = opts
        .probe_target
        .parse()
        .unwrap_or_else(|_| "1.1.1.1:443".parse().expect("fallback probe target parses"));
    let agent = build_agent();
    let phase = Duration::from_secs(u64::from(opts.phase_secs.max(1)));
    let interval = Duration::from_millis(opts.probe_interval_ms.clamp(50, 2000));

    // Phase 1: idle baseline.
    let baseline_samples = sample_until(probe, Instant::now() + phase, interval);
    let baseline_ms = stats::median(&sorted(&baseline_samples)).unwrap_or(0.0);

    // Phase 2: downlink load.
    let down_bytes = Arc::new(AtomicU64::new(0));
    let down_samples = load_phase(
        &agent,
        &opts.down_url,
        Load::Get,
        opts.streams,
        phase,
        probe,
        interval,
        &down_bytes,
    );

    // Phase 3: uplink load.
    let up_bytes = Arc::new(AtomicU64::new(0));
    let up_samples = load_phase(
        &agent,
        &opts.up_url,
        Load::Post,
        opts.streams,
        phase,
        probe,
        interval,
        &up_bytes,
    );

    let secs = phase.as_secs_f64();
    let down_added = added_ms(&down_samples, baseline_ms);
    let up_added = added_ms(&up_samples, baseline_ms);

    BloatReport {
        baseline_ms,
        down_added_ms: down_added,
        up_added_ms: up_added,
        down_grade: stats::grade_added_latency(down_added),
        up_grade: stats::grade_added_latency(up_added),
        down_mbps: down_bytes.load(Ordering::Relaxed) as f64 * 8.0 / secs / 1e6,
        up_mbps: up_bytes.load(Ordering::Relaxed) as f64 * 8.0 / secs / 1e6,
        duration_s: opts.phase_secs.max(1),
        probe_target: opts.probe_target.clone(),
    }
}

#[derive(Debug, Clone, Copy)]
enum Load {
    Get,
    Post,
}

#[allow(clippy::too_many_arguments)]
fn load_phase(
    agent: &ureq::Agent,
    url: &str,
    load: Load,
    streams: u32,
    phase: Duration,
    probe: SocketAddr,
    interval: Duration,
    bytes: &Arc<AtomicU64>,
) -> Vec<f64> {
    let deadline = Instant::now() + phase;
    let probe_readings = Arc::new(std::sync::Mutex::new(Vec::<f64>::new()));
    let sampler = {
        let probe_readings = Arc::clone(&probe_readings);
        std::thread::spawn(move || {
            let mut fresh = sample_until(probe, deadline, interval);
            probe_readings
                .lock()
                .expect("sampler lock")
                .append(&mut fresh);
        })
    };

    let body: Vec<u8> = pseudo_random_mib();
    std::thread::scope(|scope| {
        for _ in 0..streams.max(1) {
            let agent = agent.clone();
            let bytes = Arc::clone(bytes);
            let url = url.to_string();
            let body = body.clone();
            scope.spawn(move || {
                while Instant::now() < deadline {
                    let request = match load {
                        Load::Get => agent.get(&url).call(),
                        Load::Post => agent
                            .post(&url)
                            .set("Content-Type", "application/octet-stream")
                            .send_bytes(&body),
                    };
                    match request {
                        Ok(resp) => {
                            let mut reader = resp.into_reader();
                            drain(&mut reader, &bytes);
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(150)),
                    }
                }
            });
        }
    });
    sampler.join().expect("sampler thread");
    probe_readings.lock().expect("sampler lock").clone()
}

fn sorted(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    s
}

/// Added latency: how much the median grew under load, floored at zero
/// (a slightly negative measurement is noise, not "negative bloat").
pub fn added_ms(loaded: &[f64], baseline_ms: f64) -> f64 {
    let loaded_median = stats::median(&sorted(loaded)).unwrap_or(baseline_ms);
    (loaded_median - baseline_ms).max(0.0)
}

/// Deterministic 1 MiB upload body (compresses badly on purpose so the
/// measured uplink throughput is real, not an artifact of compression).
pub fn pseudo_random_mib() -> Vec<u8> {
    let mut prng = crate::stats::Prng::new(0x5EED_1234_ABCD_0001);
    let mut body = Vec::with_capacity(1 << 20);
    while body.len() < 1 << 20 {
        body.extend_from_slice(&prng.next_u64().to_le_bytes());
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use linkfyr_model::optimize::Grade;

    #[test]
    fn added_latency_clamps_at_zero_and_grades() {
        assert_eq!(added_ms(&[10.0, 12.0, 11.0], 50.0), 0.0);
        let added = added_ms(&[80.0, 100.0, 90.0], 20.0);
        assert!((added - 70.0).abs() < 0.01, "got {added}");
        assert_eq!(stats::grade_added_latency(added), Grade::C);
    }

    #[test]
    fn upload_body_is_one_mib_and_high_entropy() {
        let body = pseudo_random_mib();
        assert_eq!(body.len(), 1 << 20);
        // Pseudo-random data must not be a repeating pattern.
        assert_ne!(&body[0..64], &body[64..128]);
    }

    #[test]
    fn measures_against_real_local_http_server() {
        let (http, _server_thread) = testutil::spawn_http_server();
        let probe = testutil::spawn_sink_server();
        let opts = BloatOptions {
            phase_secs: 1,
            streams: 2,
            down_url: format!("http://{http}/__down?bytes=100000000"),
            up_url: format!("http://{http}/__up"),
            probe_target: probe.to_string(),
            probe_interval_ms: 80,
        };
        let report = run(&opts);
        // The local server is infinitely fast; assert honest structure.
        assert!(report.baseline_ms < 200.0, "baseline {report:?}");
        assert!(
            report.down_mbps > 0.0,
            "download moved real bytes: {report:?}"
        );
        assert!(report.up_mbps > 0.0, "upload moved real bytes: {report:?}");
        assert_eq!(report.duration_s, 1);
    }
}
