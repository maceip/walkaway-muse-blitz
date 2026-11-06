//! dom_ffi.rs — the Rust side of the Static Hermes + Blitz DOM bridge.
//!
//! Every function here is called from the AOT-compiled TypeScript in
//! `src/js/dom_runtime.ts`, through the thin C wrappers in
//! `c_shim/hermes_shim.cpp` (which bind the document pointer so that it never
//! crosses the JSI boundary).
//!
//! The document lives here, in Rust, owned by `BlitzHostContext`. Mutations go
//! through Blitz's `DocumentMutator`, which flags Stylo/Taffy damage; the host
//! loop then re-resolves only the damaged subtree.

use blitz_dom::{BaseDocument, LocalName, Namespace, NodeData, NodeId, QualName};
use blitz_html::HtmlDocument;
use markup5ever::ns;
use std::ffi::{CStr, c_char, c_int, c_void};

/// Owns the Blitz document, scratch buffers for FFI return values, and the
/// dirty flag the host loop polls.
pub struct BlitzHostContext {
    pub doc: HtmlDocument,
    /// Set by any JS mutation; cleared by the host after re-resolving layout.
    pub dirty: bool,
    /// Scratch buffer holding the most recent `querySelectorAll` result list.
    pub scratch_nodes: Vec<u64>,
    /// Scratch buffer holding the most recent UTF-8 string read (`getAttribute`,
    /// `innerText`, `innerHTML`).
    pub scratch_utf8: Vec<u8>,
}

impl BlitzHostContext {
    pub fn new(doc: HtmlDocument) -> Self {
        Self {
            doc,
            dirty: false,
            scratch_nodes: Vec::new(),
            scratch_utf8: Vec::new(),
        }
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

#[derive(Clone, Debug)]
struct CompoundSelector<'a> {
    tag: Option<&'a str>,
    id: Option<&'a str>,
    classes: Vec<&'a str>,
    attrs: Vec<(&'a str, Option<&'a str>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Combinator {
    Descendant,
    Child,
}

fn parse_compound(mut input: &str) -> Option<CompoundSelector<'_>> {
    input = input.trim();
    if input.is_empty() {
        return None;
    }
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    let mut attrs = Vec::new();

    // Extract `[attr]` / `[attr="val"]` blocks first.
    while let Some(open) = input.find('[') {
        let rest = &input[open + 1..];
        let close = rest.find(']')?;
        let inside = rest[..close].trim();
        if let Some((k, v)) = inside.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            attrs.push((k.trim(), Some(v)));
        } else if !inside.is_empty() {
            attrs.push((inside, None));
        }
        // Keep the prefix before `[` if it's the first bracket; any trailing
        // simple selectors after `]` are uncommon in basic compound selectors.
        input = &input[..open];
    }

    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'#' || ch == b'.' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'#' && bytes[end] != b'.' {
                end += 1;
            }
            let token = &input[start..end];
            if !token.is_empty() {
                if ch == b'#' {
                    id = Some(token);
                } else {
                    classes.push(token);
                }
            }
            i = end;
        } else {
            let start = i;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'#' && bytes[end] != b'.' {
                end += 1;
            }
            let token = input[start..end].trim();
            if !token.is_empty() && token != "*" {
                tag = Some(token);
            }
            i = end;
        }
    }

    Some(CompoundSelector {
        tag,
        id,
        classes,
        attrs,
    })
}

fn parse_selector_chain(selector: &str) -> Vec<(Option<Combinator>, CompoundSelector<'_>)> {
    let mut chain = Vec::new();
    let mut pending_comb: Option<Combinator> = None;
    for raw_part in selector.split_whitespace() {
        if raw_part == ">" {
            pending_comb = Some(Combinator::Child);
            continue;
        }
        // Handle `a>b` without spaces.
        if raw_part.contains('>') && !raw_part.contains('[') {
            for (idx, sub) in raw_part.split('>').enumerate() {
                let sub = sub.trim();
                if sub.is_empty() {
                    continue;
                }
                let comb = if idx == 0 {
                    pending_comb.or_else(|| (!chain.is_empty()).then_some(Combinator::Descendant))
                } else {
                    Some(Combinator::Child)
                };
                if let Some(comp) = parse_compound(sub) {
                    chain.push((comb, comp));
                }
            }
            pending_comb = None;
            continue;
        }
        let comb = pending_comb.or_else(|| (!chain.is_empty()).then_some(Combinator::Descendant));
        if let Some(comp) = parse_compound(raw_part) {
            chain.push((comb, comp));
        }
        pending_comb = None;
    }
    chain
}

fn node_matches_compound(doc: &BaseDocument, node_id: NodeId, comp: &CompoundSelector<'_>) -> bool {
    let Some(node) = doc.get_node(node_id) else {
        return false;
    };
    let Some(el) = node.element_data() else {
        return false;
    };
    if let Some(expected_tag) = comp.tag {
        let tag_str: &str = el.name.local.as_ref();
        if !tag_str.eq_ignore_ascii_case(expected_tag) {
            return false;
        }
    }
    if let Some(expected_id) = comp.id {
        if el.attr(LocalName::from("id")) != Some(expected_id) {
            return false;
        }
    }
    if !comp.classes.is_empty() {
        let class_attr = el.attr(LocalName::from("class")).unwrap_or_default();
        for want_cls in &comp.classes {
            if !class_attr.split_ascii_whitespace().any(|c| c == *want_cls) {
                return false;
            }
        }
    }
    for (attr_name, attr_val) in &comp.attrs {
        let actual = el.attr(LocalName::from(*attr_name));
        match (actual, attr_val) {
            (None, _) => return false,
            (Some(_), None) => {}
            (Some(act), Some(exp)) => {
                if act != *exp {
                    return false;
                }
            }
        }
    }
    true
}

fn node_matches_chain(
    doc: &BaseDocument,
    node_id: NodeId,
    chain: &[(Option<Combinator>, CompoundSelector<'_>)],
) -> bool {
    let Some((comb, last_comp)) = chain.last() else {
        return false;
    };
    if !node_matches_compound(doc, node_id, last_comp) {
        return false;
    }
    if chain.len() == 1 {
        return true;
    }
    let prefix = &chain[..chain.len() - 1];
    let Some(mut parent_id) = doc.get_node(node_id).and_then(|n| n.parent) else {
        return false;
    };
    match comb {
        Some(Combinator::Child) => node_matches_chain(doc, parent_id, prefix),
        _ => {
            loop {
                if node_matches_chain(doc, parent_id, prefix) {
                    return true;
                }
                match doc.get_node(parent_id).and_then(|n| n.parent) {
                    Some(p) => parent_id = p,
                    None => return false,
                }
            }
        }
    }
}

/// Walk the document in pre-order and collect all elements matching `selector`.
pub fn query_selector_all_in_doc(doc: &BaseDocument, selector: &str) -> Vec<NodeId> {
    let chains: Vec<Vec<(Option<Combinator>, CompoundSelector<'_>)>> = selector
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_selector_chain)
        .filter(|c| !c.is_empty())
        .collect();
    if chains.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(node_id) = stack.pop() {
        let Some(node) = doc.get_node(node_id) else {
            continue;
        };
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
        if node.element_data().is_some()
            && chains
                .iter()
                .any(|chain| node_matches_chain(doc, node_id, chain))
        {
            matches.push(node_id);
        }
    }
    matches
}

/// `document.querySelector(selector)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_query_selector(
    ctx: *mut c_void,
    selector: *const c_char,
) -> u64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    let selector = unsafe { as_str(selector) }.trim();
    if selector.is_empty() {
        return 0;
    }
    if let Some(id) = selector.strip_prefix('#') {
        if !id.contains([' ', '.', '[', '>', ',', ':']) {
            return ctx
                .doc
                .get_element_by_id(id)
                .map(|n| n.as_u64())
                .unwrap_or(0);
        }
    }
    query_selector_all_in_doc(&ctx.doc, selector)
        .first()
        .map(|n| n.as_u64())
        .unwrap_or(0)
}

/// `document.querySelectorAll(selector)` — populates `ctx.scratch_nodes` and returns count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_query_selector_all(
    ctx: *mut c_void,
    selector: *const c_char,
) -> usize {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    let selector = unsafe { as_str(selector) }.trim();
    ctx.scratch_nodes = query_selector_all_in_doc(&ctx.doc, selector)
        .into_iter()
        .map(|n| n.as_u64())
        .collect();
    ctx.scratch_nodes.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_query_selector_item(ctx: *mut c_void, index: usize) -> u64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    ctx.scratch_nodes.get(index).copied().unwrap_or(0)
}

/// `document.createElement(tag)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_create_element(ctx: *mut c_void, tag: *const c_char) -> u64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    let tag = unsafe { as_str(tag) }.trim().to_ascii_lowercase();
    if tag.is_empty() {
        return 0;
    }
    let name = QualName::new(None, ns!(html), LocalName::from(tag.as_str()));
    let id = {
        let mut mutator = ctx.doc.mutate();
        mutator.create_element(name, Vec::new())
    };
    ctx.dirty = true;
    id.as_u64()
}

/// `parent.appendChild(child)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_append_child(ctx: *mut c_void, parent: u64, child: u64) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    if parent == 0 || child == 0 {
        return;
    }
    let parent_id = NodeId::from_u64(parent);
    let child_id = NodeId::from_u64(child);
    if ctx.doc.get_node(parent_id).is_none() || ctx.doc.get_node(child_id).is_none() {
        return;
    }
    {
        let mut mutator = ctx.doc.mutate();
        mutator.append_children(parent_id, &[child_id]);
    }
    ctx.dirty = true;
}

/// `node.remove()` / `removeNode`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_remove_node(ctx: *mut c_void, node: u64) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    if node == 0 {
        return;
    }
    let node_id = NodeId::from_u64(node);
    if ctx.doc.get_node(node_id).is_none() {
        return;
    }
    {
        let mut mutator = ctx.doc.mutate();
        mutator.remove_node(node_id);
    }
    ctx.dirty = true;
}

/// `node.parentElement` / parent `NodeId` for event bubbling.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_parent_node(ctx: *mut c_void, node: u64) -> u64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    if node == 0 {
        return 0;
    }
    let node_id = NodeId::from_u64(node);
    ctx.doc
        .get_node(node_id)
        .and_then(|n| n.parent)
        .and_then(|p| {
            // Only return Element ancestors (stop before the root Document node).
            ctx.doc.get_node(p)?.element_data().map(|_| p.as_u64())
        })
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
    if ctx.doc.get_node(node).is_none() {
        return;
    }
    {
        let mut mutator = ctx.doc.mutate();
        mutator.set_style_property(node, name, value);
    }
    ctx.dirty = true;
}

/// `node.innerText = text`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_text(ctx: *mut c_void, node: u64, text: *const c_char) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let text = unsafe { as_str(text) };
    let node = NodeId::from_u64(node);
    if ctx.doc.get_node(node).is_none() {
        return;
    }

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

/// `node.innerText` getter -> writes UTF-8 into `ctx.scratch_utf8` and returns byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_load_inner_text(ctx: *mut c_void, node: u64) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let node_id = NodeId::from_u64(node);
    let Some(n) = ctx.doc.get_node(node_id) else {
        return -1;
    };
    let text = n.text_content();
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(text.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// Serialize a node's child subtree into an HTML string.
pub fn serialize_children_html(doc: &BaseDocument, node_id: NodeId, out: &mut String) {
    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    for &child_id in &node.children {
        serialize_node_html(doc, child_id, out);
    }
}

pub fn serialize_node_html(doc: &BaseDocument, node_id: NodeId, out: &mut String) {
    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    match &node.data {
        NodeData::Text(t) => {
            for ch in t.content.chars() {
                match ch {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    _ => out.push(ch),
                }
            }
        }
        NodeData::Element(el) => {
            let tag = el.name.local.as_ref();
            out.push('<');
            out.push_str(tag);
            for attr in el.attrs.iter() {
                out.push(' ');
                out.push_str(attr.name.local.as_ref());
                out.push_str("=\"");
                for ch in attr.value.chars() {
                    match ch {
                        '&' => out.push_str("&amp;"),
                        '"' => out.push_str("&quot;"),
                        '<' => out.push_str("&lt;"),
                        '>' => out.push_str("&gt;"),
                        _ => out.push(ch),
                    }
                }
                out.push('"');
            }
            out.push('>');
            let is_void = matches!(
                tag,
                "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
                    | "param" | "source" | "track" | "wbr"
            );
            if !is_void {
                serialize_children_html(doc, node_id, out);
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
        _ => {}
    }
}

/// `node.innerHTML = html`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_inner_html(
    ctx: *mut c_void,
    node: u64,
    html: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let html = unsafe { as_str(html) };
    let node_id = NodeId::from_u64(node);
    if ctx.doc.get_node(node_id).is_none() {
        return;
    }
    {
        let mut mutator = ctx.doc.mutate();
        mutator.set_inner_html(node_id, html);
    }
    ctx.dirty = true;
}

/// `node.innerHTML` getter -> writes UTF-8 into `ctx.scratch_utf8` and returns byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_load_inner_html(ctx: *mut c_void, node: u64) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let node_id = NodeId::from_u64(node);
    if ctx.doc.get_node(node_id).is_none() {
        return -1;
    }
    let mut html = String::new();
    serialize_children_html(&ctx.doc, node_id, &mut html);
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(html.as_bytes());
    ctx.scratch_utf8.len() as c_int
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
    if ctx.doc.get_node(node).is_none() {
        return;
    }
    let qual = QualName::new(None, Namespace::from(""), LocalName::from(name));
    {
        let mut mutator = ctx.doc.mutate();
        mutator.set_attribute(node, qual, value);
    }
    ctx.dirty = true;
}

/// `node.getAttribute(name)` -> writes UTF-8 into `ctx.scratch_utf8` and returns byte length, or -1 if absent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_load_attribute(
    ctx: *mut c_void,
    node: u64,
    name: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let name = unsafe { as_str(name) };
    let node_id = NodeId::from_u64(node);
    let Some(n) = ctx.doc.get_node(node_id) else {
        return -1;
    };
    let Some(val) = n.attr(LocalName::from(name)) else {
        return -1;
    };
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(val.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_scratch_byte(ctx: *mut c_void, index: usize) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    ctx.scratch_utf8.get(index).copied().unwrap_or(0) as c_int
}

/// `node.getBoundingClientRect()` field accessor (0=x, 1=y, 2=width, 3=height).
/// Re-resolves Stylo/Taffy layout synchronously if the DOM has pending mutations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_get_bounding_rect_field(
    ctx: *mut c_void,
    node: u64,
    field: c_int,
) -> f64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0.0;
    };
    if ctx.dirty {
        ctx.doc.resolve(0.0);
        ctx.dirty = false;
    }
    let node_id = NodeId::from_u64(node);
    let Some(rect) = ctx.doc.get_client_bounding_rect(node_id) else {
        return 0.0;
    };
    match field {
        0 => rect.x,
        1 => rect.y,
        2 => rect.width,
        3 => rect.height,
        _ => 0.0,
    }
}

/// `navigator.credentials.create(options)` -> runs a real NIST P-256 WebAuthn registration
/// ceremony and writes the JSON credential response into `ctx.scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_webauthn_create(
    ctx: *mut c_void,
    rp_id: *const c_char,
    user_name: *const c_char,
    challenge: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let (rp_id, user_name, challenge) = unsafe { (as_str(rp_id), as_str(user_name), as_str(challenge)) };
    let effective_rp = if rp_id.trim().is_empty() { "localhost" } else { rp_id.trim() };
    let effective_user = if user_name.trim().is_empty() { "user@localhost" } else { user_name.trim() };
    let effective_chal = if challenge.trim().is_empty() { "default_challenge" } else { challenge.trim() };
    let origin = format!("https://{effective_rp}");
    let Ok(acct) = crate::vault::get_or_create_secret(&origin, effective_user) else {
        return -1;
    };
    let Ok(bundle) = crate::vault::sign_webauthn_assertion(
        &origin,
        effective_user,
        effective_rp,
        effective_chal,
        true,
        true,
    ) else {
        return -1;
    };
    let payload = serde_json::json!({
        "id": acct.credential_id,
        "rawId": acct.credential_id,
        "type": "public-key",
        "authenticatorAttachment": "platform",
        "rpId": effective_rp,
        "userName": effective_user,
        "publicKeyPem": bundle.public_key_pem,
        "clientDataJSON": bundle.client_data_json_b64,
        "authenticatorData": bundle.authenticator_data_b64,
        "signature": bundle.signature_der_b64,
        "signCount": bundle.sign_count,
    })
    .to_string();
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(payload.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// `navigator.credentials.get(options)` -> runs a real NIST P-256 WebAuthn assertion
/// ceremony and writes the JSON assertion response into `ctx.scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_webauthn_get(
    ctx: *mut c_void,
    rp_id: *const c_char,
    challenge: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let (rp_id, challenge) = unsafe { (as_str(rp_id), as_str(challenge)) };
    let effective_rp = if rp_id.trim().is_empty() { "localhost" } else { rp_id.trim() };
    let effective_chal = if challenge.trim().is_empty() { "default_challenge" } else { challenge.trim() };
    let origin = format!("https://{effective_rp}");
    let store = crate::vault::load_store();
    let user_name = store
        .accounts
        .iter()
        .find(|a| a.origin == origin || a.origin.contains(effective_rp))
        .map(|a| a.username.clone())
        .unwrap_or_else(|| "user@localhost".to_string());
    let Ok(bundle) = crate::vault::sign_webauthn_assertion(
        &origin,
        &user_name,
        effective_rp,
        effective_chal,
        true,
        true,
    ) else {
        return -1;
    };
    let payload = serde_json::json!({
        "id": bundle.credential_id,
        "rawId": bundle.credential_id,
        "type": "public-key",
        "authenticatorAttachment": "platform",
        "rpId": bundle.rp_id,
        "publicKeyPem": bundle.public_key_pem,
        "clientDataJSON": bundle.client_data_json_b64,
        "authenticatorData": bundle.authenticator_data_b64,
        "signature": bundle.signature_der_b64,
        "signCount": bundle.sign_count,
    })
    .to_string();
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(payload.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// Mark the document damaged without mutating it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_mark_dirty(ctx: *mut c_void) {
    if let Some(ctx) = unsafe { as_ctx(ctx) } {
        ctx.dirty = true;
    }
}

/// Walk up from the hit node to the nearest element carrying an `id`
/// attribute, so a click on a descendant still reaches the element the page
/// bound a handler to.
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
