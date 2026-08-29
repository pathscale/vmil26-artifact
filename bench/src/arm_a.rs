//! Arm A: JavaScript over the existing script runtime, against the engine's DOM.
//!
//! Boa evaluates the workload and reaches the document through the DOM bindings
//! `blitz-script` already exposes. Nothing here is a special path built for the
//! benchmark: it is the binding layer a page in this engine actually uses.
//!
//! # How arm A is counted
//!
//! Existing engine instrumentation plus one small addition, all inside the
//! engine rather than inferred from the JavaScript source:
//!
//! - **DOM operations and boundary crossings** come from `script_stats`'
//!   per-binding instrumentation. Its call counts are exact.
//! - **Dispatches** come from the same log, which attributes an event delivery
//!   by name, so a click storm appears as `event:click`.
//! - **Bytes copied** come from a counter at `dom::to_rust_string`, the single
//!   point where a string leaves the JavaScript heap for the host.
//! - **Allocations** come from the harness's global allocator, below every
//!   crate involved.
//!
//! None of these are derived from reading `arm_a.js`. They are what the engine
//! did.
//!
//! # Why counting and timing are separate passes
//!
//! The instrumentation costs two clock reads per DOM operation, which on a
//! twenty-six-thousand-operation build is a measurable share of the result.
//! Both collectors are gated on `deep_profiling_enabled`, so the counting pass
//! turns it on and the timing pass leaves it off. Counts are
//! machine-independent, so perturbing the counting pass costs nothing; timing
//! is not, so the timing pass runs the same code with the collectors dormant.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use blitz_dom::{Document, DocumentConfig};
use blitz_script::ScriptDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::alloc;
use crate::counts::{Counts, Derivation, OpCounts};
use crate::dispatch;
use crate::operation::{Operation, Workload};

/// The bindings this benchmark reaches, in the labels `script_stats` records
/// them under.
///
/// Listed explicitly rather than accepting every label, so that a binding the
/// workload was not supposed to touch shows up as an unexpected entry instead
/// of being quietly folded into the total. `assert_expected` enforces it.
const EXPECTED_LABELS: [&str; 7] = [
    "dom:createElement",
    "dom:createTextNode",
    "dom:appendChild",
    "dom:attr=",
    "dom:textContent=",
    "dom:addEventListener",
    "event:click",
];

/// Arm A's instruments are process-global; only one measurement at a time.
///
/// `script_stats` keeps one log for the process and `deep_profiling_enabled` is
/// a single atomic, so two concurrent measurements interleave their DOM call
/// counts into the same buckets and race on the switch. Under `cargo test`,
/// which runs test functions on parallel threads by default, that showed up as
/// arm A reporting three times the appends it made and two thirds of the bytes:
/// one test's totals absorbed another's calls while a third turned the
/// collectors off underneath it.
///
/// Held for the whole of `measure`, and for `time_once` as well. Timing does
/// not read the counters, but a `measure` running beside it would switch the
/// collectors on and make it pay for clock reads the sample is reported
/// without.
static SERIAL: Mutex<()> = Mutex::new(());

/// Poisoning is not meaningful here: the guarded state is a counter log cleared
/// at the start of every measurement, so a panicked predecessor cannot leave
/// anything a successor would misread.
fn serialise() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A document with the workload's JavaScript loaded.
pub struct ArmA {
    doc: ScriptDocument,
}

impl ArmA {
    pub fn new() -> Self {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(1024, 768, 1.0, ColorScheme::Light)),
            ..Default::default()
        };
        let mut doc = ScriptDocument::from_html("<html><body></body></html>", config);

        // The seed is injected rather than written into the JavaScript, so
        // there is exactly one place it is defined and both arms read it.
        let source = format!(
            "{}\nSEED = {}; __state = SEED;\n",
            include_str!("arm_a.js"),
            workload::SEED
        );
        doc.eval(&source);
        Self { doc }
    }

    /// The tree, serialised. Used only to compare arms.
    pub fn page_source(&self) -> String {
        self.doc.page_source()
    }

    /// Everything the workload needs in place before the measured window, and
    /// nothing that belongs inside it.
    ///
    /// For a sweep point this expands the program and allocates every string it
    /// will send: guest-side work that is not the boundary, and that would put
    /// a string formatter into a measurement of a binding if it ran inside the
    /// clock.
    fn set_up(&mut self, workload: Workload) {
        match workload {
            Workload::Upstream(operation) => {
                self.doc.eval("prepare()");
                if operation == Operation::PartialUpdate {
                    self.create_rows(workload::op::UPDATE_TABLE_ROWS);
                }
            }
            Workload::Sweep(config) => {
                let [kind, size, names, chattiness, shape, elements, listeners] = config.as_ints();
                self.doc.eval(&format!(
                    "sweepPrepare({kind},{size},{names},{chattiness},{shape},{elements},{listeners})"
                ));
            }
        }
    }

    fn run(&mut self, workload: Workload) {
        match workload {
            Workload::Upstream(Operation::Create1000) => {
                self.create_rows(workload::op::CREATE_ROWS);
            }
            Workload::Upstream(Operation::PartialUpdate) => {
                self.doc.eval("updateEveryTenth()");
            }
            Workload::Sweep(config) => {
                self.doc.eval("sweepBuild()");
                for _ in 0..config.dispatches {
                    self.click();
                }
            }
        }
    }

    /// Write the dispatch tally into the document, so a storm leaves a trace
    /// the identity assertion can see. Outside the measured window.
    fn settle(&mut self, workload: Workload) {
        if matches!(workload, Workload::Sweep(_)) {
            self.doc.eval("sweepSettle()");
        }
    }

    fn create_rows(&mut self, count: usize) {
        self.doc.eval(&format!("createRows({count})"));
    }

    /// Deliver one synthetic click through the engine's own event driver.
    ///
    /// The target is resolved by walking the document rather than through any
    /// bookkeeping of arm A's own, so it is provably the same element arm C
    /// clicks. See [`crate::dispatch`].
    fn click(&mut self) {
        let event = {
            let doc = self.doc.inner();
            dispatch::first_body_element(&doc).and_then(|target| dispatch::click(&doc, target))
        };
        if let Some(event) = event {
            self.doc.dispatch_dom_event(event);
        }
    }
}

/// There is one way to build this arm, so `Default` is `new`. Present for the
/// lint, not because a second construction path is wanted.
impl Default for ArmA {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the workload with the collectors on, and report what it cost.
///
/// Returns the counts and the resulting tree, so the caller can compare the
/// tree against arm C's without running the workload a third time.
pub fn measure(workload: Workload) -> (Counts, String) {
    let _serial = serialise();

    let mut arm = ArmA::new();
    arm.set_up(workload);

    // Cleared *after* setup: building 10,000 rows would otherwise be counted
    // as part of an operation defined as touching 1,000 of them.
    blitz_script::script_stats::clear();
    blitz_script::script_stats::reset_boundary_counters();
    blitz_traits::profiling::set_deep_profiling_enabled(true);

    let allocations_before = alloc::Snapshot::take();
    arm.run(workload);
    let allocated = allocations_before.since();

    blitz_traits::profiling::set_deep_profiling_enabled(false);

    let breakdown = blitz_script::script_stats::work_breakdown();
    let boundary = blitz_script::script_stats::boundary_counters();

    let mut operations: Vec<OpCounts> = breakdown
        .iter()
        .filter(|(label, ..)| EXPECTED_LABELS.contains(&label.as_str()))
        .map(|(label, calls, ..)| OpCounts {
            name: label.clone(),
            calls: *calls,
            // Per-binding byte attribution would need a counter in each
            // binding rather than one at the conversion. The total is exact
            // and is what the comparison uses; splitting it per operation
            // would be a second instrument to get wrong, so it is left out
            // rather than estimated.
            bytes_copied: 0,
        })
        .collect();
    operations.sort_by(|a, b| a.name.cmp(&b.name));

    assert_expected(&breakdown);

    let dom_operations: u64 = operations.iter().map(|op| op.calls).sum();

    let mut counts = Counts::new(Derivation::Measured);
    counts.dom_operations = dom_operations;
    // Every DOM operation in this binding is exactly one call across the
    // boundary. There is no setup crossing and no interning step, so the two
    // columns are equal by construction rather than by coincidence.
    counts.boundary_crossings = dom_operations;
    counts.bytes_copied = boundary.bytes_copied;
    // No interned tier exists, so nothing is amortised and the steady state is
    // the same as the first paint. This equality is arm A's result, not a gap
    // in the report.
    counts.bytes_copied_steady_state = boundary.bytes_copied;
    counts.process_allocations = allocated.allocations;
    counts.process_allocated_bytes = allocated.allocated_bytes;
    counts.binding_reported_allocations = None;
    counts.operations = operations;

    counts.note(format!(
        "{} strings crossed from the JavaScript heap, none of them interned: a \
         tag or attribute name is copied again on every call that uses it.",
        boundary.strings_crossed
    ));
    counts.note(
        "Per-operation `bytes_copied` is reported as 0 because the byte counter \
         sits at the single string-conversion point rather than in each \
         binding. The arm total is exact."
            .to_string(),
    );
    counts.note(
        "`process_allocations` includes Boa's own work for this call, not only \
         the DOM bindings. It is the cost of running the workload in this arm, \
         not an attribution to the boundary alone."
            .to_string(),
    );

    arm.settle(workload);
    (counts, arm.page_source())
}

/// One timed iteration, collectors dormant.
///
/// Setup happens before the clock starts, so a `partial-update` sample is a
/// thousand text writes and not the ten thousand rows they are written into,
/// and a sweep sample is the program's host calls and not the program's
/// expansion.
pub fn time_once(workload: Workload) -> Duration {
    let _serial = serialise();

    let mut arm = ArmA::new();
    arm.set_up(workload);

    let started = Instant::now();
    arm.run(workload);
    started.elapsed()
}

/// Fail loudly if the workload touched a binding it was not supposed to.
///
/// A silent extra binding would inflate `dom_operations` for arm A only, and
/// the comparison would show a difference that came from the harness rather
/// than from the boundary.
fn assert_expected(breakdown: &[(String, u64, f64, f64)]) {
    for (label, calls, ..) in breakdown {
        // `startup:` and `poll_hook` are the runtime's own buckets, not DOM
        // bindings, and are not part of any measured window here.
        if label.starts_with("startup:") || label == "poll_hook" || label == "timers" {
            continue;
        }
        assert!(
            EXPECTED_LABELS.contains(&label.as_str()),
            "arm A touched an unexpected binding `{label}` ({calls} calls). Either \
             the JavaScript changed or a binding was renamed; both need the \
             comparison re-checked rather than the label added here."
        );
    }
}
