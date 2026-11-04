//! dom_ffi.rs — the Rust side of the document bridge.
//!
//! Every function here is called from the AOT-compiled TypeScript in
//! `src/js/dom_runtime.ts`, through the thin C wrappers in
//! `c_shim/hermes_shim.cpp` (which bind the document pointer so that it never
//! crosses the JSI boundary).
//!
//! The document lives here, in Rust, owned by `BlitzHostContext`. Mutations go
//! through Blitz's `DocumentMutator`, which flags Stylo/Taffy damage; the host
//! loop then re-resolves only the damaged subtree.

use blitz_dom::{LocalName, Namespace, NodeData, NodeId, QualName};
use blitz_html::HtmlDocument;
use std::ffi::{CStr, c_char, c_void};

/// Owns the Blitz document and the dirty flag the host loop polls.
pub struct BlitzHostContext {
    pub doc: HtmlDocument,
    /// Set by any JS mutation; cleared by the host after re-resolving layout.
    pub dirty: bool,
}

impl BlitzHostContext {
    pub fn new(doc: HtmlDocument) -> Self {
        Self { doc, dirty: false }
    }
}

#[inline]
unsafe fn as_ctx<'a>(ctx: *mut c_void) -> Option<&'a mut BlitzHostContext> {
    if ctx.is_null() {
        None
    } else {
        Some(unsafe { &mut *(ctx as *mut BlitzHostContext) })
    }
}

#[inline]
unsafe fn as_str<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(p) }.to_str().unwrap_or("")
}

/// Resolve an element by HTML `id`. Returns the packed `NodeId`, or 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_find_element(ctx: *mut c_void, id: *const c_char) -> u64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    let id = unsafe { as_str(id) };
    if id.is_empty() {
        return 0;
    }
    ctx.doc
        .get_element_by_id(id)
        .map(|node| node.as_u64())
        .unwrap_or(0)
}

/// `node.style.setProperty(name, value)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_style(
    ctx: *mut c_void,
    node: u64,
    name: *const c_char,
    value: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let (name, value) = unsafe { (as_str(name), as_str(value)) };
    let node = NodeId::from_u64(node);
    {
        let mut mutator = ctx.doc.mutate();
        mutator.set_style_property(node, name, value);
    }
    ctx.dirty = true;
}

/// `node.innerText = text`
///
/// Blitz's `DocumentMutator::set_node_text` only accepts *text* nodes; on an
/// element it returns early (upstream TODO for `textContent`). `innerText` on
/// an element has to reach the element's text child, so resolve it here and
/// create one if the element has none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_text(ctx: *mut c_void, node: u64, text: *const c_char) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let text = unsafe { as_str(text) };
    let node = NodeId::from_u64(node);

    let node_is_text = matches!(
        ctx.doc.get_node(node).map(|n| &n.data),
        Some(NodeData::Text(_))
    );
    let existing_text_child = ctx.doc.get_node(node).and_then(|n| {
        n.children.iter().copied().find(|child| {
            matches!(
                ctx.doc.get_node(*child).map(|c| &c.data),
                Some(NodeData::Text(_))
            )
        })
    });

    {
        let mut mutator = ctx.doc.mutate();
        if node_is_text {
            mutator.set_node_text(node, text);
        } else if let Some(text_node) = existing_text_child {
            mutator.set_node_text(text_node, text);
        } else {
            let created = mutator.create_text_node(text);
            mutator.append_children(node, &[created]);
        }
    }
    ctx.dirty = true;
}

/// `node.setAttribute(name, value)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_attribute(
    ctx: *mut c_void,
    node: u64,
    name: *const c_char,
    value: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let (name, value) = unsafe { (as_str(name), as_str(value)) };
    let node = NodeId::from_u64(node);
    let name = QualName::new(None, Namespace::from(""), LocalName::from(name));
    {
        let mut mutator = ctx.doc.mutate();
        mutator.set_attribute(node, name, value);
    }
    ctx.dirty = true;
}

/// Mark the document damaged without mutating it (used after JS side effects
/// that already went through one of the setters above).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_mark_dirty(ctx: *mut c_void) {
    if let Some(ctx) = unsafe { as_ctx(ctx) } {
        ctx.dirty = true;
    }
}

/// Walk up from the hit node to the nearest element carrying an `id`
/// attribute, so a click on a descendant still reaches the element the page
/// bound a handler to. Mirrors the bubbling the document describes.
pub fn bubble_target(doc: &blitz_dom::BaseDocument, start: NodeId) -> NodeId {
    let mut current = start;
    loop {
        let Some(node) = doc.get_node(current) else {
            return start;
        };
        if node.attr(LocalName::from("id")).is_some() {
            return current;
        }
        match node.parent {
            Some(parent) => current = parent,
            None => return start,
        }
    }
}
