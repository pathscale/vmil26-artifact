//! The checks that make the comparison mean anything.
//!
//! Every number in the report is a difference between arms. A difference is
//! only about the binding if everything else was held equal, and "everything
//! else" here is the tree. If the arms built different documents then the
//! numbers compare two different pieces of work and the whole exercise is void.
//!
//! This matters more for the sweep than it did for the two fixed operations.
//! Arm A's JavaScript transcribes the *generator*, not each scenario, so a
//! transcription bug does not show up at one hand-written case: it shows up at
//! whichever configuration first exercises the branch that was got wrong. The
//! identity assertion therefore runs across the whole sweep, and that is what
//! makes a parameterised generator safe to add configurations to.

use dom_boundary_bench::arm_b::Interning;
use dom_boundary_bench::operation::{Operation, Workload};
use dom_boundary_bench::runner::difference_window;
use dom_boundary_bench::{arm_a, arm_b, arm_c, calls, runner, sweep};
use workload::scenario::{Chattiness, Config, NameLength, TreeShape, Vocabulary};

/// Cheaper than the real sweep, so the suite stays runnable, but the same
/// generator and the same code paths.
fn small(config: Config) -> Config {
    Config {
        elements: 200,
        dispatches: config.dispatches.min(50),
        listeners: config.listeners.min(20),
        ..config
    }
}

/// **The one the brief asks for: the identity assertion, for every
/// configuration in the sweep.**
///
/// Byte-for-byte over the whole serialisation, not a spot check of node counts.
/// A generator that had drifted by one draw between the arms would still
/// produce the right *number* of nodes with the wrong classes on them, and a
/// length comparison would pass.
#[test]
fn arms_agree_at_every_sweep_configuration() {
    for point in sweep::points() {
        let workload = Workload::Sweep(small(point.config));
        let (_, tree_a) = arm_a::measure(workload);
        let (_, tree_c) = arm_c::measure(workload);

        assert!(
            tree_a == tree_c,
            "{} produced different trees.\n{}",
            point.id,
            difference_window(&tree_a, &tree_c)
        );
        assert!(
            tree_a.len() > 500,
            "{} produced a suspiciously small tree ({} bytes); two empty documents \
             compare equal and this test would pass without testing anything",
            point.id,
            tree_a.len()
        );
    }
}

/// And the same for the upstream baseline.
#[test]
fn arms_agree_on_the_upstream_operations() {
    for operation in Operation::ALL {
        let workload = Workload::Upstream(operation);
        let (_, tree_a) = arm_a::measure(workload);
        let (_, tree_c) = arm_c::measure(workload);
        assert!(
            tree_a == tree_c,
            "{} produced different trees.\n{}",
            operation.name(),
            difference_window(&tree_a, &tree_c)
        );
    }
}

/// The corners the sweep exists to reach, checked directly rather than relying
/// on the sweep table still containing them.
#[test]
fn arms_agree_at_the_adversarial_corners() {
    let base = Config {
        elements: 200,
        ..Config::baseline()
    };
    let corners = [
        // Long and distinct together: the worst case for the atom tier.
        Config {
            vocabulary: Vocabulary::UniquePerElement,
            names: NameLength::Long,
            ..base
        },
        // A deep chain with few large writes.
        Config {
            shape: TreeShape::Deep,
            chattiness: Chattiness::FewLarge,
            ..base
        },
        // A wide fan with a dispatch storm.
        Config {
            shape: TreeShape::Wide,
            listeners: 20,
            dispatches: 50,
            ..base
        },
    ];
    for config in corners {
        let workload = Workload::Sweep(config);
        let (_, tree_a) = arm_a::measure(workload);
        let (_, tree_c) = arm_c::measure(workload);
        assert!(
            tree_a == tree_c,
            "{config:?} produced different trees.\n{}",
            difference_window(&tree_a, &tree_c)
        );
    }
}

/// A dispatch storm must actually deliver, and both arms must deliver the same
/// number of events.
///
/// Without this the event axis could be measuring nothing: a storm that matched
/// no listeners would leave both trees identical and cost both arms nothing,
/// and the identity assertion would happily pass.
#[test]
fn the_storm_lands_and_lands_equally() {
    let config = Config {
        elements: 200,
        listeners: 20,
        dispatches: 50,
        ..Config::baseline()
    };
    let workload = Workload::Sweep(config);
    let (counts_c, tree_c) = arm_c::measure(workload);
    let (counts_a, tree_a) = arm_a::measure(workload);

    assert!(
        tree_c.contains("fired:50"),
        "the storm should have run 50 handlers; the tree says otherwise"
    );
    assert_eq!(tree_a, tree_c, "both arms must fire the same number");

    let dispatches = counts_c
        .operations
        .iter()
        .find(|op| op.name == "dispatch")
        .expect("arm C reports a dispatch row");
    assert_eq!(dispatches.calls, 50);
    // The claim the event axis exists to test.
    assert_eq!(
        dispatches.bytes_copied, 0,
        "a click must copy zero bytes across the boundary"
    );

    let arm_a_dispatches = counts_a
        .operations
        .iter()
        .find(|op| op.name == "event:click")
        .expect("arm A attributes dispatches by event name");
    assert_eq!(arm_a_dispatches.calls, 50);
}

/// Both measured arms against the arm-independent call sequence, across the
/// sweep.
///
/// Three descriptions of one workload from places that do not know about each
/// other: Boa's binding instrumentation, `blitz-wasm`'s counters, and
/// `calls.rs`. Agreement between all three is worth more than any one of them
/// being carefully written.
#[test]
fn both_measured_arms_match_the_specified_call_sequence() {
    for point in sweep::points().into_iter().take(6) {
        let workload = Workload::Sweep(small(point.config));
        let (counts_a, _) = arm_a::measure(workload);
        let (counts_c, _) = arm_c::measure(workload);
        runner::cross_check(workload, &point.id, &counts_a, &counts_c);
    }
    for operation in Operation::ALL {
        let workload = Workload::Upstream(operation);
        let (counts_a, _) = arm_a::measure(workload);
        let (counts_c, _) = arm_c::measure(workload);
        runner::cross_check(workload, operation.name(), &counts_a, &counts_c);
    }
}

/// Arm C interns each distinct name exactly once, however many elements use it.
///
/// This is the claim the interned tier exists to make, and the denominator of
/// the repetition ratio. A regression that started re-interning per use would
/// fail here rather than quietly show up as a larger number in a report nobody
/// diffed.
#[test]
fn arm_c_interns_each_name_once_however_often_it_is_used() {
    let config = Config {
        elements: 200,
        vocabulary: Vocabulary::Distinct(8),
        ..Config::baseline()
    };
    let workload = Workload::Sweep(config);
    let (counts, _) = arm_c::measure(workload);

    let interns = counts
        .operations
        .iter()
        .find(|op| op.name == "intern")
        .expect("arm C reports an intern row");
    assert_eq!(
        interns.calls as usize,
        calls::vocabulary_size(workload),
        "one intern per distinct name, not one per use"
    );
    // Eight class values, plus `div` and `class`.
    assert_eq!(interns.calls, 10);
}

/// The structural operations really do copy nothing, at every configuration.
#[test]
fn arm_c_moves_no_bytes_for_structure() {
    for names in [NameLength::Short, NameLength::Long] {
        let workload = Workload::Sweep(Config {
            elements: 200,
            names,
            ..Config::baseline()
        });
        let (counts, _) = arm_c::measure(workload);
        for name in ["create_element", "append_child", "set_attribute"] {
            let op = counts
                .operations
                .iter()
                .find(|op| op.name == name)
                .unwrap_or_else(|| panic!("arm C reports a {name} row"));
            assert!(op.calls > 0, "{name} should have been called");
            assert_eq!(
                op.bytes_copied, 0,
                "{name} carries only handles and atoms, so it must copy nothing"
            );
        }
    }
}

/// Arm B is derived, so it is checked against the arms that are not.
///
/// In the configuration that matches an arm's design, the model must land on
/// that arm's measured byte count exactly: wasm-bindgen with interning off
/// copies every string on every call, as Boa does; with interning on it sends
/// each name once, as the native binding does.
#[test]
fn the_derived_arm_agrees_with_the_measured_arms_on_bytes() {
    let mut workloads: Vec<Workload> = Operation::ALL.map(Workload::Upstream).to_vec();
    workloads.extend(
        sweep::points()
            .into_iter()
            .take(6)
            .map(|point| Workload::Sweep(small(point.config))),
    );

    for workload in workloads {
        let (counts_a, _) = arm_a::measure(workload);
        let (counts_c, _) = arm_c::measure(workload);

        assert_eq!(
            arm_b::analyse(workload, Interning::Off).bytes_copied,
            counts_a.bytes_copied,
            "a copying binding must move the same bytes as Boa does"
        );
        assert_eq!(
            arm_b::analyse(workload, Interning::On).bytes_copied,
            counts_c.bytes_copied,
            "an interning binding must move the same bytes as the native binding does"
        );
    }
}

/// Arm B is never timed. Ever.
///
/// A derived number that acquires a duration is a derived number that will be
/// plotted against a measured one.
#[test]
fn the_derived_arm_carries_no_timing() {
    let report = runner::run_workload(
        Workload::Sweep(Config {
            elements: 100,
            ..Config::baseline()
        }),
        "timing-guard".to_string(),
        "checks that a derived arm never acquires a duration".to_string(),
    );
    for arm in &report.arms {
        let derived =
            arm.counts.derivation == dom_boundary_bench::counts::Derivation::StaticAnalysis;
        assert_eq!(
            derived,
            arm.timing.is_none(),
            "{} is {}derived but {} a timing",
            arm.description,
            if derived { "" } else { "not " },
            if arm.timing.is_some() {
                "has"
            } else {
                "has no"
            }
        );
    }
}

/// The vocabulary axis must actually move the repetition ratio, or the
/// crossover has nothing to interpolate between.
#[test]
fn the_sweep_spans_a_real_range_of_repetition_ratios() {
    let ratios: Vec<f64> = sweep::points()
        .iter()
        .map(|point| {
            let (distinct, _, uses) = calls::vocabulary_stats(Workload::Sweep(point.config));
            uses as f64 / distinct as f64
        })
        .collect();
    let low = ratios.iter().cloned().fold(f64::INFINITY, f64::min);
    let high = ratios.iter().cloned().fold(0.0, f64::max);
    assert!(low < 4.0, "nothing adversarial: lowest ratio was {low}");
    assert!(high > 100.0, "nothing favourable: highest ratio was {high}");
}

/// The comparison is about the binding, so the operation counts must not differ
/// between the arms. Same tree, same work.
#[test]
fn the_arms_issue_the_same_dom_operations() {
    for point in sweep::points().into_iter().take(6) {
        let workload = Workload::Sweep(small(point.config));
        let (a, _) = arm_a::measure(workload);
        let (c, _) = arm_c::measure(workload);
        assert_eq!(a.dom_operations, c.dom_operations, "{}", point.id);

        // But not the same crossings: arm C adds `intern`, and that asymmetry
        // is the thing being reported rather than an error.
        assert!(c.boundary_crossings >= c.dom_operations);
    }
}
