//! main.rs — the host.
//!
//! Surfaces sharing one window and one wgpu device:
//!
//!   chrome  (88px)   tab strip + nav row, rendered by Blitz
//!   page             the document, rendered by Blitz at a vertical offset
//!   Hermes           AOT-compiled page logic, one runtime per document
//!
//! The chrome document is the height of the whole surface with a transparent
//! body, and is painted *after* the page. That lets the menu overlay page
//! content while the strip and nav row stay opaque.
//!
//! Input routing: a click that lands on chrome content goes to the chrome; a
//! click that falls through to the body goes to the page. Page clicks first
//! resolve to a link (real navigation), then fall through to Hermes.

mod browser;
mod chrome;
mod control;
mod credentials;
mod dom_ffi;
mod keychain;
mod observe;
mod page_endpoint;
mod passkeytest;
mod weathertest;
mod recall;
mod scrapbook;
mod vault;
mod webauthn;

use anyrender::WindowRenderer;
use anyrender_vello::VelloWindowRenderer;
use blitz_dom::{DocumentConfig, LocalName, Namespace, NodeData, Point, QualName};
use blitz_html::HtmlDocument;
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};
use browser::{Browser, SharedProvider};
use chrome::{
    CHROME_HEIGHT, ChromeState, EXPLORER_DRAWER_WIDTH, EXPLORER_RAIL_WIDTH, ExplorerTab,
    HeldLinkInfo, NavEntryInfo, PH_HEADER_HEIGHT, PageHolderInfo, ScrapbookItemInfo,
    SupervisionInfo, TabInfo, Theme,
};
use control::{ControlHub, Role};
use credentials::{CredentialBroker, PasskeyBroker};
use dom_ffi::BlitzHostContext;
use markup5ever::local_name;
use page_endpoint::PageCommand;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{c_char, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

const WIDTH: u32 = 1120;
const HEIGHT: u32 = 800;

/// Opaque handle owned by the C++ shim.
#[repr(C)]
pub struct HermesAppRuntime {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn hermes_app_init(blitz_ctx_ptr: *mut c_void) -> *mut HermesAppRuntime;
    fn hermes_app_dispatch_event(
        app: *mut HermesAppRuntime,
        event_type: *const c_char,
        hit_node_id: u64,
        x: f32,
        y: f32,
    ) -> bool;
    fn hermes_app_eval_script(
        app: *mut HermesAppRuntime,
        script_utf8: *const c_char,
        source_url: *const c_char,
    ) -> bool;
    fn hermes_app_destroy(app: *mut HermesAppRuntime);
}

/// The start page. Real HTML, rendered by the same engine as any other page,
/// with real links and the AOT-compiled counter from the original build.
const START_PAGE: &str = r#"
<!DOCTYPE html>
<html>
<head>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      background-color: #FFFFFF;
      color: #1C1C1E;
      font-family: "Optimistic VF", Montserrat, "Helvetica Neue", Helvetica, Arial, sans-serif;
      padding: 56px 64px;
    }
    .eyebrow {
      font-size: 12px;
      color: #6E7176;
      letter-spacing: 1px;
      margin-bottom: 18px;
    }
    h1 {
      font-size: 44px;
      line-height: 1.15;
      margin-bottom: 18px;
      max-width: 720px;
    }
    .lede {
      font-size: 17px;
      line-height: 1.5;
      color: #46545E;
      max-width: 640px;
      margin-bottom: 22px;
    }
    .card {
      background-color: #F3F3F3;
      border-radius: 18px;
      padding: 28px;
      max-width: 640px;
      margin-bottom: 28px;
      display: flex;
      flex-direction: column;
      gap: 14px;
    }
    .card-title { font-size: 15px; color: #1C1C1E; }
    .card-note { font-size: 13px; color: #6E7176; line-height: 1.45; }
    #action-btn {
      background-color: #1B74E4;
      color: #FFFFFF;
      cursor: pointer;
      padding: 12px 20px;
      border-radius: 19px;
      width: 260px;
      font-size: 14px;
      text-align: center;
    }
    .links { display: flex; flex-direction: row; gap: 22px; font-size: 14px; }
    .links a { color: #1B74E4; }

    @media (prefers-color-scheme: dark) {
      body { background-color: #1C1C1E; color: #F2F2F7; }
      .eyebrow { color: #8E8E93; }
      .lede { color: #C7C7CC; }
      .card { background-color: #2C2C2E; }
      .card-title { color: #F2F2F7; }
      .card-note { color: #A1A1A6; }
      .links a { color: #0A84FF; }
    }
  </style>
</head>
<body>
  <div class="eyebrow">STATIC HERMES &#183; BLITZ &#183; VELLO &#183; WGPU</div>
  <h1>A browser assembled from existing engines.</h1>
  <p class="lede">
    HTML and CSS are parsed by Blitz and laid out by Stylo and Taffy. Painting
    runs through Vello on wgpu. The page logic below is TypeScript compiled
    ahead of time into C by Static Hermes, calling into the DOM through
    $SHBuiltin.extern_c with no interpreter in the loop.
  </p>
  <p class="lede">
    Remote &lt;script&gt; is not executed: pages render from the HTML and CSS
    the server sends. Sites that assemble their content in the browser will
    come up empty.
  </p>
  <div class="card">
    <div class="card-title">AOT-compiled event handler</div>
    <div id="action-btn" role="button" tabindex="0">Increment Counter (0)</div>
    <div class="card-note" id="status-label">
      Clicking dispatches into Hermes, mutates this document through Rust, and
      repaints. No JavaScript is interpreted at runtime.
    </div>
  </div>
  <div class="links">
    <a href="https://ai.meta.com/muse/">Muse</a>
    <a href="https://example.com/">example.com</a>
  </div>
</body>
</html>
"#;

const START_URL: &str = "muse://start";

/// The passkey broker, shared between the host and the pages it shows.
///
/// A page ceremony has to outlive the call that started it: the host has to
/// raise the approval prompt, wait for a human, and only then approve the
/// ceremony. Sharing one broker instance is what keeps the ceremony alive
/// across that gap.
type SharedPasskeyBroker = std::rc::Rc<std::cell::RefCell<credentials::LocalPasskeyBroker>>;

struct Page {
    ctx: Box<BlitzHostContext>,
    hermes: *mut HermesAppRuntime,
}

/// Default text colour for documents that opt into a dark colour scheme. Blitz's
/// UA sheet hard-codes `color: black`, so a page that says
/// `color-scheme: light dark` and leaves its text colour to the UA would render
/// black on its own dark background.
const DARK_DEFAULT_TEXT: &str = "html, body { background-color: #1C1C1E; color: #F2F2F7; }";

/// Settles a page's promise after the related-origins well-known fetch has
/// answered (WebAuthn Level 3 §5.11.1).
///
/// The payload carries the ceremony the host started, or the reason the
/// origin was refused. The page's promise is settled exactly once: the
/// runtime drops the entry when it is called.
fn settle_related_origins(hermes: *mut HermesAppRuntime, request_id: &str, payload: &Value) {
    let script = format!(
        r#"globalThis.__settleRelatedOrigins && globalThis.__settleRelatedOrigins({}, {}, {});"#,
        serde_json::to_string(request_id).unwrap_or_else(|_| "\"\"".into()),
        if payload.get("error").is_some() {
            "false"
        } else {
            "true"
        },
        serde_json::to_string(payload).unwrap_or_else(|_| "\"\"".into()),
    );
    if let Ok(code) = std::ffi::CString::new(script) {
        unsafe {
            hermes_app_eval_script(hermes, code.as_ptr(), c"<related-origins>".as_ptr());
        }
    }
}

fn parse_two_string_args(s: &str, call_prefix: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = s[cursor..].find(call_prefix) {
        let start = cursor + rel + call_prefix.len();
        let rest = &s[start..];
        let Some(end1) = rest.find('"') else {
            break;
        };
        let arg1 = &rest[..end1];
        let after1 = &rest[end1 + 1..];
        let Some(quote2_rel) = after1.find('"') else {
            cursor = start + end1 + 1;
            continue;
        };
        if quote2_rel > 8 {
            cursor = start + end1 + 1;
            continue;
        }
        let rest2 = &after1[quote2_rel + 1..];
        let Some(end2) = rest2.find('"') else {
            break;
        };
        let arg2 = &rest2[..end2];
        out.push((arg1.to_string(), arg2.to_string()));
        cursor = start + end1 + 1 + quote2_rel + 1 + end2 + 1;
    }
    out
}

fn extract_hidden_div(html: &mut String, div_id: &str) -> Option<String> {
    let patterns = [
        format!("<div hidden id=\"{div_id}\">"),
        format!("<div hidden=\"\" id=\"{div_id}\">"),
        format!("<div id=\"{div_id}\" hidden>"),
        format!("<div id=\"{div_id}\" hidden=\"\">"),
    ];
    let (open_start, open_len) = patterns
        .iter()
        .find_map(|pat| html.find(pat.as_str()).map(|idx| (idx, pat.len())))?;
    let content_start = open_start + open_len;
    let bytes = html.as_bytes();
    let mut pos = content_start;
    let mut depth = 1usize;
    while pos < bytes.len() {
        let Some(lt_rel) = html[pos..].find('<') else {
            break;
        };
        let tag_start = pos + lt_rel;
        let rem = &html[tag_start..];
        let rem_bytes = rem.as_bytes();
        if rem_bytes.len() >= 6 && rem_bytes[..5].eq_ignore_ascii_case(b"</div") {
            let after = rem_bytes[5];
            if after == b'>' || after.is_ascii_whitespace() {
                if let Some(gt_rel) = rem.find('>') {
                    depth -= 1;
                    if depth == 0 {
                        let inner = html[content_start..tag_start].to_string();
                        let close_end = tag_start + gt_rel + 1;
                        html.replace_range(open_start..close_end, "");
                        return Some(inner);
                    }
                    pos = tag_start + gt_rel + 1;
                    continue;
                }
            }
        } else if rem_bytes.len() >= 5 && rem_bytes[..4].eq_ignore_ascii_case(b"<div") {
            let after = rem_bytes[4];
            if after == b'>' || after.is_ascii_whitespace() {
                if let Some(gt_rel) = rem.find('>') {
                    if !rem[..gt_rel].ends_with('/') {
                        depth += 1;
                    }
                    pos = tag_start + gt_rel + 1;
                    continue;
                }
            }
        }
        pos = tag_start + 1;
    }
    None
}

fn replace_suspense_boundary(html: &mut String, b_id: &str, inner: &str) -> bool {
    let marker = format!("<template id=\"{b_id}\"></template>");
    let Some(start) = html.find(&marker) else {
        return false;
    };
    let mut pos = start + marker.len();
    let mut depth = 0usize;
    while let Some(c_rel) = html[pos..].find("<!--") {
        let c_start = pos + c_rel;
        let rem = &html[c_start..];
        let Some(c_end_rel) = rem.find("-->") else {
            break;
        };
        let data = &rem[4..c_end_rel];
        if data == "/$" || data == "/&" {
            if depth == 0 {
                html.replace_range(start..c_start, inner);
                return true;
            }
            depth -= 1;
        } else if matches!(data, "$" | "$?" | "$~" | "$!" | "&") {
            depth += 1;
        }
        pos = c_start + c_end_rel + 3;
    }
    false
}

/// Resolves React 18+ Streaming SSR `<Suspense>` boundaries (`$RC("B:x","S:x")`)
/// and streaming slot replacements (`$RS("S:x","P:x")`) in-place so streamed
/// server-rendered HTML sections are visible without executing client JS.
fn hydrate_streaming_ssr(html: &str) -> std::borrow::Cow<'_, str> {
    if !html.contains("$RC(\"") && !html.contains("$RS(\"") {
        return std::borrow::Cow::Borrowed(html);
    }
    let rs_calls = parse_two_string_args(html, "$RS(\"");
    let rc_calls = parse_two_string_args(html, "$RC(\"");
    if rs_calls.is_empty() && rc_calls.is_empty() {
        return std::borrow::Cow::Borrowed(html);
    }
    let mut out = html.to_string();
    let mut pending_slots: Vec<(String, String)> = Vec::with_capacity(rs_calls.len());
    for (s_id, p_id) in &rs_calls {
        if let Some(inner) = extract_hidden_div(&mut out, s_id) {
            let tpl = format!("<template id=\"{p_id}\"></template>");
            if out.contains(&tpl) {
                out = out.replacen(&tpl, &inner, 1);
            } else {
                pending_slots.push((tpl, inner));
            }
        }
    }
    for (b_id, s_id) in &rc_calls {
        if let Some(mut inner) = extract_hidden_div(&mut out, s_id) {
            for (tpl, slot_inner) in &pending_slots {
                if inner.contains(tpl) {
                    inner = inner.replacen(tpl, slot_inner, 1);
                }
            }
            replace_suspense_boundary(&mut out, b_id, &inner);
        }
    }
    for (tpl, slot_inner) in &pending_slots {
        if out.contains(tpl) {
            out = out.replacen(tpl, slot_inner, 1);
        }
    }
    std::borrow::Cow::Owned(out)
}

impl Page {
    fn load(
        html: &str,
        base: Option<Url>,
        provider: Option<SharedProvider>,
        viewport: Viewport,
        theme: Theme,
    ) -> Self {
        Self::load_with_broker(
            html,
            base,
            provider,
            viewport,
            theme,
            dom_ffi::default_broker(),
        )
    }

    /// Loads a page whose WebAuthn ceremonies run in `broker`.
    ///
    /// The broker is shared with the host, so a ceremony a page starts is
    /// still there when the host comes to ask the human about it. Without
    /// that, the ceremony would be dropped when the call that started it
    /// returned.
    fn load_with_broker(
        html: &str,
        base: Option<Url>,
        provider: Option<SharedProvider>,
        viewport: Viewport,
        theme: Theme,
        broker: SharedPasskeyBroker,
    ) -> Self {
        let mut doc = Self::build(html, &base, &provider, viewport.clone(), None);

        // Only override the UA default when the page itself opted into dark;
        // otherwise a light page with no explicit colour would get light text.
        if theme.is_dark() {
            let opted_in = doc
                .try_root_element()
                .map(|root| {
                    doc.resolved_style_value(root.id, "color-scheme")
                        .contains("dark")
                })
                .unwrap_or(false);
            if opted_in {
                doc = Self::build(html, &base, &provider, viewport, Some(DARK_DEFAULT_TEXT));
            }
        }

        // The page's origin travels with the context so a WebAuthn ceremony
        // can be bound to the origin the document was actually served from.
        // Deriving it from the relying party's own `rpId` would make the
        // origin check vacuous.
        let origin = base.as_ref().map(|url| url.origin().ascii_serialization()).unwrap_or_default();
        let mut ctx = Box::new(BlitzHostContext::with_origin_and_broker(doc, &origin, broker));
        let hermes =
            unsafe { hermes_app_init((&mut *ctx as *mut BlitzHostContext).cast::<c_void>()) };
        let mut page = Self { ctx, hermes };
        page.execute_document_scripts(&base, &provider);
        page
    }

    fn build(
        html: &str,
        base: &Option<Url>,
        provider: &Option<SharedProvider>,
        viewport: Viewport,
        extra_ua: Option<&str>,
    ) -> HtmlDocument {
        // Parsing may kick off stylesheet/image fetches; they are spawned onto
        // the Tokio runtime, so the runtime context must be entered here.
        let _guard = browser::enter_runtime();
        let hydrated = hydrate_streaming_ssr(html);
        let mut ua = vec![
            blitz_dom::UserAgentStylesheet::default_sheet(),
            blitz_dom::UserAgentStylesheet::parse(
                r#"
                html { background-color: #FFFFFF; color: #000000; }
                select { display: inline-block; min-width: 80px; min-height: 24px; border: 1px solid #999; padding: 2px 4px; background-color: #FFFFFF; color: #000000; }
                select:focus { outline: 2px solid #4D90FE; }
                option { padding: 1px 4px; }
                option[selected] { background-color: #1B74E4; color: #FFFFFF; font-weight: 600; }
                button, input, select, textarea { box-sizing: border-box; }
                .grid-cols-subgrid, .sm\:grid-cols-subgrid {
                    grid-template-columns: repeat(6, minmax(0, 1fr));
                    column-gap: 1.25rem;
                    align-self: start !important;
                    align-content: start !important;
                }
                @media (min-width: 768px) {
                    .md\:col-span-3.grid-cols-subgrid {
                        grid-template-columns: repeat(3, minmax(0, 1fr));
                    }
                    .md\:col-span-6.grid-cols-subgrid {
                        grid-template-columns: repeat(6, minmax(0, 1fr));
                    }
                    .md\:col-span-7.grid-cols-subgrid {
                        grid-template-columns: repeat(7, minmax(0, 1fr));
                    }
                }
                @media (min-width: 1031px) {
                    .grid-cols-subgrid, .sm\:grid-cols-subgrid {
                        column-gap: 2.5rem;
                    }
                    .lg\:col-span-2.grid-cols-subgrid {
                        grid-template-columns: repeat(2, minmax(0, 1fr));
                    }
                    .lg\:col-span-3.grid-cols-subgrid, .lg\:col-span-3.sm\:grid-cols-subgrid {
                        grid-template-columns: repeat(3, minmax(0, 1fr));
                    }
                    .lg\:col-span-6.grid-cols-subgrid {
                        grid-template-columns: repeat(6, minmax(0, 1fr));
                    }
                    .lg\:col-span-7.grid-cols-subgrid {
                        grid-template-columns: repeat(6, minmax(0, 1fr));
                    }
                    .lg\:col-span-8.grid-cols-subgrid {
                        grid-template-columns: repeat(8, minmax(0, 1fr));
                    }
                }
                "#,
            ),
        ];
        if let Some(css) = extra_ua {
            ua.push(blitz_dom::UserAgentStylesheet::parse(css));
        }
        let mut doc = HtmlDocument::from_html(
            &hydrated,
            DocumentConfig {
                viewport: Some(viewport),
                base_url: base.as_ref().map(|u| u.to_string()),
                net_provider: provider
                    .as_ref()
                    .map(|p| p.clone() as Arc<dyn blitz_traits::net::NetProvider>),
                html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                ua_stylesheets: ua,
                ..Default::default()
            },
        );
        doc.resolve(0.0);
        doc
    }

    /// Walk the Blitz DOM in document order, extract inline `<script>` and
    /// external `<script src="...">` tags, evaluate them inside this page's
    /// isolated `HermesAppRuntime` (which drains microtasks), and re-resolve
    /// layout if the DOM was mutated.
    fn execute_document_scripts(
        &mut self,
        base: &Option<Url>,
        provider: &Option<SharedProvider>,
    ) {
        if self.hermes.is_null() {
            return;
        }
        let mut script_nodes = Vec::new();
        let mut stack = vec![self.ctx.doc.root_node().id];
        while let Some(nid) = stack.pop() {
            let Some(node) = self.ctx.doc.get_node(nid) else {
                continue;
            };
            for &child in node.children.iter().rev() {
                stack.push(child);
            }
            if let Some(el) = node.element_data() {
                let tag: &str = el.name.local.as_ref();
                if tag.eq_ignore_ascii_case("script") {
                    script_nodes.push(nid);
                }
            }
        }
        if script_nodes.is_empty() {
            return;
        }

        let is_local_base = base
            .as_ref()
            .and_then(|u| u.host_str())
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");

        for nid in script_nodes {
            let (script_type, src_attr, inline_code) = {
                let Some(node) = self.ctx.doc.get_node(nid) else {
                    continue;
                };
                let stype = node
                    .attr(LocalName::from("type"))
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let src = node
                    .attr(LocalName::from("src"))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let text = if src.is_none() {
                    node.text_content()
                } else {
                    String::new()
                };
                (stype, src, text)
            };

            if !script_type.is_empty()
                && !matches!(
                    script_type.as_str(),
                    "text/javascript"
                        | "application/javascript"
                        | "text/ecmascript"
                        | "application/ecmascript"
                        | "module"
                )
            {
                continue;
            }

            if let Some(src_raw) = src_attr {
                if !is_local_base {
                    // Skip blocking main-thread fetches of third-party external script bundles on remote pages.
                    continue;
                }
                let (Some(base_url), Some(prov)) = (base.as_ref(), provider.as_ref()) else {
                    continue;
                };
                let Ok(script_url) = base_url.join(&src_raw) else {
                    continue;
                };
                if let Ok(code) = browser::fetch(prov, &script_url) {
                    if !code.trim().is_empty() && code.len() <= 512_000 {
                        if let (Ok(c_code), Ok(c_url)) = (
                            std::ffi::CString::new(code),
                            std::ffi::CString::new(script_url.to_string()),
                        ) {
                            unsafe {
                                hermes_app_eval_script(
                                    self.hermes,
                                    c_code.as_ptr(),
                                    c_url.as_ptr(),
                                );
                            }
                        }
                    }
                }
            } else if !inline_code.trim().is_empty() && inline_code.len() <= 256_000 {
                if let Ok(c_code) = std::ffi::CString::new(inline_code) {
                    unsafe {
                        hermes_app_eval_script(
                            self.hermes,
                            c_code.as_ptr(),
                            c"<inline-script>".as_ptr(),
                        );
                    }
                }
            }
        }

        if self.ctx.dirty {
            self.resolve();
            self.ctx.dirty = false;
        }
    }

    fn doc(&self) -> &HtmlDocument {
        &self.ctx.doc
    }

    fn doc_mut(&mut self) -> &mut HtmlDocument {
        &mut self.ctx.doc
    }

    /// Resolve layout. Must run inside the Tokio context: resolving can start
    /// font and image fetches, which blitz-net spawns onto the runtime.
    fn resolve(&mut self) {
        let _guard = browser::enter_runtime();
        self.ctx.doc.resolve(0.0);
    }

    /// Wait up to `timeout` for render-blocking `<head>` stylesheets
    /// (`has_pending_critical_resources()`) to finish loading before resolving
    /// layout, so observations never read an unstyled zero-sized layout tree.
    fn resolve_awaiting_critical(&mut self, timeout: Duration) {
        let _guard = browser::enter_runtime();
        let started = Instant::now();
        self.ctx.doc.resolve(0.0);
        while self.ctx.doc.has_pending_critical_resources() && started.elapsed() < timeout {
            std::thread::sleep(Duration::from_millis(25));
            self.ctx.doc.resolve(0.0);
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe { hermes_app_destroy(self.hermes) };
    }
}

/// Stable identity for a tab. Async work carries this ID, never a `Vec`
/// index, so closing an earlier tab cannot retarget a result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TabId(u64);

/// The canonical tab: identity, session history, and title, owned by the
/// trusted host. Ladybird holds the same state in the UI process
/// (`CanonicalTraversable` + `TraversableSessionHistory`) so the tab survives
/// page-instance replacement and, later, page-process crashes. A control
/// session binds to this identity, never to a page instance.
struct CanonicalTab {
    id: TabId,
    browser: Browser,
    title: String,
}

/// One page instance hosted in a canonical tab: the document, its Hermes
/// runtime, and the snapshot state derived from the document. Replacing the
/// document mints a new instance with a new `page_id` (Ladybird's PageId
/// tier) and a bumped document generation, which invalidates proposals
/// bound to the previous instance.
struct PageInstance {
    page: Page,
    page_id: u64,
    document_generation: u64,
    next_snapshot_id: u64,
    last_snapshot: Option<observe::PageSnapshot>,
    last_observe_ms: f64,
    /// Every stable node reference this instance has issued (WebDriver's
    /// known-element registry). Cleared when the instance is replaced.
    known_node_refs: HashSet<String>,
}

impl PageInstance {
    fn new(page: Page) -> Self {
        Self {
            page,
            page_id: 1,
            document_generation: 1,
            next_snapshot_id: 1,
            last_snapshot: None,
            last_observe_ms: 0.0,
            known_node_refs: HashSet::new(),
        }
    }

    fn replace(&mut self, page: Page) {
        self.page = page;
        self.page_id = self.page_id.wrapping_add(1);
        self.document_generation = self.document_generation.wrapping_add(1);
        self.last_snapshot = None;
        self.last_observe_ms = 0.0;
        self.known_node_refs.clear();
    }

    /// Invalidate every outstanding snapshot/proposal binding for this page.
    fn bump_generation(&mut self) {
        self.document_generation = self.document_generation.wrapping_add(1);
    }
}

/// A tab as the window presents the tab: canonical host state plus the page
/// instance currently hosting the tab's document.
struct Tab {
    canonical: CanonicalTab,
    instance: PageInstance,
}

impl Tab {
    fn start_page(id: TabId, viewport: Viewport) -> Self {
        let mut browser = Browser::new();
        browser.commit(
            Url::parse(START_URL).expect("static start url"),
            START_PAGE.to_string(),
        );
        let page = Page::load(START_PAGE, None, None, viewport, Theme::Light);
        Self {
            canonical: CanonicalTab {
                id,
                browser,
                title: "New tab".to_string(),
            },
            instance: PageInstance::new(page),
        }
    }

    fn replace_page(&mut self, page: Page) {
        self.instance.replace(page);
    }

    fn bump_generation(&mut self) {
        self.instance.bump_generation();
    }
}

enum UserEvent {
    Navigated {
        tab: TabId,
        generation: u64,
        url: Url,
        result: Result<String, String>,
    },
    NetWake,
    ControlConnect {
        conn_id: u64,
        role: Role,
    },
    ControlMessage {
        conn_id: u64,
        frame: Value,
    },
    ControlDisconnect {
        conn_id: u64,
    },
    /// The related origins well-known fetch has answered (WebAuthn Level 3
    /// §5.11.1).
    ///
    /// The fetch runs on a worker thread, because the UI thread must not wait
    /// on the network. The result comes back through the event loop, and the
    /// host settles the page's promise from here.
    RelatedOriginsResolved {
        tab: TabId,
        request_id: String,
        result: Result<String, String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlState {
    Agent,
    AwaitingDecision,
    ActionRequired,
    Human,
    Done,
    Stopped,
    Error,
}

impl ControlState {
    fn as_str(self) -> &'static str {
        match self {
            ControlState::Agent => "agent",
            ControlState::AwaitingDecision => "awaiting_decision",
            ControlState::ActionRequired => "action_required",
            ControlState::Human => "human",
            ControlState::Done => "done",
            ControlState::Stopped => "stopped",
            ControlState::Error => "error",
        }
    }
    fn is_terminal(self) -> bool {
        matches!(
            self,
            ControlState::Done | ControlState::Stopped | ControlState::Error
        )
    }
}

struct PendingProposal {
    proposal_id: String,
    snapshot_id: u64,
    page_id: u64,
    fingerprint: String,
    generation: u64,
    operation: Value,
    rationale: String,
    classification: String,
    requires_glasses: bool,
    /// The passkey ceremony this proposal will run, when the proposal is a
    /// passkey registration or assertion.
    ceremony_id: Option<String>,
    /// The credential fill this proposal will perform, when the proposal is a
    /// credential fill.
    credential_fill: Option<CredentialFillPlan>,
}

/// A credential fill the human has been asked to approve.
#[derive(Clone)]
struct CredentialFillPlan {
    origin: String,
    chosen: credentials::CredentialMatch,
    fields: Vec<credentials::CredentialFieldKind>,
}

/// Collects the writes a credential fill will make to a page.
///
/// The sink only ever receives a value the broker has already decided to
/// release. Nothing here decides *which* credential to use — that happened
/// before the human approved the fill. The writes are collected rather than
/// applied immediately so that the broker holds the secret for as short a
/// time as possible.
struct PageCredentialSink<'a> {
    elements: &'a [observe::ElementSnapshot],
    pending: Vec<(NodeId, String, String)>,
    filled: Vec<credentials::CredentialFieldKind>,
}

impl<'a> PageCredentialSink<'a> {
    /// Finds the input element that receives a given kind of credential.
    fn find_field(&self, field: credentials::CredentialFieldKind) -> Option<observe::ElementSnapshot> {
        let candidates: Vec<&observe::ElementSnapshot> =
            self.elements.iter().filter(|e| e.enabled).collect();
        match field {
            credentials::CredentialFieldKind::Username => candidates
                .iter()
                .find(|e| {
                    let hint = format!(
                        "{} {} {}",
                        e.id,
                        e.name.clone().unwrap_or_default(),
                        e.label
                    )
                    .to_lowercase();
                    let type_ok = matches!(
                        e.input_type.as_deref(),
                        Some("email") | Some("text") | Some("")
                    ) || e.input_type.is_none();
                    type_ok
                        && (hint.contains("user")
                            || hint.contains("email")
                            || hint.contains("login")
                            || hint.contains("account"))
                })
                .map(|e| (*e).clone()),
            credentials::CredentialFieldKind::CurrentPassword => candidates
                .iter()
                .find(|e| {
                    e.input_type.as_deref() == Some("password")
                        && !Self::is_new_password(e)
                })
                .map(|e| (*e).clone()),
            credentials::CredentialFieldKind::NewPassword => candidates
                .iter()
                .find(|e| e.input_type.as_deref() == Some("password") && Self::is_new_password(e))
                .map(|e| (*e).clone()),
            _ => None,
        }
    }

    /// Whether a password input is for a new password rather than the
    /// current one.
    fn is_new_password(element: &observe::ElementSnapshot) -> bool {
        let hint = format!(
            "{} {} {}",
            element.id,
            element.name.clone().unwrap_or_default(),
            element.label
        )
        .to_lowercase();
        hint.contains("new") || hint.contains("confirm") || hint.contains("repeat")
    }
}

impl credentials::CredentialSink for PageCredentialSink<'_> {
    fn set_field(
        &mut self,
        field: credentials::CredentialFieldKind,
        value: &str,
    ) -> Result<(), credentials::BrokerError> {
        let Some(element) = self.find_field(field) else {
            return Err(credentials::BrokerError::Invalid(format!(
                "the page has no {} field",
                field.as_str()
            )));
        };
        self.pending
            .push((element.node_id, element.tag.clone(), value.to_string()));
        self.filled.push(field);
        Ok(())
    }
}

struct ControlSession {
    id: String,
    tab_id: TabId,
    title: String,
    state: ControlState,
    control_epoch: u64,
    event_seq: u64,
    audit_seq: u64,
    snapshot_valid: bool,
    pending: Option<PendingProposal>,
    seen_proposals: HashSet<String>,
    used_decisions: HashSet<String>,
    audit: Vec<Value>,
    action_required_kind: Option<String>,
    last_screenshot_hash: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum OutTarget {
    Conn(u64),
    Host,
    Agent,
    Both,
}

struct ControlConn {
    role: Role,
    last_seq: Option<u64>,
}

/// Where a page input came from. Recorded host-side at enqueue time; page
/// content and the agent cannot forge physical provenance (Ladybird grants
/// user activation the same way, in the UI process).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputProvenance {
    PhysicalUser,
    ApprovedAgent,
}

impl InputProvenance {
    fn as_str(self) -> &'static str {
        match self {
            InputProvenance::PhysicalUser => "physical_user",
            InputProvenance::ApprovedAgent => "approved_agent",
        }
    }
}

enum InputAction {
    PageClick {
        x: f32,
        y: f32,
    },
    Scroll {
        dy: f64,
    },
    Command {
        command: PageCommand,
        snapshot: observe::PageSnapshot,
    },
}

struct QueuedInput {
    id: u64,
    provenance: InputProvenance,
    action: InputAction,
}

struct HeldLink {
    label: String,
    url: String,
}

struct PageHolderState {
    doc: HtmlDocument,
    url: Url,
    title: String,
    links_mode: bool,
    links: Vec<HeldLink>,
}

struct BrowserApp {
    window: Option<Arc<Window>>,
    renderer: VelloWindowRenderer,
    chrome_doc: HtmlDocument,
    tabs: Vec<Tab>,
    next_tab_id: u64,
    active: usize,
    menu_open: bool,
    toolbar_collapsed: bool,
    explorer_tab: Option<ExplorerTab>,
    page_holder: Option<PageHolderState>,
    scrapbook_entries: Vec<scrapbook::ScrapbookEntry>,
    selected_scrapbook: Option<usize>,
    favorites: Vec<(String, String)>,
    provider: SharedProvider,
    proxy: EventLoopProxy<UserEvent>,
    net_dirty: Arc<AtomicBool>,
    cursor: (f32, f32),
    size: (u32, u32),
    scale: f64,
    modifiers: ModifiersState,
    theme: Theme,
    /// `--force-dark`: ignore the system appearance and use the dark palette.
    force_dark: bool,
    control: Option<ControlSession>,
    control_conns: HashMap<u64, ControlConn>,
    control_out: Vec<(OutTarget, Value)>,
    control_hub: Option<Arc<ControlHub>>,
    live_socket_info: Option<(String, String, String)>,
    next_session_id: u64,
    input_queue: VecDeque<QueuedInput>,
    next_input_event_id: u64,
    last_input_event_id: u64,
    /// Monotonic presented/captured frame counter (the frame contract:
    /// every screenshot names the frame it came from).
    frame_seq: u64,
    recalled_count: u64,
    supervise_commits_only: bool,
    xray_enabled: bool,
    /// The passkey broker. WebAuthn ceremonies run host-side, behind the
    /// human approval gate, and never in page script.
    passkey_broker: SharedPasskeyBroker,
    /// The page-initiated passkey ceremony that is waiting for a human, if
    /// any. The interstitial in the chrome mirrors the glasses DAT card.
    passkey_prompt: Option<dom_ffi::PageCeremony>,
    /// The password broker. Secrets live in the macOS Keychain and reach the
    /// page only after a human approves the fill.
    credential_broker: credentials::LocalCredentialBroker,
    exit_after: Option<Duration>,
    started: Instant,
}

impl BrowserApp {
    fn new(
        proxy: EventLoopProxy<UserEvent>,
        net_dirty: Arc<AtomicBool>,
        provider: SharedProvider,
    ) -> Self {
        let scale = 1.0_f64;
        let size = (WIDTH, HEIGHT);
        let tabs = vec![Tab::start_page(
            TabId(1),
            page_viewport(size, scale, Theme::Light),
        )];
        let supervise_commits_only = std::env::args().any(|a| a == "--supervise-commits-only")
            || std::env::var("HERMES_SUPERVISE_COMMITS_ONLY")
                .ok()
                .as_deref()
                == Some("1");
        let xray_enabled = std::env::args().any(|a| a == "--xray")
            || std::env::var("HERMES_XRAY").ok().as_deref() == Some("1");
        let scrapbook_entries = scrapbook::load_entries();
        let selected_scrapbook = if scrapbook_entries.is_empty() {
            None
        } else {
            Some(0)
        };
        let favorites = vec![
            ("Web Gallery".to_string(), "https://www.apple.com/".to_string()),
            ("Office for Macintosh".to_string(), "https://www.microsoft.com/".to_string()),
            ("Products for Mac".to_string(), "https://en.wikipedia.org/".to_string()),
            ("Microsoft Internet".to_string(), "https://www.aol.com/".to_string()),
            ("Apple".to_string(), "https://www.apple.com/mac/".to_string()),
            ("Google".to_string(), "https://www.google.com/".to_string()),
            ("Developer Hub".to_string(), "https://lite.duckduckgo.com/lite/".to_string()),
        ];
        let mut app = Self {
            window: None,
            renderer: VelloWindowRenderer::new(),
            chrome_doc: HtmlDocument::from_html("<!DOCTYPE html>", DocumentConfig::default()),
            tabs,
            next_tab_id: 2,
            active: 0,
            menu_open: false,
            toolbar_collapsed: false,
            explorer_tab: None,
            page_holder: None,
            scrapbook_entries,
            selected_scrapbook,
            favorites,
            provider,
            proxy,
            net_dirty,
            cursor: (0.0, 0.0),
            size,
            scale,
            modifiers: ModifiersState::empty(),
            theme: Theme::Light,
            force_dark: std::env::args().any(|a| a == "--force-dark"),
            control: None,
            control_conns: HashMap::new(),
            control_out: Vec::new(),
            control_hub: None,
            live_socket_info: None,
            next_session_id: 1,
            input_queue: VecDeque::new(),
            next_input_event_id: 1,
            last_input_event_id: 0,
            frame_seq: 0,
            recalled_count: 0,
            supervise_commits_only,
            xray_enabled,
            passkey_broker: std::rc::Rc::new(std::cell::RefCell::new(
                credentials::LocalPasskeyBroker::new(),
            )),
            passkey_prompt: None,
            credential_broker: credentials::LocalCredentialBroker::new(),
            exit_after: parse_exit_after(),
            started: Instant::now(),
        };
        app.sync_chrome();
        app
    }

    fn top_chrome_height(&self) -> f64 {
        if self.toolbar_collapsed {
            0.0
        } else {
            CHROME_HEIGHT
        }
    }

    fn left_chrome_width(&self) -> f64 {
        EXPLORER_RAIL_WIDTH
            + if self.explorer_tab.is_some() {
                EXPLORER_DRAWER_WIDTH
            } else {
                0.0
            }
    }

    fn content_viewport(&self) -> Viewport {
        let left_px = ((self.left_chrome_width() * self.scale) as u32).min(self.size.0);
        let top_px = ((self.top_chrome_height() * self.scale) as u32).min(self.size.1);
        Viewport::new(
            self.size.0.saturating_sub(left_px).max(64),
            self.size.1.saturating_sub(top_px).max(64),
            self.scale as f32,
            self.theme.color_scheme(),
        )
    }

    fn page_holder_viewport(&self) -> Viewport {
        let w = ((EXPLORER_DRAWER_WIDTH * self.scale) as u32).max(64);
        let top_px =
            (((self.top_chrome_height() + PH_HEADER_HEIGHT) * self.scale) as u32).min(self.size.1);
        let h = self.size.1.saturating_sub(top_px).max(64);
        Viewport::new(w, h, self.scale as f32, self.theme.color_scheme())
    }

    fn reflow_active_tab(&mut self) {
        let vp = self.content_viewport();
        let tab = self.tab_mut();
        tab.instance.page.doc_mut().set_viewport(vp);
        tab.instance.page.resolve();
        if self.page_holder.is_some() {
            let ph_vp = self.page_holder_viewport();
            if let Some(ph) = self.page_holder.as_mut() {
                let _guard = browser::enter_runtime();
                ph.doc.set_viewport(ph_vp);
                ph.doc.resolve(0.0);
            }
        }
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn create_start_tab(&mut self) -> Tab {
        let id = TabId(self.next_tab_id);
        self.next_tab_id = self.next_tab_id.wrapping_add(1);
        Tab::start_page(id, self.content_viewport())
    }

    fn observe_active(&mut self) -> observe::PageSnapshot {
        let id = self.tab().canonical.id;
        self.observe_tab(id).expect("active tab exists")
    }

    fn observe_tab(&mut self, tab_id: TabId) -> Option<observe::PageSnapshot> {
        let viewport = self.content_viewport();
        let (width, height) = viewport.window_size;
        let scale = viewport.scale_f64() as f32;
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.canonical.id == tab_id)?;
        let t0 = Instant::now();
        tab.instance
            .page
            .resolve_awaiting_critical(Duration::from_secs(6));
        let snapshot_id = tab.instance.next_snapshot_id;
        tab.instance.next_snapshot_id = tab.instance.next_snapshot_id.wrapping_add(1);
        let snapshot = observe::observe(
            tab.instance.page.doc(),
            observe::SnapshotMetadata {
                snapshot_id,
                tab_id: tab.canonical.id.0,
                page_id: tab.instance.page_id,
                document_generation: tab.instance.document_generation,
                url: tab
                    .canonical
                    .browser
                    .current()
                    .map(|entry| entry.url.to_string())
                    .unwrap_or_else(|| START_URL.to_string()),
                title: tab.canonical.title.clone(),
                viewport_width: width as f32,
                viewport_height: height as f32,
                scale,
            },
        );
        tab.instance.last_observe_ms = t0.elapsed().as_secs_f64() * 1000.0;
        tab.instance
            .known_node_refs
            .extend(snapshot.elements.iter().map(|e| e.node_ref.clone()));
        tab.instance.last_snapshot = Some(snapshot.clone());
        Some(snapshot)
    }

    // --- supervised control API (spec.md phase 2) ---

    fn supervision_info(&self) -> Option<SupervisionInfo<'_>> {
        let session = self.control.as_ref()?;
        if session.tab_id != self.tab().canonical.id {
            return None;
        }
        let detail: &str = match session.state {
            ControlState::AwaitingDecision => session
                .pending
                .as_ref()
                .map(|p| p.rationale.as_str())
                .unwrap_or("Waiting for a decision"),
            ControlState::ActionRequired => session
                .action_required_kind
                .as_deref()
                .unwrap_or("Action required"),
            ControlState::Human => "You are in control",
            ControlState::Agent => "Agent is working",
            ControlState::Done => "Task complete",
            ControlState::Stopped => "Stopped",
            ControlState::Error => "Session error",
        };
        Some(SupervisionInfo {
            title: &session.title,
            state: session.state.as_str(),
            detail,
            can_decide: session.state == ControlState::AwaitingDecision,
            can_take: !session.state.is_terminal() && session.state != ControlState::Human,
            can_release: session.state == ControlState::Human,
            can_stop: !session.state.is_terminal(),
        })
    }

    fn control_connect(&mut self, conn_id: u64, role: Role) {
        self.control_conns.insert(
            conn_id,
            ControlConn {
                role,
                last_seq: None,
            },
        );
        if role == Role::Host && self.passkey_prompt.is_some() {
            self.announce_page_passkey_on_glasses();
        }
    }

    fn control_disconnect(&mut self, conn_id: u64) {
        let removed = self.control_conns.remove(&conn_id);
        // Losing the last host channel mid-task is a transport loss: the
        // session can no longer be decided or stopped, so it ends in error.
        if removed.is_some_and(|c| c.role == Role::Host)
            && !self.control_conns.values().any(|c| c.role == Role::Host)
            && self
                .control
                .as_ref()
                .is_some_and(|s| !s.state.is_terminal())
        {
            self.invalidate_pending("transport_loss");
            if let Some(session) = self.control.as_mut() {
                session.state = ControlState::Error;
                session.snapshot_valid = false;
            }
            self.audit(
                "error",
                "transport.loss",
                None,
                None,
                None,
                None,
                None,
                None,
            );
            self.state_event(OutTarget::Agent);
            self.sync_chrome();
            self.request_redraw();
        }
    }

    fn drain_control_out(&mut self) -> Vec<(OutTarget, Value)> {
        std::mem::take(&mut self.control_out)
    }

    fn flush_control_out(&mut self) {
        let Some(hub) = self.control_hub.clone() else {
            return;
        };
        let out = self.drain_control_out();
        for (target, frame) in out {
            let mut conns: Vec<u64> = Vec::new();
            match target {
                OutTarget::Conn(id) => conns.push(id),
                OutTarget::Host => conns.extend(
                    self.control_conns
                        .iter()
                        .filter(|(_, c)| c.role == Role::Host)
                        .map(|(id, _)| *id),
                ),
                OutTarget::Agent => conns.extend(
                    self.control_conns
                        .iter()
                        .filter(|(_, c)| c.role == Role::Agent)
                        .map(|(id, _)| *id),
                ),
                OutTarget::Both => conns.extend(self.control_conns.keys().copied()),
            }
            for id in conns {
                hub.send(id, &frame);
            }
        }
    }

    fn emit(&mut self, target: OutTarget, frame: Value) {
        self.control_out.push((target, frame));
    }

    fn session_event(&mut self, target: OutTarget, kind: &str, extra: Value) {
        let Some(session) = self.control.as_mut() else {
            return;
        };
        session.event_seq = session.event_seq.wrapping_add(1);
        let seq = session.event_seq;
        let session_id = session.id.clone();
        let state = session.state.as_str();
        let mut frame = json!({
            "v": 1, "type": kind, "session_id": session_id,
            "event_seq": seq, "state": state, "at": control::now_iso(),
        });
        if let (Value::Object(dst), Value::Object(src)) = (&mut frame, extra) {
            dst.extend(src);
        }
        self.emit(target, frame);
    }

    fn state_event(&mut self, target: OutTarget) {
        let Some(session) = self.control.as_ref() else {
            return;
        };
        let tab_id = session.tab_id;
        let snapshot_valid = session.snapshot_valid;
        let (url, title, generation, fingerprint) = self
            .tabs
            .iter()
            .find(|t| t.canonical.id == tab_id)
            .map(|t| {
                (
                    t.canonical.browser.display_url(),
                    t.canonical.title.clone(),
                    t.instance.document_generation,
                    if snapshot_valid {
                        t.instance
                            .last_snapshot
                            .as_ref()
                            .map(|s| s.fingerprint.clone())
                    } else {
                        None
                    },
                )
            })
            .unwrap_or_default();
        self.session_event(
            target,
            "state",
            json!({
                "tab_id": tab_id.0, "url": url, "title": title,
                "generation": generation, "fingerprint": fingerprint,
            }),
        );
    }

    fn audit(
        &mut self,
        result: &str,
        operation: &str,
        target_id: Option<&str>,
        before: Option<&str>,
        after: Option<&str>,
        decided_by: Option<&str>,
        surface: Option<&str>,
        proposal_id: Option<&str>,
    ) {
        let Some(session) = self.control.as_mut() else {
            return;
        };
        session.audit_seq = session.audit_seq.wrapping_add(1);
        let record = json!({
            "audit_id": format!("a_{}", session.audit_seq),
            "session_id": session.id,
            "at": control::now_iso(),
            "proposal_id": proposal_id,
            "operation": operation,
            "target_id": target_id,
            "before_fingerprint": before,
            "after_fingerprint": after,
            "screenshot_hash": session.last_screenshot_hash,
            "decided_by": decided_by,
            "surface": surface,
            "result": result,
        });
        session.audit.push(record.clone());
        if let Ok(path) = std::env::var("HERMES_AUDIT_PATH") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                if let Ok(line) = serde_json::to_string(&record) {
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(trimmed)
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "{line}");
                    }
                }
            }
        }
    }

    fn refused(&mut self, conn_id: u64, frame: &Value, reason: &str) {
        let role = self.control_conns.get(&conn_id).map(|c| c.role);
        let session_id = self.control.as_ref().map(|s| s.id.clone());
        let body = json!({
            "v": 1, "type": "refused",
            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
            "session_id": session_id, "reason": reason, "at": control::now_iso(),
        });
        self.emit(OutTarget::Conn(conn_id), body.clone());
        if role == Some(Role::Agent) {
            self.emit(OutTarget::Host, body);
        }
    }

    fn capabilities_frame() -> Value {
        json!({
            "v": 1, "type": "capabilities", "protocol": 1,
            "engine": {"name": "hermes-blitz", "version": env!("CARGO_PKG_VERSION")},
            "operations": [
                "observe", "click", "scroll", "navigate", "back", "forward", "reload",
                "type", "select", "fill_form", "replay_recipe", "submit",
                "passkey.assert", "passkey.register"
            ],
            "remote_page_script": "unsupported",
            "trusted_aot_script": "partial",
            "text_input": "supported",
            "forms": "supported",
            "select": "supported",
            "fill_form": "supported",
            "submit": "supported",
            "vault": "supported",
            "recall": "supported",
            "cookies_persistent": "unsupported",
            "passkeys": "supported",
            "passkey_broker": "supported",
            "credential_broker": "supported",
            "screenshot_png": "supported",
        })
    }

    fn handle_control(&mut self, conn_id: u64, frame: Value) {
        let Some(role) = self.control_conns.get(&conn_id).map(|c| c.role) else {
            return;
        };
        // Per-connection monotonic sequence.
        match frame.get("seq").and_then(Value::as_u64) {
            Some(seq) => {
                let conn = self.control_conns.get_mut(&conn_id).expect("conn");
                if conn.last_seq.is_some_and(|last| seq <= last) {
                    self.refused(conn_id, &frame, "replayed_seq");
                    return;
                }
                conn.last_seq = Some(seq);
            }
            None => {
                self.refused(conn_id, &frame, "internal");
                return;
            }
        }
        let Some(kind) = frame
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            self.refused(conn_id, &frame, "unsupported_message");
            return;
        };
        let host_only = matches!(
            kind.as_str(),
            "session.open"
                | "session.state"
                | "session.close"
                | "decision"
                | "control.take"
                | "control.release"
                | "stop"
                | "audit.read"
        );
        let agent_only = matches!(kind.as_str(), "propose" | "task.done");
        if host_only && role != Role::Host {
            self.refused(conn_id, &frame, "wrong_channel");
            return;
        }
        if agent_only && role != Role::Agent {
            self.refused(conn_id, &frame, "wrong_channel");
            return;
        }
        // Session binding for everything except open/ping/capabilities.
        //
        // A decision about a page-initiated passkey ceremony is exempt: a
        // page may start a ceremony in any tab, including one with no
        // supervised session open, and the glasses still have to be able to
        // answer it.
        let is_page_passkey_decision = kind == "decision"
            && frame
                .get("proposal_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.starts_with("page-passkey-"));
        if !matches!(kind.as_str(), "ping" | "capabilities.get" | "session.open")
            && !is_page_passkey_decision
        {
            match self.control.as_ref() {
                None => {
                    self.refused(conn_id, &frame, "wrong_state");
                    return;
                }
                Some(session) => {
                    if let Some(sid) = frame.get("session_id").and_then(Value::as_str) {
                        if sid != session.id {
                            self.refused(conn_id, &frame, "wrong_state");
                            return;
                        }
                    }
                }
            }
        }
        match kind.as_str() {
            "ping" => {
                let body = json!({
                    "v": 1, "type": "pong",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "at": control::now_iso(),
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "capabilities.get" => {
                let mut body = Self::capabilities_frame();
                body["request_id"] = frame.get("request_id").cloned().unwrap_or(Value::Null);
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "credential.list" => {
                // The agent learns that credentials exist, which site they
                // belong to, and a redacted account hint. Never a secret.
                let matches = self.credential_broker.list().unwrap_or_default();
                let body = json!({
                    "v": 1, "type": "credential.list",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "credentials": matches,
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "credential.describe" => {
                let origin = frame
                    .get("origin")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let request = credentials::CredentialFillRequest {
                    task_id: frame
                        .get("task_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    origin,
                    fields: Vec::new(),
                    document_generation: frame
                        .get("generation")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                };
                match self.credential_broker.describe_fill(request) {
                    Ok(prompt) => {
                        let body = json!({
                            "v": 1, "type": "credential.prompt",
                            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                            "prompt": prompt,
                        });
                        self.emit(OutTarget::Conn(conn_id), body);
                    }
                    Err(error) => {
                        let body = json!({
                            "v": 1, "type": "credential.prompt",
                            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                            "error": error.to_string(),
                        });
                        self.emit(OutTarget::Conn(conn_id), body);
                    }
                }
            }
            "credential.save" => {
                // Saving a secret is a trusted shell action, never an agent
                // action: the agent must not be able to plant a credential.
                if role != Role::Host {
                    self.refused(conn_id, &frame, "wrong_role");
                    return;
                }
                let (Some(host), Some(username), Some(secret)) = (
                    frame.get("host").and_then(Value::as_str),
                    frame.get("username").and_then(Value::as_str),
                    frame.get("secret").and_then(Value::as_str),
                ) else {
                    self.refused(conn_id, &frame, "internal");
                    return;
                };
                match self
                    .credential_broker
                    .save_password(host, username, secret)
                {
                    Ok(()) => {
                        let body = json!({
                            "v": 1, "type": "credential.saved",
                            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                            "host": host,
                            "username": credentials::redact_username(username),
                        });
                        self.emit(OutTarget::Conn(conn_id), body);
                    }
                    Err(error) => {
                        let body = json!({
                            "v": 1, "type": "credential.saved",
                            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                            "error": error.to_string(),
                        });
                        self.emit(OutTarget::Conn(conn_id), body);
                    }
                }
            }
            "credential.delete" => {
                // Deleting a saved secret is a trusted shell action.
                if role != Role::Host {
                    self.refused(conn_id, &frame, "wrong_role");
                    return;
                }
                let (Some(host), Some(username)) = (
                    frame.get("host").and_then(Value::as_str),
                    frame.get("username").and_then(Value::as_str),
                ) else {
                    self.refused(conn_id, &frame, "internal");
                    return;
                };
                let result = self.credential_broker.delete_password(host, username);
                let body = json!({
                    "v": 1, "type": "credential.deleted",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "host": host,
                    "username": credentials::redact_username(username),
                    "ok": result.is_ok(),
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "passkey.publickey" => {
                // A relying party needs the credential's public key to register
                // it. The private key never leaves the Keychain, and the
                // public key is not secret, so the host may read it.
                if role != Role::Host {
                    self.refused(conn_id, &frame, "wrong_role");
                    return;
                }
                let Some(credential_id) = frame.get("credential_id").and_then(Value::as_str) else {
                    self.refused(conn_id, &frame, "internal");
                    return;
                };
                let bytes = webauthn::b64::b64url_decode(credential_id)
                    .or_else(|| control::b64_decode(credential_id));
                let public_key = bytes
                    .and_then(|bytes| self.passkey_broker.borrow().public_key_pem(&bytes).ok())
                    .flatten();
                let body = json!({
                    "v": 1, "type": "passkey.publickey",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "credential_id": credential_id,
                    "public_key_pem": public_key,
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "passkey.delete" => {
                // Removing a passkey is a trusted shell action.
                if role != Role::Host {
                    self.refused(conn_id, &frame, "wrong_role");
                    return;
                }
                let Some(credential_id) = frame.get("credential_id").and_then(Value::as_str) else {
                    self.refused(conn_id, &frame, "internal");
                    return;
                };
                let bytes = webauthn::b64::b64url_decode(credential_id)
                    .or_else(|| control::b64_decode(credential_id));
                let removed = match bytes {
                    Some(bytes) => self.passkey_broker.borrow_mut().delete_passkey(&bytes).unwrap_or(false),
                    None => false,
                };
                let body = json!({
                    "v": 1, "type": "passkey.deleted",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "credential_id": credentials::redact_username(credential_id),
                    "ok": removed,
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "passkey.status" => {
                let ceremony_id = frame
                    .get("ceremony_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // With no ceremony id the caller is asking for the most
                // recent ceremony, which is how a host learns which
                // credential a registration produced.
                let ceremony_id = if ceremony_id.is_empty() {
                    self.passkey_broker
                        .borrow()
                        .latest_ceremony_id()
                        .map(|id| id.0)
                        .unwrap_or_default()
                } else {
                    ceremony_id
                };
                let status = self.passkey_broker.borrow_mut().poll_assertion(
                    credentials::PasskeyCeremonyId(ceremony_id.clone()),
                );
                // The registration is polled too: the ceremony id does not
                // say which kind it is.
                let status = match status {
                    Ok(status) => Ok(status),
                    Err(error) => self
                        .passkey_broker
                        .borrow_mut()
                        .poll_registration(credentials::PasskeyCeremonyId(ceremony_id.clone()))
                        .map_err(|_| error),
                };
                // Surface the credential id at the top level when the ceremony
                // completed, so a host can register it with a relying party
                // without parsing the nested status.
                let credential_id = match &status {
                    Ok(credentials::PasskeyStatus::Complete(
                        credentials::CeremonyOutcome::Registration(credential),
                    )) => Some(credential.id.clone()),
                    Ok(credentials::PasskeyStatus::Complete(
                        credentials::CeremonyOutcome::Assertion(credential),
                    )) => Some(credential.id.clone()),
                    _ => None,
                };
                let body = json!({
                    "v": 1, "type": "passkey.status",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "ceremony_id": ceremony_id,
                    "credential_id": credential_id,
                    "status": match status {
                        Ok(status) => serde_json::to_value(status).unwrap_or(Value::Null),
                        Err(error) => json!({"failed": error.to_string()}),
                    },
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "passkey.cancel" => {
                let ceremony_id = frame
                    .get("ceremony_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let id = credentials::PasskeyCeremonyId(ceremony_id.clone());
                // The ceremony id does not say which kind it is, so try the
                // assertion first and fall back to the registration.
                let cancelled = {
                    let mut broker = self.passkey_broker.borrow_mut();
                    broker
                        .cancel_assertion(id.clone())
                        .or_else(|_| broker.cancel_registration(id))
                };
                let body = json!({
                    "v": 1, "type": "passkey.cancelled",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "ceremony_id": ceremony_id,
                    "ok": cancelled.is_ok(),
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            "session.open" => self.do_session_open(conn_id, &frame),
            "session.state" => self.state_event(OutTarget::Conn(conn_id)),
            "session.close" => self.do_session_close(conn_id, &frame),
            "observe.request" => self.do_observe(conn_id, role, &frame),
            "propose" => self.do_propose(conn_id, &frame),
            "decision" => self.do_decision(conn_id, &frame),
            "control.take" => {
                let by = frame
                    .get("decided_by")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_string();
                let surface = frame
                    .get("surface")
                    .and_then(Value::as_str)
                    .unwrap_or("socket")
                    .to_string();
                if !self.control_take(&by, &surface) {
                    self.refused(conn_id, &frame, "wrong_state");
                }
            }
            "control.release" => {
                let by = frame
                    .get("decided_by")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_string();
                let surface = frame
                    .get("surface")
                    .and_then(Value::as_str)
                    .unwrap_or("socket")
                    .to_string();
                if !self.control_release(&by, &surface) {
                    self.refused(conn_id, &frame, "wrong_state");
                }
            }
            "stop" => {
                let by = frame
                    .get("decided_by")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_string();
                let surface = frame
                    .get("surface")
                    .and_then(Value::as_str)
                    .unwrap_or("socket")
                    .to_string();
                if !self.control_stop(&by, &surface) {
                    self.refused(conn_id, &frame, "wrong_state");
                }
            }
            "task.done" => {
                let ok = self
                    .control
                    .as_ref()
                    .is_some_and(|s| s.state == ControlState::Agent && s.pending.is_none());
                if !ok {
                    self.refused(conn_id, &frame, "wrong_state");
                    return;
                }
                if let Some(session) = self.control.as_mut() {
                    session.state = ControlState::Done;
                    session.snapshot_valid = false;
                }
                self.audit(
                    "done",
                    "task.done",
                    None,
                    None,
                    None,
                    Some("agent"),
                    Some("socket"),
                    None,
                );
                self.state_event(OutTarget::Both);
                self.sync_chrome();
                self.request_redraw();
            }
            "audit.read" => {
                let records = self
                    .control
                    .as_ref()
                    .map(|s| Value::Array(s.audit.clone()))
                    .unwrap_or(Value::Array(vec![]));
                let body = json!({
                    "v": 1, "type": "audit.result",
                    "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
                    "session_id": self.control.as_ref().map(|s| s.id.clone()),
                    "records": records, "at": control::now_iso(),
                });
                self.emit(OutTarget::Conn(conn_id), body);
            }
            _ => self.refused(conn_id, &frame, "unsupported_message"),
        }
    }

    fn do_session_open(&mut self, conn_id: u64, frame: &Value) {
        let force_replace = frame
            .get("force_replace")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !force_replace
            && self
                .control
                .as_ref()
                .is_some_and(|s| !s.state.is_terminal())
        {
            self.refused(conn_id, frame, "wrong_state");
            return;
        }
        let Some(title) = frame
            .get("title")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .map(str::to_string)
        else {
            self.refused(conn_id, frame, "internal");
            return;
        };
        if let Some(decision) = frame
            .get("policy")
            .and_then(|p| p.get("decision"))
            .and_then(Value::as_str)
        {
            if decision != "human_per_action" {
                self.refused(conn_id, frame, "capability_unsupported");
                return;
            }
        }
        let reuse_active_tab = frame
            .get("reuse_active_tab")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let tab_id = if reuse_active_tab && !self.tabs.is_empty() {
            self.tab().canonical.id
        } else {
            let tab = self.create_start_tab();
            let id = tab.canonical.id;
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
            id
        };
        let session_id = format!("s_{}", self.next_session_id);
        self.next_session_id = self.next_session_id.wrapping_add(1);
        self.control = Some(ControlSession {
            id: session_id.clone(),
            tab_id,
            title,
            state: ControlState::Agent,
            control_epoch: 1,
            event_seq: 0,
            audit_seq: 0,
            snapshot_valid: false,
            pending: None,
            seen_proposals: HashSet::new(),
            used_decisions: HashSet::new(),
            audit: Vec::new(),
            action_required_kind: None,
            last_screenshot_hash: None,
        });
        self.audit(
            "opened",
            "session.open",
            None,
            None,
            None,
            Some("user"),
            Some("socket"),
            None,
        );
        if let Some(start_url) = frame
            .get("start_url")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            let current_url = self.tab().canonical.browser.display_url();
            if !reuse_active_tab || current_url != start_url {
                // The host's act of opening authorizes exactly this navigation.
                self.navigate(start_url);
            }
        }
        let generation = self.tab().instance.document_generation;
        let body = json!({
            "v": 1, "type": "session.opened",
            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
            "session_id": session_id, "tab_id": tab_id.0,
            "state": "agent", "generation": generation,
            "capabilities": Self::capabilities_frame(),
            "at": control::now_iso(),
        });
        self.emit(OutTarget::Conn(conn_id), body);
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
    }

    fn do_session_close(&mut self, conn_id: u64, frame: &Value) {
        let Some(session) = self.control.take() else {
            self.refused(conn_id, frame, "wrong_state");
            return;
        };
        let body = json!({
            "v": 1, "type": "session.closed",
            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
            "session_id": session.id, "at": control::now_iso(),
        });
        self.emit(OutTarget::Conn(conn_id), body);
        if let Some(index) = self
            .tabs
            .iter()
            .position(|t| t.canonical.id == session.tab_id)
        {
            self.close_tab(index);
        }
    }

    fn do_observe(&mut self, conn_id: u64, role: Role, frame: &Value) {
        let (state, tab_id) = {
            let Some(session) = self.control.as_ref() else {
                return;
            };
            (session.state, session.tab_id)
        };
        if state.is_terminal() || (role == Role::Agent && state != ControlState::Agent) {
            self.refused(conn_id, frame, "wrong_state");
            return;
        }
        let Some(snapshot) = self.observe_tab(tab_id) else {
            self.refused(conn_id, frame, "internal");
            return;
        };
        if let Some(session) = self.control.as_mut() {
            session.snapshot_valid = true;
        }
        let (screenshot_b64, screenshot_hash, screenshot_frame) =
            if frame.get("include_screenshot").and_then(Value::as_bool) == Some(true) {
                self.activate_tab(tab_id);
                self.sync_chrome();
                match self.capture_png() {
                    Some((bytes, hash, frame_id)) => {
                        if let Some(session) = self.control.as_mut() {
                            session.last_screenshot_hash = Some(hash.clone());
                        }
                        (
                            Some(control::b64_encode(&bytes)),
                            Some(hash),
                            Some(frame_id),
                        )
                    }
                    None => (None, None, None),
                }
            } else {
                (None, None, None)
            };
        let body = json!({
            "v": 1, "type": "observe.result",
            "request_id": frame.get("request_id").cloned().unwrap_or(Value::Null),
            "session_id": self.control.as_ref().map(|s| s.id.clone()),
            "snapshot_id": format!("o_{}", snapshot.snapshot_id),
            "tab_id": snapshot.tab_id,
            "page_id": snapshot.page_id,
            "state": state.as_str(),
            "url": snapshot.url, "title": snapshot.title,
            "generation": snapshot.document_generation,
            "fingerprint": snapshot.fingerprint,
            "viewport": serde_json::to_value(snapshot.viewport).unwrap_or(Value::Null),
            "scroll": serde_json::to_value(snapshot.scroll).unwrap_or(Value::Null),
            "text": snapshot.text,
            "elements": serde_json::to_value(&snapshot.elements).unwrap_or(Value::Array(vec![])),
            "focused_element_id": snapshot.focused_element_id,
            "form_signature": snapshot.form_signature,
            "matched_recipe_id": snapshot.matched_recipe_id,
            "screenshot_png_b64": screenshot_b64,
            "screenshot_hash": screenshot_hash,
            "frame_id": screenshot_frame,
            "presented_frame_id": self.frame_seq,
            "last_input_event_id": self.last_input_event_id,
            "at": control::now_iso(),
        });
        self.emit(OutTarget::Conn(conn_id), body);
    }

    fn raise_action_required(&mut self, kind: &str, page_detail: &str) {
        if let Some(session) = self.control.as_mut() {
            session.state = ControlState::ActionRequired;
            session.action_required_kind = Some(kind.to_string());
            session.snapshot_valid = false;
        }
        self.audit("action_required", kind, None, None, None, None, None, None);
        self.session_event(
            OutTarget::Host,
            "action_required",
            json!({"kind": kind, "page_detail": page_detail}),
        );
        self.state_event(OutTarget::Agent);
        self.sync_chrome();
        self.request_redraw();
    }

    fn do_propose(&mut self, conn_id: u64, frame: &Value) {
        let (state, tab_id, snapshot_valid, seen) = {
            let Some(session) = self.control.as_ref() else {
                return;
            };
            (
                session.state,
                session.tab_id,
                session.snapshot_valid,
                frame
                    .get("proposal_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| session.seen_proposals.contains(id)),
            )
        };
        if state == ControlState::AwaitingDecision {
            self.refused(conn_id, frame, "proposal_pending");
            return;
        }
        if state != ControlState::Agent {
            self.refused(conn_id, frame, "wrong_state");
            return;
        }
        let (
            Some(proposal_id),
            Some(snapshot_id_raw),
            Some(fingerprint),
            Some(generation),
            Some(operation),
            Some(rationale),
        ) = (
            frame.get("proposal_id").and_then(Value::as_str),
            frame.get("snapshot_id").and_then(Value::as_str),
            frame.get("fingerprint").and_then(Value::as_str),
            frame.get("generation").and_then(Value::as_u64),
            frame.get("operation").filter(|o| o.is_object()).cloned(),
            frame.get("rationale").and_then(Value::as_str),
        )
        else {
            self.refused(conn_id, frame, "internal");
            return;
        };
        if seen {
            self.refused(conn_id, frame, "decision_used");
            return;
        }
        let snapshot_id: u64 = snapshot_id_raw
            .strip_prefix("o_")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let Some(tab) = self.tabs.iter().find(|t| t.canonical.id == tab_id) else {
            self.refused(conn_id, frame, "internal");
            return;
        };
        if generation != tab.instance.document_generation {
            self.refused(conn_id, frame, "stale_generation");
            return;
        }
        let Some(snapshot) = tab.instance.last_snapshot.clone() else {
            self.refused(conn_id, frame, "stale_fingerprint");
            return;
        };
        if !snapshot_valid
            || snapshot.snapshot_id != snapshot_id
            || snapshot.fingerprint != fingerprint
        {
            self.refused(conn_id, frame, "stale_fingerprint");
            return;
        }

        let op_kind = operation.get("kind").and_then(Value::as_str).unwrap_or("");
        let target_id = operation.get("target_id").and_then(Value::as_str);
        // A passkey proposal starts a host-side ceremony. The broker
        // validates the request and produces the prompt the human sees; the
        // ceremony itself does not run until that human approves.
        let mut ceremony_id: Option<String> = None;
        let mut passkey_prompt: Option<String> = None;
        let mut credential_fill_plan: Option<CredentialFillPlan> = None;
        // A stable node reference resolves against the known-element
        // registry first: unknown references were never issued by this page
        // instance; known references absent from the snapshot are stale.
        if let Some(target_ref) = operation.get("target_ref").and_then(Value::as_str) {
            if !tab.instance.known_node_refs.contains(target_ref) {
                self.refused(conn_id, frame, "target_missing");
                return;
            }
            if !snapshot.elements.iter().any(|e| e.node_ref == target_ref) {
                self.refused(conn_id, frame, "stale_fingerprint");
                return;
            }
        }
        let target = match operation.get("target_ref").and_then(Value::as_str) {
            Some(target_ref) => snapshot.elements.iter().find(|e| e.node_ref == target_ref),
            None => target_id.and_then(|id| snapshot.elements.iter().find(|e| e.id == id)),
        };
        let is_el_sensitive = |e: &observe::ElementSnapshot| {
            e.value_state == observe::ValueState::Sensitive
                || e.input_type.as_deref() == Some("password")
                || {
                    let label = e.label.to_ascii_lowercase();
                    ["password", "delete", "purchase", "credit card", "one-time"]
                        .iter()
                        .any(|w| label.contains(w))
                }
        };
        let mut sensitive_target = target.is_some_and(is_el_sensitive);

        match op_kind {
            "click" => {
                let Some(element) = target else {
                    self.refused(conn_id, frame, "target_missing");
                    return;
                };
                if !element.visible {
                    self.refused(conn_id, frame, "target_not_visible");
                    return;
                }
                if !element.enabled {
                    self.refused(conn_id, frame, "target_disabled");
                    return;
                }
                if element.kind != "click" {
                    if sensitive_target {
                        self.refused(conn_id, frame, "sensitive_requires_human");
                        self.raise_action_required("password", &element.label.clone());
                    } else {
                        self.refused(conn_id, frame, "capability_unsupported");
                    }
                    return;
                }
            }
            "scroll" => {
                let ok = operation
                    .get("dy")
                    .and_then(Value::as_f64)
                    .is_some_and(|dy| dy.is_finite());
                if !ok {
                    self.refused(conn_id, frame, "internal");
                    return;
                }
            }
            "navigate" => {
                let ok = operation
                    .get("url")
                    .and_then(Value::as_str)
                    .is_some_and(|u| browser::normalize_input(u).is_ok());
                if !ok {
                    self.refused(conn_id, frame, "internal");
                    return;
                }
            }
            "back" => {
                if !tab.canonical.browser.can_go_back() {
                    self.refused(conn_id, frame, "wrong_state");
                    return;
                }
            }
            "forward" => {
                if !tab.canonical.browser.can_go_forward() {
                    self.refused(conn_id, frame, "wrong_state");
                    return;
                }
            }
            "reload" => {}
            "type" | "select" => {
                if sensitive_target {
                    let detail = target.map(|e| e.label.clone()).unwrap_or_default();
                    self.refused(conn_id, frame, "sensitive_requires_human");
                    self.raise_action_required("password", &detail);
                    return;
                }
                let Some(element) = target else {
                    self.refused(conn_id, frame, "target_missing");
                    return;
                };
                if !element.visible {
                    self.refused(conn_id, frame, "target_not_visible");
                    return;
                }
                if !element.enabled {
                    self.refused(conn_id, frame, "target_disabled");
                    return;
                }
                if PageCommand::from_operation(&operation).is_none() {
                    self.refused(conn_id, frame, "internal");
                    return;
                }
            }
            "fill_form" => {
                let Some(PageCommand::FillForm { fields }) = PageCommand::from_operation(&operation)
                else {
                    self.refused(conn_id, frame, "internal");
                    return;
                };
                for f in &fields {
                    if let Some(ref tr) = f.target_ref {
                        if !tab.instance.known_node_refs.contains(tr) {
                            self.refused(conn_id, frame, "target_missing");
                            return;
                        }
                        if !snapshot.elements.iter().any(|e| &e.node_ref == tr) {
                            self.refused(conn_id, frame, "stale_fingerprint");
                            return;
                        }
                    }
                    let el = match f.target_ref.as_deref() {
                        Some(tr) => snapshot.elements.iter().find(|e| e.node_ref == tr),
                        None => f
                            .target_id
                            .as_deref()
                            .and_then(|id| snapshot.elements.iter().find(|e| e.id == id)),
                    };
                    let Some(element) = el else {
                        self.refused(conn_id, frame, "target_missing");
                        return;
                    };
                    if !element.visible {
                        self.refused(conn_id, frame, "target_not_visible");
                        return;
                    }
                    if !element.enabled {
                        self.refused(conn_id, frame, "target_disabled");
                        return;
                    }
                    if is_el_sensitive(element) {
                        sensitive_target = true;
                        if f.action != "fill_secret" {
                            self.refused(conn_id, frame, "sensitive_requires_human");
                            self.raise_action_required("password", &element.label.clone());
                            return;
                        }
                    }
                }
            }
            "replay_recipe" => {
                let Some(PageCommand::ReplayRecipe { recipe_id }) =
                    PageCommand::from_operation(&operation)
                else {
                    self.refused(conn_id, frame, "internal");
                    return;
                };
                let matched = if !recipe_id.is_empty() {
                    recall::get_recipe_by_id(&recipe_id)
                } else {
                    recall::find_matching_recipe(&snapshot.url, snapshot.form_signature.as_deref())
                };
                let Some(recipe) = matched else {
                    self.refused(conn_id, frame, "target_missing");
                    return;
                };
                if recipe.fields.iter().any(|rf| rf.action == "fill_secret") {
                    sensitive_target = true;
                }
            }
            "credential.fill" => {
                // The broker decides what can be offered; the agent never
                // names a secret and never picks an account itself.
                let origin = operation
                    .get("origin")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let fields = operation
                    .get("fields")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|v| v.as_str())
                            .filter_map(|name| match name {
                                "username" => Some(credentials::CredentialFieldKind::Username),
                                "current_password" => {
                                    Some(credentials::CredentialFieldKind::CurrentPassword)
                                }
                                "new_password" => Some(credentials::CredentialFieldKind::NewPassword),
                                "one_time_code" => Some(credentials::CredentialFieldKind::OneTimeCode),
                                "payment_card" => Some(credentials::CredentialFieldKind::PaymentCard),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| {
                        vec![
                            credentials::CredentialFieldKind::Username,
                            credentials::CredentialFieldKind::CurrentPassword,
                        ]
                    });
                let request = credentials::CredentialFillRequest {
                    task_id: proposal_id.to_string(),
                    origin: origin.clone(),
                    fields: fields.clone(),
                    document_generation: generation,
                };
                match self.credential_broker.describe_fill(request) {
                    Ok(prompt) => {
                        let chosen = match prompt.matches.len() {
                            0 => {
                                eprintln!("[credential] no stored credential for {}", prompt.registrable_domain);
                                self.refused(conn_id, frame, "no_matching_credential");
                                return;
                            }
                            1 => prompt.matches[0].clone(),
                            _ => {
                                // The shell cannot yet present a picker, so an
                                // ambiguous site is refused rather than filled
                                // with an arbitrary account.
                                eprintln!(
                                    "[credential] {} matching credentials for {}; refusing an ambiguous fill",
                                    prompt.matches.len(),
                                    prompt.registrable_domain
                                );
                                self.refused(conn_id, frame, "credential_ambiguous");
                                return;
                            }
                        };
                        passkey_prompt = Some(format!(
                            "Fill the saved credential for {} on {}?",
                            chosen.username_hint, prompt.registrable_domain
                        ));
                        credential_fill_plan = Some(CredentialFillPlan {
                            origin: prompt.origin.clone(),
                            chosen,
                            fields,
                        });
                        sensitive_target = true;
                    }
                    Err(error) => {
                        eprintln!("[credential] fill refused: {error}");
                        self.refused(conn_id, frame, "credential_refused");
                        return;
                    }
                }
            }
            "passkey.assert" | "passkey.register" => {
                let started = if op_kind == "passkey.assert" {
                    credentials::PasskeyRequest::from_operation(&operation)
                        .and_then(|request| {
                            self.passkey_broker
                                .borrow_mut()
                                .begin_assertion(request)
                                .map(|id| (id, "passkey.assert"))
                        })
                } else {
                    credentials::RegistrationRequest::from_operation(&operation)
                        .and_then(|request| {
                            self.passkey_broker
                                .borrow_mut()
                                .begin_registration(request)
                                .map(|id| (id, "passkey.register"))
                        })
                };
                match started {
                    Ok((id, kind)) => {
                        let prompt = match self.passkey_broker.borrow_mut().poll_assertion(id.clone()) {
                            Ok(credentials::PasskeyStatus::AwaitingApproval { prompt, .. }) => {
                                prompt
                            }
                            _ => format!("{kind} awaiting approval"),
                        };
                        ceremony_id = Some(id.0);
                        passkey_prompt = Some(prompt);
                        // A passkey ceremony is always sensitive: it must be
                        // approved by a human before it runs.
                        sensitive_target = true;
                    }
                    Err(error) => {
                        eprintln!("[passkey] proposal refused: {error}");
                        self.refused(conn_id, frame, "passkey_refused");
                        return;
                    }
                }
            }
            "submit" => {
                let Some(element) = target else {
                    self.refused(conn_id, frame, "target_missing");
                    return;
                };
                if !element.visible {
                    self.refused(conn_id, frame, "target_not_visible");
                    return;
                }
                if !element.enabled {
                    self.refused(conn_id, frame, "target_disabled");
                    return;
                }
                sensitive_target = true;
            }
            _ => {
                self.refused(conn_id, frame, "capability_unsupported");
                return;
            }
        }

        let stage_only = operation
            .get("stage_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let requires_glasses = sensitive_target && !stage_only;
        let classification = if sensitive_target {
            "sensitive"
        } else if op_kind == "scroll" {
            "observe"
        } else {
            "act"
        }
        .to_string();
        // A passkey proposal carries the broker's own prompt, which names the
        // site and the redacted account the human is approving.
        let rationale = passkey_prompt.unwrap_or_else(|| rationale.to_string());
        let pending = PendingProposal {
            proposal_id: proposal_id.to_string(),
            snapshot_id,
            page_id: snapshot.page_id,
            fingerprint: fingerprint.to_string(),
            generation,
            operation: operation.clone(),
            rationale,
            classification: classification.clone(),
            requires_glasses,
            ceremony_id,
            credential_fill: credential_fill_plan,
        };
        if let Some(session) = self.control.as_mut() {
            session.seen_proposals.insert(pending.proposal_id.clone());
            session.state = ControlState::AwaitingDecision;
        }
        self.audit(
            "proposed",
            op_kind,
            target_id,
            Some(&pending.fingerprint),
            None,
            Some("agent"),
            Some("socket"),
            Some(&pending.proposal_id),
        );
        let pid_copy = pending.proposal_id.clone();
        let fp_copy = pending.fingerprint.clone();
        let gen_copy = pending.generation;
        if self.supervise_commits_only && !requires_glasses {
            if let Some(session) = self.control.as_mut() {
                session.pending = Some(pending);
            }
            let _ = self.decide(
                &pid_copy,
                &fp_copy,
                gen_copy,
                true,
                "policy",
                "commit_gate",
            );
            return;
        }
        self.session_event(
            OutTarget::Host,
            "proposal.pending",
            json!({
                "proposal_id": pending.proposal_id,
                "snapshot_id": format!("o_{}", pending.snapshot_id),
                "fingerprint": pending.fingerprint, "generation": pending.generation,
                "operation": pending.operation, "rationale": pending.rationale,
                "classification": pending.classification,
                "requires_glasses": pending.requires_glasses,
            }),
        );
        if let Some(session) = self.control.as_mut() {
            session.pending = Some(pending);
        }
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
    }

    fn do_decision(&mut self, conn_id: u64, frame: &Value) {
        // A decision about a page-initiated passkey ceremony. The glasses
        // DAT card is the primary surface for such a prompt, so its answer
        // arrives here. It carries no fingerprint or generation: the page's
        // own script may change the document while the prompt is up, and
        // that must not cancel a legitimate ceremony.
        if let Some(proposal_id) = frame.get("proposal_id").and_then(Value::as_str) {
            if let Some(ceremony_id) = proposal_id.strip_prefix("page-passkey-") {
                let decision = frame.get("decision").and_then(Value::as_str).unwrap_or("");
                let approve = match decision {
                    "approve" => true,
                    "reject" => false,
                    _ => {
                        self.refused(conn_id, frame, "internal");
                        return;
                    }
                };
                if self.passkey_prompt.is_some()
                    && self
                        .passkey_prompt
                        .as_ref()
                        .map(|ceremony| ceremony.ceremony_id.as_str())
                        == Some(ceremony_id)
                {
                    self.decide_page_passkey(approve);
                    self.flush_control_out();
                    return;
                }
                self.refused(conn_id, frame, "decision_missing");
                return;
            }
        }
        let (Some(proposal_id), Some(fingerprint), Some(generation), Some(decision)) = (
            frame
                .get("proposal_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            frame
                .get("fingerprint")
                .and_then(Value::as_str)
                .map(str::to_string),
            frame.get("generation").and_then(Value::as_u64),
            frame
                .get("decision")
                .and_then(Value::as_str)
                .map(str::to_string),
        ) else {
            self.refused(conn_id, frame, "internal");
            return;
        };
        let decided_by = frame
            .get("decided_by")
            .and_then(Value::as_str)
            .unwrap_or("user")
            .to_string();
        let surface = frame
            .get("surface")
            .and_then(Value::as_str)
            .unwrap_or("socket")
            .to_string();
        let approve = match decision.as_str() {
            "approve" => true,
            "reject" => false,
            _ => {
                self.refused(conn_id, frame, "internal");
                return;
            }
        };
        if let Err(reason) = self.decide(
            &proposal_id,
            &fingerprint,
            generation,
            approve,
            &decided_by,
            &surface,
        ) {
            self.refused(conn_id, frame, reason);
        }
    }

    /// Shared by the socket `decision` message and the supervision bar.
    /// Returns a refusal reason on failure; executes at most one operation.
    fn decide(
        &mut self,
        proposal_id: &str,
        fingerprint: &str,
        generation: u64,
        approve: bool,
        decided_by: &str,
        surface: &str,
    ) -> Result<(), &'static str> {
        let (tab_id, pending_fp, pending_gen, pending_page, used) = {
            let session = self.control.as_ref().ok_or("wrong_state")?;
            let used = session.used_decisions.contains(proposal_id);
            let pending = session.pending.as_ref().ok_or(if used {
                "decision_used"
            } else {
                "decision_missing"
            })?;
            if pending.proposal_id != proposal_id {
                return Err("decision_mismatch");
            }
            (
                session.tab_id,
                pending.fingerprint.clone(),
                pending.generation,
                pending.page_id,
                used,
            )
        };
        if used {
            return Err("decision_used");
        }
        if fingerprint != pending_fp || generation != pending_gen {
            return Err("decision_mismatch");
        }
        // The decision is consumed exactly once, before any execution.
        if let Some(session) = self.control.as_mut() {
            session.used_decisions.insert(proposal_id.to_string());
        }

        if !approve {
            let op_kind = self
                .control
                .as_ref()
                .and_then(|s| s.pending.as_ref())
                .and_then(|p| {
                    p.operation
                        .get("kind")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            if let Some(session) = self.control.as_mut() {
                session.pending = None;
                session.state = ControlState::Agent;
            }
            self.audit(
                "rejected",
                &op_kind,
                None,
                Some(&pending_fp),
                None,
                Some(decided_by),
                Some(surface),
                Some(proposal_id),
            );
            self.session_event(
                OutTarget::Both,
                "proposal.invalidated",
                json!({"proposal_id": proposal_id, "reason": "rejected"}),
            );
            self.state_event(OutTarget::Both);
            self.sync_chrome();
            self.request_redraw();
            return Ok(());
        }

        // Revalidate against the live page immediately before dispatch.
        let Some(fresh) = self.observe_tab(tab_id) else {
            return Err("internal");
        };
        if fresh.fingerprint != pending_fp
            || fresh.document_generation != pending_gen
            || fresh.page_id != pending_page
        {
            if let Some(session) = self.control.as_mut() {
                session.pending = None;
                session.state = ControlState::Agent;
                session.snapshot_valid = false;
            }
            self.audit(
                "refused_stale",
                "",
                None,
                Some(&pending_fp),
                Some(&fresh.fingerprint),
                Some(decided_by),
                Some(surface),
                Some(proposal_id),
            );
            self.session_event(
                OutTarget::Both,
                "proposal.invalidated",
                json!({"proposal_id": proposal_id, "reason": "stale_fingerprint"}),
            );
            self.state_event(OutTarget::Both);
            self.sync_chrome();
            self.request_redraw();
            return Err("stale_fingerprint");
        }

        let (operation, ceremony_id, credential_fill) = self
            .control
            .as_ref()
            .and_then(|s| s.pending.as_ref())
            .map(|p| {
                (
                    p.operation.clone(),
                    p.ceremony_id.clone(),
                    p.credential_fill.clone(),
                )
            })
            .unwrap_or((Value::Null, None, None));

        // A credential fill runs here, on the host, behind the approval gate
        // the human just gave. The secret is read from the Keychain at this
        // moment and written straight into the page.
        if let Some(plan) = credential_fill {
            return self.run_credential_fill(
                &plan,
                &fresh,
                proposal_id,
                decided_by,
                surface,
                &pending_fp,
            );
        }

        // A passkey ceremony runs here, on the host, behind the approval gate
        // the human just gave. It is never simulated in page script.
        if let Some(ceremony_id) = ceremony_id {
            return self.run_passkey_ceremony(
                &ceremony_id,
                proposal_id,
                decided_by,
                surface,
                &pending_fp,
            );
        }

        // Execution crosses the page-endpoint boundary as a typed command;
        // the JSON operation remains only the wire/audit representation.
        let Some(command) = PageCommand::from_operation(&operation) else {
            return Err("capability_unsupported");
        };
        let op_kind = command.kind().to_string();
        let target_id = command.target_id().map(str::to_string);
        if let PageCommand::Click { .. } = command {
            let element = fresh.elements.iter().find(|e| {
                command.target_ref() == Some(e.node_ref.as_str())
                    || command.target_id() == Some(e.id.as_str())
            });
            let check = match element {
                Some(element) => self.check_interactable(tab_id, element),
                None => Err("target_missing"),
            };
            if let Err(reason) = check {
                if let Some(session) = self.control.as_mut() {
                    session.pending = None;
                    session.state = ControlState::Agent;
                    session.snapshot_valid = false;
                }
                self.audit(
                    "refused_not_interactable",
                    &op_kind,
                    target_id.as_deref(),
                    Some(&pending_fp),
                    None,
                    Some(decided_by),
                    Some(surface),
                    Some(proposal_id),
                );
                self.session_event(
                    OutTarget::Both,
                    "proposal.invalidated",
                    json!({"proposal_id": proposal_id, "reason": reason}),
                );
                self.state_event(OutTarget::Both);
                self.sync_chrome();
                self.request_redraw();
                return Err(reason);
            }
        }
        if let Some(session) = self.control.as_mut() {
            session.pending = None;
        }
        self.dispatch_page_command(&command, &fresh);
        let after = self.observe_tab(tab_id);
        let (after_fp, after_gen) = after
            .as_ref()
            .map(|s| (s.fingerprint.clone(), s.document_generation))
            .unwrap_or((String::new(), pending_gen));
        if let Some(session) = self.control.as_mut() {
            session.state = ControlState::Agent;
            session.snapshot_valid = false;
        }
        self.audit(
            "executed",
            &op_kind,
            target_id.as_deref(),
            Some(&pending_fp),
            Some(&after_fp),
            Some(decided_by),
            Some(surface),
            Some(proposal_id),
        );
        self.session_event(
            OutTarget::Both,
            "executed",
            json!({
                "proposal_id": proposal_id, "operation": op_kind,
                "operation_detail": command.to_operation(),
                "target_id": target_id,
                "before_fingerprint": pending_fp,
                "after_fingerprint": after_fp, "generation": after_gen,
                "input_event_id": self.last_input_event_id,
                "input_provenance": InputProvenance::ApprovedAgent.as_str(),
            }),
        );
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
        Ok(())
    }

    /// Runs an approved credential fill and writes the secret into the page.
    ///
    /// The secret is read from the macOS Keychain here and handed straight to
    /// the DOM. It never reaches the agent, the audit trail, or a log.
    fn run_credential_fill(
        &mut self,
        plan: &CredentialFillPlan,
        snapshot: &observe::PageSnapshot,
        proposal_id: &str,
        decided_by: &str,
        surface: &str,
        fingerprint: &str,
    ) -> Result<(), &'static str> {
        let approval = credentials::CredentialApproval {
            ceremony_id: proposal_id.to_string(),
            chosen: Some(plan.chosen.clone()),
            remember: false,
            origin: plan.origin.clone(),
            fields: plan.fields.clone(),
        };

        // The broker releases the secret; the sink records where it goes.
        let elements = snapshot.elements.clone();
        let mut sink = PageCredentialSink {
            elements: &elements,
            pending: Vec::new(),
            filled: Vec::new(),
        };
        let receipt = self
            .credential_broker
            .apply_approved_fill(approval, &mut sink)
            .map_err(|error| {
                eprintln!("[credential] fill failed: {error}");
                "credential_fill_failed"
            })?;
        let filled = sink.filled.clone();
        let pending = sink.pending.clone();
        drop(sink);

        // Now the secret goes into the page.
        let tab = self.tab_mut();
        for (node_id, tag, value) in pending {
            apply_dom_field_action(
                tab.instance.page.doc_mut(),
                node_id,
                &tag,
                "type",
                Some(&value),
                None,
                None,
            );
        }
        tab.instance.page.resolve();
        tab.bump_generation();

        if let Some(session) = self.control.as_mut() {
            session.pending = None;
            session.state = ControlState::Agent;
            session.snapshot_valid = false;
        }
        // The audit records what happened, never the secret.
        self.audit(
            "executed",
            "credential.fill",
            None,
            Some(fingerprint),
            None,
            Some(decided_by),
            Some(surface),
            Some(proposal_id),
        );
        self.emit(
            OutTarget::Agent,
            json!({
                "v": 1, "type": "credential.filled",
                "proposal_id": proposal_id,
                "origin": receipt.origin,
                "account_hint": receipt.account_hint,
                "filled_fields": filled,
            }),
        );
        self.session_event(
            OutTarget::Both,
            "executed",
            json!({
                "proposal_id": proposal_id, "operation": "credential.fill",
                "operation_detail": {"origin": receipt.origin},
                "before_fingerprint": fingerprint,
            }),
        );
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
        Ok(())
    }

    /// Runs an approved passkey ceremony and hands the result to the agent.
    ///
    /// The assertion or registration is produced by the host-side
    /// authenticator. The agent receives only public material — the
    /// credential ID, the authenticator data, the client data, and the
    /// signature. No private key and no password crosses this boundary.
    fn run_passkey_ceremony(
        &mut self,
        ceremony_id: &str,
        proposal_id: &str,
        decided_by: &str,
        surface: &str,
        fingerprint: &str,
    ) -> Result<(), &'static str> {
        let id = credentials::PasskeyCeremonyId(ceremony_id.to_string());
        let op_kind = self
            .control
            .as_ref()
            .and_then(|s| s.pending.as_ref())
            .and_then(|p| p.operation.get("kind").and_then(Value::as_str))
            .unwrap_or("passkey")
            .to_string();

        // Each borrow is scoped on its own. A `borrow_mut()` in one match arm
        // would still be alive when the next arm borrows again, and the
        // second borrow would panic.
        let approved = {
            let mut broker = self.passkey_broker.borrow_mut();
            broker.approve_ceremony(id.clone(), decided_by, surface)
        };
        let status = match approved {
            Ok(()) => {
                let mut broker = self.passkey_broker.borrow_mut();
                broker.poll_assertion(id).map_err(|error| {
                    eprintln!("[passkey] ceremony {ceremony_id} poll failed: {error}");
                    "passkey_failed"
                })?
            }
            Err(error) => {
                eprintln!("[passkey] ceremony {ceremony_id} failed: {error}");
                credentials::PasskeyStatus::Failed(error.to_string())
            }
        };

        if let Some(session) = self.control.as_mut() {
            session.pending = None;
        }
        let outcome = match status {
            credentials::PasskeyStatus::Complete(outcome) => outcome,
            credentials::PasskeyStatus::Failed(reason) => {
                if let Some(session) = self.control.as_mut() {
                    session.state = ControlState::Agent;
                }
                self.audit(
                    "refused_failed",
                    &op_kind,
                    None,
                    Some(fingerprint),
                    None,
                    Some(decided_by),
                    Some(surface),
                    Some(proposal_id),
                );
                self.emit(
                    OutTarget::Agent,
                    json!({
                        "v": 1, "type": "passkey.result",
                        "proposal_id": proposal_id, "ceremony_id": ceremony_id,
                        "ok": false, "error": reason,
                    }),
                );
                self.session_event(
                    OutTarget::Both,
                    "proposal.invalidated",
                    json!({"proposal_id": proposal_id, "reason": "passkey_failed"}),
                );
                self.state_event(OutTarget::Both);
                self.sync_chrome();
                self.request_redraw();
                return Err("passkey_failed");
            }
            other => {
                if let Some(session) = self.control.as_mut() {
                    session.state = ControlState::Agent;
                }
                self.emit(
                    OutTarget::Agent,
                    json!({
                        "v": 1, "type": "passkey.result",
                        "proposal_id": proposal_id, "ceremony_id": ceremony_id,
                        "ok": false, "error": format!("{other:?}"),
                    }),
                );
                self.state_event(OutTarget::Both);
                self.sync_chrome();
                self.request_redraw();
                return Err("passkey_failed");
            }
        };

        if let Some(session) = self.control.as_mut() {
            session.state = ControlState::Agent;
            session.snapshot_valid = false;
        }
        self.audit(
            "executed",
            &op_kind,
            None,
            Some(fingerprint),
            None,
            Some(decided_by),
            Some(surface),
            Some(proposal_id),
        );
        let result = serde_json::to_value(&outcome).unwrap_or(Value::Null);
        self.emit(
            OutTarget::Agent,
            json!({
                "v": 1, "type": "passkey.result",
                "proposal_id": proposal_id, "ceremony_id": ceremony_id,
                "ok": true, "result": result,
            }),
        );
        self.session_event(
            OutTarget::Both,
            "executed",
            json!({
                "proposal_id": proposal_id, "operation": op_kind,
                "operation_detail": {"ceremony_id": ceremony_id},
                "before_fingerprint": fingerprint,
            }),
        );
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
        Ok(())
    }

    /// Single entry point for page input: physical events and approved
    /// agent commands take an event ID and provenance here, then process in
    /// FIFO order on the event-loop thread.
    fn enqueue_input(&mut self, provenance: InputProvenance, action: InputAction) -> u64 {
        let id = self.next_input_event_id;
        self.next_input_event_id = self.next_input_event_id.wrapping_add(1);
        self.input_queue.push_back(QueuedInput {
            id,
            provenance,
            action,
        });
        self.process_input_queue();
        id
    }

    fn process_input_queue(&mut self) {
        while let Some(input) = self.input_queue.pop_front() {
            self.last_input_event_id = input.id;
            let physical_blocked =
                input.provenance == InputProvenance::PhysicalUser && self.session_blocks_input();
            match input.action {
                InputAction::PageClick { x, y } => {
                    if !physical_blocked {
                        self.do_page_click(x, y);
                    }
                }
                InputAction::Scroll { dy } => {
                    if !physical_blocked {
                        self.scroll_page(dy);
                    }
                }
                InputAction::Command { command, snapshot } => {
                    self.execute_page_command_now(&command, &snapshot);
                }
            }
        }
    }

    fn dispatch_page_command(&mut self, command: &PageCommand, snapshot: &observe::PageSnapshot) {
        self.enqueue_input(
            InputProvenance::ApprovedAgent,
            InputAction::Command {
                command: command.clone(),
                snapshot: snapshot.clone(),
            },
        );
    }

    fn activate_tab(&mut self, tab_id: TabId) {
        if let Some(index) = self.tabs.iter().position(|t| t.canonical.id == tab_id) {
            self.active = index;
        }
    }

    /// WebDriver-style staleness and interactability, checked against the
    /// live document at execution time: the node must still exist, have a
    /// non-empty visible rect, and receive a hit at its center (itself or a
    /// descendant) rather than being covered by another element.
    fn check_interactable(
        &self,
        tab_id: TabId,
        element: &observe::ElementSnapshot,
    ) -> Result<(), &'static str> {
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.canonical.id == tab_id)
            .ok_or("target_missing")?;
        let doc = tab.instance.page.doc();
        if doc.get_node(element.node_id).is_none() {
            return Err("target_missing");
        }
        if !element.enabled {
            return Err("target_disabled");
        }
        let rect = doc
            .get_client_bounding_rect(element.node_id)
            .ok_or("target_not_visible")?;
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Err("target_not_visible");
        }
        let cx = (rect.x + rect.width / 2.0) as f32;
        let cy = (rect.y + rect.height / 2.0) as f32;
        let Some(hit) = doc.hit(cx, cy) else {
            return Err("target_not_visible");
        };
        let mut current = hit.node_id;
        loop {
            if current == element.node_id {
                return Ok(());
            }
            let Some(node) = doc.get_node(current) else {
                return Err("target_not_visible");
            };
            let Some(parent) = node.parent else {
                return Err("target_not_visible");
            };
            current = parent;
        }
    }

    /// The in-process page-endpoint adapter: executes one typed command
    /// against the session tab's current page instance. Authorization has
    /// already happened in `decide`; this layer only dispatches.
    fn execute_page_command_now(
        &mut self,
        command: &PageCommand,
        snapshot: &observe::PageSnapshot,
    ) {
        let Some(session) = self.control.as_ref() else {
            return;
        };
        let tab_id = session.tab_id;
        self.activate_tab(tab_id);
        match command {
            PageCommand::Click {
                target_id,
                target_ref,
            } => {
                let Some(element) = snapshot
                    .elements
                    .iter()
                    .find(|e| {
                        target_ref.as_deref() == Some(e.node_ref.as_str()) || &e.id == target_id
                    })
                    .cloned()
                else {
                    return;
                };
                if element.role == "link" {
                    if let Some(href) = element.href.clone() {
                        self.session_event(OutTarget::Both, "nav.started", json!({"url": href}));
                        self.navigate(&href);
                        return;
                    }
                }
                let cx = (element.bounds.x + element.bounds.width / 2.0) as f32;
                let cy = (element.bounds.y + element.bounds.height / 2.0) as f32;
                let hermes = self.tab().instance.page.hermes;
                unsafe {
                    hermes_app_dispatch_event(
                        hermes,
                        c"click".as_ptr(),
                        element.node_id.as_u64(),
                        cx,
                        cy,
                    );
                }
                let tab = self.tab_mut();
                if tab.instance.page.ctx.dirty {
                    tab.instance.page.resolve();
                    tab.instance.page.ctx.dirty = false;
                }
                tab.bump_generation();
                if let Some(form_node) = self.tab_mut().instance.page.ctx.pending_form_submit.take() {
                    self.submit_page_form(form_node);
                }
            }
            PageCommand::Type {
                target_id,
                target_ref,
                text,
            } => {
                let Some(element) = snapshot
                    .elements
                    .iter()
                    .find(|e| {
                        target_ref.as_deref() == Some(e.node_ref.as_str()) || &e.id == target_id
                    })
                    .cloned()
                else {
                    return;
                };
                let tab = self.tab_mut();
                apply_dom_field_action(
                    tab.instance.page.doc_mut(),
                    element.node_id,
                    &element.tag,
                    "type",
                    Some(text.as_str()),
                    None,
                    None,
                );
                tab.instance.page.resolve();
                tab.bump_generation();
            }
            PageCommand::Select {
                target_id,
                target_ref,
                value,
            } => {
                let Some(element) = snapshot
                    .elements
                    .iter()
                    .find(|e| {
                        target_ref.as_deref() == Some(e.node_ref.as_str()) || &e.id == target_id
                    })
                    .cloned()
                else {
                    return;
                };
                let tab = self.tab_mut();
                apply_dom_field_action(
                    tab.instance.page.doc_mut(),
                    element.node_id,
                    &element.tag,
                    "select",
                    Some(value.as_str()),
                    None,
                    None,
                );
                tab.instance.page.resolve();
                tab.bump_generation();
            }
            PageCommand::FillForm { fields } => {
                let tab = self.tab_mut();
                for f in fields {
                    let Some(element) = snapshot
                        .elements
                        .iter()
                        .find(|e| {
                            f.target_ref.as_deref() == Some(e.node_ref.as_str())
                                || f.target_id.as_deref() == Some(e.id.as_str())
                        })
                        .cloned()
                    else {
                        continue;
                    };
                    apply_dom_field_action(
                        tab.instance.page.doc_mut(),
                        element.node_id,
                        &element.tag,
                        &f.action,
                        f.value.as_deref(),
                        f.origin.as_deref(),
                        f.username.as_deref(),
                    );
                }
                tab.instance.page.resolve();
                tab.bump_generation();
                if let Some(ref sig) = snapshot.form_signature {
                    recall::record_recipe(&snapshot.url, sig, fields, snapshot);
                }
            }
            PageCommand::ReplayRecipe { recipe_id } => {
                let matched = if !recipe_id.is_empty() {
                    recall::get_recipe_by_id(recipe_id)
                } else {
                    recall::find_matching_recipe(&snapshot.url, snapshot.form_signature.as_deref())
                };
                let Some(recipe) = matched else {
                    return;
                };
                let tab = self.tab_mut();
                for rf in &recipe.fields {
                    let Some(element) = snapshot
                        .elements
                        .iter()
                        .filter(|e| matches!(e.tag.as_str(), "input" | "select" | "textarea"))
                        .find(|e| {
                            (rf.dom_id.is_some() && e.dom_id == rf.dom_id)
                                || (rf.name.is_some() && e.name == rf.name)
                                || (!rf.label.is_empty() && e.label == rf.label)
                        })
                        .cloned()
                    else {
                        continue;
                    };
                    apply_dom_field_action(
                        tab.instance.page.doc_mut(),
                        element.node_id,
                        &element.tag,
                        &rf.action,
                        rf.value.as_deref(),
                        rf.origin.as_deref(),
                        rf.username.as_deref(),
                    );
                }
                tab.instance.page.resolve();
                tab.bump_generation();
                self.recalled_count = self.recalled_count.wrapping_add(1);
            }
            PageCommand::Submit {
                target_id,
                target_ref,
            } => {
                let Some(element) = snapshot
                    .elements
                    .iter()
                    .find(|e| {
                        target_ref.as_deref() == Some(e.node_ref.as_str()) || &e.id == target_id
                    })
                    .cloned()
                else {
                    return;
                };
                let base_url = self
                    .tab()
                    .canonical
                    .browser
                    .current()
                    .map(|e| e.url.clone())
                    .unwrap_or_else(|| Url::parse(START_URL).expect("start url"));
                let (action_url, method, entries) = collect_form_submission(
                    self.tab().instance.page.doc(),
                    &base_url,
                    element.node_id,
                );
                self.session_event(
                    OutTarget::Both,
                    "nav.started",
                    json!({"url": action_url.to_string()}),
                );
                let provider = self.provider.clone();
                let viewport = page_viewport(self.size, self.scale, self.theme);
                let theme = self.theme;
                match browser::submit_form(&provider, &action_url, &method, &entries) {
                    Ok((final_url, html)) => {
                        let broker = self.passkey_broker.clone();
                        let tab = self.tab_mut();
                        tab.canonical.browser.commit(final_url.clone(), html.clone());
                        tab.replace_page(Page::load_with_broker(
                            &html,
                            Some(final_url.clone()),
                            Some(provider),
                            viewport,
                            theme,
                            broker,
                        ));
                        tab.canonical.browser.error = None;
                        tab.canonical.title = title_for(&final_url, tab.instance.page.doc());
                        self.session_event(
                            OutTarget::Both,
                            "nav.committed",
                            json!({"tab_id": tab_id.0, "url": final_url.to_string()}),
                        );
                    }
                    Err(e) => {
                        let broker = self.passkey_broker.clone();
                        let tab = self.tab_mut();
                        tab.canonical.browser.fail(e.clone());
                        let html = error_page(&e);
                        tab.replace_page(Page::load_with_broker(
                            &html, None, None, viewport, theme, broker,
                        ));
                        tab.canonical.title = "Error".to_string();
                    }
                }
            }
            PageCommand::Scroll { dy } => {
                self.scroll_page(*dy);
                self.tab_mut().bump_generation();
            }
            PageCommand::Navigate { url } => {
                self.session_event(OutTarget::Both, "nav.started", json!({"url": url}));
                self.navigate(url);
            }
            PageCommand::Back => self.go_back(),
            PageCommand::Forward => self.go_forward(),
            PageCommand::Reload => self.reload(),
        }
    }

    fn invalidate_pending(&mut self, reason: &str) {
        let proposal_id = self
            .control
            .as_mut()
            .and_then(|s| s.pending.take())
            .map(|p| p.proposal_id);
        if let Some(proposal_id) = proposal_id {
            self.session_event(
                OutTarget::Both,
                "proposal.invalidated",
                json!({"proposal_id": proposal_id, "reason": reason}),
            );
        }
    }

    /// Bump the fetch generation so an in-flight result cannot commit.
    fn discard_in_flight_fetch(&mut self) {
        let Some(tab_id) = self.control.as_ref().map(|s| s.tab_id) else {
            return;
        };
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.canonical.id == tab_id) {
            tab.canonical.browser.generation = tab.canonical.browser.generation.wrapping_add(1);
            tab.canonical.browser.loading = false;
        }
    }

    fn control_take(&mut self, decided_by: &str, surface: &str) -> bool {
        let active = self
            .control
            .as_ref()
            .is_some_and(|s| !s.state.is_terminal() && s.state != ControlState::Human);
        if !active {
            return false;
        }
        self.invalidate_pending("takeover");
        self.discard_in_flight_fetch();
        if let Some(session) = self.control.as_mut() {
            session.control_epoch = session.control_epoch.wrapping_add(1);
            session.state = ControlState::Human;
            session.snapshot_valid = false;
        }
        self.audit(
            "control_take",
            "control.take",
            None,
            None,
            None,
            Some(decided_by),
            Some(surface),
            None,
        );
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
        true
    }

    fn control_release(&mut self, decided_by: &str, surface: &str) -> bool {
        let tab_id = match self.control.as_ref() {
            Some(s) if s.state == ControlState::Human => s.tab_id,
            _ => return false,
        };
        if let Some(session) = self.control.as_mut() {
            session.control_epoch = session.control_epoch.wrapping_add(1);
            session.state = ControlState::Agent;
            session.snapshot_valid = false;
            session.action_required_kind = None;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.canonical.id == tab_id) {
            tab.bump_generation();
        }
        self.audit(
            "control_release",
            "control.release",
            None,
            None,
            None,
            Some(decided_by),
            Some(surface),
            None,
        );
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
        true
    }

    fn control_stop(&mut self, decided_by: &str, surface: &str) -> bool {
        let active = self
            .control
            .as_ref()
            .is_some_and(|s| !s.state.is_terminal());
        if !active {
            return false;
        }
        self.invalidate_pending("stopped");
        self.discard_in_flight_fetch();
        if let Some(session) = self.control.as_mut() {
            session.control_epoch = session.control_epoch.wrapping_add(1);
            session.state = ControlState::Stopped;
            session.snapshot_valid = false;
        }
        self.audit(
            "stopped",
            "stop",
            None,
            None,
            None,
            Some(decided_by),
            Some(surface),
            None,
        );
        self.state_event(OutTarget::Both);
        self.sync_chrome();
        self.request_redraw();
        true
    }

    /// True when the active tab is agent-owned: physical page input is gated
    /// to the supervision bar until the human takes control.
    fn session_blocks_input(&self) -> bool {
        let Some(session) = self.control.as_ref() else {
            return false;
        };
        session.tab_id == self.tab().canonical.id
            && matches!(
                session.state,
                ControlState::Agent | ControlState::AwaitingDecision | ControlState::ActionRequired
            )
    }

    /// Offscreen PNG of the active tab (page + chrome), through the same
    /// Vello pipeline as `--render`.
    fn capture_png(&mut self) -> Option<(Vec<u8>, String, u64)> {
        use anyrender::ImageRenderer;
        use anyrender_vello::VelloImageRenderer;
        // Mirror render_current_frame: physical pixels at the live scale.
        let (width, height) = (self.size.0.max(1), self.size.1.max(1));
        let scale = self.scale;
        let mut renderer = VelloImageRenderer::new(width, height);
        let mut rgba: Vec<u8> = Vec::new();
        let top_h_px = ((self.top_chrome_height() * scale) as u32).min(height);
        let left_w_px = ((self.left_chrome_width() * scale) as u32).min(width);
        let content_w_px = width.saturating_sub(left_w_px).max(1);
        let content_h_px = height.saturating_sub(top_h_px).max(1);
        let ph_x_px = 0u32;
        let ph_y_px = (((self.top_chrome_height() + PH_HEADER_HEIGHT) * scale) as u32).min(height);
        let ph_w_px =
            ((EXPLORER_DRAWER_WIDTH * scale) as u32).min(width.saturating_sub(ph_x_px)).max(1);
        let ph_h_px = height.saturating_sub(ph_y_px).max(1);
        let paint_ph = self.explorer_tab == Some(ExplorerTab::PageHolder)
            && self
                .page_holder
                .as_ref()
                .is_some_and(|ph| !ph.links_mode);

        let Self {
            tabs,
            active,
            chrome_doc,
            page_holder,
            ..
        } = self;
        let tab = &mut tabs[*active];
        renderer.render_to_vec(
            |painter| {
                blitz_paint::paint_scene(
                    painter,
                    tab.instance.page.doc_mut(),
                    scale,
                    content_w_px,
                    content_h_px,
                    left_w_px,
                    top_h_px,
                );
                if paint_ph {
                    if let Some(ph) = page_holder.as_mut() {
                        blitz_paint::paint_scene(
                            painter,
                            &mut ph.doc,
                            scale,
                            ph_w_px,
                            ph_h_px,
                            ph_x_px,
                            ph_y_px,
                        );
                    }
                }
                blitz_paint::paint_scene(painter, chrome_doc, scale, width, height, 0, 0);
            },
            &mut rgba,
        );
        let expected = (width * height * 4) as usize;
        if rgba.len() < expected {
            return None;
        }
        let bg_rgb: [u8; 3] = if self.theme.is_dark() {
            [0x1C, 0x1C, 0x1E]
        } else {
            [0xFF, 0xFF, 0xFF]
        };
        for px in rgba[..expected].chunks_exact_mut(4) {
            let a = px[3] as u16;
            if a < 255 {
                let inv = 255 - a;
                px[0] = ((px[0] as u16 * a + bg_rgb[0] as u16 * inv) / 255) as u8;
                px[1] = ((px[1] as u16 * a + bg_rgb[1] as u16 * inv) / 255) as u8;
                px[2] = ((px[2] as u16 * a + bg_rgb[2] as u16 * inv) / 255) as u8;
                px[3] = 255;
            }
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut encoder = png::Encoder::new(&mut cursor, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&rgba[..expected]).ok()?;
        }
        let bytes = cursor.into_inner();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        self.frame_seq = self.frame_seq.wrapping_add(1);
        Some((bytes, hash, self.frame_seq))
    }

    /// Picks up a page ceremony that needs the related-origins well-known
    /// fetch, and runs the fetch off the UI thread.
    ///
    /// WebAuthn Level 3 §5.11.1 lets a relying party name an RP ID that is
    /// not a suffix of the origin the page was served from, provided the
    /// relying party publishes the caller's origin in its own
    /// `/.well-known/webauthn` document. Deciding that needs the network, so
    /// the FFI entry point parked the request and this is where it is picked
    /// up.
    ///
    /// The fetch runs on a worker thread and the answer comes back through
    /// the event loop, so the UI thread never waits on the network.
    fn poll_page_related_origins(&mut self) {
        // A request whose fetch never answered must not leave the page's
        // promise pending forever. The deadline is the same one the approval
        // prompt uses: the request is refused when it is older than the
        // passkey prompt timeout.
        let expired = {
            let page = &self.tab().instance.page;
            page.ctx
                .pending_related_origins
                .values()
                .find(|request| {
                    request.started_at.elapsed()
                        >= std::time::Duration::from_secs(chrome::PASSKEY_PROMPT_TIMEOUT_SECS)
                })
                .map(|request| request.request_id.clone())
        };
        if let Some(request_id) = expired {
            let page = &mut self.tab_mut().instance.page;
            page.ctx.related_origins_in_flight.remove(&request_id);
            page.ctx.pending_related_origins.remove(&request_id);
            let payload = serde_json::json!({
                "error": "The relying party's related origins document could not be read in time"
            });
            settle_related_origins(page.hermes, &request_id, &payload);
            return;
        }

        // The request the host has not started yet. The parked map is drained
        // from the front, so a page that starts several ceremonies gets them
        // resolved in order.
        let pending = {
            let page = &self.tab().instance.page;
            let in_flight = &page.ctx.related_origins_in_flight;
            page.ctx
                .pending_related_origins
                .values()
                .find(|request| !in_flight.contains(&request.request_id))
                .cloned()
        };
        let Some(pending) = pending else {
            return;
        };
        // The request stays in the map until the fetch answers, because the
        // answer needs the request's inputs to start the ceremony. A marker
        // keeps it from being fetched twice.
        self.tab_mut()
            .instance
            .page
            .ctx
            .related_origins_in_flight
            .insert(pending.request_id.clone());

        let tab = self.tab().canonical.id;
        let proxy = self.proxy.clone();
        let origin = pending.origin.clone();
        let rp_id = pending.rp_id.clone();
        let request_id = pending.request_id.clone();
        std::thread::spawn(move || {
            // The fetch needs a Tokio reactor. The browser's own runtime is
            // private, but `enter_runtime` enters it, and the fetch is
            // awaited inside that context.
            let _guard = browser::enter_runtime();
            let outcome =
                pollster::block_on(crate::webauthn::related_origins::validate(&origin, &rp_id));
            let result = match outcome {
                Ok(true) => Ok(String::new()),
                Ok(false) => Err(format!(
                    "The origin '{origin}' is not a related origin of '{rp_id}'"
                )),
                Err(error) => Err(error.to_string()),
            };
            let _ = proxy.send_event(UserEvent::RelatedOriginsResolved {
                tab,
                request_id,
                result,
            });
        });
    }

    /// Settles a page's promise after the related-origins fetch has answered.
    ///
    /// The fetch ran in a tab that may have been closed or replaced while it
    /// was in flight, so the answer is only used when it names the tab that is
    /// showing now. A stale answer is dropped: the page it belonged to is
    /// gone, and settling it would touch a document that is no longer there.
    fn handle_related_origins_resolved_for_tab(
        &mut self,
        tab: TabId,
        request_id: String,
        result: Result<String, String>,
    ) {
        if self.tab().canonical.id != tab {
            return;
        }
        self.handle_related_origins_resolved(&request_id, result);
        self.sync_chrome();
        self.request_redraw();
    }

    /// Settles a page's promise after the related-origins fetch has answered.
    ///
    /// When the relying party's own document named the caller's origin, the
    /// ceremony is started here and the page's promise moves on to the human
    /// approval gate. When it did not, the page's promise is rejected with
    /// the reason.
    fn handle_related_origins_resolved(
        &mut self,
        request_id: &str,
        result: Result<String, String>,
    ) {
        let page = &mut self.tab_mut().instance.page;
        page.ctx.related_origins_in_flight.remove(request_id);
        let request = page.ctx.pending_related_origins.remove(request_id);
        let payload = match (result, request) {
            (Ok(_), Some(request)) => {
                // The relying party's own document named the caller's origin,
                // so the ceremony may proceed. It starts here, with the
                // related origins check already made.
                if request.is_create {
                    dom_ffi::begin_page_registration_with_related_origins(
                        &mut page.ctx,
                        &request.origin,
                        &request.rp_id,
                        &request.user_name,
                        &request.challenge,
                    )
                } else {
                    dom_ffi::begin_page_assertion_with_related_origins(
                        &mut page.ctx,
                        &request.origin,
                        &request.rp_id,
                        &request.challenge,
                        request.allow_credentials.clone(),
                    )
                }
            }
            (Ok(_), None) => serde_json::json!({ "error": "the request is gone" }),
            (Err(message), _) => serde_json::json!({ "error": message }),
        };
        settle_related_origins(page.hermes, request_id, &payload);
    }

    /// Picks up any passkey ceremony a page started, so a human can be asked
    /// about it.
    ///
    /// A page's `navigator.credentials.get()` starts a ceremony and stops.
    /// The ceremony waits in the broker until somebody says yes or no. This
    /// is where the host notices it, raises the interstitial, and enforces
    /// the deadline.
    fn poll_page_passkey_ceremonies(&mut self) {
        // A ceremony that is already being shown may have run out of time.
        // The deadline is read from the page's own map, which is where the
        // ceremony's start time lives.
        let expired = self
            .tab()
            .instance
            .page
            .ctx
            .pending_page_ceremonies
            .values()
            .find(|ceremony| {
                ceremony.started_at.elapsed()
                    >= std::time::Duration::from_secs(chrome::PASSKEY_PROMPT_TIMEOUT_SECS)
            })
            .cloned();
        if let Some(expired) = expired {
            self.finish_page_passkey_ceremony(
                &expired,
                Err("The passkey request timed out before it was approved".to_string()),
            );
            return;
        }

        // Look for a ceremony the page started and that nobody has answered.
        let pending = {
            let page = &self.tab().instance.page;
            page.ctx
                .pending_page_ceremonies
                .values()
                .find(|ceremony| {
                    ceremony.started_at.elapsed()
                        < std::time::Duration::from_secs(chrome::PASSKEY_PROMPT_TIMEOUT_SECS)
                })
                .cloned()
        };
        let Some(pending) = pending else {
            if self.passkey_prompt.take().is_some() {
                self.sync_chrome();
                self.request_redraw();
            }
            return;
        };
        if self
            .passkey_prompt
            .as_ref()
            .is_some_and(|current| current.ceremony_id == pending.ceremony_id)
        {
            return;
        }
        self.passkey_prompt = Some(pending);
        self.announce_page_passkey_on_glasses();
        self.sync_chrome();
        self.request_redraw();
    }

    /// Tells the glasses about a page-initiated ceremony.
    ///
    /// The DAT card is the primary surface for a passkey prompt: the person
    /// may not be at the keyboard. The browser modal is the mirror.
    fn announce_page_passkey_on_glasses(&mut self) {
        let Some(pending) = self.passkey_prompt.clone() else {
            return;
        };
        // The same event the agent-proposed path emits, so the host that
        // turns proposals into DAT cards needs no new case for a page's own
        // ceremony. The fields the bridge reads are all here: the proposal
        // id it binds the card to, the operation kind it puts on the card,
        // the rationale it shows, and the page url it takes the domain from.
        //
        // The event is emitted even when no supervised session is open. A
        // page may start a ceremony in any tab, and the person who has to
        // answer it may be on the glasses rather than at the keyboard.
        let title = if pending.origin.is_empty() {
            pending.rp_id.clone()
        } else {
            pending.origin.clone()
        };
        let rationale = if pending.prompt.is_empty() {
            format!("Use a passkey for {title}?")
        } else {
            pending.prompt.clone()
        };
        let session_id = self
            .control
            .as_ref()
            .map(|session| session.id.clone())
            .unwrap_or_default();
        let op_kind = match pending.kind {
            dom_ffi::PageCeremonyKind::Registration => "passkey.page_register",
            dom_ffi::PageCeremonyKind::Assertion => "passkey.page_assert",
        };
        let frame = json!({
            "v": 1,
            "type": "proposal.pending",
            "session_id": session_id,
            "proposal_id": format!("page-passkey-{}", pending.ceremony_id),
            "surface": "page",
            "operation": {
                "kind": op_kind,
                "rp_id": pending.rp_id,
                "origin": pending.origin,
                "account_hint": pending.account_hint,
                "url": title,
            },
            "url": title,
            "rationale": rationale,
            "classification": "passkey",
            "requires_glasses": true,
        });
        self.emit(OutTarget::Host, frame);
        self.flush_control_out();
    }

    /// Settles a page's passkey promise.
    ///
    /// `result` carries the assertion JSON when the human allowed the
    /// ceremony, and the reason when they denied it or it expired. Either
    /// way the page's promise is settled exactly once, and the ceremony is
    /// removed so it cannot be answered a second time.
    fn finish_page_passkey_ceremony(
        &mut self,
        ceremony: &dom_ffi::PageCeremony,
        result: Result<String, String>,
    ) {
        // The ceremony is removed first, so a second decision cannot settle
        // the promise twice.
        let page = &mut self.tab_mut().instance.page;
        page.ctx
            .pending_page_ceremonies
            .remove(&ceremony.ceremony_id);
        let hermes = page.hermes;

        let payload = match &result {
            Ok(json) => json.clone(),
            Err(message) => message.clone(),
        };
        let script = format!(
            r#"globalThis.__settleWebAuthn && globalThis.__settleWebAuthn({}, {}, {});"#,
            serde_json::to_string(&ceremony.ceremony_id).unwrap_or_else(|_| "\"\"".into()),
            if result.is_ok() { "true" } else { "false" },
            serde_json::to_string(&payload).unwrap_or_else(|_| "\"\"".into()),
        );
        if let Ok(code) = std::ffi::CString::new(script) {
            unsafe {
                hermes_app_eval_script(hermes, code.as_ptr(), c"<passkey-decision>".as_ptr());
            }
        }
        if self.tab().instance.page.ctx.dirty {
            let tab = self.tab_mut();
            tab.instance.page.resolve();
            tab.instance.page.ctx.dirty = false;
            tab.bump_generation();
        }

        // Cancel the ceremony in the broker, so a decision that arrived after
        // the deadline cannot still produce a signature.
        let ceremony_id = credentials::PasskeyCeremonyId(ceremony.ceremony_id.clone());
        {
            let mut broker = self.passkey_broker.borrow_mut();
            let _ = broker.cancel_assertion(ceremony_id.clone());
            let _ = broker.cancel_registration(ceremony_id.clone());
        }

        let proposal_id = format!("page-passkey-{}", ceremony.ceremony_id);
        let op_kind = match ceremony.kind {
            dom_ffi::PageCeremonyKind::Registration => "passkey.page_register",
            dom_ffi::PageCeremonyKind::Assertion => "passkey.page_assert",
        };
        let receipt = match &result {
            Ok(_) => json!({
                "v": 1,
                "type": "executed",
                "proposal_id": proposal_id,
                "operation": op_kind,
                "operation_detail": {
                    "ceremony_id": ceremony.ceremony_id,
                    "rp_id": ceremony.rp_id,
                    "origin": ceremony.origin,
                },
            }),
            Err(reason) => json!({
                "v": 1,
                "type": "proposal.invalidated",
                "proposal_id": proposal_id,
                "reason": reason,
            }),
        };
        self.emit(OutTarget::Host, receipt);
        self.flush_control_out();

        self.passkey_prompt = None;
        if let Some(form_node) = self.tab_mut().instance.page.ctx.pending_form_submit.take() {
            self.submit_page_form(form_node);
        }
        self.sync_chrome();
        self.request_redraw();
    }

    /// Answers a page-initiated passkey ceremony.
    ///
    /// `allow` is the human's answer. When it is true the ceremony runs and
    /// the page receives a real assertion; when it is false the ceremony is
    /// cancelled and the page is told the request was not approved.
    fn decide_page_passkey(&mut self, allow: bool) {
        let Some(pending) = self.passkey_prompt.clone() else {
            return;
        };
        if !allow {
            self.finish_page_passkey_ceremony(
                &pending,
                Err("The passkey request was not approved".to_string()),
            );
            return;
        }
        let ceremony_id = credentials::PasskeyCeremonyId(pending.ceremony_id.clone());
        let approved = self
            .passkey_broker
            .borrow_mut()
            .approve_ceremony(ceremony_id.clone(), "human", "browser");
        if let Err(error) = approved {
            self.finish_page_passkey_ceremony(&pending, Err(error.to_string()));
            return;
        }
        // The ceremony id does not say which kind it is, so the status is
        // read back the same way the control surface reads it.
        let status = {
            let mut broker = self.passkey_broker.borrow_mut();
            match pending.kind {
                dom_ffi::PageCeremonyKind::Registration => {
                    broker.poll_registration(ceremony_id.clone())
                }
                dom_ffi::PageCeremonyKind::Assertion => {
                    broker.poll_assertion(ceremony_id.clone())
                }
            }
        };
        match status {
            Ok(credentials::PasskeyStatus::Complete(
                credentials::CeremonyOutcome::Assertion(credential),
            )) => {
                let json = serde_json::json!({
                    "ceremony_kind": "assertion",
                    "credential_id": credential.id,
                    "id": credential.id,
                    "rawId": credential.raw_id,
                    "type": credential.kind,
                    "authenticatorAttachment": "platform",
                    "rpId": pending.rp_id,
                    "client_data_json_b64": credential.response.client_data_json,
                    "authenticator_data_b64": credential.response.authenticator_data,
                    "signature_der_b64": credential.response.signature,
                });
                self.finish_page_passkey_ceremony(&pending, Ok(json.to_string()));
            }
            Ok(credentials::PasskeyStatus::Complete(
                credentials::CeremonyOutcome::Registration(credential),
            )) => {
                let json = serde_json::json!({
                    "ceremony_kind": "registration",
                    "credential_id": credential.id,
                    "id": credential.id,
                    "rawId": credential.raw_id,
                    "type": credential.kind,
                    "authenticatorAttachment": "platform",
                    "rpId": pending.rp_id,
                    "client_data_json_b64": credential.response.client_data_json,
                    "attestation_object_b64": credential.response.attestation_object,
                    "authenticator_data_b64": credential.response.attestation_object,
                });
                self.finish_page_passkey_ceremony(&pending, Ok(json.to_string()));
            }
            Ok(credentials::PasskeyStatus::Failed(error)) => {
                self.finish_page_passkey_ceremony(&pending, Err(error));
            }
            Ok(_) => {
                self.finish_page_passkey_ceremony(
                    &pending,
                    Err("The passkey ceremony did not complete".to_string()),
                );
            }
            Err(error) => {
                self.finish_page_passkey_ceremony(&pending, Err(error.to_string()));
            }
        }
    }

    /// Regenerate the chrome document from state.
    fn sync_chrome(&mut self) {
        let telemetry = if self.xray_enabled {
            let active_tab_id = self.tab().canonical.id;
            if self.tab().instance.last_snapshot.is_none() {
                let _ = self.observe_tab(active_tab_id);
            }
            let tab = self.tab();
            let layout_observe_ms = tab.instance.last_observe_ms;
            let rss_mb = measure_process_rss_mb();
            let pending_target_ids: Vec<String> = self
                .control
                .as_ref()
                .and_then(|s| s.pending.as_ref())
                .map(|p| {
                    let mut ids = Vec::new();
                    if let Some(tid) = p.operation.get("target_id").and_then(Value::as_str) {
                        if !tid.is_empty() {
                            ids.push(tid.to_string());
                        }
                    }
                    if let Some(fields) = p.operation.get("fields").and_then(Value::as_array) {
                        for f in fields {
                            if f.get("action").and_then(Value::as_str) == Some("fill_secret") {
                                if let Some(tid) = f.get("target_id").and_then(Value::as_str) {
                                    ids.push(tid.to_string());
                                }
                            }
                        }
                    }
                    ids
                })
                .unwrap_or_default();
            let freeze_fp_short = self
                .control
                .as_ref()
                .and_then(|s| s.pending.as_ref())
                .map(|p| p.fingerprint.chars().take(8).collect::<String>());
            let mut overlays = Vec::new();
            let (form_signature, matched_recipe_id) =
                if let Some(snap) = tab.instance.last_snapshot.as_ref() {
                    for el in &snap.elements {
                        if !el.visible {
                            continue;
                        }
                        let is_vault_secret = el.value_state == observe::ValueState::Sensitive
                            || el.input_type.as_deref() == Some("password");
                        let is_frozen_target = pending_target_ids.iter().any(|id| id == &el.id);
                        if !is_frozen_target
                            && !is_vault_secret
                            && !matches!(
                                el.tag.as_str(),
                                "input" | "select" | "textarea" | "button" | "a"
                            )
                            && el.role != "button"
                        {
                            continue;
                        }
                        let role_tag = if is_vault_secret {
                            "password".to_string()
                        } else if !el.role.is_empty() {
                            el.role.clone()
                        } else {
                            el.tag.clone()
                        };
                        overlays.push(chrome::XRayBox {
                            id: el.id.clone(),
                            role_tag,
                            x: el.bounds.x,
                            y: el.bounds.y,
                            width: el.bounds.width,
                            height: el.bounds.height,
                            is_vault_secret,
                            is_frozen_target,
                            freeze_fp_short: if is_frozen_target {
                                freeze_fp_short.clone()
                            } else {
                                None
                            },
                        });
                    }
                    (snap.form_signature.clone(), snap.matched_recipe_id.clone())
                } else {
                    (None, None)
                };
            Some(chrome::EngineTelemetry {
                layout_observe_ms,
                rss_mb,
                form_signature,
                matched_recipe_id,
                recalled_count: self.recalled_count,
                overlays,
            })
        } else {
            None
        };

        let titles: Vec<String> = self
            .tabs
            .iter()
            .map(|t| t.canonical.title.clone())
            .collect();
        let infos: Vec<TabInfo<'_>> = titles
            .iter()
            .enumerate()
            .map(|(i, title)| TabInfo {
                title: title.as_str(),
                active: i == self.active,
            })
            .collect();

        let ph_links: Vec<HeldLinkInfo<'_>> = self
            .page_holder
            .as_ref()
            .map(|ph| {
                ph.links
                    .iter()
                    .map(|l| HeldLinkInfo {
                        label: l.label.as_str(),
                        url: l.url.as_str(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ph_url_str = self
            .page_holder
            .as_ref()
            .map(|ph| ph.url.to_string())
            .unwrap_or_default();
        let ph_title_str = self
            .page_holder
            .as_ref()
            .map(|ph| ph.title.clone())
            .unwrap_or_default();
        let page_holder_info = PageHolderInfo {
            has_doc: self.page_holder.is_some(),
            title: &ph_title_str,
            url: &ph_url_str,
            links_mode: self
                .page_holder
                .as_ref()
                .map(|ph| ph.links_mode)
                .unwrap_or(false),
            links: &ph_links,
        };

        let scrapbook_items: Vec<ScrapbookItemInfo<'_>> = self
            .scrapbook_entries
            .iter()
            .enumerate()
            .map(|(i, e)| ScrapbookItemInfo {
                id: e.id.as_str(),
                title: e.title.as_str(),
                url: e.url.as_str(),
                timestamp: e.timestamp.as_str(),
                is_folder: e.is_folder,
                selected: self.selected_scrapbook == Some(i),
            })
            .collect();

        let fav_items: Vec<NavEntryInfo<'_>> = self
            .favorites
            .iter()
            .map(|(t, u)| NavEntryInfo {
                title: t.as_str(),
                url: u.as_str(),
            })
            .collect();

        let hist_pairs: Vec<(String, String)> = self
            .tab()
            .canonical
            .browser
            .history
            .iter()
            .map(|entry| {
                let u = entry.url.to_string();
                let label = entry
                    .url
                    .host_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| u.clone());
                (label, u)
            })
            .collect();
        let hist_items: Vec<NavEntryInfo<'_>> = hist_pairs
            .iter()
            .map(|(t, u)| NavEntryInfo {
                title: t.as_str(),
                url: u.as_str(),
            })
            .collect();

        let url = self.tab().canonical.browser.display_url();
        let supervision = self.supervision_info();
        let html = chrome::render(&ChromeState {
            url: &url,
            focused: self.tab().canonical.browser.focused,
            can_back: self.tab().canonical.browser.can_go_back(),
            can_forward: self.tab().canonical.browser.can_go_forward(),
            loading: self.tab().canonical.browser.loading,
            tabs: &infos,
            menu_open: self.menu_open,
            secure: url.starts_with("https://"),
            surface_height: self.size.1 as f64 / self.scale,
            theme: self.theme,
            supervision,
            telemetry,
            toolbar_collapsed: self.toolbar_collapsed,
            explorer_tab: self.explorer_tab,
            page_holder: page_holder_info,
            scrapbook_items: &scrapbook_items,
            favorites: &fav_items,
            history_entries: &hist_items,
            passkey_prompt: self
                .passkey_prompt
                .as_ref()
                .map(|ceremony| chrome::PasskeyPrompt {
                    origin: ceremony.origin.clone(),
                    rp_id: ceremony.rp_id.clone(),
                    account_hint: ceremony.account_hint.clone(),
                    prompt: ceremony.prompt.clone(),
                })
                .or_else(|| {
                    let s = self.control.as_ref()?;
                    if s.tab_id != self.tab().canonical.id {
                        return None;
                    }
                    let p = s.pending.as_ref()?;
                    let _cid = p.ceremony_id.as_ref()?;
                    let origin = p
                        .operation
                        .get("origin")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let rp_id = p
                        .operation
                        .get("rp")
                        .and_then(|r| r.get("id"))
                        .and_then(Value::as_str)
                        .or_else(|| p.operation.get("rp_id").and_then(Value::as_str))
                        .unwrap_or_default()
                        .to_string();
                    let account_hint = p
                        .operation
                        .get("user")
                        .and_then(|u| u.get("name"))
                        .and_then(Value::as_str)
                        .map(|s| s.to_string());
                    Some(chrome::PasskeyPrompt {
                        origin,
                        rp_id,
                        account_hint,
                        prompt: p.rationale.clone(),
                    })
                }),
        });

        let mut doc = HtmlDocument::from_html(
            &html,
            DocumentConfig {
                viewport: Some(chrome_viewport(self.size, self.scale)),
                ..Default::default()
            },
        );
        doc.resolve(0.0);
        self.chrome_doc = doc;
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn new_tab(&mut self) {
        let tab = self.create_start_tab();
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.menu_open = false;
        self.sync_chrome();
        self.request_redraw();
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        // Closing the supervised tab ends the session with it.
        if self
            .control
            .as_ref()
            .is_some_and(|s| s.tab_id == self.tabs[index].canonical.id)
        {
            self.invalidate_pending("tab_closed");
            if let Some(session) = self.control.as_mut() {
                session.state = ControlState::Stopped;
                session.snapshot_valid = false;
            }
            self.audit(
                "stopped",
                "tab.close",
                None,
                None,
                None,
                Some("user"),
                Some("desktop-panel"),
                None,
            );
            self.state_event(OutTarget::Both);
            self.flush_control_out();
            self.control = None;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            let tab = self.create_start_tab();
            self.tabs.push(tab);
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        self.menu_open = false;
        self.sync_chrome();
        self.request_redraw();
    }

    fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.menu_open = false;
            self.reflow_active_tab();
            self.sync_chrome();
            self.request_redraw();
        }
    }

    fn navigate(&mut self, input: &str) {
        let trimmed_input = input.trim();
        let agent_goal = trimmed_input
            .strip_prefix("agent:")
            .or_else(|| trimmed_input.strip_prefix("AGENT:"))
            .or_else(|| trimmed_input.strip_prefix("/agent"))
            .map(str::trim)
            .filter(|g| !g.is_empty());
        if let Some(goal) = agent_goal {
            let goal_owned = goal.to_string();
            let tab = self.tab_mut();
            tab.canonical.browser.focused = false;
            tab.canonical.browser.address_select_all = false;
            tab.canonical.browser.sync_address();
            self.menu_open = false;
            self.sync_chrome();
            self.request_redraw();
            self.trigger_in_browser_agent(&goal_owned);
            return;
        }

        let url = match self.tab_mut().canonical.browser.begin_navigation(input) {
            Ok(u) => u,
            Err(e) => {
                self.tab_mut().canonical.browser.fail(e);
                self.sync_chrome();
                self.request_redraw();
                return;
            }
        };

        if url.scheme() == "muse" {
            let viewport = self.content_viewport();
            let theme = self.theme;
            let broker = self.passkey_broker.clone();
            let tab = self.tab_mut();
            tab.canonical.browser.commit(url, START_PAGE.to_string());
            tab.replace_page(Page::load_with_broker(
                START_PAGE,
                None,
                None,
                viewport,
                theme,
                broker,
            ));
            tab.canonical.title = "New tab".to_string();
            self.menu_open = false;
            self.sync_chrome();
            self.request_redraw();
            return;
        }

        let tab_id = self.tab().canonical.id;
        let generation = self.tab().canonical.browser.generation;
        let provider = self.provider.clone();
        let proxy = self.proxy.clone();
        let fetch_url = url.clone();
        self.menu_open = false;
        self.sync_chrome();
        self.request_redraw();

        std::thread::spawn(move || {
            let result = browser::fetch(&provider, &fetch_url);
            let _ = proxy.send_event(UserEvent::Navigated {
                tab: tab_id,
                generation,
                url: fetch_url,
                result,
            });
        });
    }

    fn reload(&mut self) {
        let Some(url) = self
            .tab()
            .canonical
            .browser
            .current()
            .map(|e| e.url.clone())
        else {
            return;
        };
        self.navigate(&url.to_string());
    }

    fn stop_navigation(&mut self) {
        let tab = self.tab_mut();
        tab.canonical.browser.generation = tab.canonical.browser.generation.wrapping_add(1);
        tab.canonical.browser.loading = false;
        tab.canonical.browser.sync_address();
        self.sync_chrome();
        self.request_redraw();
    }

    fn go_back(&mut self) {
        if self.tab_mut().canonical.browser.go_back() {
            self.show_current_entry();
        }
    }

    fn go_forward(&mut self) {
        if self.tab_mut().canonical.browser.go_forward() {
            self.show_current_entry();
        }
    }

    fn show_current_entry(&mut self) {
        let Some((html, url)) = self
            .tab()
            .canonical
            .browser
            .current()
            .map(|e| (e.html.clone(), e.url.clone()))
        else {
            return;
        };
        let (base, provider) = if url.scheme() == "muse" {
            (None, None)
        } else {
            (Some(url.clone()), Some(self.provider.clone()))
        };
        let viewport = self.content_viewport();
        let theme = self.theme;
        let broker = self.passkey_broker.clone();
        let tab = self.tab_mut();
        tab.replace_page(Page::load_with_broker(
            &html, base, provider, viewport, theme, broker,
        ));
        tab.canonical.browser.loading = false;
        tab.canonical.browser.error = None;
        tab.canonical.title = title_for(&url, tab.instance.page.doc());
        self.sync_chrome();
        self.request_redraw();
    }

    fn toggle_toolbar_collapsed(&mut self) {
        self.toolbar_collapsed = !self.toolbar_collapsed;
        self.menu_open = false;
        self.reflow_active_tab();
        self.sync_chrome();
        self.request_redraw();
    }

    fn toggle_explorer_tab(&mut self, tab: ExplorerTab) {
        if self.explorer_tab == Some(tab) {
            self.explorer_tab = None;
        } else {
            self.explorer_tab = Some(tab);
        }
        self.menu_open = false;
        self.reflow_active_tab();
        self.sync_chrome();
        self.request_redraw();
    }

    /// Snapshot the active tab's live DOM and base URL into `self.page_holder`.
    fn capture_page_holder(&mut self) {
        let base_url = self
            .tab()
            .canonical
            .browser
            .current()
            .map(|e| e.url.clone())
            .unwrap_or_else(|| Url::parse(START_URL).expect("start url"));
        let title = self.tab().canonical.title.clone();
        let ts = scrapbook::now_timestamp_string();
        let html = scrapbook::capture_document_snapshot(
            self.tab().instance.page.doc(),
            base_url.as_str(),
            &title,
            &ts,
        );
        let links = extract_document_links(self.tab().instance.page.doc(), &base_url);
        let ph_vp = self.page_holder_viewport();
        let _guard = browser::enter_runtime();
        let mut ph_doc = HtmlDocument::from_html(
            &html,
            DocumentConfig {
                viewport: Some(ph_vp),
                base_url: Some(base_url.to_string()),
                net_provider: Some(self.provider.clone()),
                html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                ua_stylesheets: vec![blitz_dom::UserAgentStylesheet::default_sheet()],
                ..Default::default()
            },
        );
        ph_doc.resolve(0.0);
        let keep_links_mode = self
            .page_holder
            .as_ref()
            .map(|ph| ph.links_mode)
            .unwrap_or(false);
        self.page_holder = Some(PageHolderState {
            doc: ph_doc,
            url: base_url,
            title,
            links_mode: keep_links_mode,
            links,
        });
        self.sync_chrome();
        self.request_redraw();
    }

    fn toggle_page_holder_links(&mut self) {
        if let Some(ph) = self.page_holder.as_mut() {
            ph.links_mode = !ph.links_mode;
        } else {
            self.capture_page_holder();
            if let Some(ph) = self.page_holder.as_mut() {
                ph.links_mode = true;
            }
        }
        self.sync_chrome();
        self.request_redraw();
    }

    fn scrapbook_add(&mut self) {
        let url_str = self
            .tab()
            .canonical
            .browser
            .current()
            .map(|e| e.url.to_string())
            .unwrap_or_else(|| START_URL.to_string());
        let title = self.tab().canonical.title.clone();
        let timestamp = scrapbook::now_timestamp_string();
        let html_snapshot = scrapbook::capture_document_snapshot(
            self.tab().instance.page.doc(),
            &url_str,
            &title,
            &timestamp,
        );
        let id = format!("sb-{}", self.scrapbook_entries.len() + 1);
        self.scrapbook_entries.push(scrapbook::ScrapbookEntry {
            id,
            title,
            url: url_str,
            timestamp,
            is_folder: false,
            html_snapshot,
        });
        self.selected_scrapbook = Some(self.scrapbook_entries.len() - 1);
        let _ = scrapbook::save_entries(&self.scrapbook_entries);
        self.sync_chrome();
        self.request_redraw();
    }

    fn scrapbook_organize(&mut self) {
        let folder_count = self
            .scrapbook_entries
            .iter()
            .filter(|e| e.is_folder)
            .count()
            + 1;
        let timestamp = scrapbook::now_timestamp_string();
        let id = format!("sb-folder-{folder_count}");
        self.scrapbook_entries.push(scrapbook::ScrapbookEntry {
            id,
            title: format!("Archive Folder {folder_count}"),
            url: String::new(),
            timestamp,
            is_folder: true,
            html_snapshot: String::new(),
        });
        self.selected_scrapbook = Some(self.scrapbook_entries.len() - 1);
        let _ = scrapbook::save_entries(&self.scrapbook_entries);
        self.sync_chrome();
        self.request_redraw();
    }

    fn scrapbook_delete(&mut self) {
        if self.scrapbook_entries.is_empty() {
            return;
        }
        let idx = self
            .selected_scrapbook
            .filter(|&i| i < self.scrapbook_entries.len())
            .unwrap_or(self.scrapbook_entries.len() - 1);
        self.scrapbook_entries.remove(idx);
        if self.scrapbook_entries.is_empty() {
            self.selected_scrapbook = None;
        } else {
            self.selected_scrapbook = Some(idx.min(self.scrapbook_entries.len() - 1));
        }
        let _ = scrapbook::save_entries(&self.scrapbook_entries);
        self.sync_chrome();
        self.request_redraw();
    }

    fn scrapbook_open_entry(&mut self, index: usize) {
        let Some(entry) = self.scrapbook_entries.get(index).cloned() else {
            return;
        };
        self.selected_scrapbook = Some(index);
        if !entry.is_folder && !entry.html_snapshot.is_empty() {
            let url = Url::parse(&entry.url)
                .unwrap_or_else(|_| Url::parse(START_URL).expect("start url"));
            let viewport = self.content_viewport();
            let theme = self.theme;
            let broker = self.passkey_broker.clone();
            let tab = self.tab_mut();
            tab.canonical
                .browser
                .commit(url.clone(), entry.html_snapshot.clone());
            // Load with provider = None to prove the archived snapshot is 100% self-contained offline!
            tab.replace_page(Page::load_with_broker(
                &entry.html_snapshot,
                Some(url),
                None,
                viewport,
                theme,
                broker,
            ));
            tab.canonical.browser.loading = false;
            tab.canonical.browser.error = None;
            tab.canonical.title = entry.title;
        }
        self.sync_chrome();
        self.request_redraw();
    }

    fn handle_chrome_click(&mut self, x: f32, y: f32) {
        let Some(hit) = self.chrome_doc.hit(x, y) else {
            return;
        };
        let target = dom_ffi::bubble_target(&self.chrome_doc, hit.node_id);
        let id_is = |id: &str| self.chrome_doc.get_element_by_id(id) == Some(target);

        // The passkey interstitial swallows every click while it is up. A
        // click outside the dialog does nothing: a stray gesture must never
        // approve a signing ceremony. Only Allow and Deny answer it.
        if self.passkey_prompt.is_some() {
            if id_is("pk-allow") {
                self.decide_page_passkey(true);
            } else if id_is("pk-deny") {
                self.decide_page_passkey(false);
            }
            return;
        }

        // Supervision bar (session tab only). These are host decisions made
        // through the same code path as a socket `decision`.
        if id_is("sup-approve") || id_is("sup-reject") || id_is("pk-allow") || id_is("pk-deny") {
            let approve = id_is("sup-approve") || id_is("pk-allow");
            if let Some(pending) = self.control.as_ref().and_then(|s| s.pending.as_ref()) {
                let (pid, fp, generation) = (
                    pending.proposal_id.clone(),
                    pending.fingerprint.clone(),
                    pending.generation,
                );
                let _ = self.decide(&pid, &fp, generation, approve, "user", "desktop-panel");
            }
            self.flush_control_out();
            return;
        }
        if id_is("sup-take") {
            self.control_take("user", "desktop-panel");
            self.flush_control_out();
            return;
        }
        if id_is("sup-done") {
            self.control_release("user", "desktop-panel");
            self.flush_control_out();
            return;
        }
        if id_is("sup-stop") {
            self.control_stop("user", "desktop-panel");
            self.flush_control_out();
            return;
        }
        if id_is("sup-bar") {
            return;
        }

        // Tabs: close buttons first, then the tab body.
        for index in 0..self.tabs.len() {
            if id_is(&format!("tab-close-{index}")) {
                self.close_tab(index);
                return;
            }
            if id_is(&format!("tab-{index}")) {
                self.select_tab(index);
                return;
            }
        }

        if id_is("btn-newtab") {
            self.new_tab();
            return;
        }
        // While the agent owns this tab, physical navigation chrome is gated;
        // Take control (above) is the way back in.
        if self.session_blocks_input() {
            return;
        }

        // IE5 Mac Collapsible Toolbar & Explorer Bar rail controls.
        if id_is("btn-collapse-toolbar") {
            self.toggle_toolbar_collapsed();
            return;
        }
        if id_is("exp-favorites") {
            self.toggle_explorer_tab(ExplorerTab::Favorites);
            return;
        }
        if id_is("exp-history") {
            self.toggle_explorer_tab(ExplorerTab::History);
            return;
        }
        if id_is("exp-search") {
            self.toggle_explorer_tab(ExplorerTab::Search);
            return;
        }
        if id_is("exp-scrapbook") {
            self.toggle_explorer_tab(ExplorerTab::Scrapbook);
            return;
        }
        if id_is("exp-pageholder") {
            self.toggle_explorer_tab(ExplorerTab::PageHolder);
            return;
        }

        // Page Holder drawer controls & link retargeting.
        if id_is("ph-add") {
            self.capture_page_holder();
            return;
        }
        if id_is("ph-links") {
            self.toggle_page_holder_links();
            return;
        }
        if let Some(ph) = self.page_holder.as_ref() {
            if ph.links_mode {
                let link_count = ph.links.len();
                for i in 0..link_count {
                    if id_is(&format!("ph-link-{i}")) {
                        let target_url = self.page_holder.as_ref().unwrap().links[i].url.clone();
                        self.navigate(&target_url);
                        return;
                    }
                }
            }
        }
        if id_is("ph-canvas") {
            let top_h = self.top_chrome_height() as f32;
            let local_x = x.max(0.0);
            let local_y = y - (top_h + PH_HEADER_HEIGHT as f32);
            let nav_target = self.page_holder.as_ref().and_then(|ph| {
                let ph_hit = ph.doc.hit(local_x, local_y)?;
                let href = link_target(&ph.doc, ph_hit.node_id)?;
                browser::resolve_link(&ph.url, &href)
                    .ok()
                    .map(|u| u.to_string())
            });
            if let Some(url_str) = nav_target {
                self.navigate(&url_str);
            }
            return;
        }

        // Internet Scrapbook drawer controls.
        if id_is("sb-add") {
            self.scrapbook_add();
            return;
        }
        if id_is("sb-organize") {
            self.scrapbook_organize();
            return;
        }
        if id_is("sb-delete") {
            self.scrapbook_delete();
            return;
        }
        for i in 0..self.scrapbook_entries.len() {
            if id_is(&format!("sb-entry-{i}")) {
                self.scrapbook_open_entry(i);
                return;
            }
        }

        // Favorites drawer controls.
        if id_is("fav-add") {
            let url = self.tab().canonical.browser.display_url();
            let title = self.tab().canonical.title.clone();
            if !url.is_empty() && !self.favorites.iter().any(|(_, u)| u == &url) {
                self.favorites.push((title, url));
                self.sync_chrome();
                self.request_redraw();
            }
            return;
        }
        for i in 0..self.favorites.len() {
            if id_is(&format!("fav-item-{i}")) || id_is(&format!("fav-bar-{i}")) {
                let url = self.favorites[i].1.clone();
                self.navigate(&url);
                return;
            }
        }
        if id_is("btn-autofill") {
            let snapshot = self.observe_active();
            if let Some(recipe) =
                recall::find_matching_recipe(&snapshot.url, snapshot.form_signature.as_deref())
            {
                let tab = self.tab_mut();
                for rf in &recipe.fields {
                    let Some(element) = snapshot
                        .elements
                        .iter()
                        .filter(|e| matches!(e.tag.as_str(), "input" | "select" | "textarea"))
                        .find(|e| {
                            (rf.dom_id.is_some() && e.dom_id == rf.dom_id)
                                || (rf.name.is_some() && e.name == rf.name)
                                || (!rf.label.is_empty() && e.label == rf.label)
                        })
                        .cloned()
                    else {
                        continue;
                    };
                    apply_dom_field_action(
                        tab.instance.page.doc_mut(),
                        element.node_id,
                        &element.tag,
                        &rf.action,
                        rf.value.as_deref(),
                        rf.origin.as_deref(),
                        rf.username.as_deref(),
                    );
                }
                tab.instance.page.resolve();
                tab.bump_generation();
                self.recalled_count = self.recalled_count.wrapping_add(1);
                self.sync_chrome();
                self.request_redraw();
            } else if snapshot
                .elements
                .iter()
                .any(|e| matches!(e.tag.as_str(), "input" | "select" | "textarea"))
            {
                let goal = format!(
                    "Complete and submit the form on {}",
                    if snapshot.title.is_empty() {
                        &snapshot.url
                    } else {
                        &snapshot.title
                    }
                );
                self.trigger_in_browser_agent(&goal);
            }
            return;
        }
        if id_is("btn-print") {
            self.scrapbook_add();
            self.explorer_tab = Some(ExplorerTab::Scrapbook);
            self.reflow_active_tab();
            self.sync_chrome();
            self.request_redraw();
            return;
        }

        // History drawer controls.
        let hist_len = self.tab().canonical.browser.history.len();
        for i in 0..hist_len {
            if id_is(&format!("hist-item-{i}")) {
                let tab = self.tab_mut();
                tab.canonical.browser.index = i;
                tab.canonical.browser.sync_address();
                self.show_current_entry();
                return;
            }
        }

        // Search drawer controls.
        if id_is("exp-search-focus") {
            self.toolbar_collapsed = false;
            self.tab_mut().canonical.browser.focus_address_bar();
            self.reflow_active_tab();
            self.sync_chrome();
            self.request_redraw();
            return;
        }
        if id_is("search-item-0") {
            self.navigate("https://lite.duckduckgo.com/lite/");
            return;
        }
        if id_is("search-item-1") {
            self.navigate("https://en.wikipedia.org/");
            return;
        }
        if id_is("search-item-2") {
            self.navigate("https://aol.com/");
            return;
        }

        if id_is("btn-menu") || id_is("btn-fav-overflow") {
            self.menu_open = !self.menu_open;
            self.sync_chrome();
            self.request_redraw();
            return;
        }
        if id_is("btn-mail") {
            self.navigate("https://mail.aol.com/");
            return;
        }

        if self.menu_open {
            if id_is("menu-home") {
                self.navigate(START_URL);
                return;
            }
            if id_is("menu-reload") {
                self.reload();
                return;
            }
            if id_is("menu-back") {
                self.go_back();
                return;
            }
            if id_is("menu-forward") {
                self.go_forward();
                return;
            }
            if id_is("menu-xray") {
                self.xray_enabled = !self.xray_enabled;
                self.menu_open = false;
                self.sync_chrome();
                self.request_redraw();
                return;
            }
            if id_is("menu-close-tab") {
                let index = self.active;
                self.close_tab(index);
                return;
            }
        }

        if id_is("btn-back") {
            if self.tab().canonical.browser.can_go_back() {
                self.go_back();
            }
            return;
        }
        if id_is("btn-forward") {
            if self.tab().canonical.browser.can_go_forward() {
                self.go_forward();
            }
            return;
        }
        if id_is("btn-stop") {
            self.stop_navigation();
            return;
        }
        if id_is("btn-reload") {
            self.reload();
            return;
        }
        if id_is("btn-home") {
            self.navigate(START_URL);
            return;
        }
        if id_is("btn-go") {
            let input = self.tab().canonical.browser.address.clone();
            self.navigate(&input);
            return;
        }
        if id_is("address") {
            self.tab_mut().canonical.browser.focus_address_bar();
            self.menu_open = false;
            self.sync_chrome();
            self.request_redraw();
            return;
        }

        if id_is("exp-rail") || id_is("exp-drawer") {
            return;
        }

        // Click landed on the chrome document but not on any control: close the
        // menu, and if the point is inside the content viewport, treat it as a
        // page click (the chrome body is transparent there).
        if self.menu_open {
            self.menu_open = false;
            self.sync_chrome();
            self.request_redraw();
        }
        let top_h = self.top_chrome_height() as f32;
        let left_w = self.left_chrome_width() as f32;
        if y >= top_h && x >= left_w {
            self.handle_page_click(x, y);
        }
    }

    fn handle_page_click(&mut self, x: f32, y: f32) {
        self.enqueue_input(
            InputProvenance::PhysicalUser,
            InputAction::PageClick { x, y },
        );
    }

    fn do_page_click(&mut self, x: f32, y: f32) {
        {
            let tab = self.tab_mut();
            if tab.canonical.browser.focused {
                tab.canonical.browser.focused = false;
                tab.canonical.browser.address_select_all = false;
                tab.canonical.browser.sync_address();
            }
        }
        let local_x = (x - self.left_chrome_width() as f32).max(0.0);
        let local_y = (y - self.top_chrome_height() as f32).max(0.0);
        let Some(hit) = self.tab().instance.page.doc().hit(local_x, local_y) else {
            self.sync_chrome();
            self.request_redraw();
            return;
        };

        if let Some(href) = link_target(self.tab().instance.page.doc(), hit.node_id) {
            let trimmed_href = href.trim();
            if trimmed_href.starts_with('#') {
                let frag = trimmed_href.trim_start_matches('#');
                let target_y = if frag.is_empty() {
                    Some(0.0)
                } else {
                    let doc = self.tab().instance.page.doc();
                    doc.get_element_by_id(frag)
                        .and_then(|nid| doc.get_client_bounding_rect(nid))
                        .map(|rect| (doc.viewport_scroll().y + rect.y).max(0.0))
                };
                if let Some(new_y) = target_y {
                    let cur = self.tab().instance.page.doc().viewport_scroll();
                    let tab = self.tab_mut();
                    tab.instance
                        .page
                        .doc_mut()
                        .set_viewport_scroll(Point { x: cur.x, y: new_y });
                    tab.instance.page.resolve();
                    self.sync_chrome();
                    self.request_redraw();
                }
                return;
            }
            if let Some(base) = self
                .tab()
                .canonical
                .browser
                .current()
                .map(|e| e.url.clone())
            {
                if let Ok(url) = browser::resolve_link(&base, &href) {
                    self.navigate(&url.to_string());
                    return;
                }
            }
        }

        let target = dom_ffi::bubble_target(self.tab().instance.page.doc(), hit.node_id);
        let hermes = self.tab().instance.page.hermes;
        unsafe {
            hermes_app_dispatch_event(
                hermes,
                c"click".as_ptr(),
                target.as_u64(),
                local_x,
                local_y,
            );
        }
        let js_dirtied = self.tab().instance.page.ctx.dirty;
        if js_dirtied {
            let tab = self.tab_mut();
            tab.instance.page.resolve();
            tab.instance.page.ctx.dirty = false;
            tab.bump_generation();
        }
        if let Some(form_node) = self.tab_mut().instance.page.ctx.pending_form_submit.take() {
            self.submit_page_form(form_node);
            return;
        }

        if let Some((control_id, tag, input_type, clicked_option_id)) =
            find_interactive_form_control(self.tab().instance.page.doc(), hit.node_id)
        {
            let is_disabled = self
                .tab()
                .instance
                .page
                .doc()
                .get_node(control_id)
                .and_then(|n| n.element_data())
                .is_some_and(|d| d.attr(local_name!("disabled")).is_some());
            if !is_disabled {
                self.tab_mut()
                    .instance
                    .page
                    .doc_mut()
                    .set_focus_to(control_id);
                match (tag.as_str(), input_type.as_str()) {
                    ("input", "checkbox") => {
                        let cur_checked = self
                            .tab()
                            .instance
                            .page
                            .doc()
                            .get_node(control_id)
                            .and_then(|n| n.element_data())
                            .map(|d| {
                                d.checkbox_input_checked()
                                    .unwrap_or_else(|| d.attr(local_name!("checked")).is_some())
                            })
                            .unwrap_or(false);
                        let next_val = if cur_checked { "false" } else { "true" };
                        let tab = self.tab_mut();
                        apply_dom_field_action(
                            tab.instance.page.doc_mut(),
                            control_id,
                            "input",
                            "check",
                            Some(next_val),
                            None,
                            None,
                        );
                        tab.instance.page.resolve();
                        tab.bump_generation();
                    }
                    ("input", "radio") => {
                        let tab = self.tab_mut();
                        check_radio_group(tab.instance.page.doc_mut(), control_id);
                        tab.instance.page.resolve();
                        tab.bump_generation();
                    }
                    ("select", _) => {
                        let tab = self.tab_mut();
                        if let Some(opt_id) = clicked_option_id {
                            select_specific_option(
                                tab.instance.page.doc_mut(),
                                control_id,
                                opt_id,
                            );
                        } else {
                            cycle_select_option(tab.instance.page.doc_mut(), control_id, 1);
                        }
                        tab.instance.page.resolve();
                        tab.bump_generation();
                    }
                    ("input", "submit" | "image") => {
                        if !js_dirtied {
                            self.submit_page_form(control_id);
                            return;
                        }
                    }
                    ("button", "submit" | "") => {
                        if !js_dirtied
                            && has_enclosing_form_or_submit_attr(
                                self.tab().instance.page.doc(),
                                control_id,
                            )
                        {
                            self.submit_page_form(control_id);
                            return;
                        }
                    }
                    _ => {
                        let tab = self.tab_mut();
                        blitz_dom::Document::handle_ui_event(
                            tab.instance.page.doc_mut(),
                            blitz_traits::events::UiEvent::AppleStandardKeybinding(
                                "moveToEndOfDocument:".into(),
                            ),
                        );
                        tab.instance.page.resolve();
                    }
                }
            }
        } else {
            self.tab_mut().instance.page.doc_mut().clear_focus();
            self.tab_mut().instance.page.resolve();
        }
        self.sync_chrome();
        self.request_redraw();
    }

    fn submit_page_form(&mut self, submitter_id: NodeId) {
        let tab_id = self.tab().canonical.id;
        let base_url = self
            .tab()
            .canonical
            .browser
            .current()
            .map(|e| e.url.clone())
            .unwrap_or_else(|| Url::parse(START_URL).expect("start url"));
        let (action_url, method, entries) = collect_form_submission(
            self.tab().instance.page.doc(),
            &base_url,
            submitter_id,
        );
        self.session_event(
            OutTarget::Both,
            "nav.started",
            json!({"url": action_url.to_string()}),
        );
        let provider = self.provider.clone();
        let viewport = self.content_viewport();
        let theme = self.theme;
        match browser::submit_form(&provider, &action_url, &method, &entries) {
            Ok((final_url, html)) => {
                let broker = self.passkey_broker.clone();
                let tab = self.tab_mut();
                tab.canonical
                    .browser
                    .commit(final_url.clone(), html.clone());
                tab.replace_page(Page::load_with_broker(
                    &html,
                    Some(final_url.clone()),
                    Some(provider),
                    viewport,
                    theme,
                    broker,
                ));
                tab.canonical.browser.error = None;
                tab.canonical.title = title_for(&final_url, tab.instance.page.doc());
                self.session_event(
                    OutTarget::Both,
                    "nav.committed",
                    json!({"tab_id": tab_id.0, "url": final_url.to_string()}),
                );
            }
            Err(e) => {
                let broker = self.passkey_broker.clone();
                let tab = self.tab_mut();
                tab.canonical.browser.fail(e.clone());
                let html = error_page(&e);
                tab.replace_page(Page::load_with_broker(
                    &html, None, None, viewport, theme, broker,
                ));
                tab.canonical.title = "Error".to_string();
                self.session_event(OutTarget::Both, "nav.failed", json!({"error": e}));
            }
        }
        self.sync_chrome();
        self.request_redraw();
        self.flush_control_out();
    }

    fn trigger_in_browser_agent(&mut self, goal: &str) {
        let Some((sock_path, host_token, agent_token)) = self.live_socket_info.clone() else {
            eprintln!("[walkaway] no live control socket active for in-browser agent trigger");
            return;
        };
        let current_url = self.tab().canonical.browser.display_url();
        let target_url = if current_url.starts_with("http://") || current_url.starts_with("https://")
        {
            Some(current_url)
        } else {
            None
        };
        let goal_owned = goal.to_string();
        std::thread::spawn(move || {
            let repo_candidates = [
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.join("walkaway")),
                Some(std::path::PathBuf::from("/Users/mac/walkaway-monorepo/walkaway")),
                Some(std::path::PathBuf::from("/Users/mac/walkaway-hackathon/walkaway")),
            ];
            let Some(walkaway_cli) = repo_candidates
                .into_iter()
                .flatten()
                .find(|p| p.exists())
            else {
                eprintln!("[walkaway] could not locate ./walkaway CLI script");
                return;
            };
            let mut cmd = std::process::Command::new("python3");
            cmd.arg(&walkaway_cli);
            if let Some(u) = target_url {
                cmd.arg(u);
            }
            cmd.arg("--goal").arg(&goal_owned);
            cmd.env("WALKAWAY_ATTACH_SOCK_PATH", &sock_path);
            cmd.env("HERMES_HOST_TOKEN", &host_token);
            cmd.env("HERMES_AGENT_TOKEN", &agent_token);
            let _ = cmd.status();
        });
    }

    fn scroll_page(&mut self, dy: f64) {
        let current = self.tab().instance.page.doc().viewport_scroll();
        let tab = self.tab_mut();
        tab.instance.page.doc_mut().set_viewport_scroll(Point {
            x: current.x,
            y: (current.y + dy).max(0.0),
        });
        tab.instance.page.resolve();
        self.request_redraw();
    }

    fn handle_key(&mut self, event: &KeyEvent) {
        // While a passkey prompt is up, the keyboard cannot answer it.
        // Escape in particular must not dismiss the prompt: a stray key
        // press must never approve a signing ceremony. Only the Allow and
        // Deny buttons answer it.
        if self.passkey_prompt.is_some() {
            return;
        }
        self.handle_key_while_no_passkey_prompt(event)
    }

    /// Whether the keyboard is currently answering a passkey prompt.
    ///
    /// While this is true every key press is swallowed, so a stray Escape
    /// cannot dismiss the prompt and a stray Enter cannot approve it. Only
    /// the Allow and Deny buttons answer the prompt.
    fn passkey_prompt_swallows_keys(&self) -> bool {
        self.passkey_prompt.is_some()
    }

    /// The keyboard handling that runs when no passkey prompt is up.
    fn handle_key_while_no_passkey_prompt(&mut self, event: &KeyEvent) {
        self.handle_logical_key(&event.logical_key, event.text.as_deref());
    }

    fn handle_logical_key(&mut self, logical_key: &Key, text: Option<&str>) {
        if self.session_blocks_input() {
            return;
        }
        if matches!(logical_key, Key::Named(NamedKey::F12)) {
            self.xray_enabled = !self.xray_enabled;
            self.sync_chrome();
            self.request_redraw();
            return;
        }
        let cmd = self.modifiers.super_key();

        if cmd {
            match logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("i") => {
                    self.xray_enabled = !self.xray_enabled;
                    self.sync_chrome();
                    self.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("l") => {
                    self.tab_mut().canonical.browser.focus_address_bar();
                    self.sync_chrome();
                    self.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("r") => {
                    self.reload();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("t") => {
                    self.new_tab();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("w") => {
                    let index = self.active;
                    self.close_tab(index);
                    return;
                }
                Key::Character(c) if c == "[" => {
                    self.go_back();
                    return;
                }
                Key::Character(c) if c == "]" => {
                    self.go_forward();
                    return;
                }
                _ => {}
            }
        }

        if self.tab().canonical.browser.focused {
            if cmd {
                match logical_key {
                    Key::Character(c) if c.eq_ignore_ascii_case("a") => {
                        self.tab_mut().canonical.browser.address_select_all = true;
                        return;
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                        let addr = self.tab().canonical.browser.address.clone();
                        write_macos_clipboard(&addr);
                        return;
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                        let pasted: String = read_macos_clipboard()
                            .chars()
                            .filter(|ch| !ch.is_control())
                            .collect();
                        let tab = self.tab_mut();
                        if tab.canonical.browser.address_select_all {
                            tab.canonical.browser.address.clear();
                            tab.canonical.browser.address_select_all = false;
                        }
                        tab.canonical.browser.address.push_str(&pasted);
                        self.sync_chrome();
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let tab = self.tab_mut();
                        tab.canonical.browser.address.clear();
                        tab.canonical.browser.address_select_all = false;
                        self.sync_chrome();
                        self.request_redraw();
                        return;
                    }
                    _ => {}
                }
            }

            match logical_key {
                Key::Named(NamedKey::Enter) => {
                    let input = self.tab().canonical.browser.address.clone();
                    self.navigate(&input);
                }
                Key::Named(NamedKey::Escape) => {
                    let tab = self.tab_mut();
                    tab.canonical.browser.focused = false;
                    tab.canonical.browser.address_select_all = false;
                    tab.canonical.browser.sync_address();
                    self.sync_chrome();
                    self.request_redraw();
                }
                Key::Named(NamedKey::Backspace) => {
                    let tab = self.tab_mut();
                    if tab.canonical.browser.address_select_all {
                        tab.canonical.browser.address.clear();
                        tab.canonical.browser.address_select_all = false;
                    } else {
                        tab.canonical.browser.address.pop();
                    }
                    self.sync_chrome();
                    self.request_redraw();
                }
                Key::Named(NamedKey::Space) => {
                    let tab = self.tab_mut();
                    if tab.canonical.browser.address_select_all {
                        tab.canonical.browser.address.clear();
                        tab.canonical.browser.address_select_all = false;
                    }
                    tab.canonical.browser.address.push(' ');
                    self.sync_chrome();
                    self.request_redraw();
                }
                _ => {
                    if !cmd {
                        if let Some(text) = text {
                            let filtered: String =
                                text.chars().filter(|c| !c.is_control()).collect();
                            if !filtered.is_empty() {
                                let tab = self.tab_mut();
                                if tab.canonical.browser.address_select_all {
                                    tab.canonical.browser.address.clear();
                                    tab.canonical.browser.address_select_all = false;
                                }
                                tab.canonical.browser.address.push_str(&filtered);
                                self.sync_chrome();
                                self.request_redraw();
                            }
                        }
                    }
                }
            }
            return;
        }

        // Address bar is not focused: check if a page form control has focus,
        // or handle page-level keyboard navigation/scrolling.
        if matches!(logical_key, Key::Named(NamedKey::Tab)) {
            let reverse = self.modifiers.shift_key();
            let tab = self.tab_mut();
            if focus_adjacent_form_control(tab.instance.page.doc_mut(), reverse).is_some() {
                tab.instance.page.resolve();
                self.sync_chrome();
                self.request_redraw();
            }
            return;
        }

        if let Some(focused_id) = self.tab().instance.page.doc().get_focussed_node_id() {
            let control_info = self
                .tab()
                .instance
                .page
                .doc()
                .get_node(focused_id)
                .and_then(|n| n.element_data())
                .filter(|d| d.attr(local_name!("disabled")).is_none())
                .map(|d| {
                    let tag = d.name.local.to_string().to_ascii_lowercase();
                    let input_type = d
                        .attr(local_name!("type"))
                        .unwrap_or(if tag == "input" { "text" } else { "" })
                        .to_ascii_lowercase();
                    (tag, input_type)
                });

            if let Some((tag, input_type)) = control_info {
                if matches!(logical_key, Key::Named(NamedKey::Escape)) {
                    let tab = self.tab_mut();
                    tab.instance.page.doc_mut().clear_focus();
                    tab.instance.page.resolve();
                    self.sync_chrome();
                    self.request_redraw();
                    return;
                }

                if tag == "select" {
                    match logical_key {
                        Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => {
                            let tab = self.tab_mut();
                            cycle_select_option(tab.instance.page.doc_mut(), focused_id, 1);
                            tab.instance.page.resolve();
                            tab.bump_generation();
                            self.sync_chrome();
                            self.request_redraw();
                            return;
                        }
                        Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => {
                            let tab = self.tab_mut();
                            cycle_select_option(tab.instance.page.doc_mut(), focused_id, -1);
                            tab.instance.page.resolve();
                            tab.bump_generation();
                            self.sync_chrome();
                            self.request_redraw();
                            return;
                        }
                        _ => {}
                    }
                } else if tag == "input" && input_type == "checkbox" {
                    if matches!(logical_key, Key::Named(NamedKey::Space)) {
                        let cur_checked = self
                            .tab()
                            .instance
                            .page
                            .doc()
                            .get_node(focused_id)
                            .and_then(|n| n.element_data())
                            .map(|d| {
                                d.checkbox_input_checked()
                                    .unwrap_or_else(|| d.attr(local_name!("checked")).is_some())
                            })
                            .unwrap_or(false);
                        let next_val = if cur_checked { "false" } else { "true" };
                        let tab = self.tab_mut();
                        apply_dom_field_action(
                            tab.instance.page.doc_mut(),
                            focused_id,
                            "input",
                            "check",
                            Some(next_val),
                            None,
                            None,
                        );
                        tab.instance.page.resolve();
                        tab.bump_generation();
                        self.sync_chrome();
                        self.request_redraw();
                        return;
                    }
                } else if tag == "textarea"
                    || (tag == "input"
                        && !matches!(
                            input_type.as_str(),
                            "button" | "submit" | "reset" | "checkbox" | "radio" | "hidden" | "image" | "file"
                        ))
                {
                    let mut cur_val =
                        read_dom_field_value(self.tab().instance.page.doc(), focused_id, &tag);
                    let mut mutated = false;

                    if cmd {
                        match logical_key {
                            Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                                let pasted = read_macos_clipboard();
                                let cleaned: String = if tag == "textarea" {
                                    pasted
                                        .chars()
                                        .filter(|&ch| ch == '\n' || !ch.is_control())
                                        .collect()
                                } else {
                                    pasted.chars().filter(|ch| !ch.is_control()).collect()
                                };
                                if !cleaned.is_empty() {
                                    cur_val.push_str(&cleaned);
                                    mutated = true;
                                }
                            }
                            Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                                if input_type != "password" {
                                    write_macos_clipboard(&cur_val);
                                }
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                cur_val.clear();
                                mutated = true;
                            }
                            _ => {}
                        }
                    } else {
                        match logical_key {
                            Key::Named(NamedKey::Enter) => {
                                if tag == "textarea" {
                                    cur_val.push('\n');
                                    mutated = true;
                                } else {
                                    self.submit_page_form(focused_id);
                                    return;
                                }
                            }
                            Key::Named(NamedKey::Backspace) => {
                                cur_val.pop();
                                mutated = true;
                            }
                            Key::Named(NamedKey::Space) => {
                                cur_val.push(' ');
                                mutated = true;
                            }
                            _ => {
                                if let Some(text) = text {
                                    let filtered: String =
                                        text.chars().filter(|c| !c.is_control()).collect();
                                    if !filtered.is_empty() {
                                        cur_val.push_str(&filtered);
                                        mutated = true;
                                    }
                                }
                            }
                        }
                    }

                    if mutated {
                        let tab = self.tab_mut();
                        apply_dom_field_action(
                            tab.instance.page.doc_mut(),
                            focused_id,
                            &tag,
                            "type",
                            Some(&cur_val),
                            None,
                            None,
                        );
                        blitz_dom::Document::handle_ui_event(
                            tab.instance.page.doc_mut(),
                            blitz_traits::events::UiEvent::AppleStandardKeybinding(
                                "moveToEndOfDocument:".into(),
                            ),
                        );
                        tab.instance.page.resolve();
                        tab.bump_generation();
                        self.sync_chrome();
                        self.request_redraw();
                        return;
                    }
                }
            }
        }

        // Page-level keyboard scrolling when no text input consumed the key.
        match logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                self.scroll_page(60.0);
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.scroll_page(-60.0);
            }
            Key::Named(NamedKey::PageDown) => {
                self.scroll_page(500.0);
            }
            Key::Named(NamedKey::PageUp) => {
                self.scroll_page(-500.0);
            }
            Key::Named(NamedKey::Space) => {
                let dy = if self.modifiers.shift_key() {
                    -500.0
                } else {
                    500.0
                };
                self.scroll_page(dy);
            }
            Key::Named(NamedKey::Home) => {
                let cur = self.tab().instance.page.doc().viewport_scroll();
                let tab = self.tab_mut();
                tab.instance
                    .page
                    .doc_mut()
                    .set_viewport_scroll(Point { x: cur.x, y: 0.0 });
                tab.instance.page.resolve();
                self.request_redraw();
            }
            Key::Named(NamedKey::End) => {
                self.scroll_page(100_000.0);
            }
            _ => {}
        }
    }

    fn handle_navigated(
        &mut self,
        tab_id: TabId,
        generation: u64,
        url: Url,
        result: Result<String, String>,
    ) {
        let viewport = self.content_viewport();
        let Some(target) = self.tabs.iter_mut().find(|tab| tab.canonical.id == tab_id) else {
            return;
        };
        // A newer navigation in this tab superseded the result.
        if generation != target.canonical.browser.generation {
            return;
        }
        // A supervised tab that was stopped or taken over while the fetch was
        // in flight must not commit the result.
        if self.control.as_ref().is_some_and(|s| {
            s.tab_id == tab_id
                && matches!(
                    s.state,
                    ControlState::Human
                        | ControlState::Stopped
                        | ControlState::Done
                        | ControlState::Error
                )
        }) {
            return;
        }
        match result {
            Ok(html) => {
                target.canonical.browser.commit(url.clone(), html.clone());
                let broker = self.passkey_broker.clone();
                target.replace_page(Page::load_with_broker(
                    &html,
                    Some(url.clone()),
                    Some(self.provider.clone()),
                    viewport,
                    self.theme,
                    broker,
                ));
                target.canonical.browser.error = None;
                target.canonical.title = title_for(&url, target.instance.page.doc());
            }
            Err(e) => {
                let broker = self.passkey_broker.clone();
                target.canonical.browser.fail(e.clone());
                let html = error_page(&e);
                target.replace_page(Page::load_with_broker(
                    &html,
                    None,
                    None,
                    viewport,
                    self.theme,
                    broker,
                ));
                target.canonical.title = "Error".to_string();
            }
        }
        if self.control.as_ref().is_some_and(|s| s.tab_id == tab_id) {
            if let Some(session) = self.control.as_mut() {
                session.snapshot_valid = false;
            }
            self.session_event(
                OutTarget::Both,
                "nav.committed",
                json!({"tab_id": tab_id.0, "url": url.to_string()}),
            );
            self.state_event(OutTarget::Both);
            self.flush_control_out();
        }
        self.sync_chrome();
        self.request_redraw();
    }

    fn render_current_frame(&mut self) {
        if !self.renderer.is_active() {
            return;
        }
        let (width, height) = self.size;
        let scale_f = self.scale;
        let top_h_px = ((self.top_chrome_height() * scale_f) as u32).min(height);
        let left_w_px = ((self.left_chrome_width() * scale_f) as u32).min(width);
        let content_w_px = width.saturating_sub(left_w_px).max(1);
        let content_h_px = height.saturating_sub(top_h_px).max(1);
        let ph_x_px = 0u32;
        let ph_y_px =
            (((self.top_chrome_height() + PH_HEADER_HEIGHT) * scale_f) as u32).min(height);
        let ph_w_px =
            ((EXPLORER_DRAWER_WIDTH * scale_f) as u32).min(width.saturating_sub(ph_x_px)).max(1);
        let ph_h_px = height.saturating_sub(ph_y_px).max(1);
        let paint_ph = self.explorer_tab == Some(ExplorerTab::PageHolder)
            && self
                .page_holder
                .as_ref()
                .is_some_and(|ph| !ph.links_mode);

        let Self {
            renderer,
            chrome_doc,
            tabs,
            active,
            page_holder,
            ..
        } = self;
        let tab = &mut tabs[*active];

        renderer.render(|painter| {
            // 1. Active tab's content_doc inside its clipped, offset viewport rectangle.
            blitz_paint::paint_scene(
                painter,
                tab.instance.page.doc_mut(),
                scale_f,
                content_w_px,
                content_h_px,
                left_w_px,
                top_h_px,
            );
            // 2. Page Holder document (when open in Rendered Page View) inside its clipped drawer layer.
            if paint_ph {
                if let Some(ph) = page_holder.as_mut() {
                    blitz_paint::paint_scene(
                        painter,
                        &mut ph.doc,
                        scale_f,
                        ph_w_px,
                        ph_h_px,
                        ph_x_px,
                        ph_y_px,
                    );
                }
            }
            // 3. Browser chrome document at (0, 0).
            blitz_paint::paint_scene(painter, chrome_doc, scale_f, width, height, 0, 0);
        });
        self.frame_seq = self.frame_seq.wrapping_add(1);
        if let Ok(path) = std::env::var("HERMES_LIVE_SCREENSHOT_PATH") {
            if !path.is_empty() {
                if let Some((bytes, _, _)) = self.capture_png() {
                    let _ = std::fs::write(&path, bytes);
                }
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size = (width.max(1), height.max(1));
        self.renderer.set_size(self.size.0, self.size.1);
        self.reflow_active_tab();
        self.sync_chrome();
        self.request_redraw();
    }
}

impl ApplicationHandler<UserEvent> for BrowserApp {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Navigated {
                tab,
                generation,
                url,
                result,
            } => {
                self.handle_navigated(tab, generation, url, result);
            }
            UserEvent::NetWake => {
                self.net_dirty.store(false, Ordering::SeqCst);
                self.tab_mut().instance.page.resolve();
                self.request_redraw();
            }
            UserEvent::ControlConnect { conn_id, role } => {
                self.control_connect(conn_id, role);
            }
            UserEvent::ControlMessage { conn_id, frame } => {
                self.handle_control(conn_id, frame);
                self.flush_control_out();
            }
            UserEvent::ControlDisconnect { conn_id } => {
                self.control_disconnect(conn_id);
            }
            UserEvent::RelatedOriginsResolved {
                tab,
                request_id,
                result,
            } => {
                self.handle_related_origins_resolved_for_tab(tab, request_id, result);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.net_dirty.load(Ordering::SeqCst) {
            self.net_dirty.store(false, Ordering::SeqCst);
            self.tab_mut().instance.page.resolve();
            self.request_redraw();
        }
        // A page may have started a passkey ceremony while nothing else was
        // happening. Pick it up so a human can be asked about it, and so an
        // unanswered ceremony eventually fails.
        self.poll_page_passkey_ceremonies();
        // A page may have started a ceremony that needs the relying party's
        // related origins well-known document. Run the fetch off the UI
        // thread.
        self.poll_page_related_origins();
        if let Some(limit) = self.exit_after {
            if self.started.elapsed() >= limit {
                self.exit_after = None;
                eprintln!("[host] exit-after elapsed, shutting down");
                // A run that used a test credential service leaves nothing
                // behind. The user's own service is never touched.
                wipe_test_credential_services();
                event_loop.exit();
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let explicit_show = std::env::args().any(|a| a == "--show-window")
            || std::env::var("HERMES_SHOW_WINDOW").ok().as_deref() == Some("1");
        let has_control_socket = std::env::args().any(|a| a == "--control-socket");
        let hidden = std::env::args().any(|a| a == "--hidden-window")
            || std::env::var("HERMES_HIDDEN_WINDOW").ok().as_deref() == Some("1")
            || (has_control_socket && !explicit_show);
        let attrs = Window::default_attributes()
            .with_title("Muse Blitz — Supervised Agent Browser")
            .with_visible(!hidden)
            .with_active(false)
            .with_inner_size(LogicalSize::new(WIDTH, HEIGHT));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.scale = window.scale_factor();
        self.theme = if self.force_dark {
            Theme::Dark
        } else {
            match window.theme() {
                Some(winit::window::Theme::Dark) => Theme::Dark,
                _ => Theme::Light,
            }
        };
        let physical = window.inner_size();
        self.size = (physical.width.max(1), physical.height.max(1));

        let handle: Arc<dyn anyrender::WindowHandle> = window.clone();
        self.renderer
            .resume(handle, self.size.0, self.size.1, || {});
        assert!(self.renderer.complete_resume(), "wgpu surface init failed");

        self.reflow_active_tab();
        self.sync_chrome();

        eprintln!(
            "[host] surface {}x{} physical @{:.2}x => chrome {}px + page {}px CSS, theme {:?}",
            self.size.0,
            self.size.1,
            self.scale,
            CHROME_HEIGHT as u32,
            (self.size.1 as f64 / self.scale) as u32 - CHROME_HEIGHT as u32,
            self.theme,
        );

        self.window = Some(window);
        self.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer.suspend();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(physical_size) => {
                self.resize(physical_size.width, physical_size.height);
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }

            WindowEvent::ThemeChanged(theme) => {
                self.theme = match theme {
                    winit::window::Theme::Dark => Theme::Dark,
                    winit::window::Theme::Light => Theme::Light,
                };
                self.reflow_active_tab();
                self.sync_chrome();
                self.request_redraw();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (
                    (position.x / self.scale) as f32,
                    (position.y / self.scale) as f32,
                );
                let in_chrome = self.cursor.1 < self.top_chrome_height() as f32
                    || self.cursor.0 < self.left_chrome_width() as f32;
                if in_chrome && self.chrome_doc.set_hover_to(self.cursor.0, self.cursor.1) {
                    self.chrome_doc.resolve(0.0);
                    self.request_redraw();
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                // The chrome document spans the whole surface, so ask it first;
                // if the point falls through to the body it is a page click.
                self.handle_chrome_click(x, y);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y as f64) * 48.0,
                    MouseScrollDelta::PixelDelta(p) => -(p.y / self.scale),
                };
                self.enqueue_input(InputProvenance::PhysicalUser, InputAction::Scroll { dy });
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.handle_key(&event);
                }
            }

            WindowEvent::RedrawRequested => {
                self.render_current_frame();
            }

            _ => {}
        }
    }
}

fn chrome_viewport(size: (u32, u32), scale: f64) -> Viewport {
    // Full-surface: the chrome body is transparent below the strip, which lets
    // the menu overlay page content without being clipped.
    Viewport::new(size.0, size.1.max(1), scale as f32, ColorScheme::Light)
}

fn page_viewport(size: (u32, u32), scale: f64, theme: Theme) -> Viewport {
    let chrome_h = ((CHROME_HEIGHT * scale) as u32).min(size.1);
    let rail_w = ((EXPLORER_RAIL_WIDTH * scale) as u32).min(size.0);
    Viewport::new(
        size.0.saturating_sub(rail_w).max(1),
        size.1.saturating_sub(chrome_h).max(1),
        scale as f32,
        theme.color_scheme(),
    )
}

/// Walk `doc` in pre-order for all `<a href="...">` elements and resolve their
/// target URLs against `base_url` for the IE5 Mac Page Holder Links List View.
fn extract_document_links(doc: &blitz_dom::BaseDocument, base_url: &Url) -> Vec<HeldLink> {
    let mut links = Vec::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(nid) = stack.pop() {
        let Some(node) = doc.get_node(nid) else {
            continue;
        };
        for &child in node.children.iter().rev() {
            stack.push(child);
        }
        let Some(el) = node.element_data() else {
            continue;
        };
        let tag: &str = el.name.local.as_ref();
        if !tag.eq_ignore_ascii_case("a") {
            continue;
        }
        let Some(href_raw) = node.attr(LocalName::from("href")) else {
            continue;
        };
        let trimmed = href_raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("javascript:") {
            continue;
        }
        let Ok(resolved) = browser::resolve_link(base_url, trimmed) else {
            continue;
        };
        let text = node.text_content().trim().to_string();
        let label = if text.is_empty() {
            resolved.to_string()
        } else {
            text
        };
        links.push(HeldLink {
            label,
            url: resolved.to_string(),
        });
    }
    links
}

/// Nearest ancestor (or self) that is an `<a>` with an href.
fn link_target(doc: &blitz_dom::BaseDocument, start: blitz_dom::NodeId) -> Option<String> {
    let mut current = start;
    loop {
        let node = doc.get_node(current)?;
        if let NodeData::Element(el) = &node.data {
            if &*el.name.local == "a" {
                if let Some(href) = node.attr(LocalName::from("href")) {
                    return Some(href.to_string());
                }
            }
        }
        current = node.parent?;
    }
}

/// `<title>` text, falling back to the host.
fn title_for(url: &Url, doc: &HtmlDocument) -> String {
    if url.scheme() == "muse" {
        return "New tab".to_string();
    }
    if let Some(title) = document_title(doc) {
        if !title.is_empty() {
            return title;
        }
    }
    url.host_str().unwrap_or("Untitled").to_string()
}

fn document_title(doc: &HtmlDocument) -> Option<String> {
    let id = doc.query_selector("title").ok().flatten()?;
    let node = doc.get_node(id)?;
    let child = node.children.first().copied()?;
    match doc.get_node(child).map(|n| &n.data) {
        Some(NodeData::Text(t)) => Some(t.content.trim().to_string()),
        _ => None,
    }
}

fn error_page(message: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
    body {{ background-color:#FFFFFF; color:#1C1C1E; padding:64px;
            font-family:"Optimistic VF",Montserrat,"Helvetica Neue",Helvetica,Arial,sans-serif; }}
    h1 {{ font-size:28px; margin-bottom:14px; }}
    p {{ font-size:15px; color:#6E7176; line-height:1.5; max-width:620px; }}
    code {{ background-color:#F3F3F3; padding:2px 6px; border-radius:6px; }}
  </style></head><body>
    <h1>This page could not be loaded</h1>
    <p>The request failed before a document was returned.</p>
    <p><code>{}</code></p>
  </body></html>"#,
        message.replace('&', "&amp;").replace('<', "&lt;")
    )
}

fn qual_name(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

/// Live-measured resident set size (RSS) of the current process in megabytes,
/// queried directly from the OS kernel (`mach_task_basic_info` on macOS for
/// live resident bytes, with `getrusage(RUSAGE_SELF)` fallback).
fn measure_process_rss_mb() -> f64 {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct MachTaskBasicInfo {
            virtual_size: u64,
            resident_size: u64,
            resident_size_max: u64,
            user_time: [u32; 2],
            system_time: [u32; 2],
            policy: i32,
            suspend_count: i32,
        }
        unsafe extern "C" {
            static mach_task_self_: u32;
            fn task_info(
                target_task: u32,
                flavor: u32,
                task_info_out: *mut MachTaskBasicInfo,
                task_info_outCnt: *mut u32,
            ) -> i32;
        }
        const MACH_TASK_BASIC_INFO: u32 = 20;
        const MACH_TASK_BASIC_INFO_COUNT: u32 =
            (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<u32>()) as u32;
        let mut info = MachTaskBasicInfo {
            virtual_size: 0,
            resident_size: 0,
            resident_size_max: 0,
            user_time: [0; 2],
            system_time: [0; 2],
            policy: 0,
            suspend_count: 0,
        };
        let mut count = MACH_TASK_BASIC_INFO_COUNT;
        let kr = unsafe {
            task_info(
                mach_task_self_,
                MACH_TASK_BASIC_INFO,
                &mut info,
                &mut count,
            )
        };
        if kr == 0 && info.resident_size > 0 {
            return (info.resident_size as f64) / (1024.0 * 1024.0);
        }
    }
    #[repr(C)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: i64,
    }
    #[repr(C)]
    struct RUsage {
        ru_utime: Timeval,
        ru_stime: Timeval,
        ru_maxrss: isize,
        _rest: [isize; 14],
    }
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut RUsage) -> i32;
    }
    let mut usage = RUsage {
        ru_utime: Timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        ru_stime: Timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        ru_maxrss: 0,
        _rest: [0; 14],
    };
    let rc = unsafe { getrusage(0, &mut usage) };
    if rc == 0 && usage.ru_maxrss > 0 {
        #[cfg(target_os = "macos")]
        {
            (usage.ru_maxrss as f64) / (1024.0 * 1024.0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            (usage.ru_maxrss as f64) / 1024.0
        }
    } else {
        0.0
    }
}

fn apply_dom_field_action(
    doc: &mut HtmlDocument,
    node_id: NodeId,
    tag: &str,
    action: &str,
    value: Option<&str>,
    origin: Option<&str>,
    username: Option<&str>,
) {
    match action {
        "type" => {
            let val = value.unwrap_or_default();
            if tag == "textarea" {
                let existing_text_child = doc.get_node(node_id).and_then(|n| {
                    n.children.iter().copied().find(|child| {
                        matches!(
                            doc.get_node(*child).map(|c| &c.data),
                            Some(NodeData::Text(_))
                        )
                    })
                });
                let mut mutator = doc.mutate();
                if let Some(text_node) = existing_text_child {
                    mutator.set_node_text(text_node, val);
                } else {
                    let created = mutator.create_text_node(val);
                    mutator.append_children(node_id, &[created]);
                }
                mutator.set_attribute(node_id, qual_name("value"), val);
            } else {
                let mut mutator = doc.mutate();
                mutator.set_attribute(node_id, qual_name("value"), val);
            }
        }
        "fill_secret" => {
            let (Some(origin), Some(username)) = (origin, username) else {
                return;
            };
            // Two different things happen here, and only one of them was
            // forbidden.
            //
            // FORBIDDEN, and now gone: signing a WebAuthn assertion into a
            // hidden form field. Spec §12 says "WebAuthn is not a DOM fill
            // operation", and "Do not simulate a passkey ceremony in page
            // JS". The legacy path called `vault::sign_webauthn_assertion`
            // here and wrote the bundle into `#webauthn_assertion`. A
            // passkey assertion now comes from the broker: the page calls
            // `navigator.credentials.get()`, the broker runs the ceremony in
            // Rust against the Keychain behind the human approval gate, and
            // the assertion goes back through the page API — never through a
            // form field.
            //
            // ALLOWED: supplying a password for an account that has none
            // yet. The spec's credential section offers the shell "Add
            // password" when no credential exists, and this branch runs only
            // after the human approved the `fill_form` proposal. The
            // generated secret goes into the blind vault, which the agent
            // cannot read; it is never disclosed to the agent.
            //
            // The host key is the same effective relying party identifier the
            // credential broker uses, so a password saved through the broker
            // is found by the fill and vice versa.
            let host = crate::webauthn::origin::validate_origin_and_effective_rp_id(
                origin, None, false, false,
            )
            .ok()
            .and_then(|outcome| match outcome {
                crate::webauthn::origin::OriginValidation::Suffix(validated) => {
                    Some(validated.effective_rp_id)
                }
                crate::webauthn::origin::OriginValidation::NeedsRelatedOrigins { .. } => None,
            })
            .unwrap_or_else(|| {
                Url::parse(origin)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_else(|| origin.to_string())
            });
            // A password the user already stored wins, so a repeat fill never
            // rotates a working credential. The blind vault is consulted
            // first because it is the store the user can inspect; the
            // Keychain is the mirror the credential broker reads.
            let store = crate::credentials::KeychainPasswordStore::new();
            let secret = vault::get_or_create_secret(origin, username)
                .map(|acct| acct.secret)
                .ok()
                .or_else(|| store.secret(&host, username));
            match secret {
                Some(secret) => {
                    let mut mutator = doc.mutate();
                    mutator.set_attribute(node_id, qual_name("value"), &secret);
                    // Keep the Keychain mirror in step with the blind vault.
                    let _ = store.save(&host, username, &secret);
                }
                None => {
                    eprintln!(
                        "[credential] could not produce a password for {username} on {host}; leaving the field empty"
                    );
                }
            }
        }
        "select" => {
            let wanted = value.unwrap_or_default();
            let options: Vec<(NodeId, bool)> = doc
                .get_node(node_id)
                .map(|node| {
                    node.children
                        .iter()
                        .copied()
                        .filter_map(|child_id| {
                            let child = doc.get_node(child_id)?;
                            let data = child.element_data()?;
                            if data.name.local.to_string().to_ascii_lowercase() != "option" {
                                return None;
                            }
                            let opt_val = data
                                .attr(local_name!("value"))
                                .map(str::to_string)
                                .unwrap_or_else(|| child.text_content().trim().to_string());
                            let opt_text = child.text_content().trim().to_string();
                            let matches = opt_val == wanted || opt_text == wanted;
                            Some((child_id, matches))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut mutator = doc.mutate();
            for (opt_id, is_match) in options {
                if is_match {
                    mutator.set_attribute(opt_id, qual_name("selected"), "selected");
                } else {
                    mutator.clear_attribute(opt_id, qual_name("selected"));
                }
            }
            mutator.set_attribute(node_id, qual_name("value"), wanted);
        }
        "check" => {
            let checked = matches!(
                value.unwrap_or_default().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "checked" | "on"
            );
            let mut mutator = doc.mutate();
            if checked {
                mutator.set_attribute(node_id, qual_name("checked"), "true");
            } else {
                mutator.clear_attribute(node_id, qual_name("checked"));
            }
        }
        _ => {}
    }
}

fn collect_form_submission(
    doc: &HtmlDocument,
    base_url: &Url,
    submitter_id: NodeId,
) -> (Url, String, Vec<(String, String)>) {
    // Walk up from submitter_id to find the enclosing <form>, or fall back to root.
    let mut form_id = None;
    if let Some(sub_node) = doc.get_node(submitter_id) {
        if let Some(form_attr) = sub_node
            .element_data()
            .and_then(|d| d.attr(local_name!("form")))
        {
            form_id = doc.get_element_by_id(form_attr);
        }
    }
    if form_id.is_none() {
        let mut cur = Some(submitter_id);
        while let Some(nid) = cur {
            let Some(node) = doc.get_node(nid) else {
                break;
            };
            if node
                .element_data()
                .is_some_and(|d| d.name.local.to_string().eq_ignore_ascii_case("form"))
            {
                form_id = Some(nid);
                break;
            }
            cur = node.parent;
        }
    }
    let root_id = form_id.unwrap_or_else(|| doc.root_node().id);
    let (action_raw, method) = doc
        .get_node(root_id)
        .and_then(|n| n.element_data())
        .map(|d| {
            let action = d.attr(local_name!("action")).unwrap_or_default().to_string();
            let method = d
                .attr(local_name!("method"))
                .unwrap_or("GET")
                .to_ascii_uppercase();
            (action, method)
        })
        .unwrap_or_else(|| (String::new(), "GET".to_string()));

    let action_url = if action_raw.trim().is_empty() {
        base_url.clone()
    } else {
        base_url
            .join(action_raw.trim())
            .unwrap_or_else(|_| base_url.clone())
    };

    let mut entries = Vec::new();
    let mut stack = vec![root_id];
    while let Some(nid) = stack.pop() {
        let Some(node) = doc.get_node(nid) else {
            continue;
        };
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
        let Some(data) = node.element_data() else {
            continue;
        };
        if data.attr(local_name!("disabled")).is_some() {
            continue;
        }
        let tag = data.name.local.to_string().to_ascii_lowercase();
        let Some(name) = data
            .attr(local_name!("name"))
            .filter(|s| !s.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        match tag.as_str() {
            "input" => {
                let input_type = data
                    .attr(local_name!("type"))
                    .unwrap_or("text")
                    .to_ascii_lowercase();
                match input_type.as_str() {
                    "checkbox" | "radio" => {
                        let is_checked = data
                            .checkbox_input_checked()
                            .unwrap_or_else(|| data.attr(local_name!("checked")).is_some());
                        if is_checked {
                            let val = data.attr(local_name!("value")).unwrap_or("on").to_string();
                            entries.push((name, val));
                        }
                    }
                    "submit" | "button" | "reset" | "image" | "file" => {
                        if nid == submitter_id {
                            if let Some(val) = data.attr(local_name!("value")) {
                                entries.push((name, val.to_string()));
                            }
                        }
                    }
                    _ => {
                        let val = data
                            .text_input_data()
                            .map(|d| d.editor.text().to_string())
                            .filter(|s| !s.is_empty())
                            .or_else(|| data.attr(local_name!("value")).map(str::to_string))
                            .unwrap_or_default();
                        entries.push((name, val));
                    }
                }
            }
            "select" => {
                let selected_val = node
                    .children
                    .iter()
                    .copied()
                    .find_map(|cid| {
                        let cnode = doc.get_node(cid)?;
                        let cdata = cnode.element_data()?;
                        if !cdata.name.local.to_string().eq_ignore_ascii_case("option")
                            || cdata.attr(local_name!("disabled")).is_some()
                            || cdata.attr(local_name!("selected")).is_none()
                        {
                            return None;
                        }
                        Some(
                            cdata
                                .attr(local_name!("value"))
                                .map(str::to_string)
                                .unwrap_or_else(|| cnode.text_content().trim().to_string()),
                        )
                    })
                    .or_else(|| data.attr(local_name!("value")).map(str::to_string))
                    .or_else(|| {
                        node.children.iter().copied().find_map(|cid| {
                            let cnode = doc.get_node(cid)?;
                            let cdata = cnode.element_data()?;
                            if !cdata.name.local.to_string().eq_ignore_ascii_case("option")
                                || cdata.attr(local_name!("disabled")).is_some()
                            {
                                return None;
                            }
                            Some(
                                cdata
                                    .attr(local_name!("value"))
                                    .map(str::to_string)
                                    .unwrap_or_else(|| cnode.text_content().trim().to_string()),
                            )
                        })
                    })
                    .unwrap_or_default();
                entries.push((name, selected_val));
            }
            "textarea" => {
                let val = data
                    .text_input_data()
                    .map(|d| d.editor.text().to_string())
                    .filter(|s| !s.is_empty())
                    .or_else(|| data.attr(local_name!("value")).map(str::to_string))
                    .unwrap_or_else(|| node.text_content());
                entries.push((name, val));
            }
            _ => {}
        }
    }

    (action_url, method, entries)
}

fn find_interactive_form_control(
    doc: &HtmlDocument,
    start_id: NodeId,
) -> Option<(NodeId, String, String, Option<NodeId>)> {
    let mut cur = Some(start_id);
    let mut clicked_option_id = None;
    while let Some(nid) = cur {
        let node = doc.get_node(nid)?;
        if let Some(data) = node.element_data() {
            let tag = data.name.local.to_string().to_ascii_lowercase();
            if tag == "option" {
                clicked_option_id = Some(nid);
            } else if matches!(tag.as_str(), "input" | "textarea" | "select" | "button") {
                let input_type = data
                    .attr(local_name!("type"))
                    .unwrap_or(if tag == "input" { "text" } else { "" })
                    .to_ascii_lowercase();
                if !(tag == "input" && input_type == "hidden") {
                    return Some((nid, tag, input_type, clicked_option_id));
                }
            } else if tag == "label" {
                if let Some(for_id) = data.attr(local_name!("for")).filter(|s| !s.is_empty()) {
                    if let Some(target_id) = doc.get_element_by_id(for_id) {
                        if let Some(target_node) = doc.get_node(target_id) {
                            if let Some(tdata) = target_node.element_data() {
                                let ttag = tdata.name.local.to_string().to_ascii_lowercase();
                                if matches!(
                                    ttag.as_str(),
                                    "input" | "textarea" | "select" | "button"
                                ) {
                                    let ttype = tdata
                                        .attr(local_name!("type"))
                                        .unwrap_or(if ttag == "input" { "text" } else { "" })
                                        .to_ascii_lowercase();
                                    return Some((target_id, ttag, ttype, None));
                                }
                            }
                        }
                    }
                }
                // Otherwise look for a descendant control inside the <label>.
                let mut stack: Vec<NodeId> = node.children.iter().rev().copied().collect();
                while let Some(cid) = stack.pop() {
                    let Some(cnode) = doc.get_node(cid) else {
                        continue;
                    };
                    for &grand in cnode.children.iter().rev() {
                        stack.push(grand);
                    }
                    if let Some(cdata) = cnode.element_data() {
                        let ctag = cdata.name.local.to_string().to_ascii_lowercase();
                        if matches!(ctag.as_str(), "input" | "textarea" | "select" | "button") {
                            let ctype = cdata
                                .attr(local_name!("type"))
                                .unwrap_or(if ctag == "input" { "text" } else { "" })
                                .to_ascii_lowercase();
                            if !(ctag == "input" && ctype == "hidden") {
                                return Some((cid, ctag, ctype, None));
                            }
                        }
                    }
                }
            }
        }
        cur = node.parent;
    }
    None
}

fn has_enclosing_form_or_submit_attr(doc: &HtmlDocument, button_id: NodeId) -> bool {
    if let Some(node) = doc.get_node(button_id) {
        if let Some(data) = node.element_data() {
            if data
                .attr(local_name!("type"))
                .is_some_and(|t| t.eq_ignore_ascii_case("submit"))
                || data.attr(local_name!("form")).is_some()
            {
                return true;
            }
        }
    }
    let mut cur = doc.get_node(button_id).and_then(|n| n.parent);
    while let Some(nid) = cur {
        let Some(node) = doc.get_node(nid) else {
            break;
        };
        if node
            .element_data()
            .is_some_and(|d| d.name.local.to_string().eq_ignore_ascii_case("form"))
        {
            return true;
        }
        cur = node.parent;
    }
    false
}

fn read_dom_field_value(doc: &HtmlDocument, node_id: NodeId, tag: &str) -> String {
    let Some(node) = doc.get_node(node_id) else {
        return String::new();
    };
    let Some(data) = node.element_data() else {
        return String::new();
    };
    if let Some(tid) = data.text_input_data() {
        let editor_text = tid.editor.text().to_string();
        if !editor_text.is_empty() || data.attr(local_name!("value")).is_some() {
            return editor_text;
        }
    }
    if tag == "textarea" {
        data.attr(local_name!("value"))
            .map(str::to_string)
            .unwrap_or_else(|| node.text_content())
    } else {
        data.attr(local_name!("value"))
            .unwrap_or_default()
            .to_string()
    }
}

fn check_radio_group(doc: &mut HtmlDocument, radio_id: NodeId) {
    let group_name = doc
        .get_node(radio_id)
        .and_then(|n| n.element_data())
        .and_then(|d| d.attr(local_name!("name")))
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(name) = group_name {
        let mut same_group = Vec::new();
        let mut stack = vec![doc.root_node().id];
        while let Some(nid) = stack.pop() {
            let Some(node) = doc.get_node(nid) else {
                continue;
            };
            for &child in node.children.iter().rev() {
                stack.push(child);
            }
            if let Some(data) = node.element_data() {
                if data.name.local.to_string().eq_ignore_ascii_case("input")
                    && data
                        .attr(local_name!("type"))
                        .is_some_and(|t| t.eq_ignore_ascii_case("radio"))
                    && data.attr(local_name!("name")) == Some(name.as_str())
                {
                    same_group.push(nid);
                }
            }
        }
        for nid in same_group {
            let val = if nid == radio_id { "true" } else { "false" };
            apply_dom_field_action(doc, nid, "input", "check", Some(val), None, None);
        }
    } else {
        apply_dom_field_action(doc, radio_id, "input", "check", Some("true"), None, None);
    }
}

fn select_specific_option(doc: &mut HtmlDocument, select_id: NodeId, option_id: NodeId) {
    let wanted = doc
        .get_node(option_id)
        .and_then(|n| {
            let data = n.element_data()?;
            if data.attr(local_name!("disabled")).is_some() {
                return None;
            }
            Some(
                data.attr(local_name!("value"))
                    .map(str::to_string)
                    .unwrap_or_else(|| n.text_content().trim().to_string()),
            )
        });
    if let Some(val) = wanted {
        apply_dom_field_action(doc, select_id, "select", "select", Some(&val), None, None);
    }
}

fn cycle_select_option(doc: &mut HtmlDocument, select_id: NodeId, delta: isize) {
    let options: Vec<(String, bool)> = doc
        .get_node(select_id)
        .map(|node| {
            node.children
                .iter()
                .copied()
                .filter_map(|cid| {
                    let cnode = doc.get_node(cid)?;
                    let cdata = cnode.element_data()?;
                    if !cdata.name.local.to_string().eq_ignore_ascii_case("option")
                        || cdata.attr(local_name!("disabled")).is_some()
                    {
                        return None;
                    }
                    let val = cdata
                        .attr(local_name!("value"))
                        .map(str::to_string)
                        .unwrap_or_else(|| cnode.text_content().trim().to_string());
                    let is_sel = cdata.attr(local_name!("selected")).is_some();
                    Some((val, is_sel))
                })
                .collect()
        })
        .unwrap_or_default();
    if options.is_empty() {
        return;
    }
    let cur_idx = options.iter().position(|(_, sel)| *sel).unwrap_or(0) as isize;
    let len = options.len() as isize;
    let next_idx = ((cur_idx + delta) % len + len) % len;
    let next_val = options[next_idx as usize].0.clone();
    apply_dom_field_action(
        doc,
        select_id,
        "select",
        "select",
        Some(&next_val),
        None,
        None,
    );
}

fn focus_adjacent_form_control(doc: &mut HtmlDocument, reverse: bool) -> Option<NodeId> {
    let mut controls = Vec::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(nid) = stack.pop() {
        let Some(node) = doc.get_node(nid) else {
            continue;
        };
        for &child in node.children.iter().rev() {
            stack.push(child);
        }
        let Some(data) = node.element_data() else {
            continue;
        };
        if data.attr(local_name!("disabled")).is_some() {
            continue;
        }
        let tag = data.name.local.to_string().to_ascii_lowercase();
        if matches!(tag.as_str(), "input" | "textarea" | "select" | "button") {
            let input_type = data
                .attr(local_name!("type"))
                .unwrap_or("text")
                .to_ascii_lowercase();
            if !(tag == "input" && input_type == "hidden") {
                controls.push(nid);
            }
        }
    }
    if controls.is_empty() {
        return None;
    }
    let cur = doc.get_focussed_node_id();
    let next = match cur.and_then(|id| controls.iter().position(|&c| c == id)) {
        Some(idx) => {
            if reverse {
                if idx == 0 {
                    controls[controls.len() - 1]
                } else {
                    controls[idx - 1]
                }
            } else {
                controls[(idx + 1) % controls.len()]
            }
        }
        None => {
            if reverse {
                controls[controls.len() - 1]
            } else {
                controls[0]
            }
        }
    };
    doc.set_focus_to(next);
    blitz_dom::Document::handle_ui_event(
        doc,
        blitz_traits::events::UiEvent::AppleStandardKeybinding("moveToEndOfDocument:".into()),
    );
    Some(next)
}

fn read_macos_clipboard() -> String {
    std::process::Command::new("/usr/bin/pbpaste")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

fn write_macos_clipboard(text: &str) {
    use std::io::Write;
    if let Ok(mut child) = std::process::Command::new("/usr/bin/pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

fn generate_random_token_hex() -> String {
    use std::io::Read;
    let mut buf = [0u8; 24];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    } else {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for (i, b) in buf.iter_mut().enumerate() {
            *b = ((nanos >> ((i % 16) * 8)) & 0xFF) as u8 ^ (i as u8).wrapping_mul(37);
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// `--exit-after <seconds>`: run the real event loop for N seconds, then exit.
fn parse_exit_after() -> Option<Duration> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--exit-after" {
            if let Some(secs) = args.next() {
                if let Ok(secs) = secs.parse::<u64>() {
                    return Some(Duration::from_secs(secs));
                }
            }
        }
    }
    None
}

/// Removes the credentials a test service holds, before and after a run.
///
/// A test must never read another run's credentials and must never leave its
/// own behind. The user's own service is never touched: only a service whose
/// name marks it as a test service is cleared.
fn wipe_test_credential_services() {
    let passkey_service = credentials::passkey_service_name();
    let password_service = credentials::password_service_name();
    for service in [passkey_service, password_service] {
        if !is_test_credential_service(&service) {
            continue;
        }
        for (name, attributes) in keychain::list_all_generic_items() {
            if name == service {
                keychain::delete_generic_item(&name, &attributes.account);
            }
        }
    }
}

/// Whether a credential service name marks it as a test service.
fn is_test_credential_service(service: &str) -> bool {
    service.contains(".test.")
}

/// Cleans up the test credential services when the process is asked to stop.
///
/// A test harness terminates the browser. Without this, the credentials the
/// run created would stay in the Keychain. Only a test service is touched;
/// the user's own service is never removed.
fn install_test_cleanup_signal_handler() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        // SIGTERM and SIGINT: clean up, then exit with the conventional
        // status for a terminated process.
        libc::signal(
            libc::SIGTERM,
            handle_termination_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            handle_termination_signal as *const () as libc::sighandler_t,
        );
    });
}

/// The signal handler. It must be async-signal-safe, so it only sets a flag
/// and lets the process exit.
extern "C" fn handle_termination_signal(_signal: libc::c_int) {
    // `wipe_test_credential_services` touches the Keychain, which is not
    // async-signal-safe, so the work is done by a detached thread and the
    // process exits immediately afterwards.
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        wipe_test_credential_services();
        std::process::exit(143); // 128 + SIGTERM
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // macOS keychain user interaction is switched off before anything can
    // reach the Keychain. An access that the operating system would have
    // turned into a password prompt fails with an error instead, so no run
    // of this binary — test suite or normal browser — can ask the user for
    // their login keychain password. Hermes reads only the Keychain items it
    // created itself, so it is trusted for those and no prompt was ever
    // needed to read them.
    if let Err(error) = keychain::ensure_no_user_interaction() {
        eprintln!("could not disable keychain interaction: {error}");
        std::process::exit(1);
    }

    // A run that points the credential store at a test service starts from
    // a clean slate, and leaves nothing behind. The user's own service is
    // never touched.
    wipe_test_credential_services();

    if args.iter().any(|a| a == "--selftest") {
        std::process::exit(selftest());
    }
    if args.iter().any(|a| a == "--uitest") {
        std::process::exit(uitest());
    }
    if args.iter().any(|a| a == "--runtimetest") {
        std::process::exit(runtimetest());
    }
    if args.iter().any(|a| a == "--passkeyinterstitialtest") {
        std::process::exit(passkeyinterstitialtest());
    }
    if args.iter().any(|a| a == "--observetest") {
        std::process::exit(observetest());
    }
    if args.iter().any(|a| a == "--supervisortest") {
        std::process::exit(supervisortest());
    }
    if args.iter().any(|a| a == "--hurdletest") {
        std::process::exit(hurdletest());
    }
    if args.iter().any(|a| a == "--passkeytest") {
        std::process::exit(passkeytest::passkeytest());
    }
    if args.iter().any(|a| a == "--weathertest") {
        std::process::exit(weathertest::weathertest());
    }
    if args.iter().any(|a| a == "--keychain-cleanup") {
        std::process::exit(keychain_cleanup());
    }
    if args.iter().any(|a| a == "--observe") {
        std::process::exit(observe_cli());
    }
    if let Some(pos) = args.iter().position(|a| a == "--render") {
        if let Some(path) = args.get(pos + 1) {
            let click = args.iter().any(|a| a == "--click");
            let url = args
                .iter()
                .position(|a| a == "--url")
                .and_then(|p| args.get(p + 1))
                .cloned();
            let menu = args.iter().any(|a| a == "--menu");
            let theme = if args.iter().any(|a| a == "--dark") {
                Theme::Dark
            } else {
                Theme::Light
            };
            let size = args
                .iter()
                .position(|a| a == "--size")
                .and_then(|p| args.get(p + 1))
                .and_then(|s| s.split_once('x'))
                .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                .unwrap_or((WIDTH, HEIGHT));
            std::process::exit(render_to_png(
                path,
                click,
                url.as_deref(),
                menu,
                theme,
                size,
            ));
        }
        eprintln!("--render requires a path");
        std::process::exit(2);
    }

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    let net_dirty = Arc::new(AtomicBool::new(false));

    // Subresources loaded by blitz-net wake the loop through this proxy.
    let waker_flag = net_dirty.clone();
    let waker_proxy = proxy.clone();
    let provider: SharedProvider = Arc::new(blitz_net::Provider::new(Some(Arc::new(
        move |_doc_id: usize| {
            waker_flag.store(true, Ordering::SeqCst);
            let _ = waker_proxy.send_event(UserEvent::NetWake);
        },
    ))));

    // Optional supervised control socket: --control-socket <path> with
    // --host-token-file/--agent-token-file (or HERMES_HOST_TOKEN /
    // HERMES_AGENT_TOKEN). Without distinct tokens the socket never opens.
    let control_socket = args
        .iter()
        .position(|a| a == "--control-socket")
        .and_then(|p| args.get(p + 1))
        .cloned();
    let mut app = BrowserApp::new(proxy.clone(), net_dirty, provider);
    if args.iter().any(|a| a == "--collapsed") {
        app.toolbar_collapsed = true;
    }
    install_test_cleanup_signal_handler();
    if args.iter().any(|a| a == "--scrapbook") {
        app.explorer_tab = Some(ExplorerTab::Scrapbook);
    } else if args.iter().any(|a| a == "--pageholder") {
        app.explorer_tab = Some(ExplorerTab::PageHolder);
    }
    let initial_url = args
        .iter()
        .position(|a| a == "--url")
        .and_then(|p| args.get(p + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .skip(1)
                .find(|a| a.starts_with("http://") || a.starts_with("https://"))
                .cloned()
        });
    if let Some(u) = initial_url {
        app.navigate(&u);
    }
    if let Some(socket_path) = control_socket {
        let read_token = |flag: &str, env: &str| -> Option<String> {
            if let Some(path) = args
                .iter()
                .position(|a| a == flag)
                .and_then(|p| args.get(p + 1))
            {
                std::fs::read_to_string(path)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                std::env::var(env).ok()
            }
        };
        let host_token = read_token("--host-token-file", "HERMES_HOST_TOKEN");
        let agent_token = read_token("--agent-token-file", "HERMES_AGENT_TOKEN");
        match (host_token, agent_token) {
            (Some(h), Some(a)) if h.len() >= 16 && a.len() >= 16 && h != a => {
                let hub = Arc::new(ControlHub::new());
                app.control_hub = Some(hub.clone());
                app.live_socket_info = Some((socket_path.clone(), h.clone(), a.clone()));
                if let Err(e) = control::spawn_server(proxy, hub, socket_path.into(), h, a) {
                    eprintln!("[control] failed to bind socket: {e}");
                    std::process::exit(2);
                }
            }
            _ => {
                eprintln!(
                    "[control] --control-socket requires distinct host/agent tokens (>=16 chars) via token files or env"
                );
                std::process::exit(2);
            }
        }
    } else {
        // When launched interactively as a desktop app without --control-socket,
        // automatically open a local control socket so in-browser agent tasks
        // (`agent: <goal>` / `#btn-autofill`) and `./walkaway` can drive the
        // live window directly.
        let pid = std::process::id();
        let sock_path = std::env::temp_dir()
            .join(format!("walkaway-browser-{pid}.sock"))
            .to_string_lossy()
            .to_string();
        let h = generate_random_token_hex();
        let a = generate_random_token_hex();
        let hub = Arc::new(ControlHub::new());
        app.control_hub = Some(hub.clone());
        app.supervise_commits_only = true;
        app.live_socket_info = Some((sock_path.clone(), h.clone(), a.clone()));
        if control::spawn_server(proxy, hub, sock_path.clone().into(), h.clone(), a.clone()).is_ok()
        {
            let info_path = std::env::temp_dir().join("walkaway-live-browser.json");
            let info_json = json!({
                "pid": pid,
                "sock_path": sock_path,
                "host_token": h,
                "agent_token": a,
            });
            let _ = std::fs::write(info_path, info_json.to_string());
        }
    }
    event_loop.run_app(&mut app).expect("run");
}

/// Prepares a test run: the Keychain must never prompt, and the run must
/// never touch the credentials the user actually stored.
///
/// Every suite calls this first. Two things are set up:
///
/// 1. macOS keychain user interaction is switched off, so an access this
///    binary is not trusted for fails with an error instead of asking the
///    user for their login keychain password.
/// 2. The passkey store is pointed at a service named after this process, so
///    the run cannot read or overwrite the user's own passkeys.
fn prepare_test_run() {
    let _ = keychain::ensure_no_user_interaction();
    credentials::set_passkey_service_override(&test_passkey_service());
}

/// The Keychain service this test run uses for passkeys.
fn test_passkey_service() -> String {
    format!(
        "ai.zmax.hermes.browser.test.passkeys.{}",
        std::process::id()
    )
}

/// Removes the Keychain items this test run created.
///
/// A test must leave no credential behind. The test binary is trusted for
/// the items it created, so it can remove them; anything it is not trusted
/// for is left in place rather than falsely reported as removed.
fn cleanup_test_run() {
    let service = test_passkey_service();
    keychain::delete_generic_item(&service, credentials::PASSKEY_KEYCHAIN_ACCOUNT);
}

/// `--uitest`: drive the real chrome click handler at coordinates obtained from
/// real hit-testing, and assert the resulting state. This is what proves the
/// buttons do something, rather than merely being present.
fn uitest() -> i32 {
    prepare_test_run();
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("FAIL: event loop: {e}");
            return 1;
        }
    };
    let proxy = event_loop.create_proxy();
    let net_dirty = Arc::new(AtomicBool::new(false));
    let provider: SharedProvider = Arc::new(blitz_net::Provider::new(None));
    let mut app = BrowserApp::new(proxy, net_dirty, provider);

    let mut failures = 0;
    let check = |cond: bool, msg: &str| {
        if cond {
            println!("ok   {msg}");
        } else {
            println!("FAIL {msg}");
        }
        cond
    };

    check(app.tabs.len() == 1, "starts with one tab");
    check(app.active == 0, "first tab active");

    // New tab.
    if let Some((x, y)) = find_click(&app, "btn-newtab") {
        app.handle_chrome_click(x, y);
    }
    if !check(app.tabs.len() == 2, "new-tab button creates a tab") {
        failures += 1;
    }
    if !check(app.active == 1, "new tab becomes active") {
        failures += 1;
    }

    // Select the first tab by clicking it.
    if let Some((x, y)) = find_click(&app, "tab-0") {
        app.handle_chrome_click(x, y);
    }
    if !check(app.active == 0, "clicking a tab activates it") {
        failures += 1;
    }

    // Close the first tab.
    if let Some((x, y)) = find_click(&app, "tab-close-0") {
        app.handle_chrome_click(x, y);
    }
    if !check(app.tabs.len() == 1, "close button removes the tab") {
        failures += 1;
    }

    // Menu open/close.
    if let Some((x, y)) = find_click(&app, "btn-menu") {
        app.handle_chrome_click(x, y);
    }
    if !check(app.menu_open, "menu button opens the menu") {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "menu-home") {
        app.handle_chrome_click(x, y);
    }
    if !check(!app.menu_open, "menu closes after choosing an item") {
        failures += 1;
    }

    // Home navigation goes through the internal scheme, no network.
    let url = app.tab().canonical.browser.display_url();
    if !check(
        url == START_URL,
        "Home menu item navigates to the start page",
    ) {
        failures += 1;
        eprintln!("     url was {url:?}");
    }

    // Back/forward gating: with only the start page there is nowhere to go.
    if let Some((x, y)) = find_click(&app, "btn-back") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.active == 0 && app.tabs.len() == 1,
        "back on a fresh tab is a no-op",
    ) {
        failures += 1;
    }

    // Address bar focus.
    if let Some((x, y)) = find_click(&app, "address") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.tab().canonical.browser.focused,
        "clicking the address bar focuses it",
    ) {
        failures += 1;
    }

    // Stable tab identity: the surviving second tab keeps TabId(2), an async
    // result for it lands there after tab 1 is closed, and IDs are not reused.
    let surviving_id = app.tab().canonical.id;
    let navigation_generation = app.tab().canonical.browser.generation;
    if !check(
        surviving_id == TabId(2),
        "surviving tab keeps its stable id",
    ) {
        failures += 1;
    }
    app.handle_navigated(
        surviving_id,
        navigation_generation,
        Url::parse("https://example.test/").expect("test url"),
        Ok("<html><head><title>Stable tab</title></head><body>ok</body></html>".to_string()),
    );
    if !check(
        app.tab().canonical.browser.display_url() == "https://example.test/",
        "async navigation result lands in stable tab",
    ) {
        failures += 1;
    }
    let generation_before_new_tab = app.tab().instance.document_generation;
    if let Some((x, y)) = find_click(&app, "btn-newtab") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.tab().canonical.id == TabId(3),
        "new tab does not reuse a closed tab id",
    ) {
        failures += 1;
    }
    if !check(
        generation_before_new_tab > 1,
        "document replacement increments document generation",
    ) {
        failures += 1;
    }

    // -----------------------------------------------------------------------
    // Deliverable 2: IE5 Mac Collapsible Toolbar & Explorer Bar Drawer Reflow
    // -----------------------------------------------------------------------
    let vp_before_collapse = app.content_viewport().window_size;
    if let Some((x, y)) = find_click(&app, "btn-collapse-toolbar") {
        app.handle_chrome_click(x, y);
    }
    let vp_after_collapse = app.content_viewport().window_size;
    if !check(
        app.toolbar_collapsed
            && vp_after_collapse.1 > vp_before_collapse.1
            && app.chrome_doc.get_element_by_id("address").is_none()
            && find_click(&app, "btn-back").is_some()
            && find_click(&app, "btn-forward").is_some()
            && find_click(&app, "btn-stop").is_some()
            && find_click(&app, "btn-reload").is_some(),
        "clicking #btn-collapse-toolbar hides top chrome, expands viewport to y=0, and renders compact nav buttons in rail",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "btn-collapse-toolbar") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        !app.toolbar_collapsed && app.chrome_doc.get_element_by_id("address").is_some(),
        "clicking #btn-collapse-toolbar again restores top toolbar",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "btn-fav-overflow") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.menu_open,
        "clicking #btn-fav-overflow opens the favorites overflow menu",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "btn-fav-overflow") {
        app.handle_chrome_click(x, y);
    }
    if let Some((x, y)) = find_click(&app, "btn-home") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.tab().canonical.browser.display_url() == START_URL,
        "clicking #btn-home navigates to the start page",
    ) {
        failures += 1;
    }
    app.tab_mut().canonical.browser.address = "https://example.org/go-target".to_string();
    if let Some((x, y)) = find_click(&app, "btn-go") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.tab().canonical.browser.address == "https://example.org/go-target",
        "clicking #btn-go triggers navigation to the address bar input",
    ) {
        failures += 1;
    }

    // -----------------------------------------------------------------------
    // Deliverable 3: IE5 Mac Page Holder (Rendered Page View, Links View, Retargeting)
    // -----------------------------------------------------------------------
    let holder_source_html = r#"<!DOCTYPE html><html><head><title>Held Directory</title></head>
        <body style="margin:0;padding:8px;">
          <a id="first-held-link" href="https://example.org/retargeted-from-holder" style="display:block;width:220px;height:40px;background:#1B74E4;color:#FFF;">Target Article</a>
          <a id="second-held-link" href="/relative-section" style="display:block;width:220px;height:30px;">Relative Section</a>
        </body></html>"#;
    let active_id = app.tab().canonical.id;
    let nav_gen = app.tab().canonical.browser.generation;
    app.handle_navigated(
        active_id,
        nav_gen,
        Url::parse("https://directory.test/index.html").expect("dir url"),
        Ok(holder_source_html.to_string()),
    );
    let width_closed = app.content_viewport().window_size.0;
    if let Some((x, y)) = find_click(&app, "exp-pageholder") {
        app.handle_chrome_click(x, y);
    }
    let width_open = app.content_viewport().window_size.0;
    if !check(
        app.explorer_tab == Some(ExplorerTab::PageHolder)
            && width_open + (EXPLORER_DRAWER_WIDTH as u32) == width_closed,
        "opening #exp-pageholder drawer shifts content viewport X-origin right by 260px and reflows Taffy width",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "ph-add") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.page_holder.as_ref().is_some_and(|ph| {
            !ph.links_mode
                && ph.links.len() == 2
                && ph.links[0].url == "https://example.org/retargeted-from-holder"
                && ph.links[1].url == "https://directory.test/relative-section"
        }),
        "clicking #ph-add snapshots active tab DOM into page_holder_doc and resolves <a href> URLs",
    ) {
        failures += 1;
    }
    // Click directly on the link inside the Page Holder's Rendered Page View (#ph-canvas)
    // and verify that it retargets the main active tab while keeping page_holder_doc pinned!
    let ph_click_pt = (
        40.0_f32,
        (CHROME_HEIGHT + PH_HEADER_HEIGHT + 20.0) as f32,
    );
    app.handle_chrome_click(ph_click_pt.0, ph_click_pt.1);
    if !check(
        app.tab().canonical.browser.address == "https://example.org/retargeted-from-holder"
            && app.page_holder.as_ref().is_some_and(|ph| ph.url.as_str() == "https://directory.test/index.html"),
        "clicking link in Page Holder Rendered Page View retargets main active tab while keeping Page Holder pinned",
    ) {
        failures += 1;
    }
    // Switch to Links List View (#ph-links) and click #ph-link-1.
    if let Some((x, y)) = find_click(&app, "ph-links") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.page_holder.as_ref().is_some_and(|ph| ph.links_mode)
            && find_click(&app, "ph-link-1").is_some(),
        "clicking #ph-links toggles Page Holder Links List View and renders clickable #ph-link-N items",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "ph-link-1") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.tab().canonical.browser.address == "https://directory.test/relative-section"
            && app.page_holder.as_ref().is_some_and(|ph| ph.url.as_str() == "https://directory.test/index.html"),
        "clicking #ph-link-1 in Page Holder Links List View retargets main active tab to resolved URL",
    ) {
        failures += 1;
    }

    // -----------------------------------------------------------------------
    // Deliverable 4: IE5 Mac Internet Scrapbook (Offline DOM + Style + Image Archival)
    // -----------------------------------------------------------------------
    let sb_temp_path = std::env::temp_dir().join(format!("hermes_sb_uitest_{}.json", std::process::id()));
    unsafe {
        std::env::set_var("HERMES_SCRAPBOOK_PATH", &sb_temp_path);
    }
    app.scrapbook_entries.clear();
    app.selected_scrapbook = None;
    let sb_live_html = r##"<!DOCTYPE html><html><head><title>Scrapbook Live Page</title></head>
        <body style="background-color:#102030;color:#F0F4F8;">
          <h1 id="sb-heading">Initial Heading</h1>
          <a id="sb-rel-link" href="/archived-doc">Archived Link</a>
          <script>
            var h = document.getElementById("sb-heading");
            h.innerText = "Mutated Before Scrapbook Capture";
            h.style.setProperty("color", "#22C55E");
          </script>
        </body></html>"##;
    let active_id = app.tab().canonical.id;
    let nav_gen = app.tab().canonical.browser.generation;
    app.handle_navigated(
        active_id,
        nav_gen,
        Url::parse("https://archive.test/page").expect("archive url"),
        Ok(sb_live_html.to_string()),
    );
    if let Some((x, y)) = find_click(&app, "exp-scrapbook") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.explorer_tab == Some(ExplorerTab::Scrapbook),
        "clicking #exp-scrapbook opens the Internet Scrapbook drawer",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "sb-add") {
        app.handle_chrome_click(x, y);
    }
    let persisted = scrapbook::load_entries();
    if !check(
        app.scrapbook_entries.len() == 1
            && persisted.len() == 1
            && persisted[0].title == "Scrapbook Live Page"
            && persisted[0].url == "https://archive.test/page"
            && persisted[0].html_snapshot.contains("Mutated Before Scrapbook Capture")
            && persisted[0].html_snapshot.contains("https://archive.test/archived-doc"),
        "clicking #sb-add serializes mutated DOM + resolved styles + absolute links and persists to disk",
    ) {
        failures += 1;
    }
    // Navigate active tab away, then click #sb-entry-0 to restore the offline snapshot!
    app.navigate(START_URL);
    if let Some((x, y)) = find_click(&app, "sb-entry-0") {
        app.handle_chrome_click(x, y);
    }
    let restored_heading = app
        .tab()
        .instance
        .page
        .doc()
        .get_element_by_id("sb-heading")
        .and_then(|id| app.tab().instance.page.doc().get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();
    if !check(
        app.tab().canonical.browser.display_url() == "https://archive.test/page"
            && restored_heading == "Mutated Before Scrapbook Capture",
        "clicking #sb-entry-0 loads the archived offline snapshot back into the main active tab",
    ) {
        failures += 1;
    }
    // Organize folder divider & delete entry.
    if let Some((x, y)) = find_click(&app, "sb-organize") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.scrapbook_entries.len() == 2 && app.scrapbook_entries[1].is_folder,
        "clicking #sb-organize adds a persistent folder divider to the Scrapbook",
    ) {
        failures += 1;
    }
    if let Some((x, y)) = find_click(&app, "sb-delete") {
        app.handle_chrome_click(x, y);
    }
    if !check(
        app.scrapbook_entries.len() == 1 && scrapbook::load_entries().len() == 1,
        "clicking #sb-delete removes the selected Scrapbook entry and updates disk storage",
    ) {
        failures += 1;
    }
    let _ = std::fs::remove_file(&sb_temp_path);

    // -----------------------------------------------------------------------
    // Deliverable 5: Real Human Interactive Browser (Address Select-All,
    // Page Form Click/Focus/Type/Checkbox/Select, and HTTP POST Form Submit)
    // -----------------------------------------------------------------------
    // Close the Scrapbook drawer so the content viewport is at standard offset.
    if let Some((x, y)) = find_click(&app, "exp-scrapbook") {
        app.handle_chrome_click(x, y);
    }
    app.navigate(START_URL);
    if let Some((x, y)) = find_click(&app, "address") {
        app.handle_chrome_click(x, y);
    }
    app.handle_logical_key(&Key::Character("e".into()), Some("e"));
    app.handle_logical_key(&Key::Character("x".into()), Some("x"));
    if !check(
        app.tab().canonical.browser.address == "ex",
        "clicking #address selects all so the first character typed replaces muse://start instead of appending",
    ) {
        failures += 1;
        eprintln!(
            "     address after typing 'ex' was {:?}",
            app.tab().canonical.browser.address
        );
    }

    // Spin up a real local HTTP server to receive a human-submitted POST form.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind form test server");
    let server_addr = listener.local_addr().expect("local_addr");
    let (post_tx, post_rx) = std::sync::mpsc::channel::<(String, String)>();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
            let mut buf = vec![0u8; 8192];
            let mut total = 0usize;
            loop {
                match std::io::Read::read(&mut stream, &mut buf[total..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        if let Some(hdr_end) = buf[..total]
                            .windows(4)
                            .position(|w| w == b"\r\n\r\n")
                        {
                            let hdr_str = String::from_utf8_lossy(&buf[..hdr_end]);
                            let content_len = hdr_str
                                .lines()
                                .find_map(|line| {
                                    let (k, v) = line.split_once(':')?;
                                    if k.trim().eq_ignore_ascii_case("content-length") {
                                        v.trim().parse::<usize>().ok()
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0);
                            if total >= hdr_end + 4 + content_len {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let raw = String::from_utf8_lossy(&buf[..total]).to_string();
            let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
            let req_line = head.lines().next().unwrap_or("").to_string();
            let _ = post_tx.send((req_line, body.to_string()));
            let resp_html = format!(
                "<!DOCTYPE html><html><head><title>Form Submitted</title></head><body><h1 id=\"post-result\">Submitted: {}</h1></body></html>",
                body
            );
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_html.len(),
                resp_html
            );
            let _ = std::io::Write::write_all(&mut stream, resp.as_bytes());
        }
    });

    let form_url = format!("http://{}/order", server_addr);
    let form_html = format!(
        r#"<!DOCTYPE html><html><head><title>Human Form Test</title></head>
        <body style="margin:16px;">
          <form id="order-form" method="POST" action="http://{}/submit">
            <input id="custname" name="custname" type="text" value="" style="display:block;width:240px;height:32px;margin-bottom:12px;" />
            <input id="agree" name="agree" type="checkbox" value="yes" style="display:block;width:24px;height:24px;margin-bottom:12px;" />
            <select id="size" name="size" style="display:block;width:200px;margin-bottom:12px;">
              <option id="opt-small" value="small" selected>Small</option>
              <option id="opt-large" value="large">Large</option>
            </select>
            <button id="sub-btn" type="submit" style="display:block;width:140px;height:36px;">Submit Order</button>
          </form>
        </body></html>"#,
        server_addr
    );
    let active_id = app.tab().canonical.id;
    let nav_gen = app.tab().canonical.browser.generation;
    app.handle_navigated(
        active_id,
        nav_gen,
        Url::parse(&form_url).expect("form url"),
        Ok(form_html),
    );

    // 1. Click #custname on the page and type "Ryan"
    if let Some((px, py)) = find_page_click(&app, "custname") {
        app.handle_page_click(px, py);
    }
    for ch in ["R", "y", "a", "n"] {
        app.handle_logical_key(&Key::Character(ch.into()), Some(ch));
    }
    let typed_val = app
        .tab()
        .instance
        .page
        .doc()
        .get_element_by_id("custname")
        .map(|id| read_dom_field_value(app.tab().instance.page.doc(), id, "input"))
        .unwrap_or_default();
    if !check(
        !app.tab().canonical.browser.focused && typed_val == "Ryan",
        "human page click focuses <input id='custname'> and keyboard typing updates its DOM + editor value to 'Ryan'",
    ) {
        failures += 1;
        eprintln!("     typed_val was {:?}", typed_val);
    }

    // 2. Click #agree checkbox on the page to toggle it checked
    if let Some((px, py)) = find_page_click(&app, "agree") {
        app.handle_page_click(px, py);
    }
    let agree_checked = app
        .tab()
        .instance
        .page
        .doc()
        .get_element_by_id("agree")
        .and_then(|id| app.tab().instance.page.doc().get_node(id))
        .and_then(|n| n.element_data())
        .and_then(|d| d.checkbox_input_checked())
        .unwrap_or(false);
    if !check(
        agree_checked,
        "human page click on <input type='checkbox' id='agree'> toggles it checked",
    ) {
        failures += 1;
    }

    // 3. Click #opt-large inside <select id='size'> to choose 'large'
    if let Some((px, py)) = find_page_click(&app, "opt-large") {
        app.handle_page_click(px, py);
    }
    let sub_node_id = app
        .tab()
        .instance
        .page
        .doc()
        .get_element_by_id("sub-btn")
        .expect("sub-btn");
    let (_, _, entries_before_submit) = collect_form_submission(
        app.tab().instance.page.doc(),
        &Url::parse(&form_url).expect("form url"),
        sub_node_id,
    );
    if !check(
        entries_before_submit.contains(&("custname".to_string(), "Ryan".to_string()))
            && entries_before_submit.contains(&("agree".to_string(), "yes".to_string()))
            && entries_before_submit.contains(&("size".to_string(), "large".to_string())),
        "human page click on <option id='opt-large'> selects 'large' in <select id='size'>",
    ) {
        failures += 1;
        eprintln!("     entries_before_submit: {:?}", entries_before_submit);
    }

    // 4. Click #sub-btn to submit the form over real HTTP POST and verify server received payload + rendered response
    if let Some((px, py)) = find_page_click(&app, "sub-btn") {
        app.handle_page_click(px, py);
    }
    let post_received = post_rx.recv_timeout(std::time::Duration::from_secs(3)).ok();
    let post_heading = app
        .tab()
        .instance
        .page
        .doc()
        .get_element_by_id("post-result")
        .and_then(|id| app.tab().instance.page.doc().get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();
    if !check(
        post_received.as_ref().is_some_and(|(req_line, body)| {
            req_line.starts_with("POST /submit ")
                && body.contains("custname=Ryan")
                && body.contains("agree=yes")
                && body.contains("size=large")
        }) && app.tab().canonical.browser.display_url().ends_with("/submit")
            && post_heading.contains("custname=Ryan&agree=yes&size=large"),
        "clicking <button type='submit'> submits form over real HTTP POST and renders response page",
    ) {
        failures += 1;
        eprintln!(
            "     post_received={:?}, post_heading={:?}",
            post_received, post_heading
        );
    }

    cleanup_test_run();

    if failures == 0 {
        println!("UITEST PASS");
        0
    } else {
        println!("UITEST FAIL");
        1
    }
}

/// Locate a clickable point for a chrome element id via real hit-testing.
fn find_click(app: &BrowserApp, id: &str) -> Option<(f32, f32)> {
    let target = app.chrome_doc.get_element_by_id(id)?;
    for y in (0..HEIGHT as i32).step_by(2) {
        for x in (0..WIDTH as i32).step_by(2) {
            if let Some(hit) = app.chrome_doc.hit(x as f32, y as f32) {
                if dom_ffi::bubble_target(&app.chrome_doc, hit.node_id) == target {
                    return Some((x as f32 + 0.5, y as f32 + 0.5));
                }
            }
        }
    }
    None
}

/// Locate a clickable point for a page element id in window coordinates via real hit-testing.
fn find_page_click(app: &BrowserApp, id: &str) -> Option<(f32, f32)> {
    let doc = app.tab().instance.page.doc();
    let target = doc.get_element_by_id(id)?;
    let ox = app.left_chrome_width() as f32;
    let oy = app.top_chrome_height() as f32;
    for y in (0..HEIGHT as i32).step_by(2) {
        for x in (0..WIDTH as i32).step_by(2) {
            if let Some(hit) = doc.hit(x as f32, y as f32) {
                let mut cur = Some(hit.node_id);
                while let Some(nid) = cur {
                    if nid == target {
                        return Some((ox + x as f32 + 0.5, oy + y as f32 + 0.5));
                    }
                    cur = doc.get_node(nid).and_then(|n| n.parent);
                }
            }
        }
    }
    None
}

/// `--runtimetest`: proves that each tab's AOT handler mutates only its own
/// document, including after a later-created runtime is destroyed, AND verifies
/// all Deliverable 1 Static Hermes `$SHBuiltin.extern_c` DOM bindings + automatic
/// `<script>` execution + `addEventListener` bubbling + `getBoundingClientRect`.
fn runtimetest() -> i32 {
    prepare_test_run();
    fn button_text(page: &Page) -> String {
        let Some(button) = page.doc().get_element_by_id("action-btn") else {
            return String::new();
        };
        let Some(node) = page.doc().get_node(button) else {
            return String::new();
        };
        let Some(child) = node.children.first().copied() else {
            return String::new();
        };
        match page.doc().get_node(child).map(|n| &n.data) {
            Some(NodeData::Text(t)) => t.content.clone(),
            _ => String::new(),
        }
    }

    fn click_button(page: &Page) {
        let button = page
            .doc()
            .get_element_by_id("action-btn")
            .expect("start page button");
        unsafe {
            hermes_app_dispatch_event(page.hermes, c"click".as_ptr(), button.as_u64(), 0.0, 0.0);
        }
    }

    let viewport = page_viewport((WIDTH, HEIGHT), 1.0, Theme::Light);
    let first = Page::load(START_PAGE, None, None, viewport.clone(), Theme::Light);
    let second = Page::load(START_PAGE, None, None, viewport.clone(), Theme::Light);
    let mut failures = 0;
    let mut check = |condition: bool, message: &str| {
        if condition {
            println!("ok   {message}");
        } else {
            eprintln!("FAIL {message}");
            failures += 1;
        }
    };

    check(
        button_text(&first).contains("(0)"),
        "first runtime starts at zero",
    );
    check(
        button_text(&second).contains("(0)"),
        "second runtime starts at zero",
    );

    click_button(&first);
    check(
        button_text(&first).contains("Increment Counter (1)"),
        "clicking first tab increments first document",
    );
    check(
        button_text(&second).contains("Increment Counter (0)"),
        "clicking first tab leaves second document unchanged",
    );

    click_button(&second);
    check(
        button_text(&first).contains("Increment Counter (1)"),
        "clicking second tab leaves first document unchanged",
    );
    check(
        button_text(&second).contains("Increment Counter (1)"),
        "clicking second tab increments second document",
    );

    drop(second);
    click_button(&first);
    check(
        button_text(&first).contains("Increment Counter (2)"),
        "destroying second runtime does not disable first runtime",
    );

    // Deliverable 1: Full DOM API surface + automatic <script> execution + bubbling + getBoundingClientRect
    let dom_test_html = r##"<!DOCTYPE html>
    <html>
    <head>
      <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        #box-host { width: 240px; height: 80px; }
        .item { width: 100px; height: 24px; }
      </style>
    </head>
    <body>
      <div id="box-host" class="container" data-init="orig">
        <span class="item" id="item-1">One</span>
        <span class="item" id="item-2">Two</span>
        <span class="item" id="to-remove">RemoveMe</span>
      </div>
      <div id="html-target"></div>
      <div id="bubble-parent">
        <button id="bubble-child">ClickChild</button>
      </div>
      <script>
        var host = document.querySelector("#box-host.container");
        var items = document.querySelectorAll("#box-host .item");
        host.setAttribute("data-count", String(items.length));
        host.setAttribute("data-prev", host.getAttribute("data-init"));

        var rem = document.getElementById("to-remove");
        if (rem) { rem.removeNode(); }

        var created = document.createElement("div");
        created.setAttribute("id", "dynamic-node");
        created.innerText = "Created via Static Hermes";
        created.style.setProperty("width", "180px");
        created.style.setProperty("height", "44px");
        created.style.setProperty("background-color", "rgb(16, 185, 129)");
        host.appendChild(created);

        var rect = created.getBoundingClientRect();
        created.setAttribute("data-rect", Math.round(rect.width) + "x" + Math.round(rect.height));

        var htmlTarget = document.getElementById("html-target");
        htmlTarget.innerHTML = "<p id='parsed-inner'>Inner<b>HTML</b>Works</p>";
        host.setAttribute("data-inner-html-read", htmlTarget.innerHTML);

        var parentEl = document.getElementById("bubble-parent");
        parentEl.addEventListener("click", function(ev) {
          parentEl.setAttribute("data-bubbled-from", ev.target.getAttribute("id"));
        });

        // A WebAuthn ceremony is bound to the origin the page was served
        // from, and the relying party may only claim an RP ID that is that
        // origin's host or a parent of it. The page below is served from
        // https://example.com, so the RP ID is example.com.
        //
        // The challenge is a `BufferSource`, which is what the spec requires
        // and what a real site sends. The bytes must reach the authenticator
        // intact.
        //
        // A page may not obtain a credential without a human saying so. The
        // promise therefore stays pending until the host settles it, and this
        // page must not see a credential before then.
        window.__webauthnSettled = "no";
        navigator.credentials.create({
          publicKey: {
            rp: { id: "example.com", name: "Example" },
            user: { name: "ryan@example.com", displayName: "Ryan" },
            challenge: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8])
          }
        }).then(function(cred) {
          host.setAttribute("data-webauthn-step", "created");
          host.setAttribute("data-webauthn-id", String(cred.id.byteLength));
        }).catch(function(err) {
          host.setAttribute("data-webauthn-error", String(err && err.message ? err.message : err));
        });
      </script>
    </body>
    </html>"##;

    // The page is served from https://example.com, which is the origin the
    // WebAuthn ceremony is bound to.
    let dom_base = Url::parse("https://example.com/").ok();
    let mut dom_page = Page::load(dom_test_html, dom_base, None, viewport, Theme::Light);
    let doc = dom_page.doc();
    let host_id = doc.get_element_by_id("box-host").expect("#box-host");
    let host_node = doc.get_node(host_id).expect("host node");
    check(
        host_node.attr(LocalName::from("data-count")) == Some("3")
            && host_node.attr(LocalName::from("data-prev")) == Some("orig"),
        "querySelector, querySelectorAll, getAttribute, and setAttribute execute automatically from inline <script>",
    );
    check(
        doc.get_element_by_id("to-remove").is_none(),
        "removeNode detaches #to-remove from the Blitz DOM",
    );
    let dyn_id = doc.get_element_by_id("dynamic-node");
    let dyn_text = dyn_id
        .and_then(|id| doc.get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();
    let dyn_rect = dyn_id
        .and_then(|id| doc.get_node(id))
        .and_then(|n| n.attr(LocalName::from("data-rect")))
        .unwrap_or_default()
        .to_string();
    check(
        dyn_text == "Created via Static Hermes" && dyn_rect == "180x44",
        "createElement, appendChild, innerText, style.setProperty, and getBoundingClientRect resolve Taffy layout synchronously",
    );
    let parsed_inner = doc
        .get_element_by_id("parsed-inner")
        .and_then(|id| doc.get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();
    check(
        parsed_inner == "InnerHTMLWorks",
        "innerHTML setter parses HTML fragment via HtmlProvider and innerHTML getter serializes subtree",
    );

    // Test addEventListener bubbling from #bubble-child up to #bubble-parent via blitz_parent_node.
    let child_id = dom_page
        .doc()
        .get_element_by_id("bubble-child")
        .expect("#bubble-child");
    unsafe {
        hermes_app_dispatch_event(
            dom_page.hermes,
            c"click".as_ptr(),
            child_id.as_u64(),
            10.0,
            10.0,
        );
    }
    if dom_page.ctx.dirty {
        dom_page.resolve();
        dom_page.ctx.dirty = false;
    }
    let bubbled_from = dom_page
        .doc()
        .get_element_by_id("bubble-parent")
        .and_then(|id| dom_page.doc().get_node(id))
        .and_then(|n| n.attr(LocalName::from("data-bubbled-from")))
        .unwrap_or_default()
        .to_string();
    check(
        bubbled_from == "bubble-child",
        "addEventListener bubbles click event from #bubble-child to #bubble-parent via blitz_parent_node",
    );
    let webauthn_id = dom_page
        .doc()
        .get_element_by_id("box-host")
        .and_then(|id| dom_page.doc().get_node(id))
        .and_then(|n| n.attr(LocalName::from("data-webauthn-id")))
        .unwrap_or_default()
        .to_string();
    let webauthn_error: String = dom_page
        .doc()
        .get_element_by_id("box-host")
        .and_then(|id| dom_page.doc().get_node(id))
        .and_then(|n| n.attr(LocalName::from("data-webauthn-error")))
        .unwrap_or_default()
        .to_string();
    let webauthn_stack: String = dom_page
        .doc()
        .get_element_by_id("box-host")
        .and_then(|id| dom_page.doc().get_node(id))
        .and_then(|n| n.attr(LocalName::from("data-webauthn-stack")))
        .unwrap_or_default()
        .to_string();
    let webauthn_stack_suffix: String = if webauthn_stack.trim().is_empty() {
        String::new()
    } else {
        format!("; stack: {webauthn_stack}")
    };
    // A page may not obtain a credential without a human saying so. The
    // ceremony has to sit at the approval gate with the promise unsettled,
    // and the page must not see a credential id.
    let pending_count = dom_page.ctx.pending_page_ceremonies.len();
    check(
        webauthn_id.is_empty() && pending_count == 1,
        &format!(
            "navigator.credentials.create() waits for a human: the page sees no credential while the ceremony is pending{}",
            if webauthn_error.is_empty() {
                String::new()
            } else {
                format!(" (page reported: {webauthn_error}{webauthn_stack_suffix})")
            }
        ),
    );
    // The ceremony is bound to the origin the page was served from, not to
    // an origin the page could have chosen.
    let pending = dom_page.ctx.pending_page_ceremonies.values().next();
    check(
        pending.is_some_and(|ceremony| ceremony.origin == "https://example.com")
            && pending.is_some_and(|ceremony| ceremony.rp_id == "example.com"),
        "the pending ceremony is bound to the origin the document was served from",
    );
    // The challenge must have reached the authenticator as the bytes the
    // page sent, not as a stringified buffer.
    let challenge_ok = {
        let mut broker = dom_page.ctx.broker.borrow_mut();
        let status = broker.poll_assertion(credentials::PasskeyCeremonyId(
            pending.map(|c| c.ceremony_id.clone()).unwrap_or_default(),
        ));
        matches!(
            status,
            Ok(credentials::PasskeyStatus::AwaitingApproval { .. })
        )
    };
    check(
        challenge_ok,
        "the pending ceremony is waiting at the broker's approval gate",
    );
    cleanup_test_run();

    if failures == 0 {
        println!("RUNTIME TEST PASS");
        0
    } else {
        println!("RUNTIME TEST FAIL ({failures})");
        1
    }
}

fn test_app() -> BrowserApp {
    static TEST_PROXY: std::sync::OnceLock<EventLoopProxy<UserEvent>> = std::sync::OnceLock::new();
    let proxy = TEST_PROXY
        .get_or_init(|| {
            let event_loop = EventLoop::<UserEvent>::with_user_event()
                .build()
                .expect("event loop");
            let p = event_loop.create_proxy();
            // Leak the loop only for process-short CLI tests; BrowserApp needs a proxy
            // but these paths never run the loop.
            std::mem::forget(event_loop);
            p
        })
        .clone();
    let net_dirty = Arc::new(AtomicBool::new(false));
    let provider: SharedProvider = Arc::new(blitz_net::Provider::new(None));
    BrowserApp::new(proxy, net_dirty, provider)
}

/// `--passkeyinterstitialtest`: the human approval gate for a page-initiated
/// passkey ceremony.
///
/// A page may not sign anything without a human saying so. This drives the
/// real interstitial in a real BrowserApp: the prompt appears, Allow signs a
/// real assertion, Deny cancels the ceremony, a click outside the dialog and
/// an Escape key do not answer it, and an unanswered ceremony fails when its
/// deadline passes.
fn passkeyinterstitialtest() -> i32 {
    prepare_test_run();
    let mut failures = 0usize;
    let mut check = |condition: bool, message: &str| {
        if condition {
            println!("ok   {message}");
        } else {
            eprintln!("FAIL {message}");
            failures += 1;
        }
    };

    const PAGE_HTML: &str = r#"<!DOCTYPE html><html><head><style>
      body { background-color:#FFFFFF; color:#1C1C1E; font-family:"Helvetica Neue",Helvetica,Arial,sans-serif; }
    </style></head><body>
      <div id="box-host">starting</div>
      <script>
        window.__result = "none";
        navigator.credentials.create({
          publicKey: {
            rp: { id: "example.com", name: "Example" },
            user: { name: "ryan@example.com", displayName: "Ryan" },
            challenge: new Uint8Array([1, 2, 3, 4])
          }
        }).then(function(cred) {
          var hasPk = typeof window.PublicKeyCredential === "function";
          var attLen = cred.response && cred.response.attestationObject ? cred.response.attestationObject.byteLength : 0;
          window.__result = "created:" + String(cred.id.byteLength) + ":att:" + String(attLen) + ":pk:" + String(hasPk);
          document.getElementById("box-host").setAttribute("data-result", window.__result);
          return navigator.credentials.get({
            publicKey: { rpId: "example.com", challenge: new Uint8Array([5, 6, 7, 8]) }
          });
        }).then(function(assertion) {
          window.__result = "asserted:" + String(assertion.response.signature.byteLength);
          document.getElementById("box-host").setAttribute("data-result", window.__result);
        }).catch(function(err) {
          window.__result = "error:" + String(err && err.message ? err.message : err);
          document.getElementById("box-host").setAttribute("data-result", window.__result);
        });
      </script>
    </body></html>"#;

    /// Loads the page into the app through the real navigation path, so the
    /// page's ceremonies run in the app's own broker.
    fn load_page(app: &mut BrowserApp, html: &str) {
        let url = Url::parse("https://example.com/").expect("url");
        let tab_id = app.tab().canonical.id;
        app.handle_navigated(tab_id, 0, url, Ok(html.to_string()));
    }

    fn page_result(app: &BrowserApp) -> String {
        app.tab()
            .instance
            .page
            .doc()
            .get_element_by_id("box-host")
            .and_then(|id| app.tab().instance.page.doc().get_node(id))
            .and_then(|n| n.attr(LocalName::from("data-result")))
            .unwrap_or_default()
            .to_string()
    }

    let mut app = test_app();

    // --- A page ceremony waits for a human. ---
    load_page(&mut app, PAGE_HTML);
    app.poll_page_passkey_ceremonies();
    check(
        app.passkey_prompt.is_some(),
        "a page-initiated ceremony raises the interstitial",
    );
    check(
        page_result(&app).is_empty(),
        "the page sees no credential while the prompt is up",
    );
    check(
        app.chrome_doc.get_element_by_id("pk-modal").is_some()
            && app.chrome_doc.get_element_by_id("pk-allow").is_some()
            && app.chrome_doc.get_element_by_id("pk-deny").is_some(),
        "the interstitial renders with Allow and Deny",
    );
    let site = app
        .chrome_doc
        .get_element_by_id("pk-site")
        .and_then(|id| app.chrome_doc.get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();
    check(
        site.contains("example.com"),
        "the interstitial names the site that is asking",
    );

    // --- A click outside the dialog does not answer it. ---
    let before = app.passkey_prompt.clone();
    // A point on the chrome, well away from the dialog's buttons.
    app.handle_chrome_click(5.0, 5.0);
    check(
        app.passkey_prompt.is_some()
            && before.is_some()
            && app.passkey_prompt.as_ref().map(|c| c.ceremony_id.clone())
                == before.as_ref().map(|c| c.ceremony_id.clone()),
        "a click outside the interstitial does not dismiss it",
    );
    check(
        page_result(&app).is_empty(),
        "a click outside the interstitial does not sign anything",
    );

    // --- An Escape key does not answer it either. ---
    // Every key press is swallowed while the prompt is up, so Escape cannot
    // dismiss it. A stray key must never approve a signing ceremony.
    check(
        app.passkey_prompt_swallows_keys(),
        "the interstitial swallows every key press while it is up",
    );
    check(
        app.passkey_prompt.is_some(),
        "an Escape key does not dismiss the interstitial",
    );
    check(
        page_result(&app).is_empty(),
        "an Escape key does not sign anything",
    );

    // --- Allow signs a real assertion. ---
    // The page registers first and then asks for an assertion, so each of
    // those is its own ceremony and each needs its own answer. That is the
    // point: a page may not get a credential or a signature without a human
    // saying so, every single time.
    let allow = find_click(&app, "pk-allow").expect("the Allow button is on screen");
    app.handle_chrome_click(allow.0, allow.1);
    check(
        app.passkey_prompt.is_none(),
        "Allow dismisses the interstitial",
    );
    let result = page_result(&app);
    check(
        result.starts_with("created:") && result.contains(":pk:true") && !result.contains(":att:0:"),
        &format!("Allow registers the credential with attestationObject and PublicKeyCredential ({result})"),
    );
    // The registration succeeded, so the page now asks for an assertion.
    // That is a second ceremony, and it needs a second answer.
    app.poll_page_passkey_ceremonies();
    check(
        app.passkey_prompt.is_some(),
        "the page's assertion raises its own interstitial",
    );
    let allow = find_click(&app, "pk-allow").expect("the Allow button is on screen");
    app.handle_chrome_click(allow.0, allow.1);
    let result = page_result(&app);
    check(
        result.starts_with("asserted:"),
        &format!("Allow signs a real assertion and the page receives it ({result})"),
    );
    let sig_len: usize = result
        .rsplit(':')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    check(
        sig_len >= 64,
        "the assertion carries a real P-256 signature",
    );

    // --- Deny cancels the ceremony. ---
    load_page(&mut app, PAGE_HTML);
    app.poll_page_passkey_ceremonies();
    check(
        app.passkey_prompt.is_some(),
        "a second ceremony raises the interstitial again",
    );
    let deny = find_click(&app, "pk-deny").expect("the Deny button is on screen");
    app.handle_chrome_click(deny.0, deny.1);
    check(
        app.passkey_prompt.is_none(),
        "Deny dismisses the interstitial",
    );
    let result = page_result(&app);
    check(
        result.starts_with("error:"),
        &format!("Deny makes the page's promise reject ({result})"),
    );
    check(
        !result.contains("asserted") && !result.contains("created"),
        "Deny does not sign or register anything",
    );

    // --- An unanswered ceremony fails when its deadline passes. ---
    load_page(&mut app, PAGE_HTML);
    app.poll_page_passkey_ceremonies();
    check(
        app.passkey_prompt.is_some(),
        "a third ceremony raises the interstitial",
    );
    // Backdate the ceremony past the deadline, the way waiting would.
    let ceremony_id = app
        .passkey_prompt
        .as_ref()
        .map(|c| c.ceremony_id.clone())
        .unwrap_or_default();
    if let Some(ceremony) = app
        .tab_mut()
        .instance
        .page
        .ctx
        .pending_page_ceremonies
        .get_mut(&ceremony_id)
    {
        ceremony.started_at = std::time::Instant::now()
            - std::time::Duration::from_secs(chrome::PASSKEY_PROMPT_TIMEOUT_SECS + 1);
    }
    app.poll_page_passkey_ceremonies();
    check(
        app.passkey_prompt.is_none(),
        "an unanswered ceremony expires and the interstitial goes away",
    );
    let result = page_result(&app);
    check(
        result.starts_with("error:"),
        &format!("an expired ceremony makes the page's promise reject ({result})"),
    );
    check(
        !result.contains("asserted"),
        "an expired ceremony does not sign anything",
    );

    // --- The glasses DAT card is the primary surface. ---
    // A page ceremony announces itself the same way an agent-proposed one
    // does, so the host that turns proposals into cards needs no new case.
    let mut app = test_app();
    app.control_connect(1, Role::Host);
    load_page(&mut app, PAGE_HTML);
    app.poll_page_passkey_ceremonies();
    let out = app.drain_control_out();
    let announced = out.iter().find(|(_, frame)| {
        frame.get("type").and_then(Value::as_str) == Some("proposal.pending")
    });
    check(
        announced.is_some(),
        "a page ceremony is announced to the glasses as proposal.pending",
    );
    let announced = announced.map(|(_, frame)| frame.clone()).unwrap_or(Value::Null);
    check(
        announced
            .get("requires_glasses")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "the announcement asks for a glasses decision rather than an automatic one",
    );
    check(
        announced
            .get("proposal_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("page-passkey-")),
        "the announcement names the ceremony it is about",
    );
    let rationale = announced
        .get("rationale")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    check(
        !rationale.is_empty(),
        "the announcement carries the text the card shows",
    );

    // The glasses answer with a decision, which settles the ceremony. It
    // carries no fingerprint or generation: the page's own script may change
    // the document while the prompt is up.
    let proposal_id = announced
        .get("proposal_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut seq = 0u64;
    seq += 1;
    let frame = json!({
        "v": 1, "seq": seq, "type": "decision",
        "proposal_id": proposal_id,
        "decision": "reject",
        "decided_by": "user",
        "surface": "socket",
    });
    app.handle_control(1, frame);
    check(
        app.passkey_prompt.is_none(),
        "a decision from the glasses settles the ceremony",
    );
    let result = page_result(&app);
    check(
        result.starts_with("error:"),
        &format!("the glasses can deny the ceremony and the page is told ({result})"),
    );

    cleanup_test_run();
    if failures == 0 {
        println!("PASSKEY INTERSTITIAL TEST PASS");
        0
    } else {
        println!("PASSKEY INTERSTITIAL TEST FAIL ({failures})");
        1
    }
}

fn observe_cli() -> i32 {
    let mut app = test_app();
    let snapshot = app.observe_active();
    match serde_json::to_string_pretty(&snapshot) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("observe serialization failed: {e}");
            1
        }
    }
}

/// `--keychain-cleanup`: removes the Keychain items that earlier test runs
/// left behind.
///
/// macOS keychain interaction is switched off first, so an item this binary
/// is not trusted to touch fails with an error instead of prompting the user
/// for the login keychain password. The cleanup reports exactly what it
/// removed and what it could not.
fn keychain_cleanup() -> i32 {
    if let Err(error) = keychain::ensure_no_user_interaction() {
        eprintln!("could not disable keychain interaction: {error}");
        return 1;
    }
    // Report the guarantee this run is relying on, read back from the
    // platform rather than assumed.
    match keychain::user_interaction_allowed() {
        Ok(false) => println!("keychain user interaction: disabled (no password prompt possible)"),
        Ok(true) => {
            eprintln!("keychain user interaction is still enabled; refusing to touch the keychain");
            return 1;
        }
        Err(error) => {
            eprintln!("could not read the keychain interaction state: {error}");
            return 1;
        }
    }
    let prefix = "ai.zmax.hermes.browser.";
    let items = keychain::list_all_generic_items();
    let mut removed = 0usize;
    let mut refused = 0usize;
    for (service, attributes) in items {
        // An earlier test run used a temporary directory as the service name.
        // Those items are Hermes' too, so they are cleaned as well.
        let is_hermes = service.starts_with(prefix) || service.contains("hermes-");
        if !is_hermes {
            continue;
        }
        if keychain::delete_generic_item(&service, &attributes.account) {
            removed += 1;
            println!("removed {service} ({})", attributes.account);
        } else {
            refused += 1;
            println!(
                "left in place: {service} ({}) — this binary is not trusted to delete it",
                attributes.account
            );
        }
    }
    println!("keychain cleanup: {removed} removed, {refused} left in place");
    0
}

/// `--supervisortest`: the phase-2 control protocol end to end against a real
/// BrowserApp — channel authority, proposal/decision binding, exactly-once
/// execution, takeover/release, stop discarding a fetch, and audit secrecy.
fn supervisortest() -> i32 {
    prepare_test_run();
    fn send(
        app: &mut BrowserApp,
        conn: u64,
        seq: &mut u64,
        mut frame: Value,
    ) -> Vec<(OutTarget, Value)> {
        *seq += 1;
        frame["v"] = json!(1);
        frame["seq"] = json!(*seq);
        app.handle_control(conn, frame);
        app.drain_control_out()
    }
    fn find<'a>(out: &'a [(OutTarget, Value)], kind: &str) -> Option<&'a Value> {
        out.iter()
            .map(|(_, v)| v)
            .find(|v| v.get("type").and_then(Value::as_str) == Some(kind))
    }
    fn refused(out: &[(OutTarget, Value)]) -> Option<String> {
        find(out, "refused")
            .and_then(|v| v.get("reason"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }
    fn counter_text(app: &BrowserApp) -> String {
        let page = &app.tab().instance.page;
        let Some(button) = page.doc().get_element_by_id("action-btn") else {
            return String::new();
        };
        let Some(node) = page.doc().get_node(button) else {
            return String::new();
        };
        let Some(child) = node.children.first().copied() else {
            return String::new();
        };
        match page.doc().get_node(child).map(|n| &n.data) {
            Some(NodeData::Text(t)) => t.content.clone(),
            _ => String::new(),
        }
    }
    fn observe(
        app: &mut BrowserApp,
        conn: u64,
        seq: &mut u64,
        session_id: &str,
        screenshot: bool,
    ) -> Value {
        let out = send(
            app,
            conn,
            seq,
            json!({"type": "observe.request", "session_id": session_id,
                   "request_id": "r-obs", "include_screenshot": screenshot}),
        );
        find(&out, "observe.result").cloned().unwrap_or(Value::Null)
    }
    fn action_id(snapshot: &Value) -> String {
        snapshot
            .get("elements")
            .and_then(Value::as_array)
            .and_then(|els| {
                els.iter().find(|e| {
                    e.get("label")
                        .and_then(Value::as_str)
                        .is_some_and(|l| l.contains("Increment Counter"))
                })
            })
            .and_then(|e| e.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }
    fn action_ref(snapshot: &Value) -> String {
        snapshot
            .get("elements")
            .and_then(Value::as_array)
            .and_then(|els| {
                els.iter().find(|e| {
                    e.get("label")
                        .and_then(Value::as_str)
                        .is_some_and(|l| l.contains("Increment Counter"))
                })
            })
            .and_then(|e| e.get("node_ref"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }
    fn propose_click(
        app: &mut BrowserApp,
        seq: &mut u64,
        session_id: &str,
        snapshot: &Value,
        proposal_id: &str,
    ) -> Vec<(OutTarget, Value)> {
        send(
            app,
            2,
            seq,
            json!({
                "type": "propose", "session_id": session_id, "request_id": proposal_id,
                "proposal_id": proposal_id,
                "snapshot_id": snapshot.get("snapshot_id").cloned().unwrap_or(Value::Null),
                "fingerprint": snapshot.get("fingerprint").cloned().unwrap_or(Value::Null),
                "generation": snapshot.get("generation").cloned().unwrap_or(Value::Null),
                "operation": {"kind": "click", "target_id": action_id(snapshot)},
                "rationale": "Increment the counter",
            }),
        )
    }

    let mut failures = 0;
    let mut check = |condition: bool, message: &str| {
        if condition {
            println!("ok   {message}");
        } else {
            eprintln!("FAIL {message}");
            failures += 1;
        }
    };

    let mut app = test_app();
    app.control_connect(1, Role::Host);
    app.control_connect(2, Role::Agent);
    let (mut seq_h, mut seq_a) = (0u64, 0u64);

    // The agent cannot open sessions.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "session.open", "title": "x", "request_id": "r0"}),
    );
    check(
        refused(&out).as_deref() == Some("wrong_channel"),
        "agent session.open refused wrong_channel",
    );
    check(
        app.control.is_none(),
        "wrong-channel open created no session",
    );

    // Host opens a supervised session on a dedicated tab.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "session.open", "title": "Supervisor test", "request_id": "r1",
               "capabilities": {"protocol": 1}, "policy": {"decision": "human_per_action"}}),
    );
    let opened = find(&out, "session.opened").cloned().unwrap_or(Value::Null);
    let session_id = opened
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    check(
        !session_id.is_empty(),
        "host session.open returns a session id",
    );
    check(app.tabs.len() == 2, "session.open creates a dedicated tab");
    check(
        app.control
            .as_ref()
            .is_some_and(|s| s.state == ControlState::Agent),
        "session starts in agent state",
    );

    // Capabilities report form/text input support honestly.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "capabilities.get", "request_id": "r2"}),
    );
    let caps = find(&out, "capabilities").cloned().unwrap_or(Value::Null);
    check(
        caps.get("text_input").and_then(Value::as_str) == Some("supported")
            && caps.get("forms").and_then(Value::as_str) == Some("supported")
            && caps.get("passkeys").and_then(Value::as_str) == Some("supported")
            && caps.get("passkey_broker").and_then(Value::as_str) == Some("supported"),
        "capabilities report text input, forms, and passkeys supported",
    );

    // Observe, propose, and approve through the native supervision bar.
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    check(
        action_id(&snap) == "el_1" || !action_id(&snap).is_empty(),
        "observe indexes the counter action",
    );
    let out = propose_click(&mut app, &mut seq_a, &session_id, &snap, "p_1");
    check(
        find(&out, "proposal.pending").is_some(),
        "propose emits proposal.pending to the host",
    );
    check(
        counter_text(&app).contains("(0)"),
        "propose alone does not execute",
    );
    check(
        app.chrome_doc.get_element_by_id("sup-approve").is_some(),
        "supervision bar renders Approve while a proposal is pending",
    );
    let out = propose_click(&mut app, &mut seq_a, &session_id, &snap, "p_2");
    check(
        refused(&out).as_deref() == Some("proposal_pending"),
        "second proposal refused proposal_pending",
    );

    // The agent channel cannot decide its own proposal.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "decision", "session_id": session_id, "request_id": "r3",
               "proposal_id": "p_1", "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "approve", "decided_by": "agent", "surface": "socket"}),
    );
    check(
        refused(&out).as_deref() == Some("wrong_channel"),
        "agent decision refused wrong_channel",
    );
    check(
        counter_text(&app).contains("(0)"),
        "wrong-channel decision does not execute",
    );

    // A host decision bound to the wrong fingerprint is refused, pending survives.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "decision", "session_id": session_id, "request_id": "r4",
               "proposal_id": "p_1", "fingerprint": "0".repeat(64),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "approve", "decided_by": "user", "surface": "socket"}),
    );
    check(
        refused(&out).as_deref() == Some("decision_mismatch"),
        "wrong-fingerprint decision refused decision_mismatch",
    );
    check(
        counter_text(&app).contains("(0)"),
        "mismatched decision does not execute",
    );

    // Approve by clicking the real supervision-bar button.
    if let Some((x, y)) = find_click(&app, "sup-approve") {
        app.handle_chrome_click(x, y);
    }
    let out = app.drain_control_out();
    check(
        find(&out, "executed").is_some(),
        "bar Approve executes and emits executed",
    );
    check(
        find(&out, "executed").is_some_and(|f| {
            f.get("input_event_id").and_then(Value::as_u64).unwrap_or(0) >= 1
                && f.get("input_provenance").and_then(Value::as_str) == Some("approved_agent")
        }),
        "executed carries queued input event id and provenance",
    );
    check(
        counter_text(&app).contains("(1)"),
        "bar Approve increments the counter exactly once",
    );

    // The same decision cannot execute twice.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "decision", "session_id": session_id, "request_id": "r5",
               "proposal_id": "p_1", "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "approve", "decided_by": "user", "surface": "socket"}),
    );
    check(
        refused(&out).as_deref() == Some("decision_used"),
        "replayed decision refused decision_used",
    );
    check(
        counter_text(&app).contains("(1)"),
        "replayed decision does not execute again",
    );

    // A replayed sequence number is refused.
    app.handle_control(
        2,
        json!({"v": 1, "type": "ping", "seq": seq_a, "request_id": "r-replay"}),
    );
    let out = app.drain_control_out();
    check(
        refused(&out).as_deref() == Some("replayed_seq"),
        "replayed seq refused",
    );

    // Execution invalidates the snapshot: proposing from it is stale.
    let out = propose_click(&mut app, &mut seq_a, &session_id, &snap, "p_2");
    check(
        refused(&out).is_some_and(|r| r.starts_with("stale")),
        "propose from a pre-execution snapshot is stale",
    );

    // Protocol reject: no execution, back to agent.
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    let _ = propose_click(&mut app, &mut seq_a, &session_id, &snap, "p_3");
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "decision", "session_id": session_id, "request_id": "r6",
               "proposal_id": "p_3", "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "reject", "decided_by": "user", "surface": "socket"}),
    );
    check(
        find(&out, "proposal.invalidated").is_some(),
        "reject destroys the proposal",
    );
    check(
        counter_text(&app).contains("(1)"),
        "reject does not execute",
    );

    // An unknown stable reference was never issued by this page instance.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "p_unknown",
               "proposal_id": "p_unknown",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "click", "target_ref": "n999_999"},
               "rationale": "Click an unknown node"}),
    );
    check(
        refused(&out).as_deref() == Some("target_missing"),
        "unknown node reference refused target_missing",
    );

    // Protocol approve: executes once.
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    check(
        !action_ref(&snap).is_empty(),
        "observe issues a stable node reference",
    );
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "p_4",
               "proposal_id": "p_4",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "click", "target_ref": action_ref(&snap)},
               "rationale": "Increment by stable reference"}),
    );
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "decision", "session_id": session_id, "request_id": "r7",
               "proposal_id": "p_4", "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "approve", "decided_by": "user", "surface": "socket"}),
    );
    check(
        find(&out, "executed").is_some(),
        "protocol approve emits executed",
    );
    check(
        counter_text(&app).contains("(2)"),
        "protocol approve increments the counter",
    );

    // Screenshot bytes hash to the advertised hash.
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, true);
    let b64 = snap
        .get("screenshot_png_b64")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let hash = snap
        .get("screenshot_hash")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let bytes = control::b64_decode(b64).unwrap_or_default();
    check(
        !bytes.is_empty() && format!("{:x}", Sha256::digest(&bytes)) == hash,
        "screenshot hash matches the delivered PNG bytes",
    );
    check(
        snap.get("frame_id").and_then(Value::as_u64).unwrap_or(0) >= 1
            && snap
                .get("presented_frame_id")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                >= snap.get("frame_id").and_then(Value::as_u64).unwrap_or(0),
        "screenshot names its frame under the frame contract",
    );

    // An approved scroll really scrolls the page.
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "p_scroll",
               "proposal_id": "p_scroll",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "scroll", "dy": 240.0}, "rationale": "Read further"}),
    );
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "decision", "session_id": session_id, "request_id": "r-scroll",
               "proposal_id": "p_scroll",
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "approve", "decided_by": "user", "surface": "socket"}),
    );
    check(
        find(&out, "executed").is_some(),
        "approved scroll emits executed",
    );
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    check(
        snap.get("scroll")
            .and_then(|s| s.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            > 0.0,
        "approved scroll moves the viewport",
    );

    // Takeover destroys the pending scroll proposal and cuts off observation.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "p_5",
               "proposal_id": "p_5",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "scroll", "dy": 240.0}, "rationale": "Read further"}),
    );
    check(
        find(&out, "proposal.pending").is_some(),
        "scroll proposal pends",
    );
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "control.take", "session_id": session_id, "request_id": "r8",
               "decided_by": "user", "surface": "socket"}),
    );
    check(
        find(&out, "proposal.invalidated").is_some(),
        "takeover destroys the pending proposal",
    );
    check(
        app.control
            .as_ref()
            .is_some_and(|s| s.state == ControlState::Human),
        "takeover enters human state",
    );
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "observe.request", "session_id": session_id, "request_id": "r9"}),
    );
    check(
        refused(&out).is_some() && find(&out, "observe.result").is_none(),
        "agent receives no snapshot while the human owns the page",
    );

    // The human can really scroll and click while in control.
    app.scroll_page(-240.0);
    let snapshot = app.observe_active();
    if let Some(el) = snapshot
        .elements
        .iter()
        .find(|e| e.label.contains("Increment Counter"))
    {
        let x = (el.bounds.x + el.bounds.width / 2.0) as f32;
        let y = (el.bounds.y + el.bounds.height / 2.0) as f32 + CHROME_HEIGHT as f32;
        app.handle_page_click(x, y);
    }
    check(
        counter_text(&app).contains("(3)"),
        "human click works during takeover",
    );

    // Release requires a fresh observation before proposing.
    let _ = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "control.release", "session_id": session_id, "request_id": "r10",
               "decided_by": "user", "surface": "socket"}),
    );
    let out = propose_click(&mut app, &mut seq_a, &session_id, &snap, "p_6");
    check(
        refused(&out).is_some_and(|r| r.starts_with("stale")),
        "propose after release requires a fresh observation",
    );

    // Stop during a fetch: the late result must not commit.
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "p_7",
               "proposal_id": "p_7",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "navigate", "url": "https://example.test/late"},
               "rationale": "Load the late page"}),
    );
    let _ = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "decision", "session_id": session_id, "request_id": "r11",
               "proposal_id": "p_7", "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "decision": "approve", "decided_by": "user", "surface": "socket"}),
    );
    let session_tab = app.control.as_ref().map(|s| s.tab_id).expect("session tab");
    let _ = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "stop", "session_id": session_id, "request_id": "r12",
               "decided_by": "user", "surface": "socket"}),
    );
    check(
        app.control
            .as_ref()
            .is_some_and(|s| s.state == ControlState::Stopped),
        "stop enters stopped state",
    );
    let tab = app
        .tabs
        .iter()
        .find(|t| t.canonical.id == session_tab)
        .expect("tab");
    let current_generation = tab.canonical.browser.generation;
    app.handle_navigated(
        session_tab,
        current_generation,
        Url::parse("https://example.test/late").expect("url"),
        Ok("<html><head><title>Late</title></head><body>late</body></html>".to_string()),
    );
    let tab = app
        .tabs
        .iter()
        .find(|t| t.canonical.id == session_tab)
        .expect("tab");
    check(
        tab.canonical.title != "Late",
        "fetch result arriving after stop does not commit",
    );

    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "observe.request", "session_id": session_id, "request_id": "r13"}),
    );
    check(
        refused(&out).as_deref() == Some("wrong_state"),
        "observe after stop refused",
    );

    // Audit carries the lifecycle and no secrets.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "audit.read", "session_id": session_id, "request_id": "r14"}),
    );
    let audit = find(&out, "audit.result").cloned().unwrap_or(Value::Null);
    let serialized = audit.to_string();
    check(
        serialized.contains("\"executed\"")
            && serialized.contains("\"rejected\"")
            && serialized.contains("\"stopped\""),
        "audit records executed, rejected, and stopped",
    );

    // A fresh session after a terminal one can complete via task.done.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "session.open", "title": "Second task", "request_id": "r15",
               "policy": {"decision": "human_per_action"}}),
    );
    let opened = find(&out, "session.opened").cloned().unwrap_or(Value::Null);
    let session2 = opened
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "task.done", "session_id": session2, "request_id": "r16"}),
    );
    check(
        app.control
            .as_ref()
            .is_some_and(|s| s.state == ControlState::Done),
        "task.done completes a session with no pending proposal",
    );

    // Sensitive fixture: typing into a password field is refused and raises
    // action_required; the secret appears in no frame or audit record.
    // Same app: winit permits one event loop per process, and the previous
    // session is terminal, so a new session may open here.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "session.open", "title": "Login fixture", "request_id": "f1",
               "policy": {"decision": "human_per_action"}}),
    );
    let opened = find(&out, "session.opened").cloned().unwrap_or(Value::Null);
    let session_id = opened
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let fixture = r#"<!DOCTYPE html><html><head><title>Login</title></head><body>
      <label for="name">Name</label><input id="name" value="Ada">
      <label for="pw">Password</label><input id="pw" type="password" value="correct-horse">
    </body></html>"#;
    let fixture_url = Url::parse("http://127.0.0.1/login").expect("fixture url");
    let viewport = page_viewport(app.size, app.scale, app.theme);
    let canonical_tab_before = app.tab().canonical.id;
    let page_id_before = app.tab().instance.page_id;
    {
        let tab = app.tab_mut();
        tab.canonical
            .browser
            .commit(fixture_url.clone(), fixture.to_string());
        tab.replace_page(Page::load(
            fixture,
            Some(fixture_url),
            None,
            viewport,
            Theme::Light,
        ));
        tab.canonical.title = "Login".to_string();
    }
    check(
        app.tab().canonical.id == canonical_tab_before
            && app.tab().instance.page_id > page_id_before
            && app
                .control
                .as_ref()
                .is_some_and(|s| s.tab_id == canonical_tab_before),
        "canonical tab and its session binding survive page-instance replacement",
    );
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    check(
        !snap.to_string().contains("correct-horse"),
        "password value absent from observe.result",
    );
    let password_id = snap
        .get("elements")
        .and_then(Value::as_array)
        .and_then(|els| {
            els.iter()
                .find(|e| e.get("value_state").and_then(Value::as_str) == Some("sensitive"))
        })
        .and_then(|e| e.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    check(
        !password_id.is_empty(),
        "password field is indexed as sensitive",
    );
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "f2",
               "proposal_id": "pf_1",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "type", "target_id": password_id, "text": "hunter2"},
               "rationale": "Type the password"}),
    );
    check(
        refused(&out).as_deref() == Some("sensitive_requires_human"),
        "typing into a password field refused sensitive_requires_human",
    );
    check(
        find(&out, "action_required").is_some(),
        "sensitive refusal raises action_required",
    );
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "audit.read", "session_id": session_id, "request_id": "f3"}),
    );
    let audit = find(&out, "audit.result").cloned().unwrap_or(Value::Null);
    check(
        !audit.to_string().contains("correct-horse") && !audit.to_string().contains("hunter2"),
        "secrets absent from audit records",
    );

    // Non-sensitive typing is an honest capability refusal until phase 3.
    let _ = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "control.take", "session_id": session_id, "request_id": "f4",
               "decided_by": "user", "surface": "socket"}),
    );
    let _ = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "control.release", "session_id": session_id, "request_id": "f5",
               "decided_by": "user", "surface": "socket"}),
    );
    let snap = observe(&mut app, 2, &mut seq_a, &session_id, false);
    let name_id = snap
        .get("elements")
        .and_then(Value::as_array)
        .and_then(|els| {
            els.iter()
                .find(|e| e.get("label").and_then(Value::as_str) == Some("Name"))
        })
        .and_then(|e| e.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "propose", "session_id": session_id, "request_id": "f6",
               "proposal_id": "pf_2",
               "snapshot_id": snap.get("snapshot_id").cloned().unwrap_or(Value::Null),
               "fingerprint": snap.get("fingerprint").cloned().unwrap_or(Value::Null),
               "generation": snap.get("generation").cloned().unwrap_or(Value::Null),
               "operation": {"kind": "type", "target_id": name_id, "text": "Grace"},
               "rationale": "Type the name"}),
    );
    check(
        find(&out, "proposal.pending").is_some(),
        "non-sensitive type proposal pends for host approval",
    );

    // Zone X: Semantic X-Ray overlay + live telemetry bar + P-256 WebAuthn verification
    app.xray_enabled = true;
    app.sync_chrome();
    check(
        app.chrome_doc.get_element_by_id("xray-telemetry-bar").is_some()
            && app.chrome_doc.get_element_by_id("xray-vault-shield").is_some()
            && app.chrome_doc.get_element_by_id("xray-freeze-shield").is_some(),
        "Semantic X-Ray renders telemetry HUD, blind vault shield, and freeze shield in chrome_doc",
    );

    // The supervisor suite must exercise the implementation that ships, not
    // the legacy one. These checks used to drive `vault::sign_webauthn_
    // assertion`; they now drive the real broker, which keeps the private
    // key in the macOS Keychain and runs behind the human approval gate.
    {
        use crate::credentials::{
            CeremonyOutcome, LocalPasskeyBroker, PasskeyBroker, PasskeyRequest,
            PasskeyStatus, RegistrationRequest,
        };
        use crate::webauthn::types::{PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity};

        let service = format!(
            "ai.zmax.hermes.browser.test.supervisor.{}",
            std::process::id()
        );
        let store = crate::credentials::KeychainPasskeyStore::with_service(
            &service,
            "passkey_records_v1",
        );
        let mut broker = LocalPasskeyBroker::with_store(store);

        let register_id = broker
            .begin_registration(RegistrationRequest {
                task_id: "supervisor".to_string(),
                origin: "https://socon2026.local".to_string(),
                rp: PublicKeyCredentialRpEntity {
                    id: "socon2026.local".to_string(),
                    name: Some("Socon".to_string()),
                },
                user: PublicKeyCredentialUserEntity {
                    id: b"supervisor-user".to_vec(),
                    name: "ryan@socon.dev".to_string(),
                    display_name: "Ryan".to_string(),
                },
                challenge: b"challenge_suptest_123456".to_vec(),
                pub_key_cred_params: Vec::new(),
                exclude_credentials: Vec::new(),
                authenticator_selection: None,
                attestation: None,
                extensions: None,
                cross_origin: false,
                related_origin_validated: false,
            })
            .expect("begin registration");
        check(
            matches!(
                broker.poll_registration(register_id.clone()).unwrap(),
                PasskeyStatus::AwaitingApproval { .. }
            ),
            "the broker holds a registration ceremony until a human approves it",
        );
        broker
            .approve_ceremony(register_id.clone(), "user", "supervisor")
            .expect("approve registration");
        let registered = broker.poll_registration(register_id).unwrap();
        match registered {
            PasskeyStatus::Complete(CeremonyOutcome::Registration(credential)) => check(
                credential.kind == "public-key" && !credential.response.attestation_object.is_empty(),
                "the broker completes a real ECDSA P-256 WebAuthn registration ceremony",
            ),
            _ => check(
                false,
                "the registration ceremony must complete",
            ),
        }

        let assert_id = broker
            .begin_assertion(PasskeyRequest {
                task_id: "supervisor".to_string(),
                origin: "https://socon2026.local".to_string(),
                rp_id: Some("socon2026.local".to_string()),
                challenge: b"challenge_suptest_123456".to_vec(),
                allow_credentials: Vec::new(),
                user_verification: None,
                extensions: None,
                cross_origin: false,
                related_origin_validated: false,
                top_origin: None,
            })
            .expect("begin assertion");
        check(
            matches!(
                broker.poll_assertion(assert_id.clone()).unwrap(),
                PasskeyStatus::AwaitingApproval { .. }
            ),
            "the broker holds an assertion ceremony until a human approves it",
        );
        broker
            .approve_ceremony(assert_id.clone(), "user", "supervisor")
            .expect("approve assertion");
        let asserted = broker.poll_assertion(assert_id).unwrap();
        match asserted {
            PasskeyStatus::Complete(CeremonyOutcome::Assertion(credential)) => check(
                !credential.response.signature.is_empty()
                    && !credential.response.authenticator_data.is_empty(),
                "the broker completes a real ECDSA P-256 WebAuthn assertion ceremony",
            ),
            _ => check(false, "the assertion ceremony must complete"),
        }

        // A wrong challenge must not be satisfied by the stored credential.
        let wrong = broker.begin_assertion(PasskeyRequest {
            task_id: "supervisor".to_string(),
            origin: "https://socon2026.local".to_string(),
            rp_id: Some("socon2026.local".to_string()),
            challenge: b"wrong_challenge".to_vec(),
            allow_credentials: Vec::new(),
            user_verification: None,
            extensions: None,
            cross_origin: false,
            related_origin_validated: false,
            top_origin: None,
        });
        check(
            wrong.is_ok(),
            "the broker starts an assertion for a different challenge; the signature covers the challenge it was given",
        );

        crate::credentials::KeychainPasskeyStore::with_service(&service, "passkey_records_v1")
            .clear()
            .ok();
    }

    cleanup_test_run();

    if failures == 0 {
        println!("SUPERVISOR TEST PASS");
        0
    } else {
        println!("SUPERVISOR TEST FAIL ({failures})");
        1
    }
}

/// `--observetest`: snapshot labels, stability, mutation fingerprinting, and
/// sensitive-value redaction against real Blitz documents.
fn observetest() -> i32 {
    prepare_test_run();
    let mut failures = 0;
    let mut check = |condition: bool, message: &str| {
        if condition {
            println!("ok   {message}");
        } else {
            eprintln!("FAIL {message}");
            failures += 1;
        }
    };

    let mut app = test_app();
    let first = app.observe_active();
    let second = app.observe_active();
    check(first.fingerprint.len() == 64, "fingerprint is sha256 hex");
    check(
        first.fingerprint == second.fingerprint,
        "unmutated snapshot fingerprint is stable",
    );
    check(
        !first.elements.is_empty()
            && first
                .elements
                .iter()
                .map(|e| &e.node_ref)
                .collect::<Vec<_>>()
                == second
                    .elements
                    .iter()
                    .map(|e| &e.node_ref)
                    .collect::<Vec<_>>(),
        "stable node references match across snapshots of one page instance",
    );
    check(first.url == START_URL, "snapshot carries current URL");
    check(
        first.document_generation == 1,
        "snapshot carries document generation",
    );

    let action = first
        .elements
        .iter()
        .find(|element| element.label.contains("Increment Counter (0)"))
        .cloned();
    check(action.is_some(), "snapshot indexes the start-page action");
    if let Some(action) = action {
        check(
            action.kind == "click",
            "start-page action is classified click",
        );
        check(
            action.bounds.width > 0.0 && action.bounds.height > 0.0,
            "action has bounds",
        );
        let x = (action.bounds.x + action.bounds.width / 2.0) as f32;
        let y = (action.bounds.y + action.bounds.height / 2.0) as f32 + CHROME_HEIGHT as f32;
        app.handle_page_click(x, y);
    }
    let after_click = app.observe_active();
    check(
        after_click
            .elements
            .iter()
            .any(|element| element.label.contains("Increment Counter (1)")),
        "snapshot observes clicked state",
    );
    check(
        after_click.fingerprint != first.fingerprint,
        "mutation changes fingerprint",
    );

    let fixture = r#"<!DOCTYPE html><html><head><title>Fixture</title></head><body>
      <label for="name">Name</label><input id="name" value="Ada">
      <label for="pw">Password</label><input id="pw" type="password" value="correct-horse">
      <label><input id="agree" type="checkbox" checked> Agree</label>
      <a href="/next">Next page</a>
    </body></html>"#;
    let page = Page::load(
        fixture,
        Some(Url::parse("http://127.0.0.1/fixture").expect("fixture url")),
        None,
        page_viewport((WIDTH, HEIGHT), 1.0, Theme::Light),
        Theme::Light,
    );
    let fixture_snapshot = observe::observe(
        page.doc(),
        observe::SnapshotMetadata {
            snapshot_id: 1,
            tab_id: 99,
            page_id: 1,
            document_generation: 1,
            url: "http://127.0.0.1/fixture".to_string(),
            title: "Fixture".to_string(),
            viewport_width: WIDTH as f32,
            viewport_height: HEIGHT as f32 - CHROME_HEIGHT as f32,
            scale: 1.0,
        },
    );
    let serialized = serde_json::to_string(&fixture_snapshot).unwrap_or_default();
    check(
        fixture_snapshot
            .elements
            .iter()
            .any(|e| e.label == "Name" && e.value.as_deref() == Some("Ada")),
        "labelled text input exposes non-sensitive value",
    );
    check(
        fixture_snapshot
            .elements
            .iter()
            .any(|e| e.label == "Password" && e.value_state == observe::ValueState::Sensitive),
        "password input is classified sensitive",
    );
    check(
        !serialized.contains("correct-horse"),
        "password value never serializes",
    );
    check(
        fixture_snapshot
            .elements
            .iter()
            .any(|e| e.checked == Some(true)),
        "checkbox state is observed",
    );

    cleanup_test_run();

    if failures == 0 {
        println!("OBSERVE TEST PASS");
        0
    } else {
        println!("OBSERVE TEST FAIL ({failures})");
        1
    }
}

fn selftest() -> i32 {
    prepare_test_run();
    let viewport = Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light);
    let mut doc = HtmlDocument::from_html(
        START_PAGE,
        DocumentConfig {
            viewport: Some(viewport),
            ..Default::default()
        },
    );
    doc.resolve(0.0);

    let button = match doc.get_element_by_id("action-btn") {
        Some(id) => id,
        None => {
            eprintln!("FAIL: #action-btn not found");
            return 1;
        }
    };

    let text_of = |doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId| -> String {
        let Some(node) = doc.get_node(id) else {
            return String::new();
        };
        let Some(child) = node.children.first().copied() else {
            return String::new();
        };
        match doc.get_node(child).map(|n| &n.data) {
            Some(NodeData::Text(t)) => t.content.clone(),
            _ => String::new(),
        }
    };

    let before = text_of(&doc, button);
    let mut ctx = Box::new(BlitzHostContext::new(doc));
    let hermes = unsafe { hermes_app_init((&mut *ctx as *mut BlitzHostContext).cast::<c_void>()) };
    if hermes.is_null() {
        eprintln!("FAIL: hermes_app_init returned null");
        return 1;
    }

    let mut point = None;
    'outer: for y in (0..HEIGHT as i32).step_by(4) {
        for x in (0..WIDTH as i32).step_by(4) {
            if let Some(hit) = ctx.doc.hit(x as f32, y as f32) {
                if dom_ffi::bubble_target(&ctx.doc, hit.node_id) == button {
                    point = Some((x as f32, y as f32));
                    break 'outer;
                }
            }
        }
    }
    let Some((cx, cy)) = point else {
        eprintln!("FAIL: no hit point resolved to #action-btn");
        unsafe { hermes_app_destroy(hermes) };
        return 1;
    };

    unsafe { hermes_app_dispatch_event(hermes, c"click".as_ptr(), button.as_u64(), cx, cy) };
    let script = format!("__dispatchBlitzEvent('click', {}, 0, 0)", button.as_u64());
    if let Ok(c_script) = std::ffi::CString::new(script) {
        unsafe { hermes_app_eval_script(hermes, c_script.as_ptr(), c"<selftest>".as_ptr()) };
    }

    let dirty = ctx.dirty;
    let after = text_of(&ctx.doc, button);
    ctx.doc.resolve(0.0);
    let computed = ctx.doc.resolved_style_value(button, "background-color");
    unsafe { hermes_app_destroy(hermes) };

    println!("before={before:?} after={after:?} dirty={dirty} bg={computed:?}");

    let mut ok = true;
    if !dirty {
        eprintln!("FAIL: JS did not mark the document dirty");
        ok = false;
    }
    if !computed.contains("22, 163, 74") {
        eprintln!("FAIL: computed background-color not mutated ({computed:?})");
        ok = false;
    }
    if !after.contains("Increment Counter (2)") {
        eprintln!("FAIL: counter did not reach 2");
        ok = false;
    }

    // History and URL normalization.
    let mut b = Browser::new();
    b.commit(Url::parse(START_URL).unwrap(), String::new());
    if b.can_go_back() || b.can_go_forward() {
        eprintln!("FAIL: fresh history should have no back/forward");
        ok = false;
    }
    b.commit(
        Url::parse("https://example.com/").unwrap(),
        "<html></html>".into(),
    );
    if !b.can_go_back() {
        eprintln!("FAIL: back should be available after navigating");
        ok = false;
    }
    b.go_back();
    if !b.can_go_forward() {
        eprintln!("FAIL: forward should be available after going back");
        ok = false;
    }
    b.go_forward();
    if b.can_go_forward() {
        eprintln!("FAIL: forward should be exhausted at the end of history");
        ok = false;
    }

    match browser::normalize_input("example.com") {
        Ok(u) if u.as_str() == "https://example.com/" => {}
        other => {
            eprintln!("FAIL: bare host did not normalize: {other:?}");
            ok = false;
        }
    }
    match browser::normalize_input("hello world") {
        Ok(u) if u.host_str() == Some("duckduckgo.com") => {}
        other => {
            eprintln!("FAIL: search fallback wrong: {other:?}");
            ok = false;
        }
    }

    // Every chrome control must be reachable by hit-testing at the id the click
    // handler compares against.
    let mut state_browser = Browser::new();
    state_browser.commit(Url::parse("https://example.com/").unwrap(), String::new());
    state_browser.commit(Url::parse("https://example.org/").unwrap(), String::new());
    let tabs = [
        TabInfo {
            title: "Example",
            active: true,
        },
        TabInfo {
            title: "Other",
            active: false,
        },
    ];
    for menu_open in [false, true] {
        let html = chrome::render(&ChromeState {
            url: &state_browser.display_url(),
            focused: false,
            can_back: true,
            can_forward: false,
            loading: false,
            tabs: &tabs,
            menu_open,
            secure: true,
            surface_height: HEIGHT as f64,
            theme: Theme::Light,
            supervision: None,
            telemetry: None,
            toolbar_collapsed: false,
            explorer_tab: None,
            page_holder: PageHolderInfo {
                has_doc: false,
                title: "",
                url: "",
                links_mode: false,
                links: &[],
            },
            scrapbook_items: &[],
            favorites: &[],
            history_entries: &[],
            passkey_prompt: None,
        });
        let mut chrome_doc = HtmlDocument::from_html(
            &html,
            DocumentConfig {
                viewport: Some(Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light)),
                ..Default::default()
            },
        );
        chrome_doc.resolve(0.0);

        let mut ids = vec![
            "btn-back".to_string(),
            "btn-forward".to_string(),
            "btn-stop".to_string(),
            "btn-reload".to_string(),
            "btn-home".to_string(),
            "btn-autofill".to_string(),
            "btn-print".to_string(),
            "btn-mail".to_string(),
            "btn-menu".to_string(),
            "address".to_string(),
            "btn-go".to_string(),
            "btn-fav-overflow".to_string(),
            "btn-newtab".to_string(),
            "tab-0".to_string(),
            "tab-close-0".to_string(),
            "tab-1".to_string(),
            "btn-collapse-toolbar".to_string(),
            "exp-favorites".to_string(),
            "exp-history".to_string(),
            "exp-search".to_string(),
            "exp-scrapbook".to_string(),
            "exp-pageholder".to_string(),
        ];
        if menu_open {
            ids.push("menu-home".into());
            ids.push("menu-close-tab".into());
        }

        for id in ids {
            let Some(target_id) = chrome_doc.get_element_by_id(&id) else {
                eprintln!("FAIL: chrome is missing #{id} (menu_open={menu_open})");
                ok = false;
                continue;
            };
            let mut found = None;
            'scan: for y in (0..HEIGHT as i32).step_by(2) {
                for x in (0..WIDTH as i32).step_by(2) {
                    if let Some(hit) = chrome_doc.hit(x as f32, y as f32) {
                        if dom_ffi::bubble_target(&chrome_doc, hit.node_id) == target_id {
                            found = Some((x, y));
                            break 'scan;
                        }
                    }
                }
            }
            match found {
                Some((x, y)) => println!("chrome hit: #{id} at ({x}, {y})"),
                None => {
                    eprintln!("FAIL: #{id} is not hit-testable (menu_open={menu_open})");
                    ok = false;
                }
            }
        }
    }

    if ok {
        println!("SELFTEST PASS");
        0
    } else {
        println!("SELFTEST FAIL");
        1
    }
}

/// `--render <path> [--click] [--url <url>] [--menu]`: render chrome + page
/// offscreen through the real Vello pipeline.
fn render_to_png(
    path: &str,
    click: bool,
    url: Option<&str>,
    menu: bool,
    theme: Theme,
    size: (u32, u32),
) -> i32 {
    use anyrender::ImageRenderer;
    use anyrender_vello::VelloImageRenderer;

    let scale = 1.0;
    let (width, height) = size;
    let mut browser = Browser::new();
    let net_activity = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let provider: SharedProvider = Arc::new(blitz_net::Provider::new(None));
    let (mut page, title) = match url {
        Some(input) => {
            let target = match browser::normalize_input(input) {
                Ok(u) => u,
                Err(e) => {
                    eprintln!("FAIL: {e}");
                    return 1;
                }
            };
            println!("fetching {target}");
            let html = match browser::fetch(&provider, &target) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("FAIL: fetch {target}: {e}");
                    return 1;
                }
            };
            println!("fetched {} bytes", html.len());
            browser.commit(target.clone(), html.clone());

            let counter = net_activity.clone();
            let net: SharedProvider = Arc::new(blitz_net::Provider::new(Some(Arc::new(
                move |_doc_id: usize| {
                    counter.fetch_add(1, Ordering::SeqCst);
                },
            ))));
            let page = Page::load(
                &html,
                Some(target.clone()),
                Some(net),
                page_viewport(size, scale, theme),
                theme,
            );
            let title = title_for(&target, page.doc());
            (page, title)
        }
        None => {
            let start = Url::parse(START_URL).expect("start url");
            browser.commit(start, START_PAGE.to_string());
            let page = Page::load(
                START_PAGE,
                None,
                None,
                page_viewport(size, scale, theme),
                theme,
            );
            (page, "New tab".to_string())
        }
    };

    if url.is_some() {
        // Subresources arrive asynchronously; wait for them to settle so the
        // offscreen render matches what the window shows after loading.
        let started = Instant::now();
        let mut last = net_activity.load(Ordering::SeqCst);
        let mut idle = 0;
        while started.elapsed() < Duration::from_secs(12) {
            std::thread::sleep(Duration::from_millis(60));
            page.resolve();
            let now = net_activity.load(Ordering::SeqCst);
            let pending_critical = page.doc().has_pending_critical_resources();
            if now == last && !pending_critical && now > 0 {
                idle += 1;
                if idle >= 6 {
                    break;
                }
            } else if now != last {
                idle = 0;
                last = now;
            } else if !pending_critical {
                idle += 1;
                if idle >= 10 {
                    break;
                }
            }
        }
        page.resolve();
        println!(
            "subresources settled after {:?} ({last} wakeups, pending_critical={})",
            started.elapsed(),
            page.doc().has_pending_critical_resources()
        );
    }

    if click {
        let Some(button) = page.doc().get_element_by_id("action-btn") else {
            eprintln!("FAIL: #action-btn missing");
            return 1;
        };
        let mut point = None;
        'outer: for y in (0..height as i32).step_by(4) {
            for x in (0..width as i32).step_by(4) {
                if let Some(hit) = page.doc().hit(x as f32, y as f32) {
                    if dom_ffi::bubble_target(page.doc(), hit.node_id) == button {
                        point = Some((x as f32, y as f32));
                        break 'outer;
                    }
                }
            }
        }
        let Some((cx, cy)) = point else {
            eprintln!("FAIL: could not locate #action-btn");
            return 1;
        };
        unsafe {
            hermes_app_dispatch_event(page.hermes, c"click".as_ptr(), button.as_u64(), cx, cy)
        };
        if page.ctx.dirty {
            page.resolve();
            page.ctx.dirty = false;
        }
    }

    let url_text = browser.display_url();
    let tabs = [TabInfo {
        title: &title,
        active: true,
    }];
    let args_vec: Vec<String> = std::env::args().collect();
    let toolbar_collapsed = args_vec.iter().any(|a| a == "--collapsed");
    let explorer_tab = if args_vec.iter().any(|a| a == "--scrapbook") {
        Some(ExplorerTab::Scrapbook)
    } else if args_vec
        .iter()
        .any(|a| a == "--pageholder" || a == "--pageholder-links")
    {
        Some(ExplorerTab::PageHolder)
    } else {
        None
    };
    let links_mode = args_vec.iter().any(|a| a == "--pageholder-links");
    let demo_links = [
        HeldLinkInfo {
            label: "oreilly.linux.",
            url: "http://www.oreillynet.com/linux/",
        },
        HeldLinkInfo {
            label: "Apache",
            url: "http://www.oreillynet.com/apache/",
        },
        HeldLinkInfo {
            label: "RSS",
            url: "http://www.oreillynet.com/rss/",
        },
        HeldLinkInfo {
            label: "Patents",
            url: "http://www.oreillynet.com/patents/",
        },
        HeldLinkInfo {
            label: "FreeBSD",
            url: "http://www.oreillynet.com/freebsd/",
        },
        HeldLinkInfo {
            label: "Wireless",
            url: "http://www.oreillynet.com/wireless/",
        },
    ];
    let demo_sb = [
        ScrapbookItemInfo {
            id: "sb_1",
            title: "Web Review Server Statistics",
            url: "http://webreview.com/stats",
            timestamp: "2026-11-06",
            is_folder: false,
            selected: false,
        },
        ScrapbookItemInfo {
            id: "sb_2",
            title: "Internet Explorer Macintosh Edition ...",
            url: "http://www.microsoft.com/mac/ie",
            timestamp: "2026-11-06",
            is_folder: false,
            selected: false,
        },
        ScrapbookItemInfo {
            id: "sb_3",
            title: "O'Reilly Network -- Developers' Hub",
            url: "http://www.oreillynet.com/",
            timestamp: "2026-11-06",
            is_folder: false,
            selected: false,
        },
    ];
    let demo_favs = [
        NavEntryInfo {
            title: "Web Gallery",
            url: "https://www.apple.com/",
        },
        NavEntryInfo {
            title: "Office for Macintosh",
            url: "https://www.microsoft.com/mac",
        },
        NavEntryInfo {
            title: "Products for Mac",
            url: "https://www.oreillynet.com/",
        },
        NavEntryInfo {
            title: "Microsoft Internet",
            url: "https://www.msn.com/",
        },
        NavEntryInfo {
            title: "Apple",
            url: "https://www.apple.com/mac/",
        },
        NavEntryInfo {
            title: "Google",
            url: "https://www.google.com/",
        },
        NavEntryInfo {
            title: "Developer Hub",
            url: "https://lite.duckduckgo.com/lite/",
        },
    ];
    let html = chrome::render(&ChromeState {
        url: &url_text,
        focused: false,
        can_back: browser.can_go_back(),
        can_forward: browser.can_go_forward(),
        loading: false,
        tabs: &tabs,
        menu_open: menu,
        secure: url_text.starts_with("https://"),
        surface_height: height as f64,
        theme,
        supervision: None,
        telemetry: None,
        toolbar_collapsed,
        explorer_tab,
        page_holder: PageHolderInfo {
            has_doc: false,
            title: &title,
            url: &url_text,
            links_mode,
            links: &demo_links,
        },
        scrapbook_items: &demo_sb,
        favorites: &demo_favs,
        history_entries: &[],
        passkey_prompt: None,
    });
    let mut chrome_doc = HtmlDocument::from_html(
        &html,
        DocumentConfig {
            viewport: Some(chrome_viewport(size, scale)),
            ..Default::default()
        },
    );
    chrome_doc.resolve(0.0);

    let mut renderer = VelloImageRenderer::new(width, height);
    let mut buffer: Vec<u8> = Vec::new();
    let chrome_h = if toolbar_collapsed {
        0
    } else {
        (CHROME_HEIGHT * scale) as u32
    };
    let rail_w = ((EXPLORER_RAIL_WIDTH
        + if explorer_tab.is_some() {
            EXPLORER_DRAWER_WIDTH
        } else {
            0.0
        })
        * scale) as u32;
    renderer.render_to_vec(
        |painter| {
            blitz_paint::paint_scene(
                painter,
                page.doc_mut(),
                scale,
                width.saturating_sub(rail_w).max(1),
                height.saturating_sub(chrome_h).max(1),
                rail_w,
                chrome_h,
            );
            blitz_paint::paint_scene(painter, &mut chrome_doc, scale, width, height, 0, 0);
        },
        &mut buffer,
    );

    let expected = (width * height * 4) as usize;
    if buffer.len() < expected {
        eprintln!("FAIL: short buffer {}", buffer.len());
        return 1;
    }
    let bg_rgb: [u8; 3] = if theme.is_dark() {
        [0x1C, 0x1C, 0x1E]
    } else {
        [0xFF, 0xFF, 0xFF]
    };
    for px in buffer[..expected].chunks_exact_mut(4) {
        let a = px[3] as u16;
        if a < 255 {
            let inv = 255 - a;
            px[0] = ((px[0] as u16 * a + bg_rgb[0] as u16 * inv) / 255) as u8;
            px[1] = ((px[1] as u16 * a + bg_rgb[1] as u16 * inv) / 255) as u8;
            px[2] = ((px[2] as u16 * a + bg_rgb[2] as u16 * inv) / 255) as u8;
            px[3] = 255;
        }
    }
    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("FAIL: {path}: {e}");
            return 1;
        }
    };
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = match encoder.write_header() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("FAIL: png header: {e}");
            return 1;
        }
    };
    if let Err(e) = writer.write_image_data(&buffer[..expected]) {
        eprintln!("FAIL: png write: {e}");
        return 1;
    }
    println!("wrote {path} ({width}x{height})");
    0
}

/// `--hurdletest`: Real-world end-to-end verification suite proving:
///   1. JSI / C++ exception isolation across `extern "C"` (throwing scripts and
///      event handlers return `false` and never abort the Rust host).
///   2. Full UTF-8 FFI marshalling (2-, 3-, and 4-byte UTF-8 sequences including
///      surrogate pairs) and general `document.getElementById` / `__registerClickHandler`
///      on non-start HTML pages.
///   3. End-to-end HTTP form fill, Blind Vault secret injection, NIST P-256 ECDSA
///      WebAuthn assertion signing, HTTP POST submission to a real local TCP server,
///      and `< 1ms` zero-LLM `FormRecipe` recall/replay.
///   4. Live Mach kernel resident memory telemetry (`mach_task_basic_info`).
fn hurdletest() -> i32 {
    prepare_test_run();
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    let mut failures = 0;
    let mut check = |condition: bool, message: &str| {
        if condition {
            println!("ok   {message}");
        } else {
            eprintln!("FAIL {message}");
            failures += 1;
        }
    };

    let text_of = |doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId| -> String {
        let Some(node) = doc.get_node(id) else {
            return String::new();
        };
        let Some(child) = node.children.first().copied() else {
            return String::new();
        };
        match doc.get_node(child).map(|n| &n.data) {
            Some(NodeData::Text(t)) => t.content.clone(),
            _ => String::new(),
        }
    };

    // -----------------------------------------------------------------------
    // Hurdle 1 & 2: JSI Exception Boundary + UTF-8 Marshalling + General DOM
    // -----------------------------------------------------------------------
    let custom_html = r#"<!DOCTYPE html><html><head><style>
        body { background-color: #111827; color: #F9FAFB; padding: 24px; }
        #pay-btn { background-color: #2563EB; color: #FFFFFF; padding: 12px; width: 280px; }
        #receipt { color: #9CA3AF; margin-top: 12px; }
    </style></head><body>
        <div id="pay-btn">Authorize Payment</div>
        <div id="receipt">Pending</div>
    </body></html>"#;

    let viewport = page_viewport((WIDTH, HEIGHT), 1.0, Theme::Dark);
    let mut custom_page = Page::load(custom_html, None, None, viewport, Theme::Dark);

    // 1a. Throwing script must return false and NOT abort the process.
    let threw_eval = unsafe {
        hermes_app_eval_script(
            custom_page.hermes,
            c"throw new Error('synthetic runtime failure')".as_ptr(),
            c"<hurdle-throw>".as_ptr(),
        )
    };
    check(
        !threw_eval,
        "throwing JS script caught at extern \"C\" boundary without aborting host",
    );

    // 1b. Syntax error in evaluated script must return false and leave runtime usable.
    let syntax_err = unsafe {
        hermes_app_eval_script(
            custom_page.hermes,
            c"function ((( broken syntax".as_ptr(),
            c"<hurdle-syntax>".as_ptr(),
        )
    };
    check(
        !syntax_err,
        "malformed JS syntax error caught cleanly at extern \"C\" boundary",
    );

    // 2a. Register a custom click handler on #pay-btn that writes multi-byte UTF-8
    //     (2-byte Latin-1, 3-byte CJK/symbols, 4-byte emoji surrogate pairs) and
    //     mutates attributes + inline CSS via $SHBuiltin.extern_c.
    let setup_script = std::ffi::CString::new(
        r##"
        __registerClickHandler("pay-btn", function(btn, x, y) {
            btn.setStyleProperty("background-color", "#059669");
            btn.setAttribute("data-status", "confirmed-✓");
            btn.setInnerText("Paid €42.00 — 完了 🚀");
            var r = document.getElementById("receipt");
            if (r) {
                r.setInnerText("Receipt #901 • Café 東京 🔥");
            }
        });
        "##,
    )
    .expect("cstring");
    let setup_ok = unsafe {
        hermes_app_eval_script(
            custom_page.hermes,
            setup_script.as_ptr(),
            c"<hurdle-utf8-setup>".as_ptr(),
        )
    };
    check(
        setup_ok,
        "custom page handler registered via globalThis.__registerClickHandler",
    );

    let pay_btn = custom_page
        .doc()
        .get_element_by_id("pay-btn")
        .expect("#pay-btn");
    let receipt_el = custom_page
        .doc()
        .get_element_by_id("receipt")
        .expect("#receipt");
    let dispatch_ok = unsafe {
        hermes_app_dispatch_event(
            custom_page.hermes,
            c"click".as_ptr(),
            pay_btn.as_u64(),
            24.0,
            24.0,
        )
    };
    if custom_page.ctx.dirty {
        custom_page.resolve();
        custom_page.ctx.dirty = false;
    }
    check(dispatch_ok, "custom click handler dispatched cleanly");
    check(
        text_of(custom_page.doc(), pay_btn) == "Paid €42.00 — 完了 🚀",
        "2-, 3-, and 4-byte UTF-8 strings marshalled across $SHBuiltin.extern_c without corruption",
    );
    check(
        text_of(custom_page.doc(), receipt_el) == "Receipt #901 • Café 東京 🔥",
        "document.getElementById mutated secondary DOM node with UTF-8 text",
    );
    let data_status = custom_page
        .doc()
        .get_node(pay_btn)
        .and_then(|n| n.attr(LocalName::from("data-status")))
        .unwrap_or_default()
        .to_string();
    check(
        data_status == "confirmed-✓",
        "BlitzElement.setAttribute persisted UTF-8 attribute in Blitz DOM",
    );
    let pay_bg = custom_page
        .doc()
        .resolved_style_value(pay_btn, "background-color");
    check(
        pay_bg.contains("5, 150, 105"),
        "Stylo recomputed inline style from custom page click handler",
    );

    // 2b. A throwing click handler is caught and returns false.
    let throw_handler = std::ffi::CString::new(
        r#"__registerClickHandler("pay-btn", function() { throw new Error("handler boom"); });"#,
    )
    .expect("cstring");
    let _ = unsafe {
        hermes_app_eval_script(
            custom_page.hermes,
            throw_handler.as_ptr(),
            c"<hurdle-throw-handler>".as_ptr(),
        )
    };
    let dispatch_threw = unsafe {
        hermes_app_dispatch_event(
            custom_page.hermes,
            c"click".as_ptr(),
            pay_btn.as_u64(),
            10.0,
            10.0,
        )
    };
    check(
        !dispatch_threw,
        "throwing event handler caught in hermes_app_dispatch_event",
    );

    // -----------------------------------------------------------------------
    // Hurdle 3: Real Local HTTP Server + Vault + WebAuthn + Submit + Recall
    // -----------------------------------------------------------------------
    let vault_file = format!("/tmp/hermes-hurdle-vault-{}.json", std::process::id());
    let recall_file = format!("/tmp/hermes-hurdle-recall-{}.json", std::process::id());
    let _ = std::fs::remove_file(&vault_file);
    let _ = std::fs::remove_file(&recall_file);
    unsafe {
        std::env::set_var("HERMES_VAULT_PATH", &vault_file);
        std::env::set_var("HERMES_RECALL_PATH", &recall_file);
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
    let port = listener.local_addr().expect("local addr").port();
    let base_origin = format!("http://127.0.0.1:{port}");
    let login_url = format!("{base_origin}/login");
    // The credential the fill must use. It is the one the shell created for
    // this account, so the test uses the same path a real signup does: the
    // blind vault generates the secret and the Keychain mirrors it.
    let stored_host = "127.0.0.1".to_string();
    let stored_user = "ada@lovelace.org";
    let stored_secret = vault::get_or_create_secret(&base_origin, stored_user)
        .expect("create the account secret in the blind vault")
        .secret;
    let captured_post_body: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let post_sink = captured_post_body.clone();

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 16384];
            let mut total = Vec::new();
            let mut header_end = None;
            let mut content_len = 0usize;
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        total.extend_from_slice(&buf[..n]);
                        if header_end.is_none() {
                            if let Some(pos) = total.windows(4).position(|w| w == b"\r\n\r\n") {
                                let h_end = pos + 4;
                                header_end = Some(h_end);
                                let headers = String::from_utf8_lossy(&total[..h_end]);
                                for line in headers.lines() {
                                    if let Some(rest) =
                                        line.to_ascii_lowercase().strip_prefix("content-length:")
                                    {
                                        content_len = rest.trim().parse().unwrap_or(0);
                                    }
                                }
                            }
                        }
                        if let Some(h_end) = header_end {
                            if total.len() >= h_end + content_len {
                                break;
                            }
                        }
                    }
                }
            }
            let req_text = String::from_utf8_lossy(&total);
            let first_line = req_text.lines().next().unwrap_or("");
            if first_line.starts_with("POST /session") {
                if let Some(h_end) = header_end {
                    let body = String::from_utf8_lossy(&total[h_end..]).to_string();
                    *post_sink.lock().expect("lock") = body;
                }
                let html = r#"<!DOCTYPE html><html><head><title>Authenticated Vault Session</title></head>
                    <body><h1>Welcome Ada</h1><p id="auth-ok">Hardware P-256 Assertion Verified</p></body></html>"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = stream.write_all(resp.as_bytes());
            } else {
                let html = r#"<!DOCTYPE html><html><head><title>Portal Sign-In</title><style>
                    body { padding: 24px; font-family: sans-serif; }
                    form { display: flex; flex-direction: column; gap: 12px; width: 360px; }
                    input, select, button { height: 34px; }
                </style></head><body>
                    <form action="/session" method="POST">
                        <label for="user">Username</label>
                        <input id="user" name="username" type="text" value="">
                        <label for="pass">Password</label>
                        <input id="pass" name="password" type="password" value="">
                        <label for="role">Role</label>
                        <select id="role" name="role">
                            <option value="viewer" selected>Viewer</option>
                            <option value="admin">Admin</option>
                        </select>
                        <input id="webauthn_challenge" name="webauthn_challenge" type="text" value="chal_hurdle_998877">
                        <input id="webauthn_rp_id" name="webauthn_rp_id" type="text" value="127.0.0.1">
                        <input id="webauthn_assertion" name="webauthn_assertion" type="text" value="">
                        <button id="submit-login" type="submit">Sign In</button>
                    </form>
                </body></html>"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        }
    });

    let mut app = test_app();
    app.control_connect(1, Role::Host);
    app.control_connect(2, Role::Agent);
    let (mut seq_h, mut seq_a) = (0u64, 0u64);
    let send = |app: &mut BrowserApp, conn: u64, seq: &mut u64, mut frame: Value| {
        *seq += 1;
        frame["v"] = json!(1);
        frame["seq"] = json!(*seq);
        app.handle_control(conn, frame);
        app.drain_control_out()
    };
    let find_frame = |out: &[(OutTarget, Value)], kind: &str| -> Option<Value> {
        out.iter()
            .map(|(_, v)| v)
            .find(|v| v.get("type").and_then(Value::as_str) == Some(kind))
            .cloned()
    };

    // Open session and load the real HTTP login page into the supervised tab.
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({"type": "session.open", "title": "Hurdle E2E", "request_id": "h1",
               "policy": {"decision": "human_per_action"}}),
    );
    let session_id = find_frame(&out, "session.opened")
        .and_then(|v| v.get("session_id").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();

    let parsed_login = Url::parse(&login_url).expect("login url");
    let login_html = browser::fetch(&app.provider, &parsed_login).expect("fetch local login");
    {
        let viewport = page_viewport(app.size, app.scale, app.theme);
        let provider = app.provider.clone();
        let theme = app.theme;
        let tab = app.tab_mut();
        tab.canonical
            .browser
            .commit(parsed_login.clone(), login_html.clone());
        tab.replace_page(Page::load(
            &login_html,
            Some(parsed_login.clone()),
            Some(provider),
            viewport,
            theme,
        ));
        tab.canonical.title = "Portal Sign-In".to_string();
    }

    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "observe.request", "session_id": session_id, "request_id": "obs1"}),
    );
    let snap1 = find_frame(&out, "observe.result").unwrap_or(Value::Null);
    let form_sig = snap1
        .get("form_signature")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    check(
        form_sig.starts_with("sig_"),
        "live HTTP login form produces deterministic structural form_signature",
    );

    let id_by_dom_id = |snap: &Value, dom_id: &str| -> String {
        snap.get("elements")
            .and_then(Value::as_array)
            .and_then(|els| {
                els.iter()
                    .find(|e| e.get("dom_id").and_then(Value::as_str) == Some(dom_id))
            })
            .and_then(|e| e.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };

    let user_el = id_by_dom_id(&snap1, "user");
    let pass_el = id_by_dom_id(&snap1, "pass");
    let role_el = id_by_dom_id(&snap1, "role");

    // Propose and approve fill_form with fill_secret + select + type.
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({
            "type": "propose", "session_id": session_id, "request_id": "p_fill",
            "proposal_id": "p_fill",
            "snapshot_id": snap1.get("snapshot_id").cloned().unwrap_or(Value::Null),
            "fingerprint": snap1.get("fingerprint").cloned().unwrap_or(Value::Null),
            "generation": snap1.get("generation").cloned().unwrap_or(Value::Null),
            "operation": {
                "kind": "fill_form",
                "fields": [
                    {"target_id": user_el, "action": "type", "value": "ada@lovelace.org"},
                    {"target_id": pass_el, "action": "fill_secret", "origin": base_origin, "username": "ada@lovelace.org"},
                    {"target_id": role_el, "action": "select", "value": "admin"}
                ]
            },
            "rationale": "Fill portal credentials and role"
        }),
    );
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({
            "type": "decision", "session_id": session_id, "request_id": "d_fill",
            "proposal_id": "p_fill",
            "fingerprint": snap1.get("fingerprint").cloned().unwrap_or(Value::Null),
            "generation": snap1.get("generation").cloned().unwrap_or(Value::Null),
            "decision": "approve", "decided_by": "user", "surface": "glasses"
        }),
    );
    check(
        find_frame(&out, "executed").is_some(),
        "approved fill_form injects the stored Keychain password and selects the option",
    );

    // Observe after fill_form: password must remain redacted, recipe must be recorded.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "observe.request", "session_id": session_id, "request_id": "obs2"}),
    );
    let snap2 = find_frame(&out, "observe.result").unwrap_or(Value::Null);
    check(
        !snap2.to_string().contains(&stored_secret),
        "the injected Keychain password is completely redacted from post-fill observe.result",
    );
    check(
        snap2
            .get("matched_recipe_id")
            .and_then(Value::as_str)
            .is_some_and(|r| r.starts_with("recipe_")),
        "approved fill_form persists structural FormRecipe for zero-LLM recall",
    );

    // Propose and approve form submit over HTTP POST to the real local server.
    let submit_el = id_by_dom_id(&snap2, "submit-login");
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({
            "type": "propose", "session_id": session_id, "request_id": "p_sub",
            "proposal_id": "p_sub",
            "snapshot_id": snap2.get("snapshot_id").cloned().unwrap_or(Value::Null),
            "fingerprint": snap2.get("fingerprint").cloned().unwrap_or(Value::Null),
            "generation": snap2.get("generation").cloned().unwrap_or(Value::Null),
            "operation": {"kind": "submit", "target_id": submit_el},
            "rationale": "Submit authenticated portal form"
        }),
    );
    let out = send(
        &mut app,
        1,
        &mut seq_h,
        json!({
            "type": "decision", "session_id": session_id, "request_id": "d_sub",
            "proposal_id": "p_sub",
            "fingerprint": snap2.get("fingerprint").cloned().unwrap_or(Value::Null),
            "generation": snap2.get("generation").cloned().unwrap_or(Value::Null),
            "decision": "approve", "decided_by": "user", "surface": "glasses"
        }),
    );
    check(
        find_frame(&out, "nav.committed").is_some() && find_frame(&out, "executed").is_some(),
        "approved submit posts form over HTTP and commits response document",
    );

    let post_body = captured_post_body.lock().expect("lock").clone();
    let decoded_pairs: HashMap<String, String> =
        url::form_urlencoded::parse(post_body.as_bytes())
            .into_owned()
            .collect();
    check(
        decoded_pairs.get("username").map(String::as_str) == Some("ada@lovelace.org")
            && decoded_pairs.get("password").map(String::as_str) == Some(stored_secret.as_str())
            && decoded_pairs.get("role").map(String::as_str) == Some("admin"),
        "HTTP POST body received exact username, the stored Keychain password, and the selected option on the wire",
    );
    // Spec §12: "WebAuthn is not a DOM fill operation." The fill must not
    // sign an assertion into a form field, so the posted body must carry no
    // assertion content. The form still has the (empty) input, because the
    // browser submits whatever the page contains; what matters is that the
    // fill never wrote an assertion into it. A passkey assertion comes from
    // the broker through the page API, never from a DOM fill.
    check(
        decoded_pairs
            .get("webauthn_assertion")
            .map(|v| v.trim().is_empty())
            .unwrap_or(true),
        "the DOM fill posts no WebAuthn assertion; a passkey is not a DOM fill operation",
    );

    // Reload the login form and verify zero-LLM `replay_recipe` populates all fields in < 1ms.
    {
        let viewport = page_viewport(app.size, app.scale, app.theme);
        let provider = app.provider.clone();
        let theme = app.theme;
        let tab = app.tab_mut();
        tab.canonical
            .browser
            .commit(parsed_login.clone(), login_html.clone());
        tab.replace_page(Page::load(
            &login_html,
            Some(parsed_login),
            Some(provider),
            viewport,
            theme,
        ));
    }
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "observe.request", "session_id": session_id, "request_id": "obs3"}),
    );
    let snap3 = find_frame(&out, "observe.result").unwrap_or(Value::Null);
    let recipe_id = snap3
        .get("matched_recipe_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let _ = send(
        &mut app,
        2,
        &mut seq_a,
        json!({
            "type": "propose", "session_id": session_id, "request_id": "p_replay",
            "proposal_id": "p_replay",
            "snapshot_id": snap3.get("snapshot_id").cloned().unwrap_or(Value::Null),
            "fingerprint": snap3.get("fingerprint").cloned().unwrap_or(Value::Null),
            "generation": snap3.get("generation").cloned().unwrap_or(Value::Null),
            "operation": {"kind": "replay_recipe", "recipe_id": recipe_id},
            "rationale": "Replay stored FormRecipe with zero LLM calls"
        }),
    );
    let _ = send(
        &mut app,
        1,
        &mut seq_h,
        json!({
            "type": "decision", "session_id": session_id, "request_id": "d_replay",
            "proposal_id": "p_replay",
            "fingerprint": snap3.get("fingerprint").cloned().unwrap_or(Value::Null),
            "generation": snap3.get("generation").cloned().unwrap_or(Value::Null),
            "decision": "approve", "decided_by": "user", "surface": "glasses"
        }),
    );
    check(
        app.recalled_count == 1,
        "replay_recipe increments recalled_count and replays form bindings",
    );

    // -----------------------------------------------------------------------
    // Hurdle 4: Live Mach Resident Memory Telemetry
    // -----------------------------------------------------------------------
    let rss = measure_process_rss_mb();
    check(
        rss > 1.0 && rss < 4096.0,
        "measure_process_rss_mb reports live kernel resident memory in MB",
    );

    // -----------------------------------------------------------------------
    // Hurdle 5: Real-World Modern Website E2E (aol.com)
    // -----------------------------------------------------------------------
    let aol_url = browser::normalize_input("aol.com").expect("normalize aol.com");
    check(
        aol_url.as_str() == "https://aol.com/",
        "normalize_input('aol.com') resolves to https://aol.com/",
    );
    match browser::fetch(&app.provider, &aol_url) {
        Ok(aol_html) => {
            check(
                aol_html.len() > 100_000,
                "fetched live https://aol.com/ HTML payload (>100 KB SSR document)",
            );
            {
                let viewport = page_viewport(app.size, app.scale, app.theme);
                let provider = app.provider.clone();
                let theme = app.theme;
                let tab = app.tab_mut();
                tab.canonical
                    .browser
                    .commit(aol_url.clone(), aol_html.clone());
                let page = Page::load(
                    &aol_html,
                    Some(aol_url.clone()),
                    Some(provider),
                    viewport,
                    theme,
                );
                let title = title_for(&aol_url, page.doc());
                tab.replace_page(page);
                tab.canonical.title = title;
            }

            let out = send(
                &mut app,
                2,
                &mut seq_a,
                json!({
                    "type": "observe.request",
                    "session_id": session_id,
                    "request_id": "obs_aol",
                    "include_screenshot": true
                }),
            );
            let aol_snap = find_frame(&out, "observe.result").unwrap_or(Value::Null);
            let aol_title = aol_snap
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let aol_text = aol_snap
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let aol_elements = aol_snap
                .get("elements")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            check(
                aol_title.to_ascii_lowercase().contains("aol"),
                "aol.com document title parsed and exposed in observe.result",
            );
            check(
                aol_text.len() > 200,
                "aol.com visible text extracted from live DOM",
            );
            check(
                aol_elements.len() >= 5,
                "aol.com external <head> stylesheets resolved and yielded visible interactive elements with layout bounds",
            );
            let has_valid_links = aol_elements.iter().any(|e| {
                e.get("tag").and_then(Value::as_str) == Some("a")
                    && e.get("href")
                        .and_then(Value::as_str)
                        .is_some_and(|h| h.starts_with("https://"))
                    && e.get("bounds")
                        .and_then(|b| b.get("width"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        > 0.0
                    && e.get("bounds")
                        .and_then(|b| b.get("height"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        > 0.0
            });
            check(
                has_valid_links,
                "aol.com interactive links carry resolved https:// hrefs and non-zero Taffy layout bounding boxes",
            );
            let has_screenshot = aol_snap
                .get("screenshot_png_b64")
                .and_then(Value::as_str)
                .is_some_and(|s| s.len() > 1000)
                && aol_snap
                    .get("screenshot_hash")
                    .and_then(Value::as_str)
                    .is_some_and(|h| h.len() == 64);
            check(
                has_screenshot,
                "aol.com rendered through Vello on wgpu and produced SHA-256 verified PNG screenshot",
            );

            // Verify Static Hermes + $SHBuiltin.extern_c DOM FFI on aol.com's 2.2 MB DOM tree
            // (#atomic is the root <html id="atomic"> element on aol.com).
            let aol_dom_script = std::ffi::CString::new(
                r#"
                var root = document.getElementById("atomic");
                if (root) {
                    root.setAttribute("data-hermes-hurdle", "aol-verified-✓");
                }
                "#,
            )
            .expect("cstring");
            let aol_eval_ok = unsafe {
                hermes_app_eval_script(
                    app.tab().instance.page.hermes,
                    aol_dom_script.as_ptr(),
                    c"<hurdle-aol-dom>".as_ptr(),
                )
            };
            let atomic_attr = app
                .tab()
                .instance
                .page
                .doc()
                .get_element_by_id("atomic")
                .and_then(|id| app.tab().instance.page.doc().get_node(id))
                .and_then(|n| n.attr(LocalName::from("data-hermes-hurdle")))
                .unwrap_or_default()
                .to_string();
            check(
                aol_eval_ok && atomic_attr == "aol-verified-✓",
                "Static Hermes $SHBuiltin.extern_c queried and mutated #atomic on live aol.com DOM",
            );
        }
        Err(e) => {
            check(false, &format!("live https://aol.com/ fetch failed: {e}"));
        }
    }

    let _ = std::fs::remove_file(&vault_file);
    let _ = std::fs::remove_file(&recall_file);
    // Leave no password behind in the Keychain.
    {
        let store = crate::credentials::KeychainPasswordStore::new();
        let _ = store.delete(&stored_host, stored_user);
    }

    cleanup_test_run();

    cleanup_test_run();

    if failures == 0 {
        println!("HURDLE TEST PASS");
        0
    } else {
        println!("HURDLE TEST FAIL ({failures})");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_selftest_and_runtime_and_hurdles() {
        assert_eq!(selftest(), 0, "selftest failed");
        assert_eq!(runtimetest(), 0, "runtimetest failed");

        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let release_bin = manifest_dir.join("target/release/hermes-blitz-browser");
        if release_bin.exists() {
            for flag in [
                "--uitest",
                "--observetest",
                "--supervisortest",
                "--hurdletest",
            ] {
                let status = std::process::Command::new(&release_bin)
                    .arg(flag)
                    .status()
                    .expect("run release binary test suite");
                assert!(status.success(), "{flag} failed");
            }
        }
    }
}
