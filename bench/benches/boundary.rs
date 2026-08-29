//! Criterion timings for the two measured arms.
//!
//! # What this file is for, and what it deliberately does not touch
//!
//! **No count comes from here.** Crossings, bytes copied and steady-state bytes
//! are deterministic properties of the interface, produced by the instrumented
//! path in `src/` and asserted in `tests/arms_agree.rs`. They do not vary
//! between runs and they do not need statistics. This file measures time, which
//! does.
//!
//! It exists to answer one question the hand-rolled sampler in `src/timing.rs`
//! could not, and the answer turned out to be that the question was wrong.
//!
//! That sampler prices a boundary crossing at around 155 ns from pairs that
//! differ only in call count, and cannot produce a matching per-byte figure:
//! across seven pairs the estimates disagree in sign. The obvious diagnosis was
//! that copying is below the harness's noise floor and confidence intervals
//! would confirm it.
//!
//! They refute it. On a quiet machine, `v8-short-many-real` against
//! `v8-long-many-real` holds arm C's crossings identical at 12,010 and moves
//! its bytes copied by 552 --- and moves its time by 1.1 ms, with intervals
//! nowhere near touching. That prices a byte at two microseconds, against a
//! `memcpy` of 552 bytes costing tens of nanoseconds. The effect is not noise
//! and it is not copying.
//!
//! **The pair does not vary bytes alone.** `NameLength` changes how long the
//! page's names are, and a longer name is hashed longer, compared longer and
//! stored in a bigger slot in the atom table. There is no content-length axis
//! in `Config` --- the only lever that moves payload size also moves string
//! processing --- so no pair this harness can build isolates a per-byte cost,
//! at any sample size. One such pair is kept below as evidence of the confound.
//! The rest are removed, because running them would produce a number with a
//! confidence interval and no meaning.
//!
//! # Why `iter_custom`
//!
//! `arm_a::time_once` and `arm_c::time_once` return the duration of the
//! measured window only, with tree setup and program expansion outside it.
//! Wrapping them in `b.iter(...)` would time the setup too and measure a
//! different thing than the rest of the harness does. `iter_custom` hands
//! Criterion the same window the JSON reports, so the two are comparable.

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use dom_boundary_bench::operation::Workload;
use dom_boundary_bench::{arm_a, arm_c, sweep};

/// The vocabulary ladder, in the order the paper's table lists it.
///
/// One axis varied and everything else held, which is what makes the crossover
/// readable. The other four axes in `sweep::points()` are not timed here.
const LADDER: [&str; 6] = [
    "v8-short-many-real",
    "v80-short-many-real",
    "v150-short-many-real",
    "v300-short-many-real",
    "v800-short-many-real",
    "vunique-short-many-real",
];

/// Pairs differing only in **call count**, bytes held fixed.
///
/// The contrast that already works. Included so the run carries its own
/// control: if these intervals are also wide, the machine was not quiet and
/// nothing else in the run should be believed either.
const CROSSING_PAIRS: [(&str, &str); 2] = [
    ("v8-short-many-real", "v8-short-few-real"),
    ("vunique-short-many-real", "vunique-short-few-real"),
];

/// The one byte pair kept, and it is kept as a negative result.
///
/// Not a measurement of what a byte costs. Arm C crosses 552 more bytes here
/// and makes exactly as many crossings, and the time difference is three
/// orders of magnitude larger than copying 552 bytes can account for. It is in
/// the run so the confound is demonstrated from data rather than asserted in a
/// comment, and so that anyone who later adds a content-length axis has the
/// before number to compare against.
const CONFOUNDED_PAIR: (&str, &str) = ("v8-short-many-real", "v8-long-many-real");

fn workload(id: &str) -> Workload {
    let point = sweep::points()
        .into_iter()
        .find(|point| point.id == id)
        .unwrap_or_else(|| panic!("no sweep point `{id}`; see sweep::points()"));
    Workload::Sweep(point.config)
}

/// Time both arms on one configuration, under one group.
///
/// Both arms in the same group and adjacent in submission order, so that any
/// drift over the run lands on the pair rather than on one side of it. This is
/// the same reasoning as `runner::time_interleaved`, which alternates within a
/// pair; Criterion will not interleave at that granularity, so adjacency is as
/// close as this gets and the control pairs above are how we check it was
/// enough.
fn pair(criterion: &mut Criterion, group_name: &str, id: &str) {
    let work = workload(id);
    let mut group = criterion.benchmark_group(group_name);

    group.bench_with_input(BenchmarkId::new("A-in-engine-js", id), &work, |b, &work| {
        b.iter_custom(|iters| {
            (0..iters).map(|_| arm_a::time_once(work)).sum::<Duration>()
        })
    });

    group.bench_with_input(BenchmarkId::new("C-native", id), &work, |b, &work| {
        b.iter_custom(|iters| {
            (0..iters).map(|_| arm_c::time_once(work)).sum::<Duration>()
        })
    });

    group.finish();
}

fn ladder(criterion: &mut Criterion) {
    for id in LADDER {
        pair(criterion, "ladder", id);
    }
}

fn contrasts(criterion: &mut Criterion) {
    // Deduplicated: several pairs share an endpoint with the ladder and with
    // each other, and benching the same configuration twice would spend the
    // time without adding a sample.
    let mut seen: Vec<&str> = LADDER.to_vec();
    for (from, to) in CROSSING_PAIRS.into_iter().chain([CONFOUNDED_PAIR]) {
        for id in [from, to] {
            if !seen.contains(&id) {
                seen.push(id);
                pair(criterion, "contrast", id);
            }
        }
    }
}

criterion_group! {
    name = boundary;
    // The workloads run in single-digit milliseconds and the question is about
    // interval width, so the defaults are loosened rather than trimmed: a wide
    // interval has to mean "the cost is not there" and not "we did not look
    // long enough". Stated here rather than left implicit, because a reader of
    // the intervals needs to know what produced them.
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(6))
        .sample_size(60);
    targets = ladder, contrasts
}
criterion_main!(boundary);
