//! The report: the JSON the paper's table is generated from.
//!
//! Generated rather than transcribed, because a table transcribed by hand is a
//! table that disagrees with the run six weeks later and nobody notices. The
//! summary printed to the terminal is derived from this same structure, so the
//! two cannot drift from each other either.

use serde::Serialize;

use crate::counts::Counts;
use crate::machine::Machine;
use crate::timing::Samples;

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub workload: Workload,
    pub machine: Machine,
    /// **The headline.** The repetition ratio above which interning pays, the
    /// worst configuration for the native binding, and the margin there.
    pub crossover: crate::crossover::Crossover,
    /// The parameterised sweep: one entry per configuration.
    pub sweep: Vec<SweepEntry>,
    /// The two upstream operations, kept as the cited baseline.
    pub operations: Vec<OperationReport>,
    /// Axes that are named, understood, and cannot be run yet. Documented
    /// rather than omitted: an unmeasured axis that is written down is a
    /// finding, and one that is silently absent is a hole.
    pub unmeasured_axes: Vec<UnmeasuredAxis>,
    /// Read this before reading the numbers.
    pub reading_notes: Vec<&'static str>,
}

/// One configuration of the sweep.
#[derive(Debug, Clone, Serialize)]
pub struct SweepEntry {
    pub id: String,
    pub axis: &'static str,
    pub rationale: &'static str,
    /// The raw knob settings, kept so the crossover analysis can find pairs
    /// that differ in exactly one axis without parsing the id.
    #[serde(skip)]
    pub config: workload::scenario::Config,
    pub configuration: ConfigSummary,
    #[serde(flatten)]
    pub report: OperationReport,
}

/// The knob settings, spelled out so the matrix can be read without decoding
/// the id.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigSummary {
    pub vocabulary: String,
    pub name_length: &'static str,
    pub chattiness: &'static str,
    pub tree_shape: &'static str,
    pub elements: usize,
    pub listeners: usize,
    pub dispatches: usize,
    /// Measured while generating, not asserted.
    pub achieved_max_depth: usize,
    pub achieved_leaves: usize,
    pub achieved_mean_fanout: f64,
}

/// An axis the design cannot currently be tested on.
#[derive(Debug, Clone, Serialize)]
pub struct UnmeasuredAxis {
    pub axis: &'static str,
    pub why_it_matters: &'static str,
    pub what_blocks_it: &'static str,
    /// The operations the ABI would need before this axis could be swept.
    pub operations_required: Vec<&'static str>,
    pub expected_finding: &'static str,
}

/// The two axes that are named and cannot be run.
///
/// Both are places the handle-and-atom design is most likely to be *worse*
/// than a glue path, which is exactly why they are written down rather than
/// left out. A reviewer will find them; better that they find them here.
pub fn unmeasured_axes() -> Vec<UnmeasuredAxis> {
    vec![
        UnmeasuredAxis {
            axis: "read-heavy workloads",
            why_it_matters: "This is the most likely weakness in the whole design. Every write in the \
                 ABI moves handles and atoms and copies nothing, but a read has to get \
                 data back into guest memory, and the facade's readers return owned \
                 `String`s. A read is therefore expected to cost two copies: one into the \
                 host's `String` and one into the guest's linear memory. A page that reads \
                 as much as it writes could erase the entire interning advantage, and \
                 nothing here would show it.",
            what_blocks_it: "The ABI has no read operations at all. There is nothing to call, so there \
                 is nothing to count, and a model of a read would be a guess about an \
                 implementation nobody has written.",
            operations_required: vec![
                "get_attribute(node, name_atom) -> atom or string",
                "text_content(node) -> string into guest memory",
                "child_count(node) / child_at(node, index) -> handle",
                "get_bounding_client_rect(node) -> geometry (also needs a layout flush, \
                 which the facade does not do)",
            ],
            expected_finding: "Two copies per read against wasm-bindgen's one, which would make reads \
                 the axis where the native binding loses outright. An atom-returning \
                 `get_attribute` would avoid it for the interned tier; text and geometry \
                 could not.",
        },
        UnmeasuredAxis {
            axis: "structural churn",
            why_it_matters: "Handles are never reused and never freed, by design: a detached node \
                 stays addressable, so a handle to it stays meaningful. That is the right \
                 trade for a page that grows and the wrong one for a list that churns. A \
                 create/destroy/reorder cycle would grow the handle table without bound, \
                 and nothing currently measures that.",
            what_blocks_it: "There is no `remove` and no `insert_before`, so a create/destroy/reorder \
                 cycle cannot be expressed. `append_child` detaches from the previous \
                 parent, which permits a move but not a removal and not an ordered \
                 insertion.",
            operations_required: vec![
                "remove_child(parent, child)",
                "insert_before(parent, child, reference)",
                "a generational handle, so a freed slot can be reused without a stale \
                 handle silently addressing a new node",
            ],
            expected_finding: "Handle-table growth proportional to total nodes ever created rather than \
                 nodes alive, and a swap-rows workload that costs a long-lived page more \
                 the longer it runs. wasm-bindgen's heap slots are freed on drop, so it \
                 has no equivalent leak.",
        },
    ]
}

/// What was built, and where the specification came from.
#[derive(Debug, Clone, Serialize)]
pub struct Workload {
    pub source: &'static str,
    pub source_licence: &'static str,
    pub row_markup: &'static str,
    pub adjectives: usize,
    pub colours: usize,
    pub nouns: usize,
    pub seed: u32,
    pub generator: &'static str,
    pub determinism_note: &'static str,
}

impl Workload {
    pub fn describe() -> Self {
        Self {
            source: "krausest/js-framework-benchmark, frameworks/keyed/vanillajs/src/Main.js",
            source_licence: "Apache-2.0",
            row_markup: "<tr><td class='col-md-1'>{id}</td><td class='col-md-4'><a>{label}</a>\
                         </td><td class='col-md-1'><a><span class='glyphicon glyphicon-remove' \
                         aria-hidden='true'></span></a></td><td class='col-md-6'></td></tr>",
            adjectives: workload::ADJECTIVES.len(),
            colours: workload::COLOURS.len(),
            nouns: workload::NOUNS.len(),
            seed: workload::SEED,
            generator: "xorshift32",
            determinism_note: "Upstream draws labels with Math.random(). This harness uses a \
                               fixed-seed xorshift32 instead, and every arm consumes the \
                               identical sequence, so all arms build a byte-identical tree. \
                               Without that the arms would not be comparable at all: different \
                               text is a different number of bytes across the boundary. \
                               xorshift32 specifically, because it is 32-bit and JavaScript can \
                               therefore execute it exactly. Upstream's biased `% max` fold is \
                               kept so the label distribution still matches upstream's.",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationReport {
    pub name: String,
    pub description: String,
    /// Distinct tag and attribute strings the workload uses. **The denominator
    /// of the repetition ratio**, and what an interned tier pays for.
    pub vocabulary_strings: usize,
    /// Their total length in bytes: what interning copies, once.
    pub vocabulary_bytes: usize,
    /// Total uses of those strings. **The numerator.**
    pub vocabulary_uses: usize,
    /// `vocabulary_uses / vocabulary_strings`.
    pub repetition_ratio: f64,
    /// True when every arm that ran produced the same tree.
    pub trees_identical: bool,
    pub arms: Vec<ArmReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArmReport {
    /// "A", "B", or "C".
    pub arm: &'static str,
    pub description: &'static str,
    pub counts: Counts,
    /// `None` for arm B, always. A derived arm has no time and must never be
    /// given one.
    pub timing: Option<Samples>,
}

pub const READING_NOTES: [&str; 10] = [
    "The point of the sweep is to find where the native binding LOSES. If it appears to win \
     everywhere, treat that as evidence the axes are too narrow or the benchmark is broken, \
     not that the ABI is perfect.",
    "`crossover.worst_for_native` is reported as prominently as the best case, on purpose. \
     Read it first.",
    "The crossover ratio R is machine-local, because it depends on an exchange rate between a \
     crossing and a byte that has to be measured. `crossover.machine_independent` states the \
     half of the trade that needs no exchange rate and holds everywhere.",
    "Two axes are named in `unmeasured_axes` and cannot be run: read-heavy workloads and \
     structural churn. Both are places this design is most likely to lose. They are absent \
     from the numbers, not from the analysis.",
    "Counts are the result. They are properties of the interface and are identical on every \
     machine that runs this workload.",
    "Timings are secondary, are valid only against each other within this one run on this one \
     machine, and must never be compared across machines or sessions.",
    "Arm B was not run. Every arm-B number is statically derived from web-sys signatures and \
     wasm-bindgen's ABI, is marked `\"derivation\": \"static_analysis\"`, and has no timing.",
    "Arm B is reported in two configurations because wasm-bindgen's string interning is a \
     non-default cargo feature. Comparing only against the default would be comparing against \
     interning switched off.",
    "`bytes_copied_steady_state` excludes the cost of learning a page's vocabulary. Read it \
     together with `bytes_copied`: the first is what a running application pays per mutation, \
     the second is what the first paint pays including the learning.",
    "`process_allocations` is taken from a global allocator below every crate involved, so it \
     includes the language runtime, not only the binding. Where an arm also reports \
     `binding_reported_allocations`, the gap between them is what that binding cannot see.",
];
