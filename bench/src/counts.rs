//! The primary output: what an operation costs the boundary, in counts.
//!
//! Counts rather than times because a count is a property of the interface and
//! a time is a property of the afternoon. `create-1000` copies the same number
//! of bytes on every machine that ever runs it; the same run takes a different
//! number of milliseconds on a busy laptop. Only one of those two is evidence
//! about a binding design, and it is the one that does not need a footnote
//! about thermal state.
//!
//! # The four numbers, and what each does not include
//!
//! Every column here has a boundary of its own, and reporting a number without
//! its boundary is how an instrumented evaluation ends up agreeing with its
//! author. Each field below says what it excludes.

use serde::Serialize;

/// Whether a number was observed or worked out on paper.
///
/// This exists so that arm B cannot be mistaken for a run. It is carried on
/// every arm's counts, serialised into the JSON, and printed in the summary
/// table, because a derived number that travels without its label eventually
/// gets quoted as a measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Derivation {
    /// Observed by running the operation in this process.
    Measured,
    /// Worked out by reading generated glue. Never timed, never compared
    /// against a measured time.
    StaticAnalysis,
}

impl Derivation {
    pub fn label(self) -> &'static str {
        match self {
            Derivation::Measured => "measured",
            Derivation::StaticAnalysis => "static analysis",
        }
    }
}

/// One row of the per-operation table: what a single named operation cost.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OpCounts {
    /// The host-side operation, named as the arm's binding names it.
    pub name: String,
    /// How many times it was invoked.
    pub calls: u64,
    /// Bytes copied out of the guest's memory by this operation.
    pub bytes_copied: u64,
}

/// Everything counted for one arm running one operation.
#[derive(Debug, Clone, Serialize)]
pub struct Counts {
    pub derivation: Derivation,

    /// Mutations issued against the document.
    ///
    /// The unit is a DOM operation as the engine's facade sees it: one
    /// `create_element`, one `append_child`, one attribute write. It counts
    /// what reached the document, not what the guest language called, so a
    /// guest helper that expands into three facade calls counts as three.
    pub dom_operations: u64,

    /// Transitions from guest code into host code.
    ///
    /// Equal to `dom_operations` for a binding where every operation is one
    /// call, and larger for one that needs extra crossings to set an
    /// operation up. Arm C's `intern` is the clearest case: it is a crossing
    /// that is not a DOM operation, and folding it into the operations that
    /// consume its atoms is exactly how this measurement would flatter itself.
    pub boundary_crossings: u64,

    /// Bytes read out of the guest's memory by the host.
    ///
    /// Excludes anything the host allocates internally while servicing the
    /// call, and excludes bytes travelling host-to-guest, of which these
    /// operations move none: all six of arm C's imports and all of arm A's
    /// binding arguments carry data inward only.
    pub bytes_copied: u64,

    /// Bytes that crossed for something other than learning a name.
    ///
    /// The steady-state figure. Once a page's vocabulary is interned, this is
    /// what a further mutation costs. For an arm with no interned tier it
    /// equals [`Self::bytes_copied`], and that equality is the finding rather
    /// than a missing feature of the report.
    pub bytes_copied_steady_state: u64,

    /// Allocations made by the whole process while the operation ran.
    ///
    /// Counted by a wrapping global allocator, so it includes the engine, the
    /// interpreter, and the binding layer alike. It is deliberately not the
    /// binding's own opinion of what it allocated.
    ///
    /// `blitz-wasm`'s ABI.md records why: an earlier revision of its own
    /// counter read zero while two `String`s were allocated per attribute
    /// write, because a counter that records only what its author chose to
    /// record will agree with its author. An allocator hook cannot be fooled
    /// that way. The cost is that it also catches allocation that has nothing
    /// to do with the boundary, which is why the binding's self-reported
    /// figure is kept beside it rather than replaced by it.
    pub process_allocations: u64,

    /// Bytes requested from the allocator over the same window.
    pub process_allocated_bytes: u64,

    /// What the binding layer itself claims to have allocated.
    ///
    /// `None` for arms that have no such counter. Where both are present the
    /// gap between this and [`Self::process_allocations`] is the interesting
    /// quantity: it is everything the binding cannot see.
    pub binding_reported_allocations: Option<u64>,

    /// Per-operation breakdown, in the arm's own vocabulary.
    pub operations: Vec<OpCounts>,

    /// Anything a reader needs in order not to over-read the numbers above.
    pub notes: Vec<String>,
}

impl Counts {
    pub fn new(derivation: Derivation) -> Self {
        Self {
            derivation,
            dom_operations: 0,
            boundary_crossings: 0,
            bytes_copied: 0,
            bytes_copied_steady_state: 0,
            process_allocations: 0,
            process_allocated_bytes: 0,
            binding_reported_allocations: None,
            operations: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }
}
