//! The secondary output. Subordinate on purpose.
//!
//! Timing is here because a reviewer will ask for it, not because it is the
//! result. The result is in `counts.rs`. A time from this file is valid only
//! against another time from the same run on the same machine, and the report
//! is shaped to make that hard to forget: arm B has no timing at all, and the
//! machine is stamped into every report.
//!
//! # Interleaved, not blocked
//!
//! Arms alternate: A, C, A, C, and so on, rather than twenty of A followed by
//! twenty of C. Running in blocks makes the arm order a variable. Whichever
//! arm runs second inherits a warmed allocator, a populated page cache and a
//! CPU that has had time to clock up or thermally throttle, and none of that is
//! attributable to the binding. Interleaving does not remove drift; it spreads
//! it across both arms so it cancels in the comparison instead of landing on
//! one of them.
//!
//! # Median and interquartile range, never a bare mean
//!
//! A mean over twenty samples on a general-purpose machine reports the worst
//! scheduling accident as though it were a property of the code. The median
//! ignores it and the interquartile range says how spread the middle was, so a
//! run that was quietly bimodal is visible instead of averaged flat.
//!
//! # Drift
//!
//! Even interleaved, a long run can trend: thermal throttling in one direction,
//! cache warming in the other. [`Samples`] compares its first quarter against
//! its last and reports the shift, so a reader can see that a number moved
//! during the run rather than discovering it later.

use std::time::Duration;

use serde::Serialize;

/// How many timed iterations each arm gets. The brief's floor is 20.
pub const ITERATIONS: usize = 24;

/// Untimed iterations before the counted ones, to pay for first-touch page
/// faults and allocator warmup once rather than inside sample one.
pub const WARMUP: usize = 3;

/// The timings for one arm on one operation.
#[derive(Debug, Clone, Serialize)]
pub struct Samples {
    pub iterations: usize,
    pub median_ms: f64,
    /// 25th percentile.
    pub q1_ms: f64,
    /// 75th percentile.
    pub q3_ms: f64,
    /// `q3 - q1`. The spread of the middle half.
    pub iqr_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub drift: Drift,
    /// Every sample, in the order observed, so a reader can re-derive any of
    /// the above or plot the run rather than trusting this summary.
    pub samples_ms: Vec<f64>,
}

/// Whether the run trended from start to finish.
#[derive(Debug, Clone, Serialize)]
pub struct Drift {
    pub first_quarter_median_ms: f64,
    pub last_quarter_median_ms: f64,
    /// Last quarter against first, as a percentage. Positive means it got
    /// slower as the run went on.
    pub shift_percent: f64,
    /// `true` once the shift exceeds [`Drift::THRESHOLD_PERCENT`], at which
    /// point the median is summarising a moving target and the report says so
    /// instead of quietly presenting it as a steady state.
    pub significant: bool,
}

impl Drift {
    /// Chosen as the point where drift stops being ordinary machine noise and
    /// starts being large enough to change how two arms compare. It is a
    /// reporting threshold, not a pass mark: nothing fails because of it, a
    /// line appears in the output.
    pub const THRESHOLD_PERCENT: f64 = 10.0;

    fn of(sorted_by_time: &[f64]) -> Self {
        let quarter = (sorted_by_time.len() / 4).max(1);
        let first = median_of(&sorted_by_time[..quarter]);
        let last = median_of(&sorted_by_time[sorted_by_time.len() - quarter..]);
        let shift = if first > 0.0 {
            (last - first) / first * 100.0
        } else {
            0.0
        };
        Self {
            first_quarter_median_ms: first,
            last_quarter_median_ms: last,
            shift_percent: shift,
            significant: shift.abs() > Self::THRESHOLD_PERCENT,
        }
    }
}

/// Median of an unsorted slice.
fn median_of(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    percentile(&sorted, 0.5)
}

/// Linear-interpolated percentile of an already-sorted slice.
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = fraction * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        return sorted[lower];
    }
    sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64)
}

impl Samples {
    /// Summarise durations given in observation order.
    #[must_use]
    pub fn from(observed: Vec<Duration>) -> Self {
        let in_order: Vec<f64> = observed.iter().map(|d| d.as_secs_f64() * 1_000.0).collect();
        let mut sorted = in_order.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let q1 = percentile(&sorted, 0.25);
        let q3 = percentile(&sorted, 0.75);

        Self {
            iterations: in_order.len(),
            median_ms: percentile(&sorted, 0.5),
            q1_ms: q1,
            q3_ms: q3,
            iqr_ms: q3 - q1,
            min_ms: sorted.first().copied().unwrap_or(0.0),
            max_ms: sorted.last().copied().unwrap_or(0.0),
            drift: Drift::of(&in_order),
            samples_ms: in_order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(values: &[f64]) -> Vec<Duration> {
        values
            .iter()
            .map(|v| Duration::from_secs_f64(v / 1000.0))
            .collect()
    }

    /// The reason the report does not carry a mean.
    #[test]
    fn one_outlier_does_not_move_the_median() {
        let mut values = vec![10.0; 23];
        values.push(900.0);
        let samples = Samples::from(ms(&values));
        assert!((samples.median_ms - 10.0).abs() < 0.001);
        assert!((samples.max_ms - 900.0).abs() < 0.001);
    }

    #[test]
    fn the_iqr_reports_the_spread_of_the_middle() {
        let samples = Samples::from(ms(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]));
        assert!((samples.q1_ms - 3.0).abs() < 0.001, "{:?}", samples.q1_ms);
        assert!((samples.q3_ms - 7.0).abs() < 0.001, "{:?}", samples.q3_ms);
        assert!((samples.iqr_ms - 4.0).abs() < 0.001);
    }

    #[test]
    fn a_steady_run_reports_no_drift() {
        let samples = Samples::from(ms(&[10.0; 24]));
        assert!(!samples.drift.significant, "{:?}", samples.drift);
    }

    /// A run that doubles by the end must not be summarised as a steady median.
    #[test]
    fn a_run_that_slows_down_is_flagged() {
        let values: Vec<f64> = (0..24).map(|i| 10.0 + i as f64).collect();
        let samples = Samples::from(ms(&values));
        assert!(samples.drift.significant, "{:?}", samples.drift);
        assert!(samples.drift.shift_percent > 0.0);
    }

    /// Drift is measured in observation order, so a run that happened to be
    /// sorted must not be mistaken for a steady one.
    #[test]
    fn drift_uses_observation_order_not_sorted_order() {
        let speeding_up: Vec<f64> = (0..24).map(|i| 40.0 - i as f64).collect();
        let samples = Samples::from(ms(&speeding_up));
        assert!(samples.drift.shift_percent < 0.0, "{:?}", samples.drift);
    }
}
