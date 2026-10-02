//! Latency statistics shared by the optimization tools.

use linkfyr_model::optimize::Grade;

/// Median of an already-sorted, non-empty slice.
pub fn median(sorted: &[f64]) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[mid])
    } else {
        Some(f64::midpoint(sorted[mid - 1], sorted[mid]))
    }
}

/// Nearest-rank percentile of a sorted, non-empty slice. `pct` is 0-100.
pub fn percentile(sorted: &[f64], pct: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (pct / 100.0 * sorted.len() as f64).ceil();
    let idx = (rank as usize).clamp(1, sorted.len()) - 1;
    Some(sorted[idx])
}

/// Mean absolute deviation between consecutive samples: the jitter the
/// user actually feels (spiky = bad for real-time traffic).
pub fn jitter(samples: &[f64]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let deltas: Vec<f64> = samples.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    Some(deltas.iter().sum::<f64>() / deltas.len() as f64)
}

/// Summary of a latency sample set.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LatencySummary {
    pub min_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub jitter_ms: f64,
}

/// Compute min/median/p95/jitter over raw (unsorted) samples.
/// Returns `None` for an empty set.
pub fn summarize(samples: &[f64]) -> Option<LatencySummary> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite latency samples"));
    Some(LatencySummary {
        min_ms: sorted[0],
        median_ms: median(&sorted)?,
        p95_ms: percentile(&sorted, 95.0)?,
        jitter_ms: jitter(samples).unwrap_or(0.0),
    })
}

/// Grade added latency under load (bufferbloat). Thresholds follow the
/// widely used consumer grading scale (Waveform-style).
pub fn grade_added_latency(added_ms: f64) -> Grade {
    if added_ms <= 5.0 {
        Grade::APlus
    } else if added_ms <= 30.0 {
        Grade::A
    } else if added_ms <= 60.0 {
        Grade::B
    } else if added_ms <= 100.0 {
        Grade::C
    } else if added_ms <= 200.0 {
        Grade::D
    } else {
        Grade::F
    }
}

/// Deterministic xorshift PRNG so tests (and uncached DNS probe labels)
/// never depend on external crates.
#[derive(Debug, Clone)]
pub struct Prng(u64);

impl Prng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn from_clock() -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x2545_F491_4F6C_DD1D, |d| d.as_nanos() as u64);
        Self(n | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Random-looking hex string of `len` characters.
    pub fn hex(&mut self, len: usize) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(len);
        while out.len() < len {
            let _ = write!(out, "{:016x}", self.next_u64());
        }
        out.truncate(len);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_percentile_known_vectors() {
        let odd = [1.0, 2.0, 100.0];
        assert_eq!(median(&odd), Some(2.0));
        let even = [1.0, 2.0, 3.0, 10.0];
        assert_eq!(median(&even), Some(2.5));
        assert_eq!(percentile(&odd, 50.0), Some(2.0));
        assert_eq!(percentile(&odd, 100.0), Some(100.0));
        assert_eq!(percentile(&odd, 0.0), Some(1.0));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn percentile_nearest_rank_matches_spreadsheet() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 95.0), Some(95.0));
        assert_eq!(percentile(&v, 99.0), Some(99.0));
    }

    #[test]
    fn jitter_is_mean_abs_delta() {
        assert_eq!(jitter(&[10.0, 12.0, 10.0, 16.0]), Some(10.0 / 3.0));
        assert_eq!(jitter(&[5.0]), None);
        assert_eq!(jitter(&[5.0, 5.0, 5.0]), Some(0.0));
    }

    #[test]
    fn summarize_orders_stats() {
        let s = summarize(&[30.0, 10.0, 20.0]).expect("non-empty");
        assert_eq!(s.min_ms, 10.0);
        assert_eq!(s.median_ms, 20.0);
        assert!(s.p95_ms >= s.median_ms);
        assert_eq!(summarize(&[]), None);
    }

    #[test]
    fn grade_boundaries() {
        assert_eq!(grade_added_latency(0.0), Grade::APlus);
        assert_eq!(grade_added_latency(5.0), Grade::APlus);
        assert_eq!(grade_added_latency(5.1), Grade::A);
        assert_eq!(grade_added_latency(30.0), Grade::A);
        assert_eq!(grade_added_latency(30.5), Grade::B);
        assert_eq!(grade_added_latency(61.0), Grade::C);
        assert_eq!(grade_added_latency(150.0), Grade::D);
        assert_eq!(grade_added_latency(200.1), Grade::F);
    }

    #[test]
    fn prng_is_deterministic_and_hex_shaped() {
        let mut a = Prng::new(42);
        let mut b = Prng::new(42);
        assert_eq!(a.next_u64(), b.next_u64());
        let h = a.hex(12);
        assert_eq!(h.len(), 12);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
