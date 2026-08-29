//! Measures the cost of the boundary between a guest language and a DOM.
//!
//! Not the speed of frameworks. There is no framework here and no framework
//! comparison: one workload, taken from a benchmark that does compare
//! frameworks, run through three different ways of reaching the same document
//! in the same engine. What differs between the arms is the binding, so what
//! the numbers describe is the binding.
//!
//! See `README.md` for provenance, the determinism change from upstream, and
//! why arm B is derived rather than run.
//!
//! # Why this is a library as well as a binary
//!
//! So the tests can drive the arms. The identical-tree assertion is the thing
//! that makes the comparison meaningful at all, and a test that could not
//! construct both arms would have to re-implement them to check them.

pub mod alloc;
pub mod arm_a;
pub mod arm_b;
pub mod arm_c;
pub mod calls;
pub mod counts;
pub mod crossover;
pub mod dispatch;
pub mod machine;
pub mod operation;
pub mod report;
pub mod runner;
pub mod sweep;
pub mod timing;

/// Counted below every crate involved, so no binding can under-report itself.
/// See [`alloc`] for why this is not the binding's own counter.
///
/// Declared in the library rather than the binary so the test harness installs
/// it too: a test asserting an allocation figure against a binary that had the
/// counter and a test process that did not would be comparing two different
/// programs.
#[global_allocator]
static ALLOCATOR: alloc::Counting = alloc::Counting;
