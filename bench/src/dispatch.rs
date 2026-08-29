//! Finding the node a synthetic event targets.
//!
//! Both arms must click the *same* element or the event axis compares two
//! different dispatches. Rather than each arm resolving a target through its
//! own bookkeeping — a handle table on one side, nothing at all on the other —
//! both call this, which navigates the document itself.
//!
//! The target is the first element child of `<body>`, which is element 0 of the
//! generated program under every tree shape: `Deep` starts its chain there,
//! `Wide` makes it the first of the fan, and `Realistic` always attaches the
//! first element to the mount. Element 0 also always carries a listener when a
//! configuration asks for any, because the listener stride starts at zero.
//!
//! It is a root-level element under all three shapes, so it has no
//! listener-bearing ancestors and a click on it matches exactly one listener.
//! That keeps the queued count equal across configurations, so the event axis
//! measures dispatch throughput rather than how much propagation each tree
//! shape happens to produce. `Dispatched::queued` is reported anyway, so the
//! assumption is visible rather than trusted.

use blitz_dom::{BaseDocument, NodeId};
use blitz_traits::events::DomEvent;
// Through the re-export rather than a direct dependency, so this harness cannot
// end up resolving a different `keyboard_types` than the engine it is measuring.
use blitz_traits::keyboard_types::Modifiers;

/// The first element child of `<body>`, or `None` for an empty document.
pub fn first_body_element(doc: &BaseDocument) -> Option<NodeId> {
    let body = find_by_tag(doc, doc.root_node().id, "body")?;
    doc.get_node(body)?
        .children
        .iter()
        .copied()
        .find(|id| is_element(doc, *id))
}

fn is_element(doc: &BaseDocument, id: NodeId) -> bool {
    doc.get_node(id)
        .and_then(|node| node.element_data())
        .is_some()
}

fn find_by_tag(doc: &BaseDocument, from: NodeId, tag: &str) -> Option<NodeId> {
    let node = doc.get_node(from)?;
    if node
        .element_data()
        .is_some_and(|element| &*element.name.local == tag)
    {
        return Some(from);
    }
    node.children
        .iter()
        .find_map(|child| find_by_tag(doc, *child, tag))
}

/// A synthetic click on `target`.
pub fn click(doc: &BaseDocument, target: NodeId) -> Option<DomEvent> {
    Some(DomEvent::new(
        target,
        doc.get_node(target)?
            .synthetic_click_event(Modifiers::empty()),
    ))
}
