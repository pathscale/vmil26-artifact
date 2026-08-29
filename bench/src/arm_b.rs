//! Arm B: Rust/wasm through generated wasm-bindgen glue. **Statically derived.**
//!
//! Nothing in this module runs. Every number it produces is worked out from the
//! call sequence in [`crate::calls`] and the code-generation scheme wasm-bindgen
//! uses, and every one is stamped [`Derivation::StaticAnalysis`] so it cannot be
//! mistaken for an observation. Arm B is never timed and its numbers are never
//! compared against a measured time.
//!
//! # What counts as a crossing
//!
//! Stated here because the arms are only comparable if they all mean the same
//! thing by it, and one earlier version of this file did not.
//!
//! **A crossing is one call that leaves the caller's representation domain and
//! has to marshal its arguments to enter the callee's.** By that rule arm A's
//! JavaScript makes one crossing per DOM operation, which is what
//! `arm_a.rs` counts, and arm C's guest makes one, which is what `arm_c.rs`
//! counts. Arm B's guest makes *two*: wasm to the JavaScript shim, then the
//! shim to the DOM. The second one is the same call arm A makes; the first is
//! what the glue adds.
//!
//! The absolute figures move if a reader prefers a different rule. The ratio
//! does not: whatever is counted, the shim contributes exactly one more per
//! operation than either arm that has no shim, so [`Counts`] reports both and
//! the comparison is stated per operation.
//!
//! # Where the model comes from
//!
//! Read out of the crate sources and then checked against glue actually
//! emitted for the same five operations. The exact paths and versions are in
//! [`SOURCES`], and the four facts the model rests on are:
//!
//! 1. **The signatures**, from `web-sys`. They decide how many strings each call
//!    carries and whether it returns a JS object. `append_child` returning
//!    `Result<Node, JsValue>` is the load-bearing one and the easiest to
//!    overlook.
//! 2. **`&str` crosses as a pointer and length**, and the JS shim decodes it out
//!    of linear memory. So the Rust side does not copy and the JS side does,
//!    once per call, with no memory between calls.
//! 3. **A returned DOM object costs nothing.** This is the fact the model got
//!    wrong until it was checked. Objects cross as externrefs, so there is no
//!    heap-table slot to write and no `__wbindgen_object_drop_ref` to call when
//!    the guest drops one. See [`RECLAMATION`].
//! 4. **Interning exists but is off by default.** `enable-interning` is a
//!    non-default cargo feature and it only helps strings the guest has
//!    explicitly passed to `wasm_bindgen::intern`.
//!
//! # Why two configurations
//!
//! Point 4 is why this arm is reported twice. Modelling only the default would
//! compare arm C's interned ABI against a wasm-bindgen that had its interning
//! switched off, which is a strawman, and a reviewer would say so. The
//! interned configuration is the fair comparison and it is the one the summary
//! leads with when it talks about arm B's best case.
//!
//! # What this arm is not
//!
//! It is not a measurement of a browser. It models the glue, not the DOM
//! implementation underneath it, so it says what the *binding* costs and says
//! nothing about what Blink or Gecko then do. The crossing counts and the
//! shim bodies are confirmed against emitted glue; the interned byte figure is
//! not, because the probe was built without that feature. [`VERIFICATION`] says
//! exactly which half is which, in the report itself.

use crate::calls::{Call, call_sequence};
use crate::counts::{Counts, Derivation, OpCounts};
use crate::operation::Workload;

/// The files the model was read out of, reported so a reader can check it.
pub const SOURCES: [&str; 5] = [
    "web-sys-0.3.104/src/features/gen_Document.rs (create_element, create_text_node)",
    "web-sys-0.3.104/src/features/gen_Node.rs (append_child, set_text_content)",
    "web-sys-0.3.104/src/features/gen_Element.rs (set_attribute)",
    "wasm-bindgen-0.2.127/src/{lib.rs,convert/slices.rs,cache/intern.rs}",
    "glue emitted by wasm-bindgen-cli 0.2.127 --target web for a probe crate \
     calling the same five operations",
];

/// How much of this arm has been checked against glue a reader can regenerate,
/// and how much is still read off the sources.
///
/// Kept as a single string because it is reported verbatim: an arm that is
/// derived rather than measured has to carry its own warrant.
pub const VERIFICATION: &str = "Confirmed against emitted glue \
     (wasm-bindgen-cli 0.2.127, web-sys 0.3.104, --target web, a probe crate \
     issuing the same five operations): each shim decodes its string arguments \
     with getStringFromWasm0 and performs exactly one DOM interaction, and no \
     object-reclamation intrinsic is emitted. Not confirmed: the interned \
     configuration, because the probe was built without `enable-interning`. \
     Its byte figure is still read from cache/intern.rs, and it is the figure \
     that favours wasm-bindgen.";

/// Why a returned DOM object is free, recorded because the model asserted the
/// opposite for long enough to reach a draft.
///
/// wasm-bindgen used to keep JS values in a side table: a returned object was a
/// slot write, and dropping the `JsValue` called `__wbindgen_object_drop_ref`,
/// a second crossing carrying no data. Under that ABI `create-1000` paid 20,000
/// reclamation crossings, one per element and one per node `append_child`
/// returns and the caller discards.
///
/// That ABI is gone. With reference types the object is an externref returned
/// on the stack, and the emitted glue has no `addHeapObject`, no `takeObject`,
/// no `getObject` and no `__wbindgen_object_drop_ref`. The externref table is
/// grown once at init for the four sentinel values and touched afterwards only
/// by `handleError` on the throwing path, which this workload does not take.
///
/// The correction removes 20,000 crossings from arm B. Making the counting
/// convention consistent then adds 26,000. Both directions were wrong at once,
/// which is why the old total looked plausible.
pub const RECLAMATION: &str = "Returned DOM objects cross as externrefs and cost \
     no crossing to release: emitted glue contains no `__wbindgen_object_drop_ref`, \
     `addHeapObject`, `takeObject` or `getObject`. An earlier version of this \
     model charged 20,000 reclamation crossings on `create-1000`, which is the \
     pre-reference-types ABI and no longer what the toolchain generates.";

/// Whether the modelled guest opted into wasm-bindgen's string cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interning {
    /// The default. `enable-interning` off, every string copied on every call.
    Off,
    /// `enable-interning` on and every name in the workload's fixed vocabulary
    /// passed to `wasm_bindgen::intern` at startup. The best case wasm-bindgen
    /// can reach without changing its ABI.
    On,
}

impl Interning {
    pub fn label(self) -> &'static str {
        match self {
            Interning::Off => "wasm-bindgen, default (interning off)",
            Interning::On => "wasm-bindgen, enable-interning + explicit intern()",
        }
    }
}

/// What one modelled call costs, before any vocabulary charge.
struct Cost {
    /// Transitions between the guest module and the JavaScript shim. One per
    /// call, in whichever direction the call runs.
    shim_crossings: u64,
    /// Transitions the shim then makes between JavaScript and the DOM. Held
    /// separately rather than assumed equal to the field above, so that a shim
    /// which touched the DOM twice, or not at all, could say so.
    dom_crossings: u64,
    /// UTF-8 bytes of *content*: text that is not drawn from the page's fixed
    /// vocabulary and therefore copies under every configuration.
    content_bytes: u64,
}

/// The model, one arm of a match per generated shim.
///
/// Vocabulary bytes are deliberately *not* charged here. Whether a tag or
/// attribute name costs anything depends on whether that particular string has
/// been sent before, which is a property of the run and not of the call, and
/// the caller owns that knowledge.
///
/// An earlier version did charge them here, taking a `first_use` flag for the
/// whole call. It over-counted `create-1000` by exactly 15 bytes, because
/// `set_attribute("class", "col-md-4")` on the first row has a new *value* and
/// an already-sent *name*, and one flag cannot say so. Splitting the two is not
/// tidiness; a per-call flag cannot express the thing being modelled.
fn cost(call: &Call) -> Cost {
    match call {
        // `arg0.createElement(getStringFromWasm0(arg1, arg2))`. Returns
        // `Result<Element, JsValue>`; the element comes back as an externref
        // and costs nothing to hold or to drop. See `RECLAMATION`.
        Call::CreateElement(_) => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: 0,
        },
        // `arg0.createTextNode(getStringFromWasm0(arg1, arg2))`. The content is
        // never vocabulary.
        Call::CreateText(text) => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: text.len() as u64,
        },
        // `arg0.appendChild(arg1)`, inside `handleError`. Both arguments are
        // externrefs, so the shim carries no bytes at all. It still returns a
        // `Node` the caller discards, which under the old heap-table ABI was a
        // second crossing and under this one is not.
        Call::AppendChild => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: 0,
        },
        // Two `getStringFromWasm0` calls, one DOM interaction.
        Call::SetAttribute(..) => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: 0,
        },
        // A property assignment rather than a method call, but still one
        // transition into the DOM. The text is content, so interning cannot
        // help it, which is the whole reason `partial-update` is here.
        Call::SetTextContent(text) => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: text.len() as u64,
        },
        // `add_event_listener_with_callback(&str, &Function)`. The event name
        // is vocabulary; the callback is a `Closure` the guest allocated once,
        // outlives the call, and is passed as an externref.
        Call::AddListener(_) => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: 0,
        },
        // The one call that goes the other way, and it is symmetric: the DOM
        // invokes the JavaScript trampoline, which calls the guest's exported
        // closure. Two transitions, no bytes, the event passed as an externref.
        Call::Dispatch => Cost {
            shim_crossings: 1,
            dom_crossings: 1,
            content_bytes: 0,
        },
    }
}

/// Work out what the operation costs through wasm-bindgen glue.
pub fn analyse(workload: Workload, interning: Interning) -> Counts {
    let calls = call_sequence(workload);

    let mut seen: Vec<&str> = Vec::new();
    let mut vocabulary_bytes: u64 = 0;

    let mut shim_crossings = 0u64;
    let mut dom_crossings = 0u64;
    let mut bytes = 0u64;
    let mut per_op: Vec<(&'static str, u64, u64)> = Vec::new();

    for call in &calls {
        let cost = cost(call);

        // Each vocabulary string is charged on its own terms. A string is new
        // the first time the *operation* sends it, tracked across the whole
        // operation rather than per call site, because wasm-bindgen's cache is
        // per module and does not care which call site reached it first.
        let mut vocabulary_charge = 0u64;
        for text in call.vocabulary() {
            let already_sent = seen.contains(&text);
            if !already_sent {
                seen.push(text);
                vocabulary_bytes += text.len() as u64;
            }
            vocabulary_charge += match interning {
                // No cache: the shim decodes it out of linear memory again.
                Interning::Off => text.len() as u64,
                // Cached: `unsafe_get_cached_str` returns
                // `WasmSlice { ptr: 0, len: idx }` and the shim reads the
                // cached object out of the heap table by index, copying
                // nothing. Only the call that first interned it paid.
                Interning::On if !already_sent => text.len() as u64,
                Interning::On => 0,
            };
        }

        shim_crossings += cost.shim_crossings;
        dom_crossings += cost.dom_crossings;
        bytes += cost.content_bytes + vocabulary_charge;

        let name = call.shim_name();
        let call_bytes = cost.content_bytes + vocabulary_charge;
        let call_crossings = cost.shim_crossings + cost.dom_crossings;
        match per_op.iter_mut().find(|(n, ..)| *n == name) {
            Some(entry) => {
                entry.1 += call_crossings;
                entry.2 += call_bytes;
            }
            None => per_op.push((name, call_crossings, call_bytes)),
        }
    }

    let mut operations: Vec<OpCounts> = per_op
        .into_iter()
        .map(|(name, calls, bytes_copied)| OpCounts {
            name: name.to_string(),
            calls,
            bytes_copied,
        })
        .collect();
    operations.sort_by(|a, b| a.name.cmp(&b.name));

    // One DOM operation per modelled call, which is what makes the arms
    // comparable: every arm builds the same tree by issuing the same sequence.
    // What differs is how many transitions each arm needs to issue one.
    let dom_operations = shim_crossings;

    let mut counts = Counts::new(Derivation::StaticAnalysis);
    counts.dom_operations = dom_operations;
    counts.boundary_crossings = shim_crossings + dom_crossings;
    counts.bytes_copied = bytes;
    counts.bytes_copied_steady_state = match interning {
        // Nothing is amortised, so there is no steady state to reach. The
        // equality is the finding.
        Interning::Off => bytes,
        Interning::On => bytes - vocabulary_bytes,
    };
    counts.process_allocations = 0;
    counts.process_allocated_bytes = 0;
    // There is no side table to report. Under the ABI this arm used to model
    // there was, and this field held its occupancy; under externrefs the
    // honest answer is that the binding allocates nothing observable here.
    counts.binding_reported_allocations = None;
    counts.operations = operations;

    counts.note(VERIFICATION.to_string());
    counts.note(format!("Model read from: {}.", SOURCES.join("; ")));
    counts.note(format!(
        "{shim_crossings} guest-to-shim transitions and {dom_crossings} \
         shim-to-DOM transitions, two per operation. The second of each pair is \
         the call arm A makes directly, so the glue's contribution is the \
         first: {shim_crossings} transitions that exist only because a \
         JavaScript intermediary is in the path."
    ));
    counts.note(RECLAMATION.to_string());
    counts.note(
        "`process_allocations` is 0 because nothing ran, and \
         `binding_reported_allocations` is absent because with externrefs there \
         is no JS-side object table whose occupancy could be reported."
            .to_string(),
    );
    if interning == Interning::On {
        counts.note(format!(
            "Assumes the guest passes all {} vocabulary strings to \
             `wasm_bindgen::intern` at startup. Interning is opt-in twice over: \
             the `enable-interning` cargo feature is not default, and each \
             string must be named explicitly.",
            seen.len()
        ));
    } else {
        counts.note(
            "This is the out-of-the-box configuration. `enable-interning` is a \
             non-default feature, so a guest that does nothing special pays \
             these numbers."
                .to_string(),
        );
    }
    counts.note(
        "Bytes are UTF-8, to stay comparable with arms A and C. wasm-bindgen's \
         own documentation notes the copy also re-encodes UTF-8 to UTF-16, so \
         the bytes written on the JavaScript side are up to twice these."
            .to_string(),
    );

    counts
}
