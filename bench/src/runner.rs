//! Running the arms and assembling the report.
//!
//! The order is deliberate: counts before timings, and the identical-tree check
//! before either is trusted. Counts are the result and are cheap; a broken
//! configuration shows up there, before twenty-four timed iterations are spent
//! on it.
//!
//! The identity assertion runs for **every** configuration in the sweep, not
//! once for a representative one. That is what makes a parameterised generator
//! safe: arm A's JavaScript transcribes the generator rather than each
//! scenario, so a transcription bug shows up as a tree mismatch at whichever
//! configuration first exercises it.

use std::time::Duration;

use workload::scenario::{Chattiness, NameLength, Program, TreeShape, Vocabulary};

use crate::calls;
use crate::counts::Counts;
use crate::crossover;
use crate::machine::Machine;
use crate::operation::{Operation, Workload};
use crate::report::{
    ArmReport, ConfigSummary, OperationReport, READING_NOTES, Report, SweepEntry, Workload as WD,
    unmeasured_axes,
};
use crate::sweep;
use crate::timing::{ITERATIONS, Samples, WARMUP};
use crate::{arm_a, arm_b, arm_c};

pub const ARM_A: &str = "JS over the existing script runtime (Boa), against the engine's DOM";
pub const ARM_C: &str =
    "Rust/wasm through the native host binding (blitz-wasm), against the same DOM";

/// Everything, into one report.
pub fn run_all() -> Report {
    let operations: Vec<OperationReport> = Operation::ALL
        .iter()
        .map(|operation| {
            run_workload(
                Workload::Upstream(*operation),
                operation.name().to_string(),
                operation.description().to_string(),
            )
        })
        .collect();

    let points = sweep::points();
    eprintln!("\nSweeping {} configurations.", points.len());

    let mut sweep_entries = Vec::new();
    for point in points {
        let report = run_workload(
            Workload::Sweep(point.config),
            point.id.clone(),
            point.rationale.to_string(),
        );

        let program = Program::build(point.config);
        sweep_entries.push(SweepEntry {
            id: point.id,
            axis: point.axis,
            rationale: point.rationale,
            config: point.config,
            configuration: ConfigSummary {
                vocabulary: match point.config.vocabulary {
                    Vocabulary::Distinct(n) => format!("{n} distinct"),
                    Vocabulary::UniquePerElement => "one per element".to_string(),
                },
                name_length: match point.config.names {
                    NameLength::Short => "short",
                    NameLength::Long => "long (Tailwind-shaped)",
                },
                chattiness: match point.config.chattiness {
                    Chattiness::ManySmall => "many small writes",
                    Chattiness::FewLarge => "few large writes",
                },
                tree_shape: match point.config.shape {
                    TreeShape::Deep => "deep chain",
                    TreeShape::Wide => "wide fan",
                    TreeShape::Realistic => "realistic",
                },
                elements: point.config.elements,
                listeners: point.config.listeners,
                dispatches: point.config.dispatches,
                achieved_max_depth: program.max_depth,
                achieved_leaves: program.leaves,
                achieved_mean_fanout: program.mean_fanout,
            },
            report,
        });
    }

    // Borrowed from the finished entries rather than cloned alongside them, so
    // there is exactly one copy of each report and the analysis cannot be run
    // against a stale duplicate.
    let analysis = crossover::analyse(
        &sweep_entries
            .iter()
            .map(|entry| crossover::Point {
                id: &entry.id,
                axis: entry.axis,
                config: entry.config,
                report: &entry.report,
            })
            .collect::<Vec<_>>(),
    );

    Report {
        schema: "dom-boundary-bench/2",
        workload: WD::describe(),
        machine: Machine::detect(),
        crossover: analysis,
        sweep: sweep_entries,
        operations,
        unmeasured_axes: unmeasured_axes(),
        reading_notes: READING_NOTES.to_vec(),
    }
}

/// One workload, all arms.
pub fn run_workload(workload: Workload, name: String, description: String) -> OperationReport {
    eprintln!("  {name}");

    let (counts_a, tree_a) = arm_a::measure(workload);
    let (counts_c, tree_c) = arm_c::measure(workload);

    // Here rather than only in the test suite, so a report cannot be produced
    // claiming to compare two arms that built different documents.
    assert_identical_trees(&name, &tree_a, &tree_c);

    // Arm B is analysed, not run. It gets no timing and never will.
    let counts_b_default = arm_b::analyse(workload, arm_b::Interning::Off);
    let counts_b_interned = arm_b::analyse(workload, arm_b::Interning::On);

    cross_check(workload, &name, &counts_a, &counts_c);

    let (timing_a, timing_c) = time_interleaved(workload);

    let (distinct, bytes, uses) = calls::vocabulary_stats(workload);

    OperationReport {
        name,
        description,
        vocabulary_strings: distinct,
        vocabulary_bytes: bytes,
        vocabulary_uses: uses,
        repetition_ratio: if distinct > 0 {
            uses as f64 / distinct as f64
        } else {
            0.0
        },
        trees_identical: true,
        arms: vec![
            ArmReport {
                arm: "A",
                description: ARM_A,
                counts: counts_a,
                timing: Some(timing_a),
            },
            ArmReport {
                arm: "B",
                description: arm_b::Interning::Off.label(),
                counts: counts_b_default,
                timing: None,
            },
            ArmReport {
                arm: "B",
                description: arm_b::Interning::On.label(),
                counts: counts_b_interned,
                timing: None,
            },
            ArmReport {
                arm: "C",
                description: ARM_C,
                counts: counts_c,
                timing: Some(timing_c),
            },
        ],
    }
}

/// The check the whole comparison rests on.
pub fn assert_identical_trees(name: &str, tree_a: &str, tree_c: &str) {
    assert!(
        tree_a == tree_c,
        "arms A and C built different trees for {name}, so their counts are not \
         comparable and this report would be meaningless.\n\
         arm A produced {} bytes of HTML, arm C produced {}.\n{}",
        tree_a.len(),
        tree_c.len(),
        difference_window(tree_a, tree_c)
    );
}

/// Alternate the arms rather than running each in a block.
///
/// In a block layout the second arm inherits a warmed allocator and a CPU that
/// has had time to clock up or throttle, and none of that belongs to the
/// binding. Alternating does not remove drift, it spreads it over both arms so
/// it largely cancels in the comparison rather than landing on one of them.
pub fn time_interleaved(workload: Workload) -> (Samples, Samples) {
    for _ in 0..WARMUP {
        arm_a::time_once(workload);
        arm_c::time_once(workload);
    }

    let mut a: Vec<Duration> = Vec::with_capacity(ITERATIONS);
    let mut c: Vec<Duration> = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        // Alternate which arm goes first within the pair too. Whichever runs
        // first in a pair pays for whatever the other one left in the caches,
        // and with a fixed order that penalty always lands on the same arm.
        if iteration % 2 == 0 {
            a.push(arm_a::time_once(workload));
            c.push(arm_c::time_once(workload));
        } else {
            c.push(arm_c::time_once(workload));
            a.push(arm_a::time_once(workload));
        }
    }

    (Samples::from(a), Samples::from(c))
}

/// Check both measured arms against the arm-independent call sequence.
///
/// Arm A's counts come from Boa's binding instrumentation and arm C's from
/// `blitz-wasm`'s counters. Neither instrument knows about `calls.rs`. If both
/// agree with it, three independent descriptions of the workload are
/// consistent, and a silent mistake would have to be the same mistake in all
/// three.
pub fn cross_check(workload: Workload, name: &str, a: &Counts, c: &Counts) {
    for (label, expected) in calls::expected_arm_a_calls(workload) {
        let observed = a
            .operations
            .iter()
            .find(|op| op.name == label)
            .map_or(0, |op| op.calls);
        assert_eq!(
            observed, expected,
            "arm A issued {observed} `{label}` for {name}, the workload specifies {expected}"
        );
    }

    for (import, expected) in calls::expected_arm_c_calls(workload) {
        let observed = c
            .operations
            .iter()
            .find(|op| op.name == import)
            .map_or(0, |op| op.calls);
        assert_eq!(
            observed, expected,
            "arm C issued {observed} `{import}` for {name}, the workload specifies {expected}"
        );
    }

    assert_eq!(
        a.dom_operations, c.dom_operations,
        "the arms built the same tree but issued different numbers of DOM \
         operations for {name}, which should be impossible"
    );
}

/// The first place two trees diverge, with enough either side to see why.
///
/// A byte offset alone sends the reader to fish 200KB of HTML out of a
/// debugger. When this fires it is usually a difference of a few characters.
pub fn difference_window(a: &str, b: &str) -> String {
    let at = a
        .bytes()
        .zip(b.bytes())
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.len().min(b.len()));
    let window = |text: &str| -> String {
        // Snap to char boundaries: the labels are ASCII, but a panic message
        // that itself panics on a slice index helps nobody.
        let mut start = at.saturating_sub(60);
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut stop = (at + 90).min(text.len());
        while stop < text.len() && !text.is_char_boundary(stop) {
            stop += 1;
        }
        text[start..stop].to_string()
    };
    format!(
        "First difference at byte {at}.\n  arm A: ...{}...\n  arm C: ...{}...",
        window(a),
        window(b)
    )
}
