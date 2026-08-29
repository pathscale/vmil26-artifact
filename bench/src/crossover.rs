//! **The primary output: the repetition ratio above which interning pays.**
//!
//! Interning trades a per-name copy for per-use freedom. For a page with `D`
//! distinct names used `U` times at a mean length of `L` bytes, an interned tier
//! costs `D` extra boundary crossings and saves `(U - D) * L` bytes. That is the
//! whole trade, it is exact, and it needs no machine to state.
//!
//! What it does not give you is a decision, because the two sides are in
//! different units. Break-even is where `D * k = (U - D) * L` for `k`, the cost
//! of one crossing expressed in bytes-of-copying. Writing `R = U / D`:
//!
//! ```text
//!     R* = 1 + k / L
//! ```
//!
//! **Interning pays above a repetition ratio of `R*`, and `R*` falls as names
//! get longer.**
//!
//! # Two estimates, because they answer different questions
//!
//! [`Observed`] is the ratio at which arm C stops beating arm A, read straight
//! off the vocabulary ladder by interpolation. It is measured, needs no model,
//! and is the number this sweep actually establishes. It compares a native
//! binding against a JavaScript one, so it is a threshold for *these two paths*,
//! engines included, not for interning in isolation.
//!
//! [`Contrasts`] estimates `k` from two controlled contrasts inside arm C
//! alone, and answers the narrower question of whether the atom tier earns its
//! keep within one ABI.
//!
//! # Why contrasts and not a regression
//!
//! A least-squares fit of `time ~ crossings + bytes` over the whole sweep was
//! tried first. It returned R^2 of 0.6 and a per-crossing cost of two
//! microseconds, which is an order of magnitude more than a wasmi host call can
//! plausibly take: with the configurations differing in tree shape, step count
//! and style work all at once, the fit was attributing everything it could not
//! explain to whichever regressor moved. Two pairs that differ in exactly one
//! thing measure that thing.
//!
//! - **Crossings**, from the chattiness pair: the same bytes delivered in 1,875
//!   fewer calls. Everything else is identical.
//! - **Bytes**, from the name-length pair: the same call count carrying up to
//!   136 KB more. Everything else is identical.

use serde::Serialize;
use workload::scenario::{Config, NameLength};

use crate::report::OperationReport;

/// A measured marginal cost, from one pair differing in one axis.
#[derive(Debug, Clone, Serialize)]
pub struct Contrast {
    pub from: String,
    pub to: String,
    pub delta_crossings: i64,
    pub delta_bytes: i64,
    pub delta_ns: f64,
    /// Nanoseconds per unit of whichever axis moved.
    pub ns_per_unit: f64,
}

/// The exchange rate between a crossing and a byte, from controlled contrasts.
#[derive(Debug, Clone, Serialize)]
pub struct Contrasts {
    pub crossing_pairs: Vec<Contrast>,
    pub byte_pairs: Vec<Contrast>,
    /// Median over [`Self::crossing_pairs`].
    pub ns_per_crossing: Option<f64>,
    /// Median over [`Self::byte_pairs`].
    ///
    /// Expected to be small and noisy: copying is a `memcpy` and these are
    /// kilobytes. Where the pairs disagree in sign, the honest reading is that
    /// the per-byte cost is below this harness's noise floor, and `k` can only
    /// be bounded from below.
    pub ns_per_byte: Option<f64>,
    /// `ns_per_crossing / ns_per_byte`: bytes of copying that cost as much as
    /// one crossing. **This is `k`.**
    pub bytes_per_crossing: Option<f64>,
    /// True when the byte pairs disagreed enough that `k` is a lower bound
    /// rather than an estimate.
    pub k_is_lower_bound: bool,
    pub caveat: String,
}

/// The threshold implied by `k`, for one name length.
#[derive(Debug, Clone, Serialize)]
pub struct Threshold {
    pub name_length: &'static str,
    pub mean_name_bytes: f64,
    pub crossover_ratio: Option<f64>,
    pub is_lower_bound: bool,
    pub explanation: String,
}

/// The ratio at which arm C stops beating arm A, read off the ladder.
#[derive(Debug, Clone, Serialize)]
pub struct Observed {
    pub name_length: &'static str,
    /// **`R`.** Interpolated in log-ratio between the bracketing points.
    pub crossover_ratio: Option<f64>,
    /// The last configuration where the native binding still won.
    pub last_win: Option<(String, f64, f64)>,
    /// The first where it lost.
    pub first_loss: Option<(String, f64, f64)>,
    pub explanation: String,
}

/// One configuration, as the crossover sees it.
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub id: String,
    pub axis: &'static str,
    pub repetition_ratio: f64,
    pub distinct_names: usize,
    pub name_uses: usize,
    pub mean_name_bytes: f64,
    /// Extra crossings the atom tier costs: one per distinct name.
    pub crossings_spent: i64,
    /// Bytes it saves against a binding that copies every use.
    pub bytes_saved: i64,
    /// **`k*`**: bytes saved per crossing spent. Machine-independent and exact.
    /// Interning pays at this configuration if and only if a crossing costs
    /// less than this many bytes of copying.
    pub break_even_bytes_per_crossing: f64,
    pub bytes_advantage: f64,
    /// Arm A median over arm C median. Above 1 means the native binding was
    /// faster.
    pub time_advantage: Option<f64>,
}

/// The whole analysis.
#[derive(Debug, Clone, Serialize)]
pub struct Crossover {
    /// One sentence, for a reader who reads nothing else.
    pub headline: String,
    /// The part that needs no exchange rate and holds on every machine.
    pub machine_independent: String,
    /// What the two estimates disagreeing means. Reported because the gap is a
    /// finding, not an inconsistency to be smoothed over.
    pub interpretation: String,
    pub observed: Vec<Observed>,
    pub contrasts: Contrasts,
    pub thresholds: Vec<Threshold>,
    /// **Reported as prominently as the best, deliberately.** A benchmark whose
    /// author only publishes favourable points is worthless.
    pub worst_for_native: Option<Observation>,
    pub best_for_native: Option<Observation>,
    pub observations: Vec<Observation>,
    /// Configurations where the native binding did not win outright.
    pub native_losses: Vec<String>,
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

/// One point of the sweep, with everything the analysis needs.
pub struct Point<'a> {
    pub id: &'a str,
    pub axis: &'static str,
    pub config: Config,
    pub report: &'a OperationReport,
}

struct Metrics {
    crossings: i64,
    bytes: i64,
    ns: f64,
    advantage: Option<f64>,
    arm_a_bytes: i64,
}

fn metrics(report: &OperationReport) -> Option<Metrics> {
    let a = report.arms.iter().find(|arm| arm.arm == "A")?;
    let c = report.arms.iter().find(|arm| arm.arm == "C")?;
    let ct = c.timing.as_ref()?;
    Some(Metrics {
        crossings: c.counts.boundary_crossings as i64,
        bytes: c.counts.bytes_copied as i64,
        ns: ct.median_ms * 1_000_000.0,
        advantage: a
            .timing
            .as_ref()
            .filter(|_| ct.median_ms > 0.0)
            .map(|at| at.median_ms / ct.median_ms),
        arm_a_bytes: a.counts.bytes_copied as i64,
    })
}

/// Two configurations that differ in exactly one axis.
fn differs_only_in_names(a: Config, b: Config) -> bool {
    a.names != b.names
        && Config {
            names: b.names,
            ..a
        } == b
}

fn differs_only_in_chattiness(a: Config, b: Config) -> bool {
    a.chattiness != b.chattiness
        && Config {
            chattiness: b.chattiness,
            ..a
        } == b
}

fn estimate(points: &[Point]) -> Contrasts {
    let mut crossing_pairs = Vec::new();
    let mut byte_pairs = Vec::new();

    for (i, left) in points.iter().enumerate() {
        for right in points.iter().skip(i + 1) {
            let (Some(lm), Some(rm)) = (metrics(left.report), metrics(right.report)) else {
                continue;
            };
            let delta_ns = rm.ns - lm.ns;
            let delta_crossings = rm.crossings - lm.crossings;
            let delta_bytes = rm.bytes - lm.bytes;

            // Bytes moved, crossings did not: a per-byte measurement.
            if differs_only_in_names(left.config, right.config)
                && delta_crossings == 0
                && delta_bytes != 0
            {
                byte_pairs.push(Contrast {
                    from: left.id.to_string(),
                    to: right.id.to_string(),
                    delta_crossings,
                    delta_bytes,
                    delta_ns,
                    ns_per_unit: delta_ns / delta_bytes as f64,
                });
            }

            // Crossings moved, bytes did not: a per-crossing measurement.
            if differs_only_in_chattiness(left.config, right.config)
                && delta_bytes == 0
                && delta_crossings != 0
            {
                crossing_pairs.push(Contrast {
                    from: left.id.to_string(),
                    to: right.id.to_string(),
                    delta_crossings,
                    delta_bytes,
                    delta_ns,
                    ns_per_unit: delta_ns / delta_crossings as f64,
                });
            }
        }
    }

    let mut crossing_rates: Vec<f64> = crossing_pairs.iter().map(|p| p.ns_per_unit).collect();
    let ns_per_crossing = median(&mut crossing_rates);

    // The largest byte contrast is the most trustworthy: the same absolute
    // noise over a bigger signal. Where the pairs disagree in sign, the per-byte
    // cost is below the noise floor and must not be reported as a number.
    let widest = byte_pairs
        .iter()
        .max_by_key(|pair| pair.delta_bytes.abs())
        .map(|pair| pair.ns_per_unit);
    let signs_agree = byte_pairs.iter().all(|pair| pair.ns_per_unit > 0.0);
    let ns_per_byte = widest.filter(|value| *value > 0.0);

    let k_is_lower_bound = !signs_agree || ns_per_byte.is_none();

    let bytes_per_crossing = match (ns_per_crossing, ns_per_byte) {
        (Some(crossing), Some(byte)) if byte > 0.0 => Some(crossing / byte),
        _ => None,
    };

    let caveat = if byte_pairs.is_empty() || ns_per_crossing.is_none() {
        "Not enough controlled pairs in this sweep to estimate an exchange rate.".to_string()
    } else if k_is_lower_bound {
        format!(
            "The per-byte contrasts disagree in sign across {} pairs, the widest of \
             which moved {} bytes for {:.0} ns. Copying at these sizes is a memcpy and \
             is below this harness's noise floor, so k cannot be estimated, only \
             bounded from below: a crossing costs far more than any byte count here \
             can pay for. Every threshold derived from it is a lower bound.",
            byte_pairs.len(),
            byte_pairs
                .iter()
                .max_by_key(|pair| pair.delta_bytes.abs())
                .map(|pair| pair.delta_bytes)
                .unwrap_or(0),
            byte_pairs
                .iter()
                .max_by_key(|pair| pair.delta_bytes.abs())
                .map(|pair| pair.delta_ns)
                .unwrap_or(0.0),
        )
    } else {
        format!(
            "From {} crossing pairs and {} byte pairs, each differing in exactly one \
             axis. Machine-local: k is a property of this chip and build, not of the \
             ABI.",
            crossing_pairs.len(),
            byte_pairs.len()
        )
    };

    Contrasts {
        crossing_pairs,
        byte_pairs,
        ns_per_crossing,
        ns_per_byte,
        bytes_per_crossing,
        k_is_lower_bound,
        caveat,
    }
}

/// Interpolate where arm C's advantage crosses 1.0 along the vocabulary ladder.
fn observe(points: &[Point], names: NameLength, label: &'static str) -> Observed {
    // Only the vocabulary ladder: everything else held at the baseline, so the
    // ratio is the only thing moving.
    let baseline = Config::baseline();
    let mut series: Vec<(String, f64, f64)> = points
        .iter()
        .filter(|point| {
            point.config.names == names
                && point.config.chattiness == baseline.chattiness
                && point.config.shape == baseline.shape
                && point.config.dispatches == 0
                && point.config.listeners == 0
        })
        .filter_map(|point| {
            let advantage = metrics(point.report)?.advantage?;
            Some((
                point.id.to_string(),
                point.report.repetition_ratio,
                advantage,
            ))
        })
        .collect();
    series.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut last_win = None;
    let mut first_loss = None;
    for window in series.windows(2) {
        let (high, low) = (&window[0], &window[1]);
        if high.2 >= 1.0 && low.2 < 1.0 {
            last_win = Some(high.clone());
            first_loss = Some(low.clone());
            break;
        }
    }

    let crossover = match (&last_win, &first_loss) {
        (Some(win), Some(loss)) if win.1 > 0.0 && loss.1 > 0.0 && win.2 != loss.2 => {
            // Linear in log(ratio): the ladder is geometric, so interpolating
            // linearly in the ratio itself would put the crossing far too close
            // to the high end.
            let t = (win.2 - 1.0) / (win.2 - loss.2);
            Some(10f64.powf(win.1.log10() + t * (loss.1.log10() - win.1.log10())))
        }
        _ => None,
    };

    let explanation = match (&crossover, &last_win, &first_loss) {
        (Some(ratio), Some(win), Some(loss)) => format!(
            "The native binding wins at a repetition ratio of {:.1} ({} at {:.2}x) and \
             loses at {:.1} ({} at {:.2}x). Interpolating in log-ratio puts the \
             crossing at R = {ratio:.0}.",
            win.1, win.0, win.2, loss.1, loss.0, loss.2
        ),
        _ if series.iter().all(|entry| entry.2 >= 1.0) => {
            "The native binding won at every ratio on this ladder, so the crossing is \
             below the lowest ratio swept. Read that as a limit on the sweep."
                .to_string()
        }
        _ if series.iter().all(|entry| entry.2 < 1.0) => {
            "The native binding lost at every ratio on this ladder, so the crossing is \
             above the highest ratio swept."
                .to_string()
        }
        _ => "No clean crossing on this ladder.".to_string(),
    };

    Observed {
        name_length: label,
        crossover_ratio: crossover,
        last_win,
        first_loss,
        explanation,
    }
}

/// Work the crossover out from a finished sweep.
pub fn analyse(points: &[Point]) -> Crossover {
    let mut observations = Vec::new();

    for point in points {
        let Some(m) = metrics(point.report) else {
            continue;
        };
        let distinct = point.report.vocabulary_strings;
        let uses = point.report.vocabulary_uses;
        let mean_length = if distinct > 0 {
            point.report.vocabulary_bytes as f64 / distinct as f64
        } else {
            0.0
        };
        let spent = m.crossings - point.report.arms[0].counts.boundary_crossings as i64;
        let saved = m.arm_a_bytes - m.bytes;

        observations.push(Observation {
            id: point.id.to_string(),
            axis: point.axis,
            repetition_ratio: point.report.repetition_ratio,
            distinct_names: distinct,
            name_uses: uses,
            mean_name_bytes: mean_length,
            crossings_spent: spent,
            bytes_saved: saved,
            break_even_bytes_per_crossing: if spent > 0 {
                saved as f64 / spent as f64
            } else {
                f64::INFINITY
            },
            bytes_advantage: if m.bytes > 0 {
                m.arm_a_bytes as f64 / m.bytes as f64
            } else {
                1.0
            },
            time_advantage: m.advantage,
        });
    }

    let contrasts = estimate(points);
    let observed = vec![
        observe(points, NameLength::Short, "short"),
        observe(points, NameLength::Long, "long"),
    ];

    let mut thresholds = Vec::new();
    for (label, names) in [("short", NameLength::Short), ("long", NameLength::Long)] {
        let lengths: Vec<f64> = points
            .iter()
            .filter(|point| point.config.names == names)
            .map(|point| {
                point.report.vocabulary_bytes as f64 / point.report.vocabulary_strings.max(1) as f64
            })
            .collect();
        if lengths.is_empty() {
            continue;
        }
        let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
        let ratio = contrasts
            .bytes_per_crossing
            .map(|k| 1.0 + k / mean.max(1.0));
        thresholds.push(Threshold {
            name_length: label,
            mean_name_bytes: mean,
            crossover_ratio: ratio,
            is_lower_bound: contrasts.k_is_lower_bound,
            explanation: match ratio {
                Some(value) => format!(
                    "At {mean:.0} bytes per name and k = {:.0} bytes per crossing, \
                     interning pays above R = {value:.0}.",
                    contrasts.bytes_per_crossing.unwrap_or_default()
                ),
                None => "No usable exchange rate, so no modelled threshold. The observed \
                         crossover and the count-based trade both still stand."
                    .to_string(),
            },
        });
    }

    let key = |o: &Observation| o.time_advantage.unwrap_or(o.bytes_advantage);
    let worst = observations
        .iter()
        .min_by(|a, b| {
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();
    let best = observations
        .iter()
        .max_by(|a, b| {
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

    let native_losses = observations
        .iter()
        .filter(|o| key(o) < 1.0)
        .map(|o| {
            format!(
                "{} (advantage {:.2}x, repetition ratio {:.1})",
                o.id,
                key(o),
                o.repetition_ratio
            )
        })
        .collect::<Vec<_>>();

    let headline = match observed
        .iter()
        .filter_map(|entry| {
            entry
                .crossover_ratio
                .map(|ratio| (entry.name_length, ratio))
        })
        .collect::<Vec<_>>()
    {
        pairs if !pairs.is_empty() => format!(
            "Interning pays above a repetition ratio of R = {} (R = total name uses / \
             distinct names). Below it the native binding is slower than the JavaScript \
             one on the same tree.",
            pairs
                .iter()
                .map(|(label, ratio)| format!("{ratio:.0} for {label} names"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "No crossover was reached on this sweep's ladder.".to_string(),
    };

    // The two estimates answer different questions, so where they disagree the
    // gap localises the cost. If interning should already be paying at a given
    // ratio and the native path still loses there, whatever is losing is not
    // the atom tier.
    let interpretation = {
        let modelled_long = thresholds
            .iter()
            .find(|threshold| threshold.name_length == "long")
            .and_then(|threshold| threshold.crossover_ratio);
        let observed_long = observed
            .iter()
            .find(|entry| entry.name_length == "long")
            .and_then(|entry| entry.crossover_ratio);
        match (modelled_long, observed_long) {
            (Some(modelled), Some(seen)) if seen > modelled * 2.0 => format!(
                "The two estimates disagree for long names, and the gap is the point. The \
                 contrast model says the atom tier already pays from R = {modelled:.0}, \
                 but the native path does not overtake JavaScript until R = {seen:.0}. \
                 Interning is therefore not what loses in between: the rest of the path \
                 is, and on this harness that means wasmi interpreting the guest. A \
                 compiled guest would move the observed crossover down toward the \
                 modelled one without changing the ABI at all."
            ),
            (Some(modelled), Some(seen)) => format!(
                "The contrast model puts the atom tier's break-even at R = {modelled:.0} \
                 and the observed crossover against JavaScript is R = {seen:.0}. They \
                 answer different questions - one compares interning against not \
                 interning, the other compares two whole paths - so agreement to within \
                 a factor of two is as much as should be expected."
            ),
            _ => "Only one of the two estimates was available, so they cannot be compared."
                .to_string(),
        }
    };

    Crossover {
        headline,
        interpretation,
        machine_independent: "Needing no exchange rate and true on every machine: an \
                              interned tier costs one extra boundary crossing per distinct \
                              name and saves (uses - distinct) x mean-name-length bytes. At \
                              a repetition ratio of exactly 1 it copies the same bytes as a \
                              non-interning binding and adds one crossing per name, so it \
                              strictly loses. Each configuration's break-even figure is the \
                              bytes it saves per crossing it spends: interning pays there if \
                              and only if a crossing is cheaper than that many bytes."
            .to_string(),
        observed,
        contrasts,
        thresholds,
        worst_for_native: worst,
        best_for_native: best,
        observations,
        native_losses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workload::scenario::Chattiness;

    #[test]
    fn the_median_is_the_middle() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&mut []), None);
    }

    /// Two configurations are a contrast pair only if they differ in exactly
    /// one axis. A pair that differs in two measures neither.
    #[test]
    fn contrast_pairs_must_differ_in_exactly_one_axis() {
        let base = Config::baseline();
        let longer = Config {
            names: NameLength::Long,
            ..base
        };
        let chattier = Config {
            chattiness: Chattiness::FewLarge,
            ..base
        };
        let both = Config {
            names: NameLength::Long,
            chattiness: Chattiness::FewLarge,
            ..base
        };

        assert!(differs_only_in_names(base, longer));
        assert!(!differs_only_in_names(base, both));
        assert!(!differs_only_in_names(base, base));

        assert!(differs_only_in_chattiness(base, chattier));
        assert!(!differs_only_in_chattiness(base, both));
    }
}
