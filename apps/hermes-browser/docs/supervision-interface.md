# Hermes Browser — Supervision, Takeover, and Agent Interface

Status: proposed contract for implementation
Applies to: `hermes-blitz-browser` on macOS first
Primary consumer: a Muse-like shell that supervises browser work
Secondary consumer: an automated agent that needs safe browser actions

## Relationship to the existing handoff spec

`walkaway-hackathon/docs/HANDOFF_SPEC.md` is normative for the external
Muse-shell ⇄ browser message contract: typed proposals, host-recorded
decisions, fingerprint binding, control states, and which surface owns each
control. This document is the implementation companion inside
`hermes-blitz-browser`: engine seams, input ownership, frame/snapshot APIs,
supervision rendering, brokers, capabilities, and tests.

Where wording appears to conflict, the handoff spec wins on externally
visible authority. In particular:

- an agent may observe and propose; the agent may not execute an `act`
  operation merely because the agent owns the task;
- execution requires a host-recorded, single-use decision bound to the exact
  proposal and page fingerprint;
- host-only decision/control/stop messages are unavailable on the agent
  channel, or, in process, absent from the agent's typed handle;
- taking control invalidates pending proposals and stops new observation
  results from reaching the agent until control is released.

## 1. Decision

Build the supervision layer as a **separate trusted shell boundary**, not as
page JavaScript and not as ordinary browser chrome.

The browser owns documents, tabs, navigation, rendering, and raw input.
The supervisor owns task lifecycle, input ownership, stop/takeover state,
human-attention prompts, credential prompts, and the audit record.
The agent owns planning. The agent never receives raw credentials and never
writes to the page while the user owns input.

The supervision bar shown in Muse screenshots is therefore part of the shell
contract. In this codebase it may be rendered in the same macOS window, but it
must render from supervisor state and route input before browser chrome or the
page. Do not implement the bar as buttons in `START_PAGE` or as another page
script.

### Target experience

1. Muse starts a task and the browser begins working.
2. The shell shows the task, host, frame, **Stop**, and **Take control**.
3. If the browser reaches login, 2-step verification, passkey, payment,
   consent, or an ambiguous irreversible action, work pauses as
   **Action required**.
4. **Take control** atomically transfers input ownership to the user. The
   header becomes **You're in control** and exposes **Done** plus **Stop**.
5. **Done** releases input only. The shell chooses resume-agent or
   keep-paused; task completion is a separate explicit transition.
6. **Stop** wins every race, revokes agent ownership, cancels queued input,
   and makes every already-issued action token unusable.

## 2. Why the current structure needs this boundary

Current seams in this repository:

- `BrowserApp` owns the window, tabs, chrome document, page documents,
  navigation threads, and input routing in `src/main.rs`.
- `Tab` contains `Browser` history plus one `Page`.
- `Page` owns one Blitz document and one Hermes runtime.
- `UserEvent::Navigated` carries a tab **index**. A tab can close while its
  fetch is in flight, so supervision work must first introduce stable tab IDs.
- Page clicks currently navigate links or dispatch a click into Hermes. There
  is no page text-input model, agent command queue, input lease, or supervisor
  state.
- Remote page `<script>` is not executed. This remains a declared capability
  limit; the supervision contract must expose the limit rather than hide it.

## 3. Terms

| Term | Meaning |
|---|---|
| Browser engine | Blitz/Hermes host that owns sessions, tabs, documents, network, paint, and raw input |
| Supervisor | Trusted shell service that owns task and input-ownership state |
| Agent | Planner that submits typed browser actions through the supervisor |
| Shell UI | Native supervision bar, task cards, prompts, and frame presentation |
| Page runtime | AOT TypeScript/Hermes code belonging to a trusted local page bundle |
| Takeover | Transfer of the single input lease from agent to user |
| Release | The user's `Done` action returning or ending that lease |
| Handoff | User-visible transfer pattern: in-place takeover or opening externally |
| Attention | A paused state requiring a human-only or human-judgment step |

## 4. Non-negotiable invariants

1. **One input writer.** Exactly one of `Agent`, `User`, or `SystemModal`
   owns page input at any instant.
2. **Stop is absolute.** `StopTask` is idempotent, preempts queued work, and
   increments the control epoch. Commands issued under an older epoch fail.
3. **Ownership is checked at execution time.** Checking only when a command is
   queued is insufficient; recheck immediately before dispatch.
4. **Snapshots expire.** A page action must name a snapshot and document
   generation. Navigation or document replacement makes the action stale.
5. **No silent credential path.** Agent-visible snapshots, logs, events, and
   errors never contain password, OTP, passkey, payment, cookie, or token
   values.
6. **No fake continuity.** Opening the current URL in another browser does
   not transfer cookies, storage, form state, or page memory. The receipt
   must say `session_transferred: false`.
7. **Unsupported means explicit.** Remote script, passkeys, forms, uploads,
   downloads, and other partial capabilities are reported through
   `ServiceCapabilities`; absence is never converted into apparent success.
8. **Frames are receipts, not truth by themselves.** Every completed agent
   action returns the post-action URL/title plus a new snapshot or frame ID.
9. **The page cannot supervise itself.** Page content cannot create trusted
   prompts, grant itself input ownership, read supervisor state, or label a
   task complete.
10. **Every executed action is auditable.** The supervisor records who owned
    input, what command ran, against which document generation, and the
    resulting state. Secrets remain outside the audit payload.
11. **No decision, no execution.** An `act` or `sensitive` proposal executes
    only with an unused host decision bound to its proposal ID, target, and
    page fingerprint. Input ownership alone is not authorization.
12. **Human control is private from the agent.** On takeover, pending
    proposals are invalidated and the supervisor stops issuing observations
    to the agent until the user releases control.

## 5. Architecture

```text
┌───────────────┐    typed proposals     ┌──────────────────┐
│ Agent planner │ ───────────────────────▶ │ Supervisor       │
└───────────────┘                          │ - task lifecycle │
                                           │ - input lease    │
┌───────────────┐   bar, prompts, frames   │ - attention      │
│ Shell UI      │ ◀──────────────────────▶ │ - audit          │
└───────────────┘                          └────────┬─────────┘
                                                    │ commands/events
                                           ┌────────▼─────────┐
                                           │ Browser engine   │
                                           │ Blitz documents  │
                                           │ Hermes runtimes  │
                                           │ network + paint  │
                                           └──────────────────┘

Credential broker and passkey broker sit beside the supervisor. They interact
with the user and browser host directly; secret material does not cross the
agent API.
```

### Process placement

Phase 1 is in-process on macOS:

- `BrowserApp` becomes the engine adapter.
- A `Supervisor` value owns task/control state next to `BrowserApp`.
- A third Blitz document renders the supervision UI from a snapshot.
- Input priority is: system modal → supervision UI → browser chrome → page.
- Agent commands enter through the existing winit `EventLoopProxy` pattern.

Keep the types process-neutral. The same messages must be serializable later
for an out-of-process Muse shell without changing semantics.

## 6. Identity and versioning

All IDs are opaque, nonzero, and unique for the life of the browser process.

```rust
pub struct SessionId(pub u64);
pub struct TaskId(pub u64);
pub struct TabId(pub u64);
pub struct CommandId(pub u64);
pub struct SnapshotId(pub u64);
pub struct FrameId(pub u64);
pub struct ControlEpoch(pub u64);
pub struct DocumentGeneration(pub u64);
pub struct AttentionId(pub u64);
pub struct ElementRef(pub u64);
```

Rules:

- Never address a tab by vector index across an asynchronous boundary.
- `DocumentGeneration` changes when a document is replaced, including
  navigation, reload, back/forward reconstruction, and error-page replacement.
- `ControlEpoch` changes on start, pause, takeover, release, attention,
  resume, stop, completion, and failure.
- Element node IDs are valid only inside the snapshot that issued them.
- Every command carries a caller-generated `CommandId` for idempotency. A
  duplicate command ID returns the recorded result instead of running twice.

## 7. State machines

### 7.1 Task state

```rust
pub enum TaskState {
    Connecting,
    Working,
    AwaitingDecision,
    Paused,
    ActionRequired,
    UserInControl,
    Completing,
    Completed,
    Stopped,
    Failed,
}
```

Allowed transitions:

```text
Connecting ──▶ Working ──▶ AwaitingDecision ──▶ Working
     │             │  └────▶ Paused ───────────▶ Working
     │             ├──▶ ActionRequired ──▶ UserInControl
     │             ├──▶ UserInControl ───▶ Working | Paused | Completing
     │             ├──▶ Completing ──────▶ Completed
     └─────────────┴──▶ Stopped | Failed
```

`Completed`, `Stopped`, and `Failed` are terminal. No command except state
queries and frame/history reads is accepted in a terminal state.

### 7.2 Input ownership

```rust
pub enum ControlOwner {
    Agent,
    User,
    SystemModal,
    None,
}
```

- `Working`: owner is `Agent` for observation and proposal submission. Page
  mutation still requires a bound host decision.
- `AwaitingDecision`: task owner remains agent-side, but page input owner is
  `None`; the pending proposal cannot execute.
- `Paused` and `ActionRequired`: owner is `None`; page input is inert.
- `UserInControl`: owner is `User`.
- While a credential/passkey/system permission prompt is actually presented,
  the task may remain `ActionRequired` but owner becomes `SystemModal`.
  Dismissing the prompt returns owner to `None` unless the user takes control.
- Terminal states: owner is `None`.

### 7.3 Attention kinds

```rust
pub enum AttentionKind {
    Login,
    PasswordEntry,
    OneTimeCode,
    Passkey,
    Biometric,
    HardwareSecurityKey,
    PaymentConfirmation,
    Consent,
    DownloadConfirmation,
    UploadFileSelection,
    LocalNetworkOrPermission,
    AmbiguousIrreversibleAction,
    BlockedByPage,
    HumanJudgment,
}
```

An attention event must include the origin, a short user-facing reason, the
available resolutions, and the latest frame/snapshot IDs. It must not include
secret values or page-supplied instructions presented as trusted shell text.

## 8. Supervisor snapshot

The supervision bar and task cards render exactly this state. UI code must not
derive lifecycle state from browser history or page contents.

```rust
pub struct SupervisorSnapshot {
    pub protocol_version: u32,
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub tab_id: TabId,
    pub control_epoch: ControlEpoch,
    pub task_state: TaskState,
    pub input_owner: ControlOwner,
    pub task_title: String,
    pub current_url: Option<Url>,
    pub current_host: Option<String>,
    pub page_title: Option<String>,
    pub loading: bool,
    pub latest_frame: Option<FrameId>,
    pub latest_snapshot: Option<SnapshotId>,
    pub pending_proposal: Option<PendingProposal>,
    pub attention: Option<AttentionRequest>,
    pub allowed_actions: SupervisorActions,
    pub last_error: Option<PublicError>,
}

pub struct PendingProposal {
    pub proposal_id: ProposalId,
    pub operation_summary: String,
    pub target_label: Option<String>,
    pub target_bounds_css: Option<RectF32>,
    pub fingerprint: PageFingerprint,
    pub snapshot_id: SnapshotId,
    pub frame_id: Option<FrameId>,
    pub classification: OperationClass,
    /// Agent rationale is untrusted display text, kept separate from the
    /// shell's trusted operation summary.
    pub agent_rationale: String,
}

pub struct SupervisorActions {
    pub can_take_control: bool,
    pub can_release_control: bool,
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_stop: bool,
    pub can_open_browser: bool,
    pub can_open_externally: bool,
}
```

`allowed_actions` is authoritative for enabled controls. The UI must not
hard-code button enablement from `task_state` alone.

## 9. Supervisor API

The following Rust trait is the normative shape. JSON/IPC bindings use the
same names in `snake_case` and must preserve the same error cases.

```rust
/// Agent-visible handle. Decision, control, stop, session, and credential
/// methods are absent by construction, matching separate authenticated
/// channels in the external handoff protocol.
pub trait AgentClient {
    fn capabilities(&self) -> ServiceCapabilities;
    fn task_view(&self, task: TaskId) -> Result<AgentTaskView, Error>;
    fn observe(&mut self, cmd: Observe) -> Result<PageSnapshot, Error>;
    fn submit_proposal(&mut self, cmd: Proposal) -> Result<ProposalReceipt, Error>;
    fn command_result(&self, task: TaskId, command: CommandId)
        -> Result<CommandStatus, Error>;
    fn drain_agent_events(&mut self, task: TaskId) -> Vec<EventEnvelope>;
}

/// Host/shell-visible handle. Only this handle can record decisions or move
/// lifecycle/control state.
pub trait ShellClient {
    fn capabilities(&self) -> ServiceCapabilities;

    fn create_session(&mut self, req: CreateSession) -> Result<SessionSnapshot, Error>;
    fn close_session(&mut self, session: SessionId) -> Result<(), Error>;

    fn start_task(&mut self, req: StartTask) -> Result<SupervisorSnapshot, Error>;
    fn task_snapshot(&self, task: TaskId) -> Result<SupervisorSnapshot, Error>;
    fn pause_task(&mut self, cmd: CommandMeta) -> Result<SupervisorSnapshot, Error>;
    fn resume_task(&mut self, cmd: CommandMeta) -> Result<SupervisorSnapshot, Error>;
    fn stop_task(&mut self, cmd: StopTask) -> Result<SupervisorSnapshot, Error>;
    fn complete_task(&mut self, cmd: CompleteTask) -> Result<SupervisorSnapshot, Error>;

    fn record_decision(&mut self, cmd: RecordDecision) -> Result<CommandTicket, Error>;
    fn take_control(&mut self, cmd: TakeControl) -> Result<TakeControlReceipt, Error>;
    fn release_control(&mut self, cmd: ReleaseControl) -> Result<SupervisorSnapshot, Error>;
    fn open_externally(&mut self, cmd: CommandMeta) -> Result<HandoffReceipt, Error>;
    fn user_input(&mut self, cmd: UserInput) -> Result<InputReceipt, Error>;

    fn resolve_attention(
        &mut self,
        cmd: ResolveAttention,
    ) -> Result<SupervisorSnapshot, Error>;

    fn drain_events(&mut self, task: TaskId) -> Vec<EventEnvelope>;
}
```

### Execution model

Lifecycle calls apply their state transition synchronously on the supervisor
thread and return the resulting snapshot. A proposal is recorded without
mutating the page. Recording an approval creates an execution ticket;
execution is asynchronous where navigation, network, timers, or settling are
involved. Terminal results arrive as `ActionCompleted` events and remain
queryable through `command_result`. A convenience client may block on a
ticket, but the browser event loop must never block waiting for the agent.

```rust
pub struct CommandTicket {
    pub command_id: CommandId,
    pub task_id: TaskId,
    pub accepted_control_epoch: ControlEpoch,
}

pub enum CommandStatus {
    Queued,
    Running,
    Completed(ActionReceipt),
    Failed(Error),
    Cancelled { reason: CancellationReason },
}

pub enum CancellationReason {
    Stopped,
    ControlChanged,
    SessionClosed,
    DeadlineExceeded,
}

pub struct ActionReceipt {
    pub command_id: CommandId,
    pub before_url: Option<Url>,
    pub after_url: Option<Url>,
    pub after_title: Option<String>,
    pub document_generation: DocumentGeneration,
    pub snapshot_id: Option<SnapshotId>,
    pub frame_id: Option<FrameId>,
    pub navigated: bool,
    pub summary: String,
}
```

### Core request types

```rust
pub struct CreateSession {
    pub profile_id: Option<String>,
    pub viewport_css: SizeF32,
    pub device_scale: f32,
    pub color_scheme: ColorScheme,
    pub start_url: Option<Url>,
}

pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: Option<TabId>,
}

pub struct StartTask {
    pub command_id: CommandId,
    pub session_id: SessionId,
    pub tab_id: TabId,
    pub title: String,
    pub initial_url: Option<Url>,
    pub policy: TaskPolicy,
}

pub struct TaskPolicy {
    pub decision_policy: DecisionPolicy,
    pub allow_local_network: bool,
    pub allow_downloads: bool,
    pub allow_external_open: bool,
}

pub enum DecisionPolicy {
    /// Normative default from HANDOFF_SPEC: every act/sensitive proposal
    /// needs a human decision bound to that exact proposal.
    HumanPerAction,
    /// The host may preauthorize named low-risk classes. The host still
    /// issues a single-use bound decision for each executed proposal; the
    /// browser never executes on class membership alone.
    HostPreauthorizedClasses { classes: Vec<PreauthorizedClass> },
    /// Observation only; act proposals are refused or held for a human.
    ObserveOnly,
}

pub enum PreauthorizedClass {
    SameOriginGetNavigation,
    Scroll,
    FocusNonSensitiveField,
}

pub struct CompleteTask {
    pub meta: CommandMeta,
    pub summary: String,
}

pub struct StopTask {
    pub command_id: CommandId,
    pub task_id: TaskId,
    // Deliberately no expected epoch: Stop must work from a stale shell view.
}

pub struct UserInput {
    pub meta: CommandMeta,
    pub tab_id: TabId,
    pub event: InputEvent,
}

pub struct AgentTaskView {
    pub task_id: TaskId,
    pub task_state: TaskState,
    pub control_epoch: ControlEpoch,
    pub pending_proposal: Option<ProposalId>,
    // During UserInControl this view intentionally excludes page URL/title,
    // snapshots, and frames. It reports lifecycle state only.
}

pub struct Observe {
    pub meta: CommandMeta,
    pub options: SnapshotOptions,
}

pub struct ProposalId(pub u64);

pub struct Proposal {
    pub meta: CommandMeta,
    pub proposal_id: ProposalId,
    pub snapshot_id: SnapshotId,
    pub fingerprint: PageFingerprint,
    pub operation: ProposedOperation,
    /// Agent-supplied human-readable intent. The shell displays this as
    /// untrusted agent text, never as trusted system copy.
    pub rationale: String,
}

pub struct ProposalReceipt {
    pub proposal_id: ProposalId,
    pub classification: OperationClass,
    pub decision_required: bool,
    pub expires_at_ms: Option<u64>,
}

pub struct RecordDecision {
    pub meta: CommandMeta,
    pub proposal_id: ProposalId,
    pub fingerprint: PageFingerprint,
    pub decision: DecisionKind,
    pub decided_by: DecisionSurface,
}

pub enum DecisionKind {
    Approve,
    Reject,
}

pub enum DecisionSurface {
    DesktopPanel,
    Mobile,
    Glasses,
    ApprovalPage,
    HostPolicy,
}

pub enum OperationClass {
    Observe,
    Act,
    Sensitive,
}

pub struct PageFingerprint(pub String); // lowercase hex digest, section 10.1

pub struct AttentionRequest {
    pub attention_id: AttentionId,
    pub kind: AttentionKind,
    pub origin: Option<Origin>,
    pub reason: String,
    pub snapshot_id: Option<SnapshotId>,
    pub frame_id: Option<FrameId>,
    pub resolutions: Vec<AttentionResolutionKind>,
}

pub struct ResolveAttention {
    pub meta: CommandMeta,
    pub attention_id: AttentionId,
    pub resolution: AttentionResolutionKind,
}

pub enum AttentionResolutionKind {
    TakeControl,
    UserCompleted,
    ApproveBrokerAction,
    DenyBrokerAction,
    Dismiss,
    StopTask,
}

pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub registrable_domain: Option<String>,
}
```

`UserInput` is a command envelope; section 10.3 defines the `InputEvent`
payload it carries.

### 9.1 Command metadata

```rust
pub struct CommandMeta {
    pub command_id: CommandId,
    pub task_id: TaskId,
    pub expected_control_epoch: ControlEpoch,
    pub deadline_ms: Option<u64>,
}
```

A stale `expected_control_epoch` returns `StaleControlEpoch`; it never falls
back to best-effort execution.

### 9.2 Take control

```rust
pub struct TakeControl {
    pub meta: CommandMeta,
    pub reason: TakeControlReason,
    pub mode: HandoffMode,
}

pub enum TakeControlReason {
    UserRequested,
    AttentionRequired,
    Login,
    PasskeyOrSecurityKey,
    ReviewBeforeSubmit,
}

pub enum HandoffMode {
    /// Same Hermes window/session. This is the default on macOS.
    InPlace,
    /// Open the current URL in the user's default browser. Cookies, storage,
    /// form values, and page memory are not transferred.
    OpenCurrentUrlExternally,
}

pub struct TakeControlReceipt {
    pub snapshot: SupervisorSnapshot,
    pub frame_id: FrameId,
    pub handoff: HandoffReceipt,
}

pub struct HandoffReceipt {
    pub mode: HandoffMode,
    pub url: Option<Url>,
    pub title: Option<String>,
    pub session_transferred: bool,
    pub note: Option<String>,
}
```

Takeover completes only at a quiescent boundary:

1. Block new agent commands and invalidate every pending proposal.
2. Let at most the currently dispatching native input call finish; do not run
   queued agent commands. Stop issuing observations to the agent.
3. Resolve layout and capture/identify the latest frame.
4. Increment `ControlEpoch`, set owner to `User`, emit `ControlChanged`.
5. Return the receipt.

If quiescence cannot be reached before the deadline, keep agent input blocked
and return `TakeoverDeadline`; do not silently transfer mid-action.

### 9.3 Done / release control

```rust
pub struct ReleaseControl {
    pub meta: CommandMeta,
    pub next: ReleaseMode,
}

pub enum ReleaseMode {
    ResumeAgent,
    KeepPaused,
}
```

The screenshot label **Done** is not allowed to be ambiguous in code. The
shell chooses a `ReleaseMode`; the default after resolving attention is
`ResumeAgent`, and the default after a purely manual inspection is
`KeepPaused` unless product requirements explicitly change that default.

Before resuming, the supervisor must issue a fresh page snapshot. The user may
have navigated, typed, scrolled, or changed form state during takeover.

### 9.4 Stop

`StopTask`:

- is accepted from every nonterminal state;
- is idempotent by `command_id` and harmless when repeated with a new ID;
- revokes the input lease before attempting browser cleanup;
- cancels queued agent input and future action receipts;
- requests navigation/load cancellation;
- emits one terminal `Stopped` state;
- never needs agent cooperation.

Closing the whole session is separate from stopping a task. Stopping a task
may leave the browser open for inspection; closing the session destroys tabs,
documents, Hermes runtimes, and frame history unless a retention policy says
otherwise.

## 10. Browser engine API

The supervisor should depend on this narrow engine trait, not on `BrowserApp`
fields. Phase 1 implements the trait as an adapter over the existing app.

```rust
pub trait BrowserEngine {
    fn capabilities(&self) -> EngineCapabilities;

    fn create_session(&mut self, cfg: SessionConfig) -> Result<SessionId, Error>;
    fn close_session(&mut self, session: SessionId) -> Result<(), Error>;

    fn open_tab(&mut self, session: SessionId, url: Option<Url>) -> Result<TabId, Error>;
    fn close_tab(&mut self, session: SessionId, tab: TabId) -> Result<(), Error>;
    fn select_tab(&mut self, session: SessionId, tab: TabId) -> Result<(), Error>;

    fn navigate(&mut self, target: PageTarget, nav: Navigation) -> Result<(), Error>;
    fn stop_loading(&mut self, target: PageTarget) -> Result<(), Error>;

    fn page_snapshot(&mut self, target: PageTarget, opts: SnapshotOptions)
        -> Result<PageSnapshot, Error>;
    fn dispatch_input(&mut self, target: PageTarget, input: InputEvent)
        -> Result<InputReceipt, Error>;

    fn capture_frame(&mut self, target: PageTarget) -> Result<CapturedFrame, Error>;
    fn drain_events(&mut self, session: SessionId) -> Vec<EngineEvent>;
}

pub struct PageTarget {
    pub session_id: SessionId,
    pub tab_id: TabId,
}

pub struct TabSnapshot {
    pub tab_id: TabId,
    pub url: Option<Url>,
    pub title: String,
    pub active: bool,
    pub loading: bool,
    pub document_generation: DocumentGeneration,
}

pub enum Navigation {
    To(Url),
    Back,
    Forward,
    Reload,
}

pub struct SnapshotOptions {
    pub max_elements: usize,
    pub include_text_excerpt: bool,
    pub capture_frame: bool,
}

pub struct InputReceipt {
    pub sequence: u64,
    pub document_generation: DocumentGeneration,
    pub focused_element: Option<ElementRef>,
    pub navigated: bool,
}

pub struct CapturedFrame {
    pub descriptor: FrameDescriptor,
    /// In-process pixel ownership. IPC transports replace this with a shared
    /// buffer or encoded side channel while retaining the descriptor.
    pub rgba: std::sync::Arc<[u8]>,
}
```

All engine calls run on the browser/event-loop thread or are marshalled to it.
No `Document`, `Page`, Hermes pointer, or Blitz node reference crosses this
boundary.

### 10.1 Page snapshot

```rust
pub struct PageSnapshot {
    pub snapshot_id: SnapshotId,
    pub session_id: SessionId,
    pub tab_id: TabId,
    pub document_generation: DocumentGeneration,
    pub fingerprint: PageFingerprint,
    pub url: Url,
    pub title: String,
    pub ready_state: ReadyState,
    pub viewport_css: SizeF32,
    pub device_scale: f32,
    pub scroll_css: PointF64,
    pub focused_element: Option<ElementRef>,
    pub elements: Vec<ElementSnapshot>,
    pub text_excerpt: String,
    pub blockers: Vec<DetectedBlocker>,
    pub frame_id: Option<FrameId>,
}

pub struct ElementSnapshot {
    pub element: ElementRef,
    pub tag: String,
    pub role: Option<String>,
    pub label: String,
    pub text: String,
    pub href: Option<Url>,
    pub bounds_css: RectF32,
    pub visible: bool,
    pub enabled: bool,
    pub editable: bool,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub value: RedactedValue,
}

pub enum RedactedValue {
    Empty,
    Text(String),
    Sensitive { length: usize },
    Unavailable,
}
```

Snapshot rules:

- `fingerprint = sha256(url || generation || index_digest)`, following the
  handoff spec. `index_digest` covers ordered `(id, role, name, bbox)`
  tuples for indexed elements.
- Include operable and semantically important elements, not a raw dump of all
  nodes by default.
- `bounds_css` is in page CSS pixels after layout, not screenshot pixels.
- Password, OTP, payment, and credential-like fields always use
  `RedactedValue::Sensitive`.
- Hidden or zero-size elements are marked not visible; the agent cannot click
  them by coordinates.
- A snapshot is an observation, not permission. The supervisor revalidates
  the element and generation before acting.

### 10.2 Proposed operations

`Proposal.operation` uses this closed set. Submission validates and stores
the proposal; submission does not mutate the page. Execution begins only
after `ShellClient::record_decision` approves the exact proposal and the
supervisor revalidates target, document generation, and fingerprint.

```rust
pub enum ProposedOperation {
    Navigate { url: Url },
    Back,
    Forward,
    Reload,
    Click { element: ElementRef },
    Focus { element: ElementRef },
    TypeText { element: ElementRef, mode: TypeMode, text: String },
    PressKey { key: KeyInput },
    Scroll { target: ScrollTarget, delta_css: PointF64 },
    WaitFor { condition: WaitCondition, timeout_ms: u64 },
    RequestHuman { kind: AttentionKind, reason: String },
    Complete { summary: String },
}

pub enum TypeMode {
    ReplaceValue,
    InsertAtCaret,
}
```

`Complete` is a proposal, not proof. The supervisor captures a fresh final
snapshot/frame and moves through `Completing`; shell policy or the user decides
whether the task becomes `Completed` or returns to `Working`/`Paused`.

Sensitive text does not travel through `TypeText`. Credential entry uses the
credential broker contract in section 12. If an agent sends text to a field
classified sensitive, return `SensitiveInputRequiresBroker` and raise the
appropriate attention state.

### 10.3 User input

```rust
pub enum InputEvent {
    PointerMove { x_css: f32, y_css: f32 },
    PointerDown { button: PointerButton, x_css: f32, y_css: f32 },
    PointerUp { button: PointerButton, x_css: f32, y_css: f32 },
    Wheel { x_css: f32, y_css: f32, delta_css: PointF64 },
    KeyDown { key: KeyInput },
    KeyUp { key: KeyInput },
    Text { text: String },
    ImeCommit { text: String },
}
```

Coordinates are page CSS pixels. The shell converts window/device pixels
using the frame's scale and viewport offset. The engine must reject input for
the wrong tab, wrong session, or wrong input owner.

### 10.4 Frames

```rust
pub struct FrameDescriptor {
    pub frame_id: FrameId,
    pub session_id: SessionId,
    pub tab_id: TabId,
    pub document_generation: DocumentGeneration,
    pub width_px: u32,
    pub height_px: u32,
    pub stride_px: u32,
    pub format: FrameFormat, // Rgba8Unorm initially
    pub scale: f32,
    pub damage_css: Vec<RectF32>,
    pub captured_at_ms: u64,
}
```

In-process callers may receive the pixels beside the descriptor. IPC callers
receive a shared-memory or encoded-frame reference; the contract must not
assume base64 screenshots on every paint.

## 11. Supervision bar contract

The bar is a pure projection of `SupervisorSnapshot`. Stable control IDs are
part of the contract so UI tests can hit-test real controls:

| ID | Label | Command | Visible/enabled rule |
|---|---|---|---|
| `sup-stop` | Stop | `StopTask` | Enabled in every nonterminal state |
| `sup-take-control` | Take control | `TakeControl(InPlace)` | `can_take_control` |
| `sup-done` | Done | `ReleaseControl` | `can_release_control` |
| `sup-open` | Open browser | focus browser window/frame | `can_open_browser` |
| `sup-open-external` | Open in my browser | `OpenExternally` | `can_open_externally` |
| `sup-pause` | Pause | `PauseTask` | `can_pause` |
| `sup-resume` | Resume | `ResumeTask` | `can_resume` |

Required projected states:

| Snapshot | Header | Primary action | Page input |
|---|---|---|---|
| `Connecting` | task title + “Connecting…” | none | inert |
| `Working` | host/title + “Working” | Take control | observe/propose only; approved execution only |
| `Paused` | host/title + “Paused” | Resume or Take control | inert |
| `ActionRequired` | specific reason, not generic “error” | Take control | inert until takeover |
| `UserInControl` | “You're in control” + host | Done | user only |
| `Completed` | “Completed” + final host/title | Open browser | inert |
| `Stopped` | “Stopped” | Open browser if session retained | inert |
| `Failed` | public error summary | Open browser if session retained | inert |

Additional UI rules:

- **Stop** remains reachable when a modal attention prompt is present.
- The bar displays the current host prominently; never display a URL supplied
  only by page text.
- When the user clicks page content while the agent owns input, do not mutate
  the page. The shell may flash or focus **Take control**.
- During `UserInControl`, show a persistent ownership indicator and remote
  input affordances only when input is actually being forwarded.
- Thumbnails are frames captured by the engine. A blank thumbnail means no
  frame yet and must render as an explicit connecting state, not a fake page.
- Prompt text from the page is quoted as untrusted detail and visually
  separated from trusted shell instructions.

### Current-code placement

Render supervision as a third Blitz document or one clearly delimited region
of the existing chrome document. Either is acceptable if all of these hold:

- state comes only from `SupervisorSnapshot`;
- hit-testing priority is supervisor → chrome → page;
- page viewport height excludes any opaque supervisor strip;
- supervisor controls still work when the page fails to load;
- `--uitest` can hit every stable supervisor control ID.

Do not hide supervisor state inside `ChromeState` booleans such as
`loading`; attach `Option<SupervisorSnapshot>` explicitly.

## 12. Credentials, passwords, and passkeys

Password entry, one-time codes, passkeys, biometrics, hardware keys, and
payments are separate broker flows. They are not `TypeText` with a secret
string.

```rust
pub trait CredentialBroker {
    fn describe_fill(
        &self,
        req: CredentialFillRequest,
    ) -> Result<CredentialPrompt, Error>;

    fn apply_approved_fill(
        &mut self,
        approval: CredentialApproval,
        engine: &mut dyn CredentialSink,
    ) -> Result<CredentialFillReceipt, Error>;
}

pub struct CredentialFillRequest {
    pub task_id: TaskId,
    pub origin: Origin,
    pub fields: Vec<CredentialFieldKind>,
    pub document_generation: DocumentGeneration,
}

pub enum CredentialFieldKind {
    Username,
    CurrentPassword,
    NewPassword,
    OneTimeCode,
    PaymentCard,
    OtherSensitive,
}
```

Rules:

- The agent may learn only that matching credentials exist, the origin, and
  whether an approved fill happened.
- The shell prompt names the exact registrable domain and distinguishes
  **Allow once**, **Always allow this site**, and **Deny**.
- “Always allow” is scoped to origin + credential kind + task class, is
  revocable, and never authorizes arbitrary future form submissions.
- If no credential exists, the shell may offer **Add password**, **Take
  control and type it**, or **Stop**. The add-password UI is trusted shell UI.
- OTP values are entered by the user or an OS provider, never requested from
  the agent as free text.
- Login approval does not imply purchase, message-send, account-change, or
  form-submit approval.

### Passkeys and security keys

WebAuthn is not a DOM fill operation. Use a distinct broker:

```rust
pub trait PasskeyBroker {
    fn begin_assertion(
        &mut self,
        req: PasskeyRequest,
    ) -> Result<PasskeyCeremonyId, Error>;
    fn poll_assertion(
        &mut self,
        id: PasskeyCeremonyId,
    ) -> Result<PasskeyStatus, Error>;
    fn cancel_assertion(&mut self, id: PasskeyCeremonyId) -> Result<(), Error>;
}
```

Until a real macOS authenticator integration exists, `EngineCapabilities.passkeys`
is `Unsupported`. Detection of a passkey-only wall raises
`AttentionKind::Passkey`; the honest resolutions are external handoff, a
future native broker, or stop. Do not simulate a passkey ceremony in page JS.

**Landed 2026-11-06.** The native broker now exists. `apps/hermes-browser/src/credentials.rs`
implements `PasskeyBroker` with a real ES256 authenticator
(`src/webauthn/`), macOS Keychain storage (`src/keychain.rs`), and a host-side
ceremony that produces nothing until a human approves it. `--passkeytest`
proves a real ceremony against the demo fixture, whose independent
`openssl dgst -sha256 -verify` accepts the assertion and rejects a replayed
challenge, an unknown credential, and a mismatched challenge.

The browser never asks macOS for the login keychain password. Keychain user
interaction is switched off for the whole process before anything reaches the
Keychain, so an access the operating system would have turned into a prompt
fails with an error instead of reaching the user. The browser reads only the
items it created itself, which the keychain ACL already trusts, so no prompt
was ever needed. The guarantee is asserted against the live platform
(`SecKeychainGetUserInteractionAllowed`) by a test that was confirmed to fail
when the interaction guard is dropped.

**Landed 2026-11-06 (second pass).** The `fill_form` path no longer signs a
WebAuthn assertion and no longer writes one into a hidden form field: a DOM
fill fills a password and nothing else. The `--supervisortest` passkey checks
now drive the broker rather than the legacy vault signer, so the supervisor
suite exercises the implementation that ships.

**Landed 2026-11-06 (third pass).** The page-facing `navigator.credentials`
path now ends at the broker. A site's `navigator.credentials.create()` and
`.get()` run the real ceremony — Keychain-backed, RP-ID bound to the origin
the document was actually served from, behind the human approval gate — and
the legacy file-based vault signer is deleted, so there is exactly one
WebAuthn implementation in the tree.

The ceremony is *not* simulated in page JS, and the page never signs
anything: the page hands the broker the challenge bytes and receives the
finished assertion. The challenge crosses as raw bytes over a binary scratch
buffer, because string-encoding it corrupts binary challenges. The origin is
taken from the document, not from the page, and an RP ID that is not the
origin's host or a parent of it is rejected.

Credential *discovery* works: `navigator.credentials.get()` with an
`allowCredentials` list uses exactly the credential the relying party named,
and with no list it discovers the relying party's resident credential, which
is what autofill and conditional mediation need. A ceremony that names a
credential which does not exist fails; it never falls back to some other
credential of the same relying party.

The browser exposing `navigator.credentials` is not the same as simulating the
ceremony in page JS, and only the latter is forbidden.

What remains an open design decision is the **approval UX** for a
page-initiated ceremony: how the host surfaces such a ceremony to the human
is not yet settled. That does not affect the correctness of the ceremony
itself, which runs through the same gate as every other broker request.

The boundary is explicit: this is a **software authenticator**. The macOS
platform authenticator (Secure Enclave) and hardware security keys are still
`Unsupported`, because they need an Objective-C bridge to
`AuthenticationServices` that this Rust host does not have. A relying party
that requires the platform authenticator will not accept this credential.

## 13. Events

Events are ordered per task and use a common envelope:

```rust
pub struct EventEnvelope {
    pub sequence: u64,
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub control_epoch: ControlEpoch,
    pub emitted_at_ms: u64,
    pub event: SupervisorEvent,
}
```

```rust
pub enum SupervisorEvent {
    TaskStateChanged { from: TaskState, to: TaskState },
    ControlChanged { from: ControlOwner, to: ControlOwner },
    NavigationStarted { tab_id: TabId, url: Url },
    NavigationCommitted { tab_id: TabId, generation: DocumentGeneration },
    NavigationFinished { tab_id: TabId, result: NavigationResult },
    FrameReady { frame: FrameDescriptor },
    SnapshotReady { snapshot_id: SnapshotId },
    ActionStarted { command_id: CommandId, kind: String },
    ActionCompleted { command_id: CommandId, receipt: ActionReceipt },
    AttentionRequired { request: AttentionRequest },
    AttentionCleared { kind: AttentionKind },
    CredentialPromptRequired { prompt: CredentialPrompt },
    DownloadProposed { download: DownloadInfo },
    ConsoleMessage { level: LogLevel, message: String },
    EngineError { error: PublicError },
}
```

Events are notifications, not the source of truth. A consumer that drops an
event must recover by calling `task_snapshot`.

## 14. Errors

```rust
pub enum ProposalInvalidationReason {
    Rejected,
    ControlTaken,
    Stopped,
    DocumentChanged,
    Expired,
    SessionClosed,
}

pub enum Error {
    UnknownSession,
    UnknownTask,
    UnknownTab,
    TerminalTask,
    WrongOwner,
    StaleControlEpoch,
    StaleSnapshot,
    StaleDocument,
    ElementNotFound,
    ElementNotVisible,
    ElementNotEnabled,
    ElementNotEditable,
    DecisionMissing,
    DecisionUsed,
    DecisionMismatch,
    ProposalInvalidated { reason: ProposalInvalidationReason },
    NavigationFailed,
    NeedsHuman { kind: AttentionKind },
    SensitiveInputRequiresBroker,
    CapabilityUnsupported { capability: &'static str },
    TakeoverDeadline,
    CancelledByStop,
    CancelledByControlChange,
    DeadlineExceeded,
    DuplicateCommandConflict,
    Internal,
}
```

Public errors contain a stable machine-readable kind and a short safe
message. Internal logs may contain diagnostic detail but never secrets or
full form values.

## 15. Capabilities that must be declared

```rust
pub enum Support {
    Supported,
    Partial { notes: &'static str },
    Unsupported,
}

pub struct EngineCapabilities {
    pub engine_name: String,
    pub engine_version: String,
    pub remote_page_script: Support,
    pub trusted_aot_page_script: Support,
    pub text_input: Support,
    pub forms_get: Support,
    pub forms_post: Support,
    pub selects_and_checkboxes: Support,
    pub file_upload: Support,
    pub downloads: Support,
    pub cookies_persistent: Support,
    pub multiple_tabs: Support,
    pub iframes: Support,
    pub find_in_page: Support,
    pub clipboard_read: Support,
    pub clipboard_write: Support,
    pub passkeys: Support,
    pub external_open: Support,
}
```

At the time of writing, the honest starting values are:

| Capability | Starting value | Evidence in current code |
|---|---|---|
| Remote page script | `Unsupported` | Start page and README state remote `<script>` is not executed |
| Trusted AOT page script | `Partial` | Start-page counter through Static Hermes FFI |
| Text input | `Partial` | Address bar only; page fields do not receive keyboard input |
| Forms | `Unsupported` | No form collection/submission path in the host |
| Multiple tabs | `Supported` | Tab create/select/close covered by `--uitest` |
| External open | `Unsupported` | No OS open/default-browser call yet |
| Passkeys | `Supported` (software authenticator only) | `apps/hermes-browser/src/webauthn/` — real ES256 authenticator, macOS Keychain storage, host-side ceremony behind the human approval gate; `--passkeytest` proves the assertion against the demo fixture's independent `openssl` verifier. The macOS **platform** authenticator (Secure Enclave, hardware security keys) is still `Unsupported` — that needs an Objective-C bridge to `AuthenticationServices` |

Do not change a capability to `Supported` without an automated check.

### Auxiliary public types

These definitions close the IDL; fields not needed by a phase may be empty,
but names and redaction behavior are stable.

```rust
pub struct ServiceCapabilities {
    pub protocol_version: u32,
    pub engine: EngineCapabilities,
    pub supervision: bool,
    pub in_place_takeover: bool,
    pub external_open: bool,
    pub credential_broker: bool,
}

pub type SessionConfig = CreateSession;

pub enum ReadyState {
    Loading,
    Interactive,
    Complete,
}

pub enum NavigationResult {
    Committed { final_url: Url },
    Failed { error: PublicError },
    Cancelled,
}

pub struct PublicError {
    pub kind: String, // snake_case Error kind
    pub message: String,
}

pub enum EngineEvent {
    NavigationStarted { tab_id: TabId, url: Url },
    NavigationCommitted { tab_id: TabId, generation: DocumentGeneration },
    NavigationFinished { tab_id: TabId, ok: bool, error: Option<PublicError> },
    FrameDamaged { tab_id: TabId },
    SubresourceFailed { tab_id: TabId, url: Url, message: String },
    ConsoleMessage { tab_id: TabId, level: LogLevel, message: String },
    TabClosed { tab_id: TabId },
}

pub struct DetectedBlocker {
    pub kind: AttentionKind,
    pub element: Option<ElementRef>,
    pub reason: String,
    /// Detector confidence. Policy decides whether this becomes attention;
    /// a detector result alone never grants or denies an action silently.
    pub confidence: f32,
}

pub enum WaitCondition {
    NavigationFinished,
    NetworkIdle { quiet_ms: u64 },
    ElementVisible { element: ElementRef },
    TextPresent { text: String },
    DocumentGenerationChanges,
}

pub enum ScrollTarget {
    Viewport,
    Element { element: ElementRef },
    NearestScrollableAncestor { element: ElementRef },
}

pub struct KeyInput {
    pub key: String,       // W3C key value where practical: "Enter", "Tab", "a"
    pub code: Option<String>,
    pub modifiers: Modifiers,
}

pub struct CredentialPrompt {
    pub attention_id: AttentionId,
    pub origin: Origin,
    pub account_hint: Option<String>, // redacted by broker policy
    pub fields: Vec<CredentialFieldKind>,
    pub can_allow_once: bool,
    pub can_always_allow_origin: bool,
    pub can_add_credential: bool,
}

pub struct CredentialApproval {
    pub attention_id: AttentionId,
    pub scope: CredentialApprovalScope,
}

pub enum CredentialApprovalScope {
    Once,
    AlwaysForOriginAndKind,
    Deny,
}

pub struct CredentialFillReceipt {
    pub origin: Origin,
    pub filled_kinds: Vec<CredentialFieldKind>,
    pub submitted: bool, // filling never implies submission
}

pub trait CredentialSink {
    /// Host-only sink. The broker supplies values directly to fields in the
    /// current document generation; values are not returned to the agent.
    fn fill_sensitive_field(
        &mut self,
        generation: DocumentGeneration,
        kind: CredentialFieldKind,
        secret: &SecretValue,
    ) -> Result<(), Error>;
}

pub struct PasskeyRequest {
    pub task_id: TaskId,
    pub origin: Origin,
    pub rp_id: String,
    pub document_generation: DocumentGeneration,
    /// Opaque WebAuthn options owned by the browser/broker boundary. The
    /// challenge and credential IDs are not exposed to the agent planner.
    pub options_digest: String,
}

pub enum PasskeyStatus {
    WaitingForUser,
    Completed,
    Denied,
    TimedOut,
    Unsupported,
    Failed(PublicError),
}
```

`SecretValue` is deliberately not serializable across the supervisor/agent
boundary. `LogLevel`, geometry primitives (`SizeF32`, `PointF64`, `RectF32`),
`Modifiers`, and `ColorScheme` should reuse existing Blitz/winit concepts in
Rust and simple numeric objects in JSON.

## 16. Custom browser APIs to add

These are the APIs I would ask the browser implementer to add, in order.
They are grouped by trust domain. Agent APIs, supervisor APIs, and page APIs
must not collapse into one unrestricted DOM bridge.

### P0 — required for a real supervised demo

1. **Stable session/tab identity and document generations.** Replace async
   tab indices with `TabId`; generation increments on document replacement.
2. **Native page snapshot.** Query operable elements with role/tag, label,
   text, href, bounds, visibility, enabled/editable state, checked/selected,
   focused element, scroll, viewport, title, and URL.
3. **Atomic approved click execution.** Resolve an `ElementRef` or CSS point,
   revalidate proposal + decision + fingerprint, re-hit-test at execution
   time, dispatch pointer down/up/click through the same path a user click
   takes, then resolve and return a receipt.
4. **Page focus and text input.** Focus editable elements; maintain caret and
   selection; support insert, delete, selection replacement, key events, and
   IME commit. Address-bar editing must remain a separate focus target.
5. **Input ownership enforcement.** One dispatch gate used by winit events,
   agent actions, IPC input, and tests. No alternate mutation shortcut.
6. **Frame capture.** Factor the existing offscreen Vello render into a
   reusable RGBA frame capture with sequence, scale, and document generation.
7. **Task stop/cancel plumbing.** Cancel queued actions and stop loading;
   stale navigation results must not commit after stop or tab close.
8. **Attention fixtures and detectors.** At minimum detect password fields,
   OTP-shaped fields, explicit agent `RequestHuman`, and login forms. Page
   heuristics may propose attention; supervisor policy decides.
9. **Decision classification.** Classify every proposal as observe, act, or
   sensitive. Act/sensitive proposals require a bound decision; page labels
   used for classification remain untrusted input.
10. **Audit/event journal.** Bounded in-memory event/action history with safe
    public summaries and no secret values.

### P1 — required for useful real sites

11. **Forms.** Collect successful controls; support GET and POST
    (`application/x-www-form-urlencoded` first), submitter selection,
    checkboxes/radios/selects, validation/focus reporting, and
    `target=_blank` mapping to a new tab.
12. **Cookies and profiles.** One explicit cookie/profile store per browser
    profile, session isolation, clear-site-data operation, and an honest
    capability flag. Never import a user's Chrome profile implicitly.
13. **Scroll containers and element scroll-into-view.** Current host scroll is
    viewport-level; agents need the nearest scrollable ancestor and target
    visibility after scrolling.
14. **Network introspection.** Per-navigation status, final URL after
    redirects, content type, failed subresources, and network-idle waiting.
    Request/response bodies stay out of the agent API by default.
15. **Downloads.** Proposed-download event with origin, filename, MIME type,
    size if known; supervisor approval; save through native file UI; completion
    receipt with path redacted to filename for the agent.
16. **Uploads.** User-selected files through native picker only. The agent can
    request an upload slot but cannot enumerate or read the filesystem.
17. **Clipboard.** User-gesture-gated copy of selected page text and paste into
    the page during `UserInControl`; agent clipboard read is unsupported.
18. **Find and zoom.** Find-in-page with match bounds; page zoom/device scale
    controls that keep input coordinates coherent.
19. **Console and page errors.** Trusted AOT runtime messages and resource
    failures surfaced as events; later, interpreted-page console if script
    support is added.
20. **External open.** Open the current URL with the OS default browser and
    return a truthful handoff receipt.

### P2 — valuable, not demo-blocking

21. **Accessibility tree snapshot** beside the DOM element snapshot.
22. **Iframes** with per-frame document generations and coordinates.
23. **Downloads/history/bookmarks UI** only after the underlying engine events
    are real; do not add decorative menu entries first.
24. **PDF/text/JSON viewers** as native document adapters where practical. The
    Greenhouse JSON case in the Muse screenshots is a useful fixture.
25. **Session restore** after process restart, explicitly excluding live
    input leases and pending credentials.
26. **Multi-window supervision** and picture-in-picture thumbnails.
27. **Record/replay** of sanitized action journals for debugging. Screenshots
    and form values require separate consent and retention policy.

### Page-runtime APIs — separate trust domain

The existing `blitz_bridge.h` surface is intentionally tiny. For trusted AOT
page bundles, useful additions are:

- query selector / query all;
- get attribute, text, value, bounds, computed style;
- create/append/remove nodes and set inner HTML through the mutator;
- generic `addEventListener` for click/input/keydown/submit;
- timers driven by the host event loop;
- fetch routed through a supervisor-approved network policy;
- origin-scoped storage.

These APIs serve first-party/trusted AOT pages. They do **not** make arbitrary
remote websites scriptable, and documentation must keep that distinction
visible. Arbitrary remote JavaScript requires a separate interpreted-Hermes
page-runtime project with Web API, origin, timer, module, and security models.

## 17. Serialized IPC form

The in-process Rust types are canonical. If the shell moves out of process,
use newline-delimited JSON or a length-prefixed equivalent with these
envelopes:

```json
{
  "protocol": 1,
  "type": "command",
  "command_id": 42,
  "method": "take_control",
  "params": {
    "task_id": 7,
    "expected_control_epoch": 12,
    "reason": "attention_required",
    "mode": "in_place"
  }
}
```

```json
{
  "protocol": 1,
  "type": "event",
  "sequence": 108,
  "task_id": 7,
  "control_epoch": 13,
  "event": "control_changed",
  "data": { "from": "agent", "to": "user" }
}
```

Errors use `{ "kind": "wrong_owner", "message": "..." }`; kinds are
`snake_case` spellings of section 14. Unknown methods and unsupported
capabilities are errors, never no-ops. Method authorization comes from the
connection role established at authentication, never from a self-declared
field: `decision`, `control.*`, `stop`, and `session.*` are valid only on
the shell connection.

## 18. Mapping onto the current codebase

| Current piece | Contract role | Required change |
|---|---|---|
| `Page` in `src/main.rs` | One document + Hermes runtime | Add `TabId` ownership outside `Page`; generation belongs to tab/session state |
| `Tab` | Browser tab | Add stable `TabId`; do not use `Vec` index in async events |
| `Browser` in `src/browser.rs` | Address/history/load state | Keep; expose final URL/status to snapshot |
| `UserEvent::Navigated` | Engine event | Replace `tab: usize` with `TabId`; add supervisor command/event variants |
| `BrowserApp::handle_page_click` | Raw input sink | Split link handling, focus handling, and pointer dispatch behind one owner gate |
| `BrowserApp::handle_key` | Raw keyboard sink | Add page focus target beside address-bar focus |
| `chrome::render` | Browser chrome | Keep browser controls; render supervisor from `SupervisorSnapshot`, not page state |
| `render_to_png` | Frame source | Factor paint-to-RGBA out of PNG writing for `capture_frame` |
| `dom_ffi::BlitzHostContext` | Trusted page mutation | Keep separate from agent API; expand only under page-runtime trust rules |
| `blitz_net::Provider` | Network | Wrap or configure for profile/cookie/status reporting as capabilities are proven |

Suggested new modules:

- `src/supervision.rs` — IDs, state machines, snapshots, errors, audit journal.
- `src/engine.rs` — `BrowserEngine` adapter and page snapshot construction.
- `src/supervisor.rs` — task lifecycle and input-lease enforcement.
- `src/supervisor_ui.rs` — Blitz rendering and hit IDs for the bar/prompts.
- `src/credentials.rs` — broker traits and redaction; no storage implementation
  until a platform credential design is approved.

Do not put supervisor policy in `dom_runtime.ts`. TypeScript page code is page
content, even when AOT-compiled.

## 19. Implementation sequence

### Phase 0 — identity and honest capabilities

- Add stable `TabId` and document generations.
- Add `EngineCapabilities` with current honest values.
- Add a regression test proving a navigation result cannot land in the wrong
  tab after the original tab closes.

### Phase 1 — supervision shell on existing pages

- Add supervisor state, bar rendering, stable control IDs, and input priority.
- Implement pause/resume/stop/take-control/release against the start page.
- Agent actions may initially support snapshot + click only.
- Add `--supervisortest` using real hit-testing and state assertions.

### Phase 2 — useful page interaction

- Add snapshots with bounds and focused-element reporting.
- Add page text input, key input, scroll, and atomic click receipts.
- Add login/OTP attention fixtures using local test pages. Fixtures are test
  content, not production UI pretending to have credential storage.

### Phase 3 — forms, cookies, and external handoff

- Implement GET forms first, then urlencoded POST.
- Prove cookie/profile behavior before advertising it.
- Add OS external-open with a truthful `session_transferred: false` receipt.

### Phase 4 — credential and passkey brokers

- Design macOS storage and origin matching before storing any secret.
- Implement password fill only through the broker.
- Treat passkeys as a separate native-authenticator project. Keep
  `Unsupported` until a real ceremony succeeds in an automated/manual test.

### Phase 5 — script strategy decision

Choose explicitly between:

1. a static/SSR + trusted-AOT browser with declared limits;
2. an interpreted Hermes runtime for arbitrary remote page JavaScript.

Do not let supervision work silently absorb option 2. It is a browser-engine
project of its own.

## 20. Acceptance tests

Add `--supervisortest` beside `--selftest` and `--uitest`. Minimum cases:

1. **Ownership and decision gate:** in `Working`, submitting a click proposal
   does not increment the start-page counter. One bound approval increments
   the counter exactly once; a direct user page click does not increment it.
2. **Takeover:** queue an agent action, take control, prove the queued action
   reaches `CommandStatus::Cancelled { reason: ControlChanged }` (or fails
   `CancelledByControlChange` if submitted after transfer), and the counter
   changes only from the user's click.
3. **Done/resume:** user changes page state, releases with `ResumeAgent`, and
   the next proposal must use a newly issued snapshot. During takeover,
   `observe` on the agent handle returns no page snapshot.
4. **Stop race:** start navigation and an action, stop, then deliver their
   completions; neither mutates the terminal task or another tab.
5. **Stable tab race:** start navigation in tab B, close tab A, deliver B's
   result, and prove B—not the tab now at B's old index—changed.
6. **Stale document:** snapshot a page, navigate, replay the old element
   action, expect `StaleDocument` and no mutation.
7. **Attention:** a local OTP fixture enters `ActionRequired`; agent input is
   rejected as `NeedsHuman`; takeover permits user typing.
8. **Sensitive redaction:** password value is absent from snapshot JSON,
   events, action receipts, and test logs.
9. **Credential scope:** an allow-once decision does not authorize a second
   fill or a different origin. This test may use a test broker; production UI
   must not contain fake stored credentials.
10. **External handoff:** receipt has the current URL/title and
    `session_transferred: false`.
11. **Frame coherence:** frame sequence and document generation never move
    backwards; coordinates map correctly at a non-1.0 scale.
12. **Unsupported is explicit:** requesting remote script, upload, download,
    or passkey before support returns `CapabilityUnsupported`, not success.
13. **Supervisor UI:** every stable control ID in section 11 is hit-testable
    in each state where the control is visible, including while the page is an
    error document.
14. **Idempotency:** sending the same `command_id` twice produces one browser
    mutation and two equivalent receipts.

The nine conformance cases in `HANDOFF_SPEC.md` section 9 are also required
verbatim: no-decision refusal, replayed decision, stale fingerprint, missing
target, takeover during `AwaitingDecision`, transport loss, agent-sent
decision refusal, password-field redaction/scope, and the stop race. A build
does not claim supervision conformance by passing only the local UI cases.

Existing gates must remain green:

```sh
./target/release/hermes-blitz-browser --selftest
./target/release/hermes-blitz-browser --uitest
./target/release/hermes-blitz-browser --supervisortest
```

## 21. Additional product ideas worth considering

These are additions to discuss, not hidden requirements:

- **Agent cursor and last-action highlight.** Show where the agent clicked and
  fade the highlight. This makes supervision legible without reading logs.
- **Approval only at commitment points.** Let low-risk navigation run, then
  require attention before submit/send/pay/delete. Per-click approval for
  every navigation will fatigue users.
- **Task timeline.** A compact, sanitized sequence of navigations, actions,
  approvals, takeovers, and errors. Frame thumbnails are optional and need a
  retention rule.
- **Origin policy memory.** Per-origin choices for login fill, external open,
  downloads, and local-network access, with a visible revoke surface.
- **Local-origin fast path.** Explicitly support `localhost`, LAN admin pages,
  and local files as first-class targets; label local-network destinations in
  the bar so agents do not wander there silently.
- **Purpose-built JSON/text view.** APIs and JSON endpoints are common agent
  targets; a readable native view is more useful than an unstyled text dump.
- **Session export for debugging.** Export the audit journal and capability
  report without cookies, secrets, or form values.
- **Voice/glasses notification adapter later.** A shell can forward
  `AttentionRequired` elsewhere, but notification transport is not part of the
  browser engine contract and should not block this work.

## 22. Explicit non-goals for the first implementation

- Replacing Chrome or claiming broad modern-web compatibility.
- Executing arbitrary remote JavaScript.
- Importing Chrome/Safari cookies, passwords, passkeys, or history.
- Storing production credentials before a platform storage design exists.
- Letting an agent approve its own consequential action.
- Training or planning logic inside the browser process.
- Decorative downloads/history/bookmarks controls without engine support.
- Silent background browsing after `StopTask`.

## 23. Open decisions for the implementing agent to surface, not guess

1. Does **Done** default to resume or stay paused in each entry path
   (manual takeover vs attention resolution)?
2. Is the first supervisor deployment in-process only, or must the Muse
   shell consume frames over IPC immediately?
3. Which actions are commit points requiring approval by default?
4. Is one task per session sufficient for the demo, or must tasks run in
   parallel tabs?
5. Which platform credential store and passkey authenticator are approved for
   a later phase?
6. Is the strategic target static/trusted-AOT pages, or is arbitrary remote
   JavaScript an explicit funded phase?

Until those are answered, implement the state machine so each choice remains
configuration or a separate broker—not scattered conditionals in the page,
chrome renderer, or agent planner.
