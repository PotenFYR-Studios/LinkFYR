//! Multi-factor interface health scoring.
//!
//! Deterministic and explainable: every factor and its weight is returned
//! with the score so the UI can show exactly why a link scored what it
//! did (docs/ux.md "HealthGauge factor breakdown"). v0 factors: status,
//! error rate growth, throughput utilization sanity, and Internet probe
//! quality applied at engine level (not per-interface).

use linkfyr_model::{HealthScore, HealthStatus, IfCounters, IfStatus};

/// Inputs for scoring one interface between two counter samples.
#[derive(Debug, Clone)]
pub struct HealthInputs {
    pub status: IfStatus,
    /// Errors per second (derived from counter deltas).
    pub rx_errors_per_s: f64,
    pub tx_errors_per_s: f64,
    /// Fraction of probe failures in the recent window (0.0 - 1.0).
    pub probe_loss_fraction: f64,
}

const STATUS_WEIGHT: f64 = 40.0;
const ERROR_WEIGHT: f64 = 30.0;
const PROBE_WEIGHT: f64 = 30.0;

/// Score an interface 0-100 with a factor breakdown.
/// Clamp-only math, no surprises: lower error rates and loss score higher.
pub fn score_interface(inputs: &HealthInputs) -> HealthScore {
    let mut factors: Vec<(String, u8)> = Vec::with_capacity(3);

    // Factor 1: link status dominates.
    let status_score = match inputs.status {
        IfStatus::Up => 1.0_f64,
        IfStatus::Dormant => 0.5,
        IfStatus::Unknown => 0.4,
        IfStatus::Down => 0.0,
    };
    factors.push(("link".into(), (status_score * 100.0) as u8));

    // Factor 2: error rate. 0 err/s = 100; >= 50 err/s = 0. Smooth linear decay.
    let err_rate = inputs.rx_errors_per_s + inputs.tx_errors_per_s;
    let error_score = (1.0 - (err_rate / 50.0)).clamp(0.0, 1.0);
    factors.push(("errors".into(), (error_score * 100.0) as u8));

    // Factor 3: probe loss on the default route (applies to egress-capable links).
    let probe_score = (1.0 - inputs.probe_loss_fraction).clamp(0.0, 1.0);
    factors.push(("reachability".into(), (probe_score * 100.0) as u8));

    // A down link is a down link: no amount of "no errors seen" rescues it.
    let overall = if inputs.status == IfStatus::Down {
        0.0
    } else {
        status_score * STATUS_WEIGHT + error_score * ERROR_WEIGHT + probe_score * PROBE_WEIGHT
    };
    let overall = overall.round().clamp(0.0, 100.0) as u8;

    let status = if inputs.status == IfStatus::Down {
        HealthStatus::Down
    } else if overall >= 80 {
        HealthStatus::Healthy
    } else if overall >= 50 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Poor
    };

    HealthScore {
        overall,
        status,
        factors,
    }
}

/// Convenience: derive per-second error rates from two counter samples.
pub fn errors_per_s(prev: &IfCounters, curr: &IfCounters) -> (f64, f64) {
    let dt_s = (curr.timestamp_ms.saturating_sub(prev.timestamp_ms)) as f64 / 1000.0;
    if dt_s <= 0.0 {
        return (0.0, 0.0);
    }
    let rx = curr.rx_errors.saturating_sub(prev.rx_errors) as f64 / dt_s;
    let tx = curr.tx_errors.saturating_sub(prev.tx_errors) as f64 / dt_s;
    (rx, tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkfyr_model::now_ms;

    fn counters(rx_err: u64, tx_err: u64, ts: u64) -> IfCounters {
        IfCounters {
            id: "x".into(),
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            rx_errors: rx_err,
            tx_errors: tx_err,
            timestamp_ms: ts,
        }
    }

    #[test]
    fn healthy_up_link_with_zero_errors_scores_high() {
        let score = score_interface(&HealthInputs {
            status: IfStatus::Up,
            rx_errors_per_s: 0.0,
            tx_errors_per_s: 0.0,
            probe_loss_fraction: 0.0,
        });
        assert_eq!(score.overall, 100);
        assert_eq!(score.status, HealthStatus::Healthy);
        assert_eq!(score.factors.len(), 3);
    }

    #[test]
    fn down_link_scores_zero_and_down_status() {
        let score = score_interface(&HealthInputs {
            status: IfStatus::Down,
            rx_errors_per_s: 0.0,
            tx_errors_per_s: 0.0,
            probe_loss_fraction: 0.0,
        });
        assert_eq!(score.overall, 0);
        assert_eq!(score.status, HealthStatus::Down);
    }

    #[test]
    fn error_rate_degrades_score_smoothly() {
        let mild = score_interface(&HealthInputs {
            status: IfStatus::Up,
            rx_errors_per_s: 5.0,
            tx_errors_per_s: 0.0,
            probe_loss_fraction: 0.0,
        });
        let heavy = score_interface(&HealthInputs {
            status: IfStatus::Up,
            rx_errors_per_s: 50.0,
            tx_errors_per_s: 0.0,
            probe_loss_fraction: 0.0,
        });
        assert!(mild.overall > heavy.overall);
        assert_eq!(heavy.overall, 70, "50 err/s saturates the error factor");
    }

    #[test]
    fn probe_loss_penalizes_reachability_factor() {
        let score = score_interface(&HealthInputs {
            status: IfStatus::Up,
            rx_errors_per_s: 0.0,
            tx_errors_per_s: 0.0,
            probe_loss_fraction: 0.1,
        });
        assert_eq!(score.overall, 97);
        let reachability = score
            .factors
            .iter()
            .find(|(n, _)| n == "reachability")
            .expect("reachability factor")
            .1;
        assert_eq!(reachability, 90);
    }

    #[test]
    fn degraded_threshold_mapping() {
        let mid = score_interface(&HealthInputs {
            status: IfStatus::Up,
            rx_errors_per_s: 50.0,
            tx_errors_per_s: 50.0,
            probe_loss_fraction: 0.0,
        });
        // status 40 + errors 0 + probe 30 = 70
        assert_eq!(mid.overall, 70);
        assert_eq!(mid.status, HealthStatus::Degraded);
    }

    #[test]
    fn error_rate_delta_math_handles_time_and_saturation() {
        let prev = counters(10, 0, 1_000);
        let curr = counters(60, 5, 2_000);
        let (rx, tx) = errors_per_s(&prev, &curr);
        assert_eq!(rx, 50.0);
        assert_eq!(tx, 5.0);

        // Counter reset (monotonicity violation): saturate to 0, never panic.
        let reset = counters(0, 0, 3_000);
        let (rx, tx) = errors_per_s(&curr, &reset);
        assert_eq!(rx, 0.0);
        assert_eq!(tx, 0.0);

        // Zero/negative elapsed: no division by zero.
        let same = counters(0, 0, now_ms());
        let (rx, _) = errors_per_s(&same, &same);
        assert_eq!(rx, 0.0);
    }
}
