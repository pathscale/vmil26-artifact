//! Arm C: Rust compiled to wasm, through the native host binding.
//!
//! The guest is instantiated by wasmi and reaches the document through the six
//! `blitz` imports described in `ps-blitz/packages/blitz-wasm/ABI.md`. No
//! JavaScript is involved at any point in this path, and no glue is generated:
//! the imports are hand-written host functions over the runtime-agnostic DOM
//! facade, which is the same facade the document underneath arm A will
//! eventually be reached through.
//!
//! # How arm C is counted
//!
//! `blitz_wasm::Counters` already records calls, bytes read out of guest linear
//! memory, and the allocations that crate makes, per import. Those are the
//! binding's own figures and this harness does not second-guess them; it
//! reports them, and reports the global allocator's figure beside them so the
//! gap between the two is visible.
//!
//! # Interning, and why it gets its own column
//!
//! Five of the six imports carry only handles and atoms, so they copy nothing.
//! The sixth, `intern`, copies a name once per distinct string for the life of
//! the instance. Reporting the five as free without reporting what `intern`
//! cost would be a set of true numbers telling a false story, so the total and
//! the total-excluding-interning are both carried through to the report.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use blitz_dom::{BaseDocument, DocumentConfig, NodeId, qual_name};
use blitz_traits::shell::{ColorScheme, Viewport};
use blitz_wasm::{Dispatched, Host};
use wasmi::{Engine, Instance, Linker, Module, Store};

use crate::alloc;
use crate::counts::{Counts, Derivation, OpCounts};
use crate::dispatch;
use crate::operation::{Operation, Workload};

/// Build the guest once per process and hand out copies of the bytes.
///
/// Building it per iteration would put a `cargo` invocation inside the timing
/// loop, and building it per arm construction would do it twenty-four times
/// for no reason.
fn guest_module() -> &'static [u8] {
    static GUEST: OnceLock<Vec<u8>> = OnceLock::new();
    GUEST.get_or_init(build_guest).as_slice()
}

fn build_guest() -> Vec<u8> {
    let guest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("guest");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let output = Command::new(&cargo)
        .current_dir(&guest_dir)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--package",
            "dom-boundary-bench-guest",
        ])
        .output()
        .unwrap_or_else(|err| panic!("could not run `{cargo} build` for the guest: {err}"));

    assert!(
        output.status.success(),
        "the guest failed to build.\n\
         If this says the wasm32-unknown-unknown target is missing, install it \
         with `rustup target add wasm32-unknown-unknown`; this harness will not \
         install it for you.\n\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wasm =
        guest_dir.join("target/wasm32-unknown-unknown/release/dom_boundary_bench_guest.wasm");
    std::fs::read(&wasm).unwrap_or_else(|err| panic!("no module at {}: {err}", wasm.display()))
}

/// `<html><head></head><body></body></html>`, and the body to mount on.
///
/// The empty `<head>` is not decoration. Arm A gets its document from the HTML
/// parser, which inserts one whether the source says so or not, so a hand-built
/// document without it differs from arm A's in the first eight bytes and the
/// identical-tree check fails on a difference that has nothing to do with
/// either binding. Building the same shape here is what lets that check mean
/// something when it passes.
fn document_with_body() -> (BaseDocument, NodeId) {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(1024, 768, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let root_id = doc.root_node().id;

    let mut mutr = doc.mutate();
    let html = mutr.create_element(qual_name!("html"), vec![]);
    let head = mutr.create_element(qual_name!("head"), vec![]);
    let body = mutr.create_element(qual_name!("body"), vec![]);
    mutr.append_children(html, &[head, body]);
    mutr.append_children(root_id, &[html]);
    drop(mutr);

    (doc, body)
}

/// An instantiated guest.
pub struct ArmC {
    store: Store<Host>,
    instance: Instance,
    /// What the dispatch storm actually delivered.
    ///
    /// Accumulated rather than assumed. The dispatch target is a root-level
    /// element under every tree shape, so propagation should match exactly the
    /// one listener attached to it; if a shape change ever made that untrue,
    /// the event axis would silently be measuring a different amount of work
    /// per configuration, and this is what makes that visible.
    dispatched: Dispatched,
}

impl ArmC {
    pub fn new() -> Self {
        let (doc, body) = document_with_body();

        let engine = Engine::default();
        let module =
            Module::new(&engine, guest_module()).expect("the guest module should validate");
        let mut store = Store::new(&engine, Host::new(doc, body));
        let mut linker = <Linker<Host>>::new(&engine);
        blitz_wasm::add_to_linker(&mut linker).expect("host functions should register");

        // `instantiate_and_start` runs the module's static initialisers. A
        // failure there is exactly the class of problem a wasm32 *compile*
        // never surfaces.
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .expect("the guest should instantiate");

        Self {
            store,
            instance,
            dispatched: Dispatched::default(),
        }
    }

    /// Call a no-argument export.
    ///
    /// Two typed accessors rather than one uniform `fn(i32) -> i32` signature,
    /// because giving `prepare` an argument it ignores would put a parameter in
    /// the guest's ABI to satisfy the harness. wasmi checks the arity, so the
    /// mismatch is caught here rather than becoming a silently wrong call.
    fn call0(&mut self, name: &str) {
        let function = self
            .instance
            .get_typed_func::<(), i32>(&self.store, name)
            .unwrap_or_else(|err| panic!("the guest should export `{name}`: {err}"));
        let status = function
            .call(&mut self.store, ())
            .unwrap_or_else(|err| panic!("`{name}` trapped, which the ABI forbids: {err}"));
        self.check(name, status);
    }

    /// Call a one-argument export.
    fn call1(&mut self, name: &str, argument: i32) {
        let function = self
            .instance
            .get_typed_func::<i32, i32>(&self.store, name)
            .unwrap_or_else(|err| panic!("the guest should export `{name}`: {err}"));
        let status = function
            .call(&mut self.store, argument)
            .unwrap_or_else(|err| panic!("`{name}` trapped, which the ABI forbids: {err}"));
        self.check(name, status);
    }

    /// A negative return is a host status code, which the ABI uses in place of
    /// a trap so that a guest mistake is reportable rather than fatal. Here it
    /// is fatal anyway: a benchmark whose workload half-failed would report a
    /// smaller number and look like a win.
    fn check(&self, name: &str, status: i32) {
        assert_eq!(
            status,
            0,
            "`{name}` returned host status {status}{}",
            self.store
                .data()
                .counters()
                .last_dom_error
                .as_deref()
                .map(|detail| format!(" ({detail})"))
                .unwrap_or_default()
        );
    }

    pub fn create_rows(&mut self, count: usize) {
        self.call1("create_rows", count as i32);
    }

    pub fn update_every_tenth(&mut self) {
        self.call0("update_every_tenth");
    }

    /// The tree, serialised the same way arm A serialises its own.
    pub fn page_source(&self) -> String {
        self.store.data().document().root_element().outer_html()
    }

    /// Everything the workload needs in place before the measured window.
    ///
    /// For a sweep point this expands the program inside the guest and
    /// allocates every string it will send. That is guest-side work, it is not
    /// the boundary, and running it inside the clock would put a string
    /// formatter into a measurement of a binding.
    fn set_up(&mut self, workload: Workload) {
        match workload {
            Workload::Upstream(operation) => {
                self.call0("prepare");
                if operation == Operation::PartialUpdate {
                    self.create_rows(workload::op::UPDATE_TABLE_ROWS);
                }
            }
            Workload::Sweep(config) => {
                let ints = config.as_ints();
                let prepare = self
                    .instance
                    .get_typed_func::<(i32, i32, i32, i32, i32, i32, i32), i32>(
                        &self.store,
                        "sweep_prepare",
                    )
                    .expect("the guest should export `sweep_prepare`");
                let status = prepare
                    .call(
                        &mut self.store,
                        (
                            ints[0], ints[1], ints[2], ints[3], ints[4], ints[5], ints[6],
                        ),
                    )
                    .expect("`sweep_prepare` trapped, which the ABI forbids");
                self.check("sweep_prepare", status);
            }
        }
    }

    fn run(&mut self, workload: Workload) {
        match workload {
            Workload::Upstream(Operation::Create1000) => {
                self.create_rows(workload::op::CREATE_ROWS)
            }
            Workload::Upstream(Operation::PartialUpdate) => self.update_every_tenth(),
            Workload::Sweep(config) => {
                self.call0("sweep_build");
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
            self.call0("sweep_settle");
        }
    }

    /// Deliver one synthetic click through the engine's own event driver.
    ///
    /// The target is resolved by walking the document rather than through the
    /// handle table, so it is provably the same element arm A clicks. See
    /// [`crate::dispatch`].
    fn click(&mut self) {
        let event = {
            let doc = self.store.data().document();
            dispatch::first_body_element(doc).and_then(|target| dispatch::click(doc, target))
        };
        let Some(event) = event else { return };
        let result = blitz_wasm::dispatch_dom_event(&mut self.store, &self.instance, event)
            .expect("dispatch should not trap");
        self.dispatched.queued += result.queued;
        self.dispatched.ran += result.ran;
        self.dispatched.failed += result.failed;
    }
}

/// There is one way to build this arm, so `Default` is `new`. Present for the
/// lint, not because a second construction path is wanted.
impl Default for ArmC {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the workload and report what it cost.
pub fn measure(workload: Workload) -> (Counts, String) {
    let mut arm = ArmC::new();
    arm.set_up(workload);

    // The counters are cumulative from instantiation, so the workload's cost
    // is the difference across it. Subtracting is what keeps the 10,000-row
    // setup out of a `partial-update` figure.
    let before = arm.store.data().counters().clone();
    let allocations_before = alloc::Snapshot::take();

    arm.run(workload);

    let allocated = allocations_before.since();
    let after = arm.store.data().counters().clone();

    let operations = vec![
        delta("intern", &before.intern, &after.intern),
        delta(
            "create_element",
            &before.create_element,
            &after.create_element,
        ),
        delta("create_text", &before.create_text, &after.create_text),
        delta("append_child", &before.append_child, &after.append_child),
        delta("set_attribute", &before.set_attribute, &after.set_attribute),
        delta("set_text", &before.set_text, &after.set_text),
        delta("add_listener", &before.add_listener, &after.add_listener),
        // The one outbound counter: the host calling into the guest. Its
        // `bytes_copied` is structurally zero, which is the claim the event
        // axis exists to test.
        delta("dispatch", &before.dispatch, &after.dispatch),
    ];

    let crossings: u64 = operations.iter().map(|op| op.calls).sum();
    // `intern` is a boundary crossing but not a DOM operation: it puts a name
    // in the host's table and touches no node. Counting it as a DOM operation
    // would make arm C look like it did more work on the document than arm A
    // for an identical tree.
    let interns = operations
        .iter()
        .find(|op| op.name == "intern")
        .map_or(0, |op| op.calls);
    let intern_bytes = operations
        .iter()
        .find(|op| op.name == "intern")
        .map_or(0, |op| op.bytes_copied);

    let bytes: u64 = operations.iter().map(|op| op.bytes_copied).sum();

    let binding_allocs: u64 = [
        after.intern.host_allocs - before.intern.host_allocs,
        after.create_element.host_allocs - before.create_element.host_allocs,
        after.create_text.host_allocs - before.create_text.host_allocs,
        after.append_child.host_allocs - before.append_child.host_allocs,
        after.set_attribute.host_allocs - before.set_attribute.host_allocs,
        after.set_text.host_allocs - before.set_text.host_allocs,
        after.add_listener.host_allocs - before.add_listener.host_allocs,
        after.dispatch.host_allocs - before.dispatch.host_allocs,
    ]
    .iter()
    .sum();

    let mut counts = Counts::new(Derivation::Measured);
    counts.dom_operations = crossings - interns;
    counts.boundary_crossings = crossings;
    counts.bytes_copied = bytes;
    counts.bytes_copied_steady_state = bytes - intern_bytes;
    counts.process_allocations = allocated.allocations;
    counts.process_allocated_bytes = allocated.allocated_bytes;
    counts.binding_reported_allocations = Some(binding_allocs);
    counts.operations = operations;

    counts.note(format!(
        "{interns} of the {crossings} crossings were `intern`, carrying \
         {intern_bytes} bytes. They are counted as crossings but not as DOM \
         operations: interning names a string, it does not touch a node."
    ));
    counts.note(
        "`bytes_copied_steady_state` excludes interning. It is what a further \
         mutation costs once the page's vocabulary is known, which is the state \
         a running application is in."
            .to_string(),
    );
    counts.note(format!(
        "The binding reports {binding_allocs} allocations; the allocator saw \
         {}. The difference is everything `blitz-wasm` cannot see from where it \
         sits, including wasmi's own work and the `String`s the DOM facade \
         allocates internally.",
        allocated.allocations
    ));
    if arm.dispatched.queued > 0 {
        counts.note(format!(
            "The storm queued {} listeners and ran {} of them ({} failed), \
             carrying zero bytes. Reported rather than assumed: the target is a \
             root-level element under every tree shape, so one click should \
             match exactly one listener.",
            arm.dispatched.queued, arm.dispatched.ran, arm.dispatched.failed
        ));
    }

    arm.settle(workload);
    (counts, arm.page_source())
}

/// One timed iteration.
///
/// The guest module is built and the instance created before the clock starts,
/// so a sample is the workload and not wasmi's instantiation, and a sweep
/// point's program is expanded before it too.
pub fn time_once(workload: Workload) -> Duration {
    let mut arm = ArmC::new();
    arm.set_up(workload);

    let started = Instant::now();
    arm.run(workload);
    started.elapsed()
}

fn delta(name: &str, before: &blitz_wasm::OpCounters, after: &blitz_wasm::OpCounters) -> OpCounts {
    OpCounts {
        name: name.to_string(),
        calls: after.calls - before.calls,
        bytes_copied: after.bytes_copied - before.bytes_copied,
    }
}
