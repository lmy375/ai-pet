//! AX tree walk → rendered text snapshot with stable element indices.
//!
//! The walk is deterministic (children in order, wrappers compressed the same
//! way every time), so the indices printed by one `state` run stay valid for
//! the next `click`/`type`/`scroll` run as long as the UI hasn't changed.

use std::collections::HashSet;

use accessibility::{AXUIElement, AXUIElementAttributes};
use core_foundation::base::TCFType;

use crate::ax::{element_actions, element_frame, element_role, element_text, frame_string};

/// Hard budget: matches what mature computer-use agents ship. Beyond it the
/// model gains noise, not signal.
const MAX_NODES: usize = 1200;
const MAX_DEPTH: usize = 64;
/// Per-node text cap so one huge value can't crowd out the rest of the tree.
const TEXT_LIMIT: usize = 200;

// CF element identity (core-foundation 0.10 doesn't wrap CFHash). AX trees
// can be cyclic — an AXApplication whose child is itself was observed on a
// hung Calculator — so every traversal needs a visited set.
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFHash(cf: *const core::ffi::c_void) -> isize;
}

fn element_hash(element: &AXUIElement) -> isize {
    unsafe { CFHash(element.as_concrete_TypeRef() as *const core::ffi::c_void) }
}

/// A walked tree: rendered lines plus the live element for each index, so
/// action calls (click/type/scroll) can act on exactly what was printed.
pub struct Snapshot {
    pub lines: Vec<String>,
    pub elements: Vec<AXUIElement>,
}

/// Walk `window` and render it. Indices are 0-based and match `elements`.
pub fn snapshot(window: &AXUIElement) -> Snapshot {
    let mut snap = Snapshot { lines: Vec::new(), elements: Vec::new() };
    let mut visited = HashSet::new();
    walk(window, 0, &mut snap, &mut visited);
    snap
}

fn walk(element: &AXUIElement, depth: usize, snap: &mut Snapshot, visited: &mut HashSet<isize>) {
    if depth >= MAX_DEPTH || snap.elements.len() >= MAX_NODES || !visited.insert(element_hash(element))
    {
        return;
    }

    let role = element_role(element);
    let text = truncate(&element_text(element));
    let actions = element_actions(element);
    let children: Vec<AXUIElement> = element
        .children()
        .map(|c| c.iter().map(|e| e.to_owned()).collect())
        .unwrap_or_default();

    // Transparent wrapper: a bare AXGroup/AXUnknown with no text, no actions,
    // and exactly one child carries no information — skip it and keep depth
    // honest for the child that actually matters.
    let transparent = matches!(role.as_str(), "AXGroup" | "AXUnknown")
        && text.is_empty()
        && actions.is_empty()
        && children.len() == 1;
    if transparent {
        walk(&children[0], depth, snap, visited);
        return;
    }

    let index = snap.elements.len();
    snap.elements.push(element.clone());
    let frame = element_frame(element).map(frame_string).unwrap_or_default();
    let actions = if actions.is_empty() {
        String::new()
    } else {
        format!(" actions=[{}]", actions.join(","))
    };
    snap.lines.push(format!(
        "{}[{}] {} \"{}\" {}{}",
        "  ".repeat(depth),
        index,
        role,
        text,
        frame,
        actions
    ));

    for child in &children {
        walk(child, depth + 1, snap, visited);
    }
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= TEXT_LIMIT {
        return text.to_string();
    }
    let cut: String = text.chars().take(TEXT_LIMIT).collect();
    format!("{cut}…")
}
