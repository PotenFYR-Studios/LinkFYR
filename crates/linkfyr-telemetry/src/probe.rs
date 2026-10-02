//! Internet latency/loss probing over TCP connect timing.
//!
//! Works unprivileged on every OS (a SYN→SYN-ACK/accept or RST is a
//! round trip to the target; failures count as loss). This is an honest
//! "Internet path" signal, not ICMP gateway RTT — per-path/per-gateway
//! ICMP arrives with the Diagnostics toolbox (Phase 2, capability-matrix
//! gated on Windows raw sockets).

use std::time::Duration;

use linkfyr_model::{ProbeSample, ProbeStats};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Debug, Clone)]
pub struct ProbeConfig {
    pub targets: Vec<String>,
    pub per_attempt_timeout: Duration,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            targets: vec!["1.1.1.1:443".into(), "8.8.8.8:443".into()],
            per_attempt_timeout: Duration::from_millis(1500),
        }
    }
}

#[derive(Debug)]
pub struct TcpProber {
    config: ProbeConfig,
}

impl TcpProber {
    pub fn new(config: ProbeConfig) -> Self {
        Self { config }
    }

    /// Probe every target once (concurrently) and return the samples.
    pub async fn probe_once(&self) -> Vec<ProbeSample> {
        let jobs: Vec<_> = self
            .config
            .targets
            .iter()
            .map(|t| {
                let target = t.clone();
                let timeout_dur = self.config.per_attempt_timeout;
                tokio::spawn(async move {
                    let started = std::time::Instant::now();
                    let ok = timeout(timeout_dur, TcpStream::connect(&target)).await;
                    let rtt = ok
                        .ok()
                        .and_then(|res| res.ok())
                        .map(|_| started.elapsed().as_secs_f64() * 1000.0);
                    ProbeSample {
                        target,
                        rtt_ms: rtt,
                        ok: rtt.is_some(),
                        timestamp_ms: linkfyr_model::now_ms(),
                    }
                })
            })
            .collect();

        let mut samples = Vec::with_capacity(jobs.len());
        for job in jobs {
            match job.await {
                Ok(s) => samples.push(s),
                Err(e) => tracing::debug!("probe task failed: {e}"),
            }
        }
        samples
    }
}

/// Aggregate probe samples over a rolling window into [`ProbeStats`].
pub fn aggregate(samples: &[ProbeSample]) -> ProbeStats {
    if samples.is_empty() {
        return ProbeStats::default();
    }
    let total = samples.len() as f64;
    let ok: Vec<f64> = samples.iter().filter_map(|s| s.rtt_ms).collect();
    let loss_pct = ((total - ok.len() as f64) / total) * 100.0;
    // One decimal: stable for display and assertions (FP-safe).
    let loss_pct = (loss_pct * 10.0).round() / 10.0;

    let (min, avg, max, jitter) = if ok.is_empty() {
        (None, None, None, None)
    } else {
        let min = ok.iter().copied().fold(f64::INFINITY, f64::min);
        let max = ok.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let avg = ok.iter().sum::<f64>() / ok.len() as f64;
        // Jitter: mean absolute successive difference over the window.
        let jitter = if ok.len() >= 2 {
            let diffs: f64 = ok.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
            Some(diffs / (ok.len() - 1) as f64)
        } else {
            None
        };
        (Some(min), Some(avg), Some(max), jitter)
    };

    ProbeStats {
        rtt_avg_ms: avg,
        rtt_min_ms: min,
        rtt_max_ms: max,
        jitter_ms: jitter,
        loss_pct: Some(loss_pct),
        sample_count: samples.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(rtt: Option<f64>) -> ProbeSample {
        ProbeSample {
            target: "1.1.1.1:443".into(),
            rtt_ms: rtt,
            ok: rtt.is_some(),
            timestamp_ms: 0,
        }
    }

    #[test]
    fn empty_window_gives_defaults() {
        let stats = aggregate(&[]);
        assert_eq!(stats.sample_count, 0);
        assert!(stats.rtt_avg_ms.is_none());
    }

    #[test]
    fn all_ok_produces_min_avg_max_and_jitter() {
        let stats = aggregate(&[sample(Some(10.0)), sample(Some(20.0)), sample(Some(30.0))]);
        assert_eq!(stats.sample_count, 3);
        assert_eq!(stats.rtt_min_ms, Some(10.0));
        assert_eq!(stats.rtt_max_ms, Some(30.0));
        assert_eq!(stats.rtt_avg_ms, Some(20.0));
        assert_eq!(stats.jitter_ms, Some(10.0));
        assert_eq!(stats.loss_pct, Some(0.0));
    }

    #[test]
    fn loss_counts_timeouts_and_is_percentage_of_window() {
        let stats = aggregate(&[sample(Some(10.0)), sample(None), sample(None)]);
        assert_eq!(stats.loss_pct, Some(66.7));
        assert_eq!(stats.rtt_avg_ms, Some(10.0));
        assert_eq!(stats.sample_count, 3);
    }

    #[test]
    fn all_lost_has_no_rtt_but_loss_hundred() {
        let stats = aggregate(&[sample(None), sample(None)]);
        assert_eq!(stats.loss_pct, Some(100.0));
        assert!(stats.rtt_avg_ms.is_none());
    }

    #[test]
    fn half_lost_rounds_to_one_decimal() {
        let stats = aggregate(&[sample(Some(5.0)), sample(None)]);
        assert_eq!(stats.loss_pct, Some(50.0));
    }
}
