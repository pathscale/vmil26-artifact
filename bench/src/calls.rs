//! The workload as a sequence of DOM calls, independent of any arm.
//!
//! Two jobs.
//!
//! Arm B needs it because arm B does not run: there is nothing to instrument,
//! so the calls have to exist as data for the model to price.
//!
//! Arms A and C do run, and this sequence is what their measured operation
//! counts are checked against. That check is the reason this module is worth
//! having twice over. Arm A's counts come from Boa's binding instrumentation
//! and arm C's from `blitz-wasm`'s counters: two unrelated instruments, in two
//! unrelated crates, neither of which knows about this file. If both agree with
//! it, all three descriptions of the workload are consistent, and a mistake in
//! any one of them would have to be a mistake in the other two in exactly the
//! same way to go unnoticed.
//!
//! For the sweep, this list is *derived from* [`workload::scenario::Program`]
//! rather than written out again. The program is the single definition; this is
//! a translation of it into binding-neutral calls, so a change to the generator
//! moves the model and both cross-checks together.

use workload::scenario::{Config, Program, Step, TAG, class_value};

use crate::operation::{Operation, Workload};

/// One call against the document, in the vocabulary the DOM itself uses rather
/// than any particular binding's spelling of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    CreateElement(String),
    CreateText(String),
    AppendChild,
    SetAttribute(String, String),
    SetTextContent(String),
    /// Register a listener for an event name.
    AddListener(String),
    /// One delivery of an event into the guest.
    ///
    /// The one call that originates on the host: every other entry is the guest
    /// asking the host to do something, and this is the host asking the guest.
    Dispatch,
}

impl Call {
    /// The strings drawn from the page's fixed vocabulary, which an interning
    /// design sends once and a copying design sends every time.
    ///
    /// Text content is deliberately absent. It is content, drawn from an
    /// unbounded set, and interning it would grow a table without bound to save
    /// nothing. That absence is what makes the chattiness axis a clean test and
    /// what makes `partial-update` the case interning cannot win.
    pub fn vocabulary(&self) -> Vec<&str> {
        match self {
            Call::CreateElement(tag) => vec![tag.as_str()],
            Call::SetAttribute(name, value) => vec![name.as_str(), value.as_str()],
            // An event name comes from a small fixed set, so it interns like
            // any other. `blitz-wasm` puts `add_listener` in the interned tier
            // for exactly this reason.
            Call::AddListener(event) => vec![event.as_str()],
            Call::CreateText(_) | Call::AppendChild | Call::SetTextContent(_) | Call::Dispatch => {
                Vec::new()
            }
        }
    }

    /// The generated shim's name, for arm B's per-operation table.
    pub fn shim_name(&self) -> &'static str {
        match self {
            Call::CreateElement(_) => "Document::create_element",
            Call::CreateText(_) => "Document::create_text_node",
            Call::AppendChild => "Node::append_child",
            Call::SetAttribute(..) => "Element::set_attribute",
            Call::SetTextContent(_) => "Node::set_text_content",
            Call::AddListener(_) => "EventTarget::add_event_listener_with_callback",
            Call::Dispatch => "Closure::call (host to guest)",
        }
    }

    /// The label `blitz-script` records this binding under, for cross-checking
    /// arm A.
    pub fn arm_a_label(&self) -> &'static str {
        match self {
            Call::CreateElement(_) => "dom:createElement",
            Call::CreateText(_) => "dom:createTextNode",
            Call::AppendChild => "dom:appendChild",
            Call::SetAttribute(..) => "dom:attr=",
            Call::SetTextContent(_) => "dom:textContent=",
            Call::AddListener(_) => "dom:addEventListener",
            // `record_work` attributes a dispatch by event name.
            Call::Dispatch => "event:click",
        }
    }

    /// The `blitz-wasm` import this becomes, for cross-checking arm C.
    pub fn arm_c_import(&self) -> &'static str {
        match self {
            Call::CreateElement(_) => "create_element",
            Call::CreateText(_) => "create_text",
            Call::AppendChild => "append_child",
            Call::SetAttribute(..) => "set_attribute",
            Call::SetTextContent(_) => "set_text",
            Call::AddListener(_) => "add_listener",
            Call::Dispatch => "dispatch",
        }
    }
}

/// The event every listener in this benchmark waits for.
pub const EVENT: &str = "click";

/// The calls one upstream row makes, in order.
///
/// Transcribed from the row markup in the README. The order matters for the
/// interning model: the first row pays for the whole vocabulary and the other
/// 999 pay for none of it.
fn row_calls(id: u32, label: &str, into: &mut Vec<Call>) {
    let attr = |name: &str, value: &str| Call::SetAttribute(name.into(), value.into());
    let element = |tag: &str| Call::CreateElement(tag.into());

    into.push(element("tr"));

    // <td class='col-md-1'>{id}</td>
    into.push(element("td"));
    into.push(attr("class", "col-md-1"));
    into.push(Call::CreateText(id.to_string()));
    into.push(Call::AppendChild);
    into.push(Call::AppendChild);

    // <td class='col-md-4'><a>{label}</a></td>
    into.push(element("td"));
    into.push(attr("class", "col-md-4"));
    into.push(element("a"));
    into.push(Call::CreateText(label.to_string()));
    into.push(Call::AppendChild);
    into.push(Call::AppendChild);
    into.push(Call::AppendChild);

    // <td class='col-md-1'><a><span class='glyphicon glyphicon-remove'
    //     aria-hidden='true'></span></a></td>
    into.push(element("td"));
    into.push(attr("class", "col-md-1"));
    into.push(element("a"));
    into.push(element("span"));
    into.push(attr("class", "glyphicon glyphicon-remove"));
    into.push(attr("aria-hidden", "true"));
    into.push(Call::AppendChild);
    into.push(Call::AppendChild);
    into.push(Call::AppendChild);

    // <td class='col-md-6'></td>
    into.push(element("td"));
    into.push(attr("class", "col-md-6"));
    into.push(Call::AppendChild);

    // The row into the tbody.
    into.push(Call::AppendChild);
}

/// Every call the workload issues, in order.
///
/// Setup is excluded throughout: `create-1000` starts from an empty table, so
/// building the table is not part of it; `partial-update` starts from a
/// populated one, so building those 10,000 rows is not part of it either; and a
/// sweep point's `sweep_prepare` expands the program without touching the
/// document.
pub fn call_sequence(workload: Workload) -> Vec<Call> {
    match workload {
        Workload::Upstream(Operation::Create1000) => {
            let mut calls = Vec::new();
            let mut store = workload::Store::new();
            for _ in 0..workload::op::CREATE_ROWS {
                let row = store.next_row();
                row_calls(row.id, &row.label, &mut calls);
            }
            calls
        }
        Workload::Upstream(Operation::PartialUpdate) => {
            // The labels the update writes are the originals with the suffix
            // appended, so the byte count has to come from the real rows rather
            // than from an assumed average length.
            let mut store = workload::Store::new();
            let rows = store.build(workload::op::UPDATE_TABLE_ROWS);
            rows.iter()
                .step_by(workload::UPDATE_STRIDE)
                .map(|row| {
                    Call::SetTextContent(format!("{}{}", row.label, workload::UPDATE_SUFFIX))
                })
                .collect()
        }
        Workload::Sweep(config) => sweep_calls(config),
    }
}

/// A sweep point's calls, translated from the generated program.
fn sweep_calls(config: Config) -> Vec<Call> {
    let program = Program::build(config);
    let vocabulary = program.vocabulary;
    let mut calls: Vec<Call> = program
        .steps
        .into_iter()
        .map(|step| match step {
            Step::Create => Call::CreateElement(TAG.into()),
            Step::SetClass(index) => Call::SetAttribute(
                "class".into(),
                vocabulary
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| class_value(index, config.names)),
            ),
            // The empty text node every element gets. Zero bytes, and it is
            // what keeps the text phase from destroying the tree.
            Step::CreateText => Call::CreateText(String::new()),
            Step::AppendTextTo { .. } | Step::Append { .. } => Call::AppendChild,
            Step::SetText { text, .. } => Call::SetTextContent(text),
            Step::AddListener { .. } => Call::AddListener(EVENT.into()),
        })
        .collect();
    calls.extend(std::iter::repeat_n(Call::Dispatch, config.dispatches));
    calls
}

/// How many times each `blitz-script` label should appear, for arm A.
pub fn expected_arm_a_calls(workload: Workload) -> Vec<(&'static str, u64)> {
    tally(call_sequence(workload).iter().map(Call::arm_a_label))
}

/// How many times each `blitz-wasm` import should appear, for arm C.
///
/// Excludes `intern`, which is not in this sequence at all: interning is a
/// property of one binding's ABI, not of the workload, and putting it here
/// would be describing the workload in arm C's terms.
pub fn expected_arm_c_calls(workload: Workload) -> Vec<(&'static str, u64)> {
    tally(call_sequence(workload).iter().map(Call::arm_c_import))
}

fn tally<'a>(labels: impl Iterator<Item = &'a str>) -> Vec<(&'a str, u64)> {
    let mut counts: Vec<(&str, u64)> = Vec::new();
    for label in labels {
        match counts.iter_mut().find(|(name, _)| *name == label) {
            Some(entry) => entry.1 += 1,
            None => counts.push((label, 1)),
        }
    }
    counts.sort_by(|a, b| a.0.cmp(b.0));
    counts
}

/// Distinct vocabulary strings, and their total length in bytes.
///
/// **The two numbers the crossover rests on.** Interning trades one crossing
/// per distinct name for the bytes every later use would have copied, so the
/// count is what it costs and the bytes are what it saves.
pub fn vocabulary_stats(workload: Workload) -> (usize, usize, usize) {
    let mut seen: Vec<String> = Vec::new();
    let mut bytes = 0;
    let mut uses = 0;
    for call in call_sequence(workload) {
        for text in call.vocabulary() {
            uses += 1;
            if !seen.iter().any(|entry| entry == text) {
                bytes += text.len();
                seen.push(text.to_string());
            }
        }
    }
    (seen.len(), bytes, uses)
}

/// The number of distinct vocabulary strings the workload uses.
pub fn vocabulary_size(workload: Workload) -> usize {
    vocabulary_stats(workload).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use workload::scenario::{NameLength, Vocabulary};

    fn upstream(operation: Operation) -> Workload {
        Workload::Upstream(operation)
    }

    /// 26 calls per row, worked out by hand from the markup: 8 elements, 2 text
    /// nodes, 6 attributes, 10 appends. If this changes, the row changed, and
    /// every number in the upstream table changed with it.
    #[test]
    fn a_row_is_twenty_six_calls() {
        let mut calls = Vec::new();
        row_calls(1, "pretty red table", &mut calls);
        assert_eq!(calls.len(), 26);
        let count = |f: fn(&Call) -> bool| calls.iter().filter(|c| f(c)).count();
        assert_eq!(count(|c| matches!(c, Call::CreateElement(_))), 8);
        assert_eq!(count(|c| matches!(c, Call::AppendChild)), 10);
        assert_eq!(count(|c| matches!(c, Call::SetAttribute(..))), 6);
        assert_eq!(count(|c| matches!(c, Call::CreateText(_))), 2);
    }

    #[test]
    fn create_1000_is_twenty_six_thousand_calls() {
        assert_eq!(call_sequence(upstream(Operation::Create1000)).len(), 26_000);
    }

    #[test]
    fn partial_update_is_one_thousand_text_writes() {
        let calls = call_sequence(upstream(Operation::PartialUpdate));
        assert_eq!(calls.len(), 1_000);
        assert!(calls.iter().all(|c| matches!(c, Call::SetTextContent(_))));
    }

    /// Eleven strings serve twenty-six thousand calls. This is the number the
    /// interning argument rests on, so it is asserted rather than described.
    #[test]
    fn the_whole_page_vocabulary_is_eleven_strings() {
        assert_eq!(vocabulary_size(upstream(Operation::Create1000)), 11);
    }

    /// And none at all in the update, which is why it is the other half of the
    /// pair: there is nothing for an interning design to win here.
    #[test]
    fn the_update_has_no_vocabulary_to_intern() {
        assert_eq!(vocabulary_size(upstream(Operation::PartialUpdate)), 0);
    }

    /// The adversarial point: one distinct name per element means interning
    /// pays for every name and never earns one back.
    #[test]
    fn the_unique_vocabulary_point_has_no_reuse_to_win() {
        let config = Config {
            vocabulary: Vocabulary::UniquePerElement,
            ..Config::baseline()
        };
        let (distinct, _, uses) = vocabulary_stats(Workload::Sweep(config));
        // Every class value distinct, plus `div` and `class` reused throughout.
        assert_eq!(distinct, Config::ELEMENTS + 2);
        assert_eq!(uses, Config::ELEMENTS * 3);
    }

    /// Long and distinct together, which is the corner varying one axis at a
    /// time never visits.
    #[test]
    fn the_worst_corner_is_reachable_and_is_actually_worst() {
        let short = Config {
            vocabulary: Vocabulary::UniquePerElement,
            names: NameLength::Short,
            ..Config::baseline()
        };
        let long = Config {
            names: NameLength::Long,
            ..short
        };
        let (_, short_bytes, _) = vocabulary_stats(Workload::Sweep(short));
        let (_, long_bytes, _) = vocabulary_stats(Workload::Sweep(long));
        assert!(
            long_bytes > short_bytes * 8,
            "long {long_bytes} vs short {short_bytes}"
        );
    }

    #[test]
    fn dispatches_appear_in_the_call_sequence() {
        let config = Config {
            listeners: 50,
            dispatches: 200,
            ..Config::baseline()
        };
        let calls = call_sequence(Workload::Sweep(config));
        assert_eq!(
            calls.iter().filter(|c| matches!(c, Call::Dispatch)).count(),
            200
        );
        assert_eq!(
            calls
                .iter()
                .filter(|c| matches!(c, Call::AddListener(_)))
                .count(),
            50
        );
    }
}
