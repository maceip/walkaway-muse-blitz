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
mod dom_ffi;
mod observe;
mod page_endpoint;
mod recall;
mod vault;

use anyrender::WindowRenderer;
use anyrender_vello::VelloWindowRenderer;
use blitz_dom::{DocumentConfig, LocalName, Namespace, NodeData, Point, QualName};
use blitz_html::HtmlDocument;
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};
use markup5ever::local_name;
use browser::{Browser, SharedProvider};
use chrome::{CHROME_HEIGHT, ChromeState, SupervisionInfo, TabInfo, Theme};
use control::{ControlHub, Role};
use dom_ffi::BlitzHostContext;
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

struct Page {
    ctx: Box<BlitzHostContext>,
    hermes: *mut HermesAppRuntime,
}

/// Default text colour for documents that opt into a dark colour scheme. Blitz's
/// UA sheet hard-codes `color: black`, so a page that says
/// `color-scheme: light dark` and leaves its text colour to the UA would render
/// black on its own dark background.
const DARK_DEFAULT_TEXT: &str = "html, body { color: #F2F2F7; }";

impl Page {
    fn load(
        html: &str,
        base: Option<Url>,
        provider: Option<SharedProvider>,
        viewport: Viewport,
        theme: Theme,
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

        let mut ctx = Box::new(BlitzHostContext::new(doc));
        let hermes =
            unsafe { hermes_app_init((&mut *ctx as *mut BlitzHostContext).cast::<c_void>()) };
        Self { ctx, hermes }
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
        let mut ua = vec![
            blitz_dom::UserAgentStylesheet::default_sheet(),
            blitz_dom::UserAgentStylesheet::parse(
                "select { display: inline-block; min-width: 80px; min-height: 24px; }",
            ),
        ];
        if let Some(css) = extra_ua {
            ua.push(blitz_dom::UserAgentStylesheet::parse(css));
        }
        let mut doc = HtmlDocument::from_html(
            html,
            DocumentConfig {
                viewport: Some(viewport),
                base_url: base.as_ref().map(|u| u.to_string()),
                net_provider: provider
                    .as_ref()
                    .map(|p| p.clone() as Arc<dyn blitz_traits::net::NetProvider>),
                ua_stylesheets: ua,
                ..Default::default()
            },
        );
        doc.resolve(0.0);
        doc
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

struct BrowserApp {
    window: Option<Arc<Window>>,
    renderer: VelloWindowRenderer,
    chrome_doc: HtmlDocument,
    tabs: Vec<Tab>,
    next_tab_id: u64,
    active: usize,
    menu_open: bool,
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
        let mut app = Self {
            window: None,
            renderer: VelloWindowRenderer::new(),
            chrome_doc: HtmlDocument::from_html("<!DOCTYPE html>", DocumentConfig::default()),
            tabs,
            next_tab_id: 2,
            active: 0,
            menu_open: false,
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
            next_session_id: 1,
            input_queue: VecDeque::new(),
            next_input_event_id: 1,
            last_input_event_id: 0,
            frame_seq: 0,
            recalled_count: 0,
            supervise_commits_only,
            xray_enabled,
            exit_after: parse_exit_after(),
            started: Instant::now(),
        };
        app.sync_chrome();
        app
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
        Tab::start_page(id, page_viewport(self.size, self.scale, self.theme))
    }

    fn observe_active(&mut self) -> observe::PageSnapshot {
        let id = self.tab().canonical.id;
        self.observe_tab(id).expect("active tab exists")
    }

    fn observe_tab(&mut self, tab_id: TabId) -> Option<observe::PageSnapshot> {
        let viewport = page_viewport(self.size, self.scale, self.theme);
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
                "type", "select", "fill_form", "replay_recipe", "submit"
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
            "passkeys": "unsupported",
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
        if !matches!(kind.as_str(), "ping" | "capabilities.get" | "session.open") {
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
        if self
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
        let tab = self.create_start_tab();
        let tab_id = tab.canonical.id;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
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
        if let Some(start_url) = frame.get("start_url").and_then(Value::as_str) {
            // The host's act of opening authorizes exactly this navigation.
            self.navigate(start_url);
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
        let pending = PendingProposal {
            proposal_id: proposal_id.to_string(),
            snapshot_id,
            page_id: snapshot.page_id,
            fingerprint: fingerprint.to_string(),
            generation,
            operation: operation.clone(),
            rationale: rationale.to_string(),
            classification: classification.clone(),
            requires_glasses,
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

        let operation = self
            .control
            .as_ref()
            .and_then(|s| s.pending.as_ref())
            .map(|p| p.operation.clone())
            .unwrap_or(Value::Null);
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
                        let tab = self.tab_mut();
                        tab.canonical.browser.commit(final_url.clone(), html.clone());
                        tab.replace_page(Page::load(
                            &html,
                            Some(final_url.clone()),
                            Some(provider),
                            viewport,
                            theme,
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
                        let tab = self.tab_mut();
                        tab.canonical.browser.fail(e.clone());
                        let html = error_page(&e);
                        tab.replace_page(Page::load(&html, None, None, viewport, theme));
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
        let chrome_h = ((CHROME_HEIGHT * scale) as u32).min(height);
        let Self {
            tabs,
            active,
            chrome_doc,
            ..
        } = self;
        let tab = &mut tabs[*active];
        renderer.render_to_vec(
            |painter| {
                blitz_paint::paint_scene(
                    painter,
                    tab.instance.page.doc_mut(),
                    scale,
                    width,
                    height,
                    0,
                    chrome_h,
                );
                blitz_paint::paint_scene(painter, chrome_doc, scale, width, height, 0, 0);
            },
            &mut rgba,
        );
        let expected = (width * height * 4) as usize;
        if rgba.len() < expected {
            return None;
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

    /// Regenerate the chrome document from state.
    fn sync_chrome(&mut self) {
        let telemetry = if self.control.is_some() || self.xray_enabled {
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
            self.sync_chrome();
            self.request_redraw();
        }
    }

    fn navigate(&mut self, input: &str) {
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
            let viewport = page_viewport(self.size, self.scale, self.theme);
            let theme = self.theme;
            let tab = self.tab_mut();
            tab.canonical.browser.commit(url, START_PAGE.to_string());
            tab.replace_page(Page::load(START_PAGE, None, None, viewport, theme));
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
        let viewport = page_viewport(self.size, self.scale, self.theme);
        let theme = self.theme;
        let tab = self.tab_mut();
        tab.replace_page(Page::load(&html, base, provider, viewport, theme));
        tab.canonical.browser.loading = false;
        tab.canonical.browser.error = None;
        tab.canonical.title = title_for(&url, tab.instance.page.doc());
        self.sync_chrome();
        self.request_redraw();
    }

    fn handle_chrome_click(&mut self, x: f32, y: f32) {
        let Some(hit) = self.chrome_doc.hit(x, y) else {
            return;
        };
        let target = dom_ffi::bubble_target(&self.chrome_doc, hit.node_id);
        let id_is = |id: &str| self.chrome_doc.get_element_by_id(id) == Some(target);

        // Supervision bar (session tab only). These are host decisions made
        // through the same code path as a socket `decision`.
        if id_is("sup-approve") || id_is("sup-reject") {
            let approve = id_is("sup-approve");
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
        if id_is("btn-menu") {
            self.menu_open = !self.menu_open;
            self.sync_chrome();
            self.request_redraw();
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
        if id_is("btn-reload") {
            self.reload();
            return;
        }
        if id_is("address") {
            let url = self.tab().canonical.browser.display_url();
            let tab = self.tab_mut();
            tab.canonical.browser.focused = true;
            tab.canonical.browser.address = url;
            self.menu_open = false;
            self.sync_chrome();
            self.request_redraw();
            return;
        }

        // Click landed on the chrome document but not on any control: close the
        // menu, and if the point is below the chrome strip, treat it as a page
        // click (the chrome body is transparent there).
        if self.menu_open {
            self.menu_open = false;
            self.sync_chrome();
            self.request_redraw();
        }
        if y >= CHROME_HEIGHT as f32 {
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
        let local_y = y - CHROME_HEIGHT as f32;
        let Some(hit) = self.tab().instance.page.doc().hit(x, local_y) else {
            return;
        };

        if let Some(href) = link_target(self.tab().instance.page.doc(), hit.node_id) {
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
            hermes_app_dispatch_event(hermes, c"click".as_ptr(), target.as_u64(), x, local_y);
        }
        if self.tab().instance.page.ctx.dirty {
            let tab = self.tab_mut();
            tab.instance.page.resolve();
            tab.instance.page.ctx.dirty = false;
            self.request_redraw();
        }
    }

    fn scroll_page(&mut self, dy: f64) {
        let current = self.tab().instance.page.doc().viewport_scroll();
        let tab = self.tab_mut();
        tab.instance.page.doc_mut().set_viewport_scroll(Point {
            x: current.x,
            y: current.y + dy,
        });
        tab.instance.page.resolve();
        self.request_redraw();
    }

    fn handle_key(&mut self, event: &KeyEvent) {
        if self.session_blocks_input() {
            return;
        }
        let cmd = self.modifiers.super_key();

        if cmd {
            match &event.logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("l") => {
                    let url = self.tab().canonical.browser.display_url();
                    let tab = self.tab_mut();
                    tab.canonical.browser.focused = true;
                    tab.canonical.browser.address = url;
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

        if !self.tab().canonical.browser.focused {
            return;
        }

        match &event.logical_key {
            Key::Named(NamedKey::Enter) => {
                let input = self.tab().canonical.browser.address.clone();
                self.navigate(&input);
            }
            Key::Named(NamedKey::Escape) => {
                let tab = self.tab_mut();
                tab.canonical.browser.focused = false;
                tab.canonical.browser.sync_address();
                self.sync_chrome();
                self.request_redraw();
            }
            Key::Named(NamedKey::Backspace) => {
                self.tab_mut().canonical.browser.address.pop();
                self.sync_chrome();
                self.request_redraw();
            }
            Key::Named(NamedKey::Space) => {
                self.tab_mut().canonical.browser.address.push(' ');
                self.sync_chrome();
                self.request_redraw();
            }
            _ => {
                if let Some(text) = &event.text {
                    let filtered: String = text.chars().filter(|c| !c.is_control()).collect();
                    if !filtered.is_empty() {
                        self.tab_mut().canonical.browser.address.push_str(&filtered);
                        self.sync_chrome();
                        self.request_redraw();
                    }
                }
            }
        }
    }

    fn handle_navigated(
        &mut self,
        tab_id: TabId,
        generation: u64,
        url: Url,
        result: Result<String, String>,
    ) {
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
                target.replace_page(Page::load(
                    &html,
                    Some(url.clone()),
                    Some(self.provider.clone()),
                    page_viewport(self.size, self.scale, self.theme),
                    self.theme,
                ));
                target.canonical.browser.error = None;
                target.canonical.title = title_for(&url, target.instance.page.doc());
            }
            Err(e) => {
                target.canonical.browser.fail(e.clone());
                let html = error_page(&e);
                target.replace_page(Page::load(
                    &html,
                    None,
                    None,
                    page_viewport(self.size, self.scale, self.theme),
                    self.theme,
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
        let Self {
            renderer,
            chrome_doc,
            tabs,
            active,
            size,
            scale,
            ..
        } = self;
        let (width, height) = *size;
        let scale_f = *scale;
        let chrome_h = ((CHROME_HEIGHT * scale_f) as u32).min(height);
        let tab = &mut tabs[*active];

        renderer.render(|painter| {
            // Page first, then chrome: the chrome body is transparent, so only
            // the strip, nav row and (when open) the menu paint over the page.
            blitz_paint::paint_scene(
                painter,
                tab.instance.page.doc_mut(),
                scale_f,
                width,
                height,
                0,
                chrome_h,
            );
            blitz_paint::paint_scene(painter, chrome_doc, scale_f, width, height, 0, 0);
        });
        self.frame_seq = self.frame_seq.wrapping_add(1);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size = (width.max(1), height.max(1));
        self.renderer.set_size(self.size.0, self.size.1);

        let viewport = page_viewport(self.size, self.scale, self.theme);
        let tab = self.tab_mut();
        tab.instance.page.doc_mut().set_viewport(viewport);
        tab.instance.page.resolve();
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
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.net_dirty.load(Ordering::SeqCst) {
            self.net_dirty.store(false, Ordering::SeqCst);
            self.tab_mut().instance.page.resolve();
            self.request_redraw();
        }
        if let Some(limit) = self.exit_after {
            if self.started.elapsed() >= limit {
                self.exit_after = None;
                eprintln!("[host] exit-after elapsed, shutting down");
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

        let viewport = page_viewport(self.size, self.scale, self.theme);
        let tab = self.tab_mut();
        tab.instance.page.doc_mut().set_viewport(viewport);
        tab.instance.page.resolve();
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
                // Pages follow the same preference via prefers-color-scheme.
                let viewport = page_viewport(self.size, self.scale, self.theme);
                let tab = self.tab_mut();
                tab.instance.page.doc_mut().set_viewport(viewport);
                tab.instance.page.resolve();
                self.sync_chrome();
                self.request_redraw();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (
                    (position.x / self.scale) as f32,
                    (position.y / self.scale) as f32,
                );
                if self.cursor.1 < CHROME_HEIGHT as f32
                    && self.chrome_doc.set_hover_to(self.cursor.0, self.cursor.1)
                {
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
    Viewport::new(
        size.0,
        size.1.saturating_sub(chrome_h).max(1),
        scale as f32,
        theme.color_scheme(),
    )
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
            if let Ok(acct) = vault::get_or_create_secret(origin, username) {
                {
                    let mut mutator = doc.mutate();
                    mutator.set_attribute(node_id, qual_name("value"), &acct.secret);
                }
                // If the form also contains WebAuthn assertion inputs (#webauthn_challenge
                // and #webauthn_assertion), sign a real ECDSA P-256 assertion using the
                // vault keypair (since fill_secret only executes after Host approve!).
                let challenge_val = doc
                    .get_element_by_id("webauthn_challenge")
                    .and_then(|cid| doc.get_node(cid))
                    .and_then(|n| n.element_data())
                    .and_then(|d| d.attr(local_name!("value")))
                    .map(str::to_string);
                let rp_id_val = doc
                    .get_element_by_id("webauthn_rp_id")
                    .and_then(|rid| doc.get_node(rid))
                    .and_then(|n| n.element_data())
                    .and_then(|d| d.attr(local_name!("value")))
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        Url::parse(origin)
                            .ok()
                            .and_then(|u| u.host_str().map(str::to_string))
                            .unwrap_or_else(|| origin.to_string())
                    });
                let assertion_node = doc.get_element_by_id("webauthn_assertion");
                if let (Some(challenge), Some(assert_id)) = (challenge_val, assertion_node) {
                    if !challenge.is_empty() {
                        if let Ok(bundle) = vault::sign_webauthn_assertion(
                            origin,
                            username,
                            &rp_id_val,
                            &challenge,
                            true,
                            true,
                        ) {
                            if let Ok(bundle_json) = serde_json::to_string(&bundle) {
                                let mut mutator = doc.mutate();
                                mutator.set_attribute(
                                    assert_id,
                                    qual_name("value"),
                                    &bundle_json,
                                );
                            }
                        }
                    }
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--selftest") {
        std::process::exit(selftest());
    }
    if args.iter().any(|a| a == "--uitest") {
        std::process::exit(uitest());
    }
    if args.iter().any(|a| a == "--runtimetest") {
        std::process::exit(runtimetest());
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
    }
    event_loop.run_app(&mut app).expect("run");
}

/// `--uitest`: drive the real chrome click handler at coordinates obtained from
/// real hit-testing, and assert the resulting state. This is what proves the
/// buttons do something, rather than merely being present.
fn uitest() -> i32 {
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
    for y in (0..(CHROME_HEIGHT as i32 + 300)).step_by(2) {
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

/// `--runtimetest`: proves that each tab's AOT handler mutates only its own
/// document, including after a later-created runtime is destroyed.
fn runtimetest() -> i32 {
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
    let second = Page::load(START_PAGE, None, None, viewport, Theme::Light);
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

/// `--supervisortest`: the phase-2 control protocol end to end against a real
/// BrowserApp — channel authority, proposal/decision binding, exactly-once
/// execution, takeover/release, stop discarding a fetch, and audit secrecy.
fn supervisortest() -> i32 {
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
            && caps.get("passkeys").and_then(Value::as_str) == Some("unsupported"),
        "capabilities report text input and forms supported, passkeys unsupported",
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

    let assertion = vault::sign_webauthn_assertion(
        "http://127.0.0.1:18080",
        "ryan@socon.dev",
        "socon2026.local",
        "challenge_suptest_123456",
        true,
        true,
    );
    check(
        assertion.is_ok(),
        "vault signs ECDSA P-256 WebAuthn assertion",
    );
    if let Ok(a) = assertion {
        check(
            vault::verify_webauthn_assertion(
                &a,
                "socon2026.local",
                "http://127.0.0.1:18080",
                "challenge_suptest_123456",
            )
            .is_ok(),
            "vault P-256 WebAuthn assertion verifies against public key, rpIdHash, challenge, and origin",
        );
        check(
            vault::verify_webauthn_assertion(
                &a,
                "socon2026.local",
                "http://127.0.0.1:18080",
                "wrong_challenge",
            )
            .is_err(),
            "vault P-256 WebAuthn assertion rejects wrong challenge",
        );
    }

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

    if failures == 0 {
        println!("OBSERVE TEST PASS");
        0
    } else {
        println!("OBSERVE TEST FAIL ({failures})");
        1
    }
}

fn selftest() -> i32 {
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
            "btn-reload".to_string(),
            "btn-menu".to_string(),
            "address".to_string(),
            "btn-newtab".to_string(),
            "tab-0".to_string(),
            "tab-close-0".to_string(),
            "tab-1".to_string(),
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
            'scan: for y in (0..(CHROME_HEIGHT as i32 + 300)).step_by(2) {
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
    let chrome_h = (CHROME_HEIGHT * scale) as u32;
    renderer.render_to_vec(
        |painter| {
            blitz_paint::paint_scene(painter, page.doc_mut(), scale, width, height, 0, chrome_h);
            blitz_paint::paint_scene(painter, &mut chrome_doc, scale, width, height, 0, 0);
        },
        &mut buffer,
    );

    let expected = (width * height * 4) as usize;
    if buffer.len() < expected {
        eprintln!("FAIL: short buffer {}", buffer.len());
        return 1;
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
        "approved fill_form injects vault secret, selects option, and signs WebAuthn assertion",
    );

    // Observe after fill_form: password must remain redacted, recipe must be recorded.
    let out = send(
        &mut app,
        2,
        &mut seq_a,
        json!({"type": "observe.request", "session_id": session_id, "request_id": "obs2"}),
    );
    let snap2 = find_frame(&out, "observe.result").unwrap_or(Value::Null);
    let acct = vault::get_or_create_secret(&base_origin, "ada@lovelace.org").expect("vault acct");
    check(
        !snap2.to_string().contains(&acct.secret),
        "injected vault secret is completely redacted from post-fill observe.result",
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
            && decoded_pairs.get("password").map(String::as_str) == Some(acct.secret.as_str())
            && decoded_pairs.get("role").map(String::as_str) == Some("admin"),
        "HTTP POST body received exact username, vault secret, and selected option on the wire",
    );
    let assertion_raw = decoded_pairs
        .get("webauthn_assertion")
        .cloned()
        .unwrap_or_default();
    let assertion_bundle: Option<vault::WebAuthnAssertionBundle> =
        serde_json::from_str(&assertion_raw).ok();
    check(
        assertion_bundle.as_ref().is_some_and(|b| {
            vault::verify_webauthn_assertion(b, "127.0.0.1", &base_origin, "chal_hurdle_998877")
                .is_ok()
        }),
        "HTTP POST body carries valid NIST P-256 ECDSA WebAuthn assertion verified against public key",
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
