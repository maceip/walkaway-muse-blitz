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
    /// Scratch buffer holding binary data the page handed to the host, such as
    /// a WebAuthn challenge.
    ///
    /// A WebAuthn challenge is an `ArrayBuffer` of arbitrary bytes. Passing it
    /// as a C string would truncate it at the first NUL byte and would corrupt
    /// any byte that is not valid UTF-8, so binary data travels through this
    /// buffer with an explicit length instead.
    pub scratch_bytes: Vec<u8>,
    /// The origin of the document the page is showing.
    ///
    /// A WebAuthn ceremony is bound to the origin the page was actually
    /// served from. Deriving the origin from the relying party's own
    /// `rpId` would make the binding meaningless, so the host states the real
    /// origin here and the authenticator checks it.
    pub origin: String,
    /// The credentials the relying party named in `allowCredentials`.
    ///
    /// When a relying party names the credentials it accepts, the
    /// authenticator must use exactly those. When it names none, the
    /// authenticator discovers the credential itself.
    pub allow_credentials: Vec<AllowCredential>,
    /// The passkey broker the page's ceremonies run in.
    ///
    /// The broker is shared with the host, because a page ceremony has to
    /// outlive the call that started it: the host has to raise the approval
    /// prompt, wait for a human, and only then approve the ceremony. A
    /// broker owned by this context alone would be dropped when the call
    /// returned, and the ceremony with it.
    pub broker: std::rc::Rc<std::cell::RefCell<crate::credentials::LocalPasskeyBroker>>,
    /// The page ceremonies that are waiting for a human, by ceremony id.
    pub pending_page_ceremonies: std::collections::HashMap<String, PageCeremony>,
    /// Page ceremonies waiting for the related-origins well-known fetch.
    ///
    /// When a relying party names an RP ID that is not a suffix of the
    /// origin's host, the spec's related origins procedure has to fetch the
    /// relying party's `/.well-known/webauthn` document before the ceremony
    /// may start. That is a network fetch, and the FFI entry point is
    /// synchronous, so the request is parked here and the host resolves it
    /// off the UI thread. See `PendingRelatedOrigins`.
    pub pending_related_origins: std::collections::HashMap<String, PendingRelatedOrigins>,
    /// The related-origins requests whose well-known fetch is already running.
    ///
    /// A request stays in `pending_related_origins` until the fetch answers,
    /// because the answer needs the request's inputs to start the ceremony.
    /// This set keeps a request from being fetched twice.
    pub related_origins_in_flight: std::collections::HashSet<String>,
    /// The counter that names the next parked related-origins request.
    pub next_related_origins_id: u64,
    /// A `<form>` element (or submitter) that page JS requested to submit via `form.submit()`.
    pub pending_form_submit: Option<NodeId>,
    /// Full URL of the current document (`https://example.com/path?query#hash`).
    pub doc_url: String,
    /// Pending JS-initiated navigation (`window.location.href = ...` / `location.assign` / `location.reload`).
    pub pending_navigation: Option<String>,
    /// Per-tab `sessionStorage` map.
    pub session_storage: std::collections::HashMap<String, String>,
    /// Per-page JS console log ring buffer (`console.log`, `warn`, `error`, plus uncaught JSIExceptions).
    pub console_logs: Vec<ConsoleEntry>,
    /// Set to `true` when the most recently dispatched JS event called `event.preventDefault()`.
    pub last_event_default_prevented: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConsoleEntry {
    pub level: String,
    pub message: String,
}

/// What kind of ceremony a page started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageCeremonyKind {
    /// The page asked to register a new credential.
    Registration,
    /// The page asked for an assertion.
    Assertion,
}

/// A page-initiated ceremony that is waiting for a human.
#[derive(Clone, Debug)]
pub struct PageCeremony {
    /// The ceremony id the broker gave it.
    pub ceremony_id: String,
    /// What kind of ceremony this is.
    pub kind: PageCeremonyKind,
    /// The prompt shown to the human.
    pub prompt: String,
    /// The origin the ceremony is bound to.
    pub origin: String,
    /// The relying party the ceremony is for.
    pub rp_id: String,
    /// The account the ceremony would use.
    pub account_hint: Option<String>,
    /// When the ceremony started.
    pub started_at: std::time::Instant,
}

impl Default for PageCeremony {
    fn default() -> Self {
        Self {
            ceremony_id: String::new(),
            kind: PageCeremonyKind::Assertion,
            prompt: String::new(),
            origin: String::new(),
            rp_id: String::new(),
            account_hint: None,
            started_at: std::time::Instant::now(),
        }
    }
}

/// A credential the relying party named in `allowCredentials`.
#[derive(Clone, Debug, Default)]
pub struct AllowCredential {
    /// The credential id, base64url.
    pub id: Option<String>,
    /// The transports the relying party supports.
    pub transports: Vec<String>,
    /// Whether the credential must be discoverable.
    pub rk: Option<bool>,
    /// The credential type.
    pub kind: String,
}

/// A page ceremony that is waiting for the related-origins well-known fetch.
///
/// The page called `navigator.credentials.get()` (or `create()`) with an RP
/// ID that is not a suffix of the origin's host. The spec's related origins
/// procedure has to fetch the relying party's `/.well-known/webauthn`
/// document before the ceremony may start, and that fetch must not run on
/// the UI thread. The FFI entry point parks the request here and returns a
/// "pending" result to the page; the host picks it up, runs the fetch on a
/// worker thread, and then starts the ceremony (or rejects it).
#[derive(Clone, Debug)]
pub struct PendingRelatedOrigins {
    /// The id the host uses to correlate the fetch result with this request.
    pub request_id: String,
    /// The origin the page was actually served from.
    pub origin: String,
    /// The RP ID the relying party asked for.
    pub rp_id: String,
    /// The challenge, in the raw bytes the page handed over.
    pub challenge: Vec<u8>,
    /// The user name the page asked for, for a registration.
    pub user_name: String,
    /// Whether this is a registration (rather than an assertion).
    pub is_create: bool,
    /// The credentials the relying party named in `allowCredentials`.
    pub allow_credentials: Vec<AllowCredential>,
    /// When the request was parked, for the deadline.
    pub started_at: std::time::Instant,
}

impl BlitzHostContext {
    pub fn new(doc: HtmlDocument) -> Self {
        Self::with_broker(doc, default_broker())
    }

    /// Creates a context that runs its page ceremonies in `broker`.
    ///
    /// The host passes the same broker it uses for its own ceremonies, so a
    /// page ceremony and an agent-proposed ceremony are both visible to the
    /// host, and the host can approve or cancel either one.
    pub fn with_broker(
        doc: HtmlDocument,
        broker: std::rc::Rc<std::cell::RefCell<crate::credentials::LocalPasskeyBroker>>,
    ) -> Self {
        Self {
            doc,
            dirty: false,
            scratch_nodes: Vec::new(),
            scratch_utf8: Vec::new(),
            scratch_bytes: Vec::new(),
            origin: String::new(),
            allow_credentials: Vec::new(),
            broker,
            pending_page_ceremonies: std::collections::HashMap::new(),
            pending_related_origins: std::collections::HashMap::new(),
            related_origins_in_flight: std::collections::HashSet::new(),
            next_related_origins_id: 1,
            pending_form_submit: None,
            doc_url: String::new(),
            pending_navigation: None,
            session_storage: std::collections::HashMap::new(),
            console_logs: Vec::new(),
            last_event_default_prevented: false,
        }
    }

    /// Creates a context with both its full document URL and serialized origin.
    pub fn with_url_and_broker(
        doc: HtmlDocument,
        doc_url: &str,
        origin: &str,
        broker: std::rc::Rc<std::cell::RefCell<crate::credentials::LocalPasskeyBroker>>,
    ) -> Self {
        Self {
            doc_url: doc_url.to_string(),
            origin: origin.to_string(),
            ..Self::with_broker(doc, broker)
        }
    }
}

/// The broker a context uses when the host does not supply one.
pub fn default_broker() -> std::rc::Rc<std::cell::RefCell<crate::credentials::LocalPasskeyBroker>> {
    std::rc::Rc::new(std::cell::RefCell::new(
        crate::credentials::LocalPasskeyBroker::new(),
    ))
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

/// Copies `len` bytes into the binary scratch buffer and returns the length.
///
/// The page hands over a WebAuthn challenge as raw bytes. This is the only
/// way in: a challenge is arbitrary binary, so it cannot travel as a C
/// string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_write_scratch_bytes(
    ctx: *mut c_void,
    bytes: *const u8,
    len: usize,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    if bytes.is_null() {
        ctx.scratch_bytes.clear();
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    ctx.scratch_bytes.clear();
    ctx.scratch_bytes.extend_from_slice(slice);
    ctx.scratch_bytes.len() as c_int
}

/// Returns the length of the binary scratch buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_scratch_bytes_len(ctx: *mut c_void) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    ctx.scratch_bytes.len() as c_int
}

/// Returns the byte at `index` of the binary scratch buffer, or -1 when the
/// index is out of range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_scratch_bytes_get(ctx: *mut c_void, index: usize) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    match ctx.scratch_bytes.get(index) {
        Some(byte) => *byte as c_int,
        None => -1,
    }
}

/// Sets the credentials the relying party named in `allowCredentials`.
///
/// The list arrives as JSON, because the page sends credential descriptors,
/// not a flat byte array. An empty list means the relying party named none,
/// and the authenticator discovers the credential itself.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_allow_credentials(ctx: *mut c_void, json: *const c_char) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let raw = unsafe { as_str(json) };
    if raw.trim().is_empty() {
        ctx.allow_credentials.clear();
        return;
    }
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => {
            ctx.allow_credentials.clear();
            return;
        }
    };
    let Some(items) = parsed.as_array() else {
        ctx.allow_credentials.clear();
        return;
    };
    ctx.allow_credentials = items
        .iter()
        .map(|item| AllowCredential {
            id: item
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            transports: item
                .get("transports")
                .and_then(serde_json::Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            rk: item.get("rk").and_then(serde_json::Value::as_bool),
            kind: item
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("public-key")
                .to_string(),
        })
        .collect();
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

/// Writes `payload` into the UTF-8 scratch buffer and returns its length.
///
/// The page reads the JSON back with `readScratchUtf8`.
fn write_json(ctx: &mut BlitzHostContext, payload: &serde_json::Value) -> c_int {
    let text = payload.to_string();
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(text.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// The error payload the page turns into a rejected promise.
fn webauthn_error(message: &str) -> serde_json::Value {
    serde_json::json!({ "error": message })
}

/// Starts a registration ceremony in the broker and records it as pending.
///
/// This is the part of the registration path that does not touch the
/// network, so it is shared by the FFI entry point and by the host's
/// related-origins resolution, which reaches the same ceremony once the
/// well-known fetch has said the relying party is related to the origin.
fn begin_page_registration(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    user_name: &str,
    challenge: &[u8],
    related_origin_validated: bool,
) -> serde_json::Value {
    begin_page_registration_inner(
        ctx,
        origin,
        rp_id,
        user_name,
        challenge,
        related_origin_validated,
    )
}

/// Starts a registration ceremony whose related origins check has already
/// succeeded.
///
/// The host calls this once the relying party's well-known document has named
/// the caller's origin (WebAuthn Level 3 §5.11.1). The ceremony then starts
/// with the related origins check made, and the page's promise moves on to
/// the human approval gate.
pub fn begin_page_registration_with_related_origins(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    user_name: &str,
    challenge: &[u8],
) -> serde_json::Value {
    begin_page_registration_inner(ctx, origin, rp_id, user_name, challenge, true)
}

/// The registration ceremony start, with the related origins check either
/// made or not.
fn begin_page_registration_inner(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    user_name: &str,
    challenge: &[u8],
    related_origin_validated: bool,
) -> serde_json::Value {
    use crate::credentials::{PasskeyBroker, PasskeyStatus, RegistrationRequest};
    use crate::webauthn::types::{PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity};

    let user_name = if user_name.trim().is_empty() {
        "user@localhost".to_string()
    } else {
        user_name.trim().to_string()
    };
    let request = RegistrationRequest {
        task_id: "page-webauthn-create".to_string(),
        origin: origin.to_string(),
        rp: PublicKeyCredentialRpEntity {
            id: rp_id.to_string(),
            name: Some(rp_id.to_string()),
        },
        user: PublicKeyCredentialUserEntity {
            id: user_name.as_bytes().to_vec(),
            name: user_name.clone(),
            display_name: user_name.clone(),
        },
        challenge: challenge.to_vec(),
        pub_key_cred_params: Vec::new(),
        exclude_credentials: Vec::new(),
        authenticator_selection: None,
        attestation: None,
        extensions: None,
        cross_origin: false,
        related_origin_validated,
    };
    // The ceremony is NOT approved here. A page may not register a
    // credential without a human saying so, so the ceremony stops here and
    // the host raises the approval prompt. The page's promise stays pending
    // until the human answers.
    let started = {
        let mut broker = ctx.broker.borrow_mut();
        broker.begin_registration(request).map(|ceremony| {
            let (prompt, account_hint) = match broker.poll_registration(ceremony.clone()) {
                Ok(PasskeyStatus::AwaitingApproval {
                    prompt,
                    account_hint,
                    ..
                }) => (prompt, account_hint),
                _ => (String::new(), None),
            };
            (ceremony, prompt, account_hint)
        })
    };
    let (ceremony, prompt, account_hint) = match started {
        Ok(started) => started,
        Err(error) => return webauthn_error(&error.to_string()),
    };
    let ceremony_id = ceremony.0;
    ctx.pending_page_ceremonies.insert(
        ceremony_id.clone(),
        PageCeremony {
            ceremony_id: ceremony_id.clone(),
            kind: PageCeremonyKind::Registration,
            prompt: prompt.clone(),
            origin: origin.to_string(),
            rp_id: rp_id.to_string(),
            account_hint,
            started_at: std::time::Instant::now(),
        },
    );
    serde_json::json!({
        "pending": true,
        "ceremony_id": ceremony_id,
        "prompt": prompt,
        "origin": origin,
        "rp_id": rp_id,
    })
}

/// Starts an assertion ceremony in the broker and records it as pending.
///
/// The shared counterpart of [`begin_page_registration`].
fn begin_page_assertion(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    challenge: &[u8],
    allow_credentials: Vec<AllowCredential>,
    related_origin_validated: bool,
) -> serde_json::Value {
    begin_page_assertion_inner(
        ctx,
        origin,
        rp_id,
        challenge,
        allow_credentials,
        related_origin_validated,
    )
}

/// Starts an assertion ceremony whose related origins check has already
/// succeeded.
///
/// The host calls this once the relying party's well-known document has named
/// the caller's origin (WebAuthn Level 3 §5.11.1).
pub fn begin_page_assertion_with_related_origins(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    challenge: &[u8],
    allow_credentials: Vec<AllowCredential>,
) -> serde_json::Value {
    begin_page_assertion_inner(ctx, origin, rp_id, challenge, allow_credentials, true)
}

/// The assertion ceremony start, with the related origins check either made
/// or not.
fn begin_page_assertion_inner(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    challenge: &[u8],
    allow_credentials: Vec<AllowCredential>,
    related_origin_validated: bool,
) -> serde_json::Value {
    use crate::credentials::{PasskeyBroker, PasskeyRequest, PasskeyStatus};

    // The relying party may name the credentials it accepts. When it does,
    // the authenticator must use exactly those; when it does not, the
    // authenticator discovers the credential itself, which is how autofill
    // and conditional mediation work.
    let allow_credentials = allow_credentials
        .into_iter()
        .filter_map(|raw| {
            let id = raw.id?;
            let bytes = crate::webauthn::b64::b64url_decode(&id)
                .or_else(|| crate::control::b64_decode(&id))?;
            Some(crate::webauthn::types::PublicKeyCredentialDescriptor {
                id: bytes,
                transports: raw.transports,
                rk: raw.rk,
                kind: if raw.kind.is_empty() {
                    "public-key".to_string()
                } else {
                    raw.kind
                },
            })
        })
        .collect();

    let request = PasskeyRequest {
        task_id: "page-webauthn-get".to_string(),
        origin: origin.to_string(),
        rp_id: Some(rp_id.to_string()),
        challenge: challenge.to_vec(),
        allow_credentials,
        user_verification: None,
        extensions: None,
        cross_origin: false,
        top_origin: None,
        related_origin_validated,
    };

    // The broker is shared with the host, so the ceremony this call starts
    // is still there when the host comes to approve it.
    let started = {
        let mut broker = ctx.broker.borrow_mut();
        broker.begin_assertion(request).map(|ceremony| {
            let (prompt, account_hint) = match broker.poll_assertion(ceremony.clone()) {
                Ok(PasskeyStatus::AwaitingApproval {
                    prompt,
                    account_hint,
                    ..
                }) => (prompt, account_hint),
                _ => (String::new(), None),
            };
            (ceremony, prompt, account_hint)
        })
    };
    let (ceremony, prompt, account_hint) = match started {
        Ok(started) => started,
        Err(error) => return webauthn_error(&error.to_string()),
    };
    let ceremony_id = ceremony.0;

    ctx.pending_page_ceremonies.insert(
        ceremony_id.clone(),
        PageCeremony {
            ceremony_id: ceremony_id.clone(),
            kind: PageCeremonyKind::Assertion,
            prompt: prompt.clone(),
            origin: origin.to_string(),
            rp_id: rp_id.to_string(),
            account_hint,
            started_at: std::time::Instant::now(),
        },
    );

    serde_json::json!({
        "pending": true,
        "ceremony_id": ceremony_id,
        "prompt": prompt,
        "origin": origin,
        "rp_id": rp_id,
    })
}

/// Parks a ceremony that needs the related-origins well-known fetch.
///
/// The FFI entry point is synchronous and the fetch is not, so the request
/// waits here until the host runs it on a worker thread. The page's promise
/// stays pending in the meantime, exactly as it does while a human is
/// deciding.
fn park_related_origins(
    ctx: &mut BlitzHostContext,
    origin: &str,
    rp_id: &str,
    challenge: &[u8],
    user_name: &str,
    is_create: bool,
    allow_credentials: Vec<AllowCredential>,
) -> serde_json::Value {
    let request_id = format!("related-origins-{}", ctx.next_related_origins_id);
    ctx.next_related_origins_id = ctx.next_related_origins_id.wrapping_add(1);
    ctx.pending_related_origins.insert(
        request_id.clone(),
        PendingRelatedOrigins {
            request_id: request_id.clone(),
            origin: origin.to_string(),
            rp_id: rp_id.to_string(),
            challenge: challenge.to_vec(),
            user_name: user_name.to_string(),
            is_create,
            allow_credentials,
            started_at: std::time::Instant::now(),
        },
    );
    serde_json::json!({
        "pending": true,
        "phase": "related-origins",
        "request_id": request_id,
        "origin": origin,
        "rp_id": rp_id,
    })
}

/// `navigator.credentials.create(options)` -> runs a real NIST P-256 WebAuthn
/// registration ceremony and writes the JSON credential response into
/// `ctx.scratch_utf8`.
///
/// The ceremony is performed by the passkey broker, in Rust, with the private
/// key in the macOS Keychain. The page never sees a private key and never
/// signs anything itself.
///
/// The challenge arrives through the binary scratch buffer, because a
/// challenge is arbitrary binary. The origin is the one the document was
/// actually served from, never one derived from the relying party's own
/// `rpId`: deriving it would make the origin check vacuous.
///
/// When the relying party names an RP ID that is not a suffix of the origin's
/// host, the spec's related origins procedure has to fetch the relying
/// party's well-known document over the network. That fetch cannot run here,
/// so the request is parked and the host resolves it; the page's promise
/// stays pending until then.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_webauthn_create(
    ctx: *mut c_void,
    rp_id: *const c_char,
    user_name: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let (rp_id, user_name) = unsafe { (as_str(rp_id), as_str(user_name)) };
    let challenge = ctx.scratch_bytes.clone();
    if challenge.is_empty() {
        return write_json(ctx, &webauthn_error("the challenge is empty"));
    }
    let origin = if ctx.origin.trim().is_empty() {
        format!("https://{}", rp_id.trim())
    } else {
        ctx.origin.clone()
    };

    // The origin has to be validated before the ceremony starts, so the
    // human is never asked to approve a ceremony that cannot succeed.
    let requested_rp = if rp_id.trim().is_empty() {
        None
    } else {
        Some(rp_id.trim().to_string())
    };
    let outcome = match crate::webauthn::origin::validate_origin_and_effective_rp_id(
        &origin,
        requested_rp.as_deref(),
        false,
        false,
    ) {
        Ok(outcome) => outcome,
        Err(error) => return write_json(ctx, &webauthn_error(&error.to_string())),
    };
    let (effective_rp, related) = match outcome {
        crate::webauthn::origin::OriginValidation::Suffix(validated) => {
            (validated.effective_rp_id, None)
        }
        crate::webauthn::origin::OriginValidation::NeedsRelatedOrigins {
            validated,
            caller_origin,
            rp_id,
        } => (validated.effective_rp_id, Some((caller_origin, rp_id))),
    };
    if let Some((caller_origin, related_rp)) = related {
        let payload = park_related_origins(
            ctx,
            &caller_origin,
            &related_rp,
            &challenge,
            user_name,
            true,
            Vec::new(),
        );
        return write_json(ctx, &payload);
    }

    let payload =
        begin_page_registration(ctx, &origin, &effective_rp, user_name, &challenge, false);
    write_json(ctx, &payload)
}

/// `navigator.credentials.get(options)` -> starts a real NIST P-256 WebAuthn
/// assertion ceremony and writes the JSON result into `ctx.scratch_utf8`.
///
/// The ceremony is performed by the passkey broker against the macOS
/// Keychain. The challenge arrives as raw bytes, and the origin is the one
/// the document was actually served from.
///
/// The ceremony is NOT approved here. The result is a *pending* result that
/// names the ceremony; the host raises the approval prompt, and the page's
/// promise is settled when the human answers. A page may not obtain a
/// signature without a human having said yes.
///
/// When the relying party names an RP ID that is not a suffix of the origin's
/// host, the spec's related origins procedure has to fetch the relying
/// party's well-known document over the network. That fetch cannot run here,
/// so the request is parked and the host resolves it; the page's promise
/// stays pending until then.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_webauthn_get(ctx: *mut c_void, rp_id: *const c_char) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let rp_id = unsafe { as_str(rp_id) };
    let challenge = ctx.scratch_bytes.clone();
    if challenge.is_empty() {
        return write_json(ctx, &webauthn_error("the challenge is empty"));
    }
    let origin = if ctx.origin.trim().is_empty() {
        format!("https://{}", rp_id.trim())
    } else {
        ctx.origin.clone()
    };

    // The relying party may name an RP ID. It must be the origin's host, a
    // parent of it, or an origin the relying party declares related to the
    // caller in its own well-known document.
    let requested_rp = if rp_id.trim().is_empty() {
        None
    } else {
        Some(rp_id.trim().to_string())
    };
    let outcome = match crate::webauthn::origin::validate_origin_and_effective_rp_id(
        &origin,
        requested_rp.as_deref(),
        false,
        false,
    ) {
        Ok(outcome) => outcome,
        Err(error) => return write_json(ctx, &webauthn_error(&error.to_string())),
    };
    let (effective_rp, related) = match outcome {
        crate::webauthn::origin::OriginValidation::Suffix(validated) => {
            (validated.effective_rp_id, None)
        }
        crate::webauthn::origin::OriginValidation::NeedsRelatedOrigins {
            validated,
            caller_origin,
            rp_id,
        } => (validated.effective_rp_id, Some((caller_origin, rp_id))),
    };
    if let Some((caller_origin, related_rp)) = related {
        let allow = ctx.allow_credentials.clone();
        let payload = park_related_origins(
            ctx,
            &caller_origin,
            &related_rp,
            &challenge,
            "",
            false,
            allow,
        );
        return write_json(ctx, &payload);
    }

    let allow = ctx.allow_credentials.clone();
    let payload = begin_page_assertion(ctx, &origin, &effective_rp, &challenge, allow, false);
    write_json(ctx, &payload)
}

/// Request the host to submit a `<form>` element (or the form enclosing `node`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_submit_form(ctx: *mut c_void, node: u64) {
    if let Some(ctx) = unsafe { as_ctx(ctx) } {
        if node != 0 {
            ctx.pending_form_submit = Some(NodeId::from_u64(node));
            ctx.dirty = true;
        }
    }
}

/// Mark the document damaged without mutating it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_mark_dirty(ctx: *mut c_void) {
    if let Some(ctx) = unsafe { as_ctx(ctx) } {
        ctx.dirty = true;
    }
}

fn local_storage_store(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, std::collections::HashMap<String, String>>>
{
    static STORE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        >,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn session_storage_store(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, std::collections::HashMap<String, String>>>
{
    static STORE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        >,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub fn clear_local_storage() {
    if let Ok(mut guard) = local_storage_store().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = session_storage_store().lock() {
        guard.clear();
    }
}

fn effective_doc_url(ctx: &BlitzHostContext) -> Option<url::Url> {
    if !ctx.doc_url.trim().is_empty() {
        if let Ok(u) = url::Url::parse(ctx.doc_url.trim()) {
            return Some(u);
        }
    }
    if !ctx.origin.trim().is_empty() {
        if let Ok(u) = url::Url::parse(ctx.origin.trim()) {
            return Some(u);
        }
    }
    None
}

/// Read `document.cookie` for the document's URL (excluding `HttpOnly` cookies).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_get_cookie(ctx: *mut c_void) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let Some(url) = effective_doc_url(ctx) else {
        ctx.scratch_utf8.clear();
        return 0;
    };
    let cookie_str = crate::browser::cookie_header_for_url(&url, true);
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(cookie_str.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// Assign `document.cookie = value` for the document's URL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_cookie(ctx: *mut c_void, cookie: *const c_char) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let cookie = unsafe { as_str(cookie) };
    if let Some(url) = effective_doc_url(ctx) {
        crate::browser::store_set_cookie(&url, cookie, true);
    }
}

/// Read the current document URL (`window.location.href`) into `scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_get_location_url(ctx: *mut c_void) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let url_str = if !ctx.doc_url.is_empty() {
        ctx.doc_url.clone()
    } else if !ctx.origin.is_empty() {
        format!("{}/", ctx.origin.trim_end_matches('/'))
    } else {
        "muse://start".to_string()
    };
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(url_str.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// Navigate or pushState to `target_url` (`push_only != 0` updates `doc_url` in place;
/// `push_only == 0` queues a real page navigation via `ctx.pending_navigation`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_location_url(
    ctx: *mut c_void,
    target_url: *const c_char,
    push_only: c_int,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let raw = unsafe { as_str(target_url) }.trim();
    if raw.is_empty() {
        return;
    }
    let resolved = if let Some(base) = effective_doc_url(ctx) {
        base.join(raw)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    };
    if push_only != 0 {
        ctx.doc_url = resolved;
    } else {
        ctx.doc_url = resolved.clone();
        ctx.pending_navigation = Some(resolved);
        ctx.dirty = true;
    }
}

/// Read a key from `localStorage` (`is_session == 0`) or `sessionStorage` (`is_session != 0`).
/// Returns `-1` when the key does not exist.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_storage_get(
    ctx: *mut c_void,
    is_session: c_int,
    key: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let key = unsafe { as_str(key) };
    let origin_key = if ctx.origin.is_empty() {
        "default".to_string()
    } else {
        ctx.origin.clone()
    };
    let val = if is_session != 0 {
        ctx.session_storage.get(key).cloned().or_else(|| {
            session_storage_store()
                .lock()
                .ok()
                .and_then(|g| g.get(&origin_key).and_then(|m| m.get(key).cloned()))
        })
    } else {
        local_storage_store()
            .lock()
            .ok()
            .and_then(|g| g.get(&origin_key).and_then(|m| m.get(key).cloned()))
    };
    match val {
        Some(v) => {
            ctx.scratch_utf8.clear();
            ctx.scratch_utf8.extend_from_slice(v.as_bytes());
            ctx.scratch_utf8.len() as c_int
        }
        None => -1,
    }
}

/// Write a key/value pair into `localStorage` (`is_session == 0`) or `sessionStorage` (`is_session != 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_storage_set(
    ctx: *mut c_void,
    is_session: c_int,
    key: *const c_char,
    value: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let key = unsafe { as_str(key) }.to_string();
    let value = unsafe { as_str(value) }.to_string();
    let origin_key = if ctx.origin.is_empty() {
        "default".to_string()
    } else {
        ctx.origin.clone()
    };
    if is_session != 0 {
        ctx.session_storage.insert(key.clone(), value.clone());
        if let Ok(mut g) = session_storage_store().lock() {
            g.entry(origin_key).or_default().insert(key, value);
        }
    } else if let Ok(mut g) = local_storage_store().lock() {
        g.entry(origin_key).or_default().insert(key, value);
    }
}

/// Remove a key from `localStorage` or `sessionStorage`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_storage_remove(
    ctx: *mut c_void,
    is_session: c_int,
    key: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let key = unsafe { as_str(key) };
    let origin_key = if ctx.origin.is_empty() {
        "default".to_string()
    } else {
        ctx.origin.clone()
    };
    if is_session != 0 {
        ctx.session_storage.remove(key);
        if let Ok(mut g) = session_storage_store().lock() {
            if let Some(m) = g.get_mut(&origin_key) {
                m.remove(key);
            }
        }
    } else if let Ok(mut g) = local_storage_store().lock() {
        if let Some(m) = g.get_mut(&origin_key) {
            m.remove(key);
        }
    }
}

/// Clear all keys for this origin (`localStorage`) or tab (`sessionStorage`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_storage_clear(ctx: *mut c_void, is_session: c_int) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let origin_key = if ctx.origin.is_empty() {
        "default".to_string()
    } else {
        ctx.origin.clone()
    };
    if is_session != 0 {
        ctx.session_storage.clear();
        if let Ok(mut g) = session_storage_store().lock() {
            g.remove(&origin_key);
        }
    } else if let Ok(mut g) = local_storage_store().lock() {
        g.remove(&origin_key);
    }
}

/// Execute an HTTP `fetch(url, { method, headers, body })` sharing the browser's RFC 6265 cookie jar.
/// Writes a JSON response envelope `{ ok, status, url, headers, body, error }` into `scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_http_fetch(
    ctx: *mut c_void,
    method: *const c_char,
    url_raw: *const c_char,
    headers_json: *const c_char,
    body_utf8: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    let method_str = unsafe { as_str(method) };
    let url_str = unsafe { as_str(url_raw) }.trim();
    let headers_str = unsafe { as_str(headers_json) };
    let body_str = unsafe { as_str(body_utf8) };

    let resolved = if let Some(base) = effective_doc_url(ctx) {
        base.join(url_str).or_else(|_| url::Url::parse(url_str))
    } else {
        url::Url::parse(url_str)
    };
    let Ok(target_url) = resolved else {
        let err_json = serde_json::json!({ "error": format!("Invalid fetch URL: {url_str}") });
        return write_json(ctx, &err_json);
    };

    let mut req_headers: Vec<(String, String)> = Vec::new();
    if !headers_str.trim().is_empty() {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(headers_str) {
            for (k, v) in map {
                if let Some(vs) = v.as_str() {
                    req_headers.push((k, vs.to_string()));
                }
            }
        }
    }
    let req_body = if method_str.eq_ignore_ascii_case("GET")
        || method_str.eq_ignore_ascii_case("HEAD")
        || body_str.is_empty()
    {
        None
    } else {
        Some(body_str.as_bytes().to_vec())
    };

    match crate::browser::execute_http(
        if method_str.is_empty() { "GET" } else { method_str },
        &target_url,
        &req_headers,
        req_body,
    ) {
        Ok(ex) => {
            let mut hdr_obj = serde_json::Map::new();
            for (k, v) in ex.headers {
                hdr_obj.insert(k.to_ascii_lowercase(), serde_json::Value::String(v));
            }
            let body_text = String::from_utf8_lossy(&ex.body).into_owned();
            let resp_json = serde_json::json!({
                "ok": (200..300).contains(&ex.status),
                "status": ex.status,
                "url": ex.final_url.to_string(),
                "content_type": ex.content_type,
                "headers": hdr_obj,
                "body": body_text,
            });
            write_json(ctx, &resp_json)
        }
        Err(e) => {
            let err_json = serde_json::json!({ "error": e });
            write_json(ctx, &err_json)
        }
    }
}

/// Append a console message (`log`, `info`, `warn`, `error`) to the page's console ring buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_console_log(
    ctx: *mut c_void,
    level: *const c_char,
    message: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    let level_str = unsafe { as_str(level) };
    let msg_str = unsafe { as_str(message) };
    if ctx.console_logs.len() >= 200 {
        ctx.console_logs.remove(0);
    }
    ctx.console_logs.push(ConsoleEntry {
        level: if level_str.is_empty() {
            "log".to_string()
        } else {
            level_str.to_string()
        },
        message: msg_str.to_string(),
    });
}

fn attr_qual(name: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(name))
}

/// Read the live value of a form control (`input`, `textarea`, `select`, `option`, `button`)
/// directly from Blitz's DOM + `text_input_data` editor + `<option selected>` state.
pub fn read_live_control_value(doc: &BaseDocument, node_id: NodeId) -> String {
    let Some(node) = doc.get_node(node_id) else {
        return String::new();
    };
    let Some(data) = node.element_data() else {
        return String::new();
    };
    let tag = data.name.local.to_string().to_ascii_lowercase();
    match tag.as_str() {
        "select" => node
            .children
            .iter()
            .copied()
            .find_map(|cid| {
                let cnode = doc.get_node(cid)?;
                let cdata = cnode.element_data()?;
                if !cdata.name.local.to_string().eq_ignore_ascii_case("option")
                    || cdata.attr(LocalName::from("disabled")).is_some()
                    || cdata.attr(LocalName::from("selected")).is_none()
                {
                    return None;
                }
                Some(
                    cdata
                        .attr(LocalName::from("value"))
                        .map(str::to_string)
                        .unwrap_or_else(|| cnode.text_content().trim().to_string()),
                )
            })
            .or_else(|| data.attr(LocalName::from("value")).map(str::to_string))
            .or_else(|| {
                node.children.iter().copied().find_map(|cid| {
                    let cnode = doc.get_node(cid)?;
                    let cdata = cnode.element_data()?;
                    if !cdata.name.local.to_string().eq_ignore_ascii_case("option")
                        || cdata.attr(LocalName::from("disabled")).is_some()
                    {
                        return None;
                    }
                    Some(
                        cdata
                            .attr(LocalName::from("value"))
                            .map(str::to_string)
                            .unwrap_or_else(|| cnode.text_content().trim().to_string()),
                    )
                })
            })
            .unwrap_or_default(),
        "textarea" => data
            .text_input_data()
            .map(|d| d.editor.text().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| data.attr(LocalName::from("value")).map(str::to_string))
            .unwrap_or_else(|| node.text_content()),
        "option" => data
            .attr(LocalName::from("value"))
            .map(str::to_string)
            .unwrap_or_else(|| node.text_content().trim().to_string()),
        _ => {
            if let Some(tid) = data.text_input_data() {
                let editor_text = tid.editor.text().to_string();
                if !editor_text.is_empty() {
                    return editor_text;
                }
            }
            data.attr(LocalName::from("value"))
                .unwrap_or_default()
                .to_string()
        }
    }
}

/// `node.removeAttribute(name)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_remove_attribute(
    ctx: *mut c_void,
    node: u64,
    name: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    if node == 0 {
        return;
    }
    let name = unsafe { as_str(name) };
    let node_id = NodeId::from_u64(node);
    if ctx.doc.get_node(node_id).is_none() {
        return;
    }
    {
        let mut mutator = ctx.doc.mutate();
        mutator.clear_attribute(node_id, attr_qual(name));
    }
    ctx.dirty = true;
}

/// `node.hasAttribute(name)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_has_attribute(
    ctx: *mut c_void,
    node: u64,
    name: *const c_char,
) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    if node == 0 {
        return 0;
    }
    let name = unsafe { as_str(name) };
    let node_id = NodeId::from_u64(node);
    ctx.doc
        .get_node(node_id)
        .and_then(|n| n.attr(LocalName::from(name)))
        .map(|_| 1)
        .unwrap_or(0)
}

/// `node.tagName` -> writes uppercase HTML tag name into `ctx.scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_load_tag_name(ctx: *mut c_void, node: u64) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    if node == 0 {
        return -1;
    }
    let node_id = NodeId::from_u64(node);
    let Some(tag_upper) = ctx
        .doc
        .get_node(node_id)
        .and_then(|n| n.element_data())
        .map(|d| d.name.local.to_string().to_ascii_uppercase())
    else {
        return -1;
    };
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(tag_upper.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// `el.value` getter -> writes live form control value into `ctx.scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_load_form_value(ctx: *mut c_void, node: u64) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    if node == 0 {
        return -1;
    }
    let val = read_live_control_value(&ctx.doc, NodeId::from_u64(node));
    ctx.scratch_utf8.clear();
    ctx.scratch_utf8.extend_from_slice(val.as_bytes());
    ctx.scratch_utf8.len() as c_int
}

/// `el.value = v` setter -> updates `<input>`, `<textarea>`, or `<select>` state in Blitz DOM.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_form_value(
    ctx: *mut c_void,
    node: u64,
    value: *const c_char,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    if node == 0 {
        return;
    }
    let val = unsafe { as_str(value) };
    let node_id = NodeId::from_u64(node);
    let Some(tag) = ctx
        .doc
        .get_node(node_id)
        .and_then(|n| n.element_data())
        .map(|d| d.name.local.to_string().to_ascii_lowercase())
    else {
        return;
    };
    match tag.as_str() {
        "select" => {
            let options: Vec<(NodeId, String, String)> = ctx
                .doc
                .get_node(node_id)
                .map(|n| {
                    n.children
                        .iter()
                        .copied()
                        .filter_map(|cid| {
                            let child = ctx.doc.get_node(cid)?;
                            let data = child.element_data()?;
                            if !data.name.local.to_string().eq_ignore_ascii_case("option") {
                                return None;
                            }
                            let opt_val = data
                                .attr(LocalName::from("value"))
                                .map(str::to_string)
                                .unwrap_or_else(|| child.text_content().trim().to_string());
                            let opt_text = child.text_content().trim().to_string();
                            Some((cid, opt_val, opt_text))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let matched_id = options
                .iter()
                .find(|(_, v, t)| v == val || t == val)
                .map(|(id, _, _)| *id);
            let mut mutator = ctx.doc.mutate();
            for (opt_id, _, _) in &options {
                if Some(*opt_id) == matched_id {
                    mutator.set_attribute(*opt_id, attr_qual("selected"), "selected");
                } else {
                    mutator.clear_attribute(*opt_id, attr_qual("selected"));
                }
            }
            mutator.set_attribute(node_id, attr_qual("value"), val);
        }
        "textarea" => {
            let existing_text_child = ctx.doc.get_node(node_id).and_then(|n| {
                n.children.iter().copied().find(|child| {
                    matches!(
                        ctx.doc.get_node(*child).map(|c| &c.data),
                        Some(NodeData::Text(_))
                    )
                })
            });
            let mut mutator = ctx.doc.mutate();
            if let Some(text_node) = existing_text_child {
                mutator.set_node_text(text_node, val);
            } else {
                let created = mutator.create_text_node(val);
                mutator.append_children(node_id, &[created]);
            }
            mutator.set_attribute(node_id, attr_qual("value"), val);
        }
        _ => {
            let mut mutator = ctx.doc.mutate();
            mutator.set_attribute(node_id, attr_qual("value"), val);
        }
    }
    ctx.dirty = true;
}

/// `el.checked` getter
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_get_checked(ctx: *mut c_void, node: u64) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    if node == 0 {
        return 0;
    }
    let node_id = NodeId::from_u64(node);
    ctx.doc
        .get_node(node_id)
        .and_then(|n| n.element_data())
        .map(|d| {
            d.checkbox_input_checked()
                .unwrap_or_else(|| d.attr(LocalName::from("checked")).is_some())
        })
        .unwrap_or(false) as c_int
}

/// `el.checked = bool` setter (handles radio group mutual exclusion too).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_checked(ctx: *mut c_void, node: u64, checked: c_int) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    if node == 0 {
        return;
    }
    let node_id = NodeId::from_u64(node);
    let is_checked = checked != 0;
    let radio_group = ctx
        .doc
        .get_node(node_id)
        .and_then(|n| n.element_data())
        .and_then(|d| {
            if d.name.local.to_string().eq_ignore_ascii_case("input")
                && d.attr(LocalName::from("type"))
                    .is_some_and(|t| t.eq_ignore_ascii_case("radio"))
            {
                d.attr(LocalName::from("name"))
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            } else {
                None
            }
        });
    if is_checked && let Some(group_name) = radio_group {
        let mut same_group = Vec::new();
        let mut stack = vec![ctx.doc.root_node().id];
        while let Some(nid) = stack.pop() {
            let Some(n) = ctx.doc.get_node(nid) else {
                continue;
            };
            for &child in n.children.iter().rev() {
                stack.push(child);
            }
            if let Some(d) = n.element_data() {
                if d.name.local.to_string().eq_ignore_ascii_case("input")
                    && d.attr(LocalName::from("type"))
                        .is_some_and(|t| t.eq_ignore_ascii_case("radio"))
                    && d.attr(LocalName::from("name")) == Some(group_name.as_str())
                {
                    same_group.push(nid);
                }
            }
        }
        let mut mutator = ctx.doc.mutate();
        for nid in same_group {
            if nid == node_id {
                mutator.set_attribute(nid, attr_qual("checked"), "true");
            } else {
                mutator.clear_attribute(nid, attr_qual("checked"));
            }
        }
    } else {
        let mut mutator = ctx.doc.mutate();
        if is_checked {
            mutator.set_attribute(node_id, attr_qual("checked"), "true");
        } else {
            mutator.clear_attribute(node_id, attr_qual("checked"));
        }
    }
    ctx.dirty = true;
}

/// `el.focus()`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_focus_node(ctx: *mut c_void, node: u64) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    if node == 0 {
        return;
    }
    let node_id = NodeId::from_u64(node);
    if ctx.doc.get_node(node_id).is_some() {
        ctx.doc.set_focus_to(node_id);
        ctx.dirty = true;
    }
}

fn query_selector_all_under(doc: &BaseDocument, root_id: NodeId, selector: &str) -> Vec<NodeId> {
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
    let Some(root_node) = doc.get_node(root_id) else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    let mut stack: Vec<NodeId> = root_node.children.iter().rev().copied().collect();
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

/// `el.querySelector(selector)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_query_selector_within(
    ctx: *mut c_void,
    root: u64,
    selector: *const c_char,
) -> u64 {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    if root == 0 {
        return 0;
    }
    let selector = unsafe { as_str(selector) }.trim();
    query_selector_all_under(&ctx.doc, NodeId::from_u64(root), selector)
        .first()
        .map(|n| n.as_u64())
        .unwrap_or(0)
}

/// `el.querySelectorAll(selector)` -> populates `ctx.scratch_nodes` and returns count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_query_selector_all_within(
    ctx: *mut c_void,
    root: u64,
    selector: *const c_char,
) -> usize {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return 0;
    };
    if root == 0 {
        ctx.scratch_nodes.clear();
        return 0;
    }
    let selector = unsafe { as_str(selector) }.trim();
    ctx.scratch_nodes = query_selector_all_under(&ctx.doc, NodeId::from_u64(root), selector)
        .into_iter()
        .map(|n| n.as_u64())
        .collect();
    ctx.scratch_nodes.len()
}

/// Collect live `[name, value]` entries for `new FormData(form)` as JSON into `ctx.scratch_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_load_form_entries_json(ctx: *mut c_void, form_node: u64) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    if form_node == 0 {
        return -1;
    }
    let root_id = NodeId::from_u64(form_node);
    if ctx.doc.get_node(root_id).is_none() {
        return -1;
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut stack = vec![root_id];
    while let Some(nid) = stack.pop() {
        let Some(node) = ctx.doc.get_node(nid) else {
            continue;
        };
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
        let Some(data) = node.element_data() else {
            continue;
        };
        if data.attr(LocalName::from("disabled")).is_some() {
            continue;
        }
        let tag = data.name.local.to_string().to_ascii_lowercase();
        let Some(name) = data
            .attr(LocalName::from("name"))
            .filter(|s| !s.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        match tag.as_str() {
            "input" => {
                let input_type = data
                    .attr(LocalName::from("type"))
                    .unwrap_or("text")
                    .to_ascii_lowercase();
                match input_type.as_str() {
                    "checkbox" | "radio" => {
                        let is_checked = data
                            .checkbox_input_checked()
                            .unwrap_or_else(|| data.attr(LocalName::from("checked")).is_some());
                        if is_checked {
                            let val = data
                                .attr(LocalName::from("value"))
                                .unwrap_or("on")
                                .to_string();
                            pairs.push((name, val));
                        }
                    }
                    "submit" | "button" | "reset" | "image" | "file" => {}
                    _ => {
                        pairs.push((name, read_live_control_value(&ctx.doc, nid)));
                    }
                }
            }
            "select" | "textarea" => {
                pairs.push((name, read_live_control_value(&ctx.doc, nid)));
            }
            _ => {}
        }
    }
    write_json(ctx, &serde_json::json!(pairs))
}

/// Fill `ctx.scratch_bytes` with `len` CSPRNG bytes (`crypto.getRandomValues`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_crypto_random_bytes(ctx: *mut c_void, len: usize) -> c_int {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return -1;
    };
    if len > 65_536 {
        return -1;
    }
    match crate::webauthn::random::bytes(len) {
        Ok(bytes) => {
            ctx.scratch_bytes = bytes;
            ctx.scratch_bytes.len() as c_int
        }
        Err(_) => -1,
    }
}

/// Record whether the currently-dispatching JS event invoked `event.preventDefault()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_dom_set_event_default_prevented(
    ctx: *mut c_void,
    prevented: c_int,
) {
    let Some(ctx) = (unsafe { as_ctx(ctx) }) else {
        return;
    };
    ctx.last_event_default_prevented = prevented != 0;
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
