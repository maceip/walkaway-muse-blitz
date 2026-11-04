# Hermes Browser control and supervision spec

Status: **implementation spec v1**
Date: 2026-11-04
Applies to: `/Users/mac/hermes-blitz-browser`
External contract source: [`HANDOFF_SPEC.md`](./HANDOFF_SPEC.md)
Browser implementation companion:
`/Users/mac/hermes-blitz-browser/docs/supervision-interface.md`

## 1. Purpose

This document turns the handoff draft and the requested custom APIs into one
buildable contract for the Hermes/Blitz browser. It does not replace the
shipped Walkaway CDP path. It defines the second path: a local native browser
that a host shell and an agent can share under explicit human control.

The implementation goal is narrow:

1. Expose the real browser through an authenticated local control API.
2. Let an agent observe an indexed page and propose typed operations.
3. Execute an operation only after a host-recorded decision binds that exact
   proposal to the current page fingerprint.
4. Let the human take control, release control, or stop without racing the
   agent.
5. Keep page/AOT APIs separate from agent and credential authority.

## 2. Alignment verdict

`HANDOFF_SPEC.md` already lines up with the requested control and supervision
APIs in intent and in most semantics. The gaps are schema and implementation
details, not direction:

| Requested API | Handoff spec | Resolution in this spec |
|---|---|---|
| `session.open/close/state` | Present, fields incomplete | Add title, start URL, capabilities, tab ID, ready event |
| Navigate/back/forward/reload | Present as operations | Keep; all require decisions except initial host-open URL |
| `page.observe` | Present | Define full snapshot schema, visible text, values, bounds |
| Indexed elements/fingerprint | Present | Define canonical digest and snapshot-local IDs |
| Propose + host approval | Present | Add explicit agent-channel `propose` schema |
| Click/type/select/scroll | Present | Keep types; type/select are later browser phases |
| Take/release/stop | Present | Keep state machine and race rules |
| Navigation/frame/execution/refusal events | Partial | Add `nav.started`, `frame.ready`, proposal lifecycle events |
| Action required | Present | Enumerate kinds and resolutions |
| Supervision receipts/audit | Present | Define audit record and browser-owned append log |
| Credentials/passkeys | Principle only | Add broker boundary; no secret storage in v1 |
| Page-facing Blitz APIs | Out of scope there | Separate trusted-AOT roadmap in §13 |

No requested API requires weakening the handoff spec's authority model.

## 3. Decisions made for v1

These choices close the handoff spec's open questions for the first build.
They can be revisited only by changing this document and its conformance
tests together.

1. **Transport:** newline-delimited JSON over a Unix domain socket on macOS.
   The socket lives under the user's private runtime directory with mode
   `0600`. A WebSocket binding may carry identical frames later.
2. **Channels:** agent and host use separate connections and different bearer
   tokens. Authorization is determined by the token used during `hello`,
   never by a role field supplied in later messages.
3. **Decisions:** every `act` and `sensitive` operation requires a human
   decision in v1, including `navigate`, `back`, `forward`, and `reload`.
   No auto-approved classes in v1. Only `observe` and viewport `scroll` run
   without a decision.
4. **Initial URL:** a URL supplied by the host in `session.open` is itself
   the recorded authorization for that first load. It is written to audit as
   `session_open_initial_url`; it does not authorize later navigation.
5. **Takeover race:** block new work immediately, invalidate proposals, and
   allow at most the currently dispatching native input call to finish. A
   fetch that has not committed is discarded by generation/tab checks.
6. **Audit:** the browser owns an append-only in-memory journal in v1 and
   streams audit events to the host. Persistent JSONL export is phase 2; the
   record schema below is stable from v1.
7. **Session/tab scope:** one session controls one dedicated tab in v1. The
   agent cannot create, close, reorder, or select tabs. A new session always
   gets a new control epoch and must re-observe.
8. **Credentials:** no credential storage or agent-supplied secret typing in
   v1. Sensitive fields raise `action_required`; a human completes them while
   in control, or a future broker fills them without exposing values.
9. **Remote JavaScript:** unsupported. `capabilities.remote_page_script` is
   `unsupported`; script-assembled pages may render empty.
10. **Passkeys:** unsupported. Detecting a passkey wall may raise
    `action_required/passkey`, but the browser must never simulate WebAuthn.

## 4. Roles and trust boundaries

| Role | Owns | May | May never |
|---|---|---|---|
| Host shell | Session lifecycle, task title, Stop/Take/Done, decisions, prompts | Open/close, observe state, record decisions, take/release/stop | Execute page operations or approve for the human |
| Agent | Next-step planning | Observe while in `agent`, propose, signal done | Execute, decide, stop, control, read secrets |
| Browser | Documents, tabs, rendering, element index | Execute a bound approved proposal; report state/frames | Invent targets or execute without a valid decision |
| Credential broker | Secret storage/fill ceremony | Fill an approved field inside the host boundary | Reveal secret values to the agent or logs |
| Human | Final authority | Approve, reject, take control, release, stop | — |

Page content is untrusted in every direction. Text returned in observations
is data, never an instruction to the browser or supervisor.

## 5. State machine

```text
idle
  └─ session.open ─▶ agent
agent
  ├─ propose ─▶ awaiting_decision
  ├─ action_required ─▶ action_required
  ├─ control.take ─▶ human
  └─ task.done ─▶ done
awaiting_decision
  ├─ decision approve ─▶ agent (execute exactly one bound operation)
  ├─ decision reject ─▶ agent
  └─ control.take ─▶ human (proposal destroyed)
action_required
  ├─ control.take ─▶ human
  └─ host records that the blocker cleared ─▶ agent
human
  └─ control.release ─▶ agent
any active state
  ├─ stop ─▶ stopped
  └─ transport loss / fatal browser error ─▶ error
```

Rules:

- Exactly one page-input owner exists: agent executor, human, system modal,
  or nobody.
- `awaiting_decision` and `action_required` are blocked substates of an
  agent-owned task; neither permits execution.
- `control.take` is unconditional from every active state.
- On takeover the snapshot/proposal stream to the agent stops. The agent may
  receive lifecycle state only until release.
- Release never navigates or mutates the page. The next agent step requires a
  fresh observation.
- `stop` destroys proposals and prevents every later operation in that task.
- A completed operation does not itself complete the task. The agent signals
  `task.done`; the browser accepts it only with no pending proposal.

## 6. Wire envelope and authentication

Every frame is one JSON object followed by `\n` on the Unix socket.

```json
{
  "v": 1,
  "type": "propose",
  "session_id": "s_01...",
  "request_id": "r_01...",
  "seq": 42,
  "at": "2026-11-04T12:00:00.000Z"
}
```

- The first frame on each connection is `hello` with `protocol`, client name,
  and token. The browser assigns role `host` or `agent` from the token.
- `seq` is monotonic per connection and session. Non-increasing values are
  refused as `replayed_seq`.
- Requests that expect a direct result echo `request_id` in either a typed
  result or `refused`. Events have no `request_id`.
- Maximum frame size in v1 is 4 MiB, except screenshot responses at 16 MiB.
- Unknown required fields are ignored only when explicitly marked optional in
  this spec; unknown message types return `unsupported_message`.
- A token is never accepted in a URL, page content, audit record, or log.

### Channel permissions

| Message family | Agent channel | Host channel |
|---|---:|---:|
| `hello`, `ping` | yes | yes |
| `observe.request` | yes, only in `agent` | yes |
| `propose` | yes | no |
| `task.done` | yes | no |
| `session.*`, `decision`, `control.*`, `stop` | **refused** | yes |
| `audit.read` | no | yes |
| `capabilities.get` | yes | yes |

Wrong-channel host messages are protocol violations. They return
`refused/wrong_channel` and do not mutate state.

## 7. Session and capability messages

### `capabilities.get` → `capabilities`

```json
{
  "type": "capabilities",
  "protocol": 1,
  "engine": {"name": "hermes-blitz", "version": "0.1.0"},
  "operations": ["observe", "click", "scroll", "navigate", "back", "forward", "reload"],
  "remote_page_script": "unsupported",
  "trusted_aot_script": "partial",
  "text_input": "unsupported",
  "forms": "unsupported",
  "cookies_persistent": "unsupported",
  "passkeys": "unsupported",
  "screenshot_png": "supported"
}
```

Capability values are `supported`, `partial`, or `unsupported`. A command
outside the advertised set returns `capability_unsupported`, never success.

### `session.open` → `session.opened`

Required request fields:

- `title`: task title for the supervision header;
- `capabilities`: protocol versions and frame modes the host accepts;
- `policy`: object defined below.

Optional request fields:

- `start_url`: first URL, authorized by the host's act of opening;
- `viewport`: `{width, height, scale}`;
- `color_scheme`: `light`, `dark`, or `system`.

`policy` for v1:

```json
{
  "decision": "human_per_action",
  "allow_local_network": false,
  "allow_downloads": false,
  "allow_external_open": false,
  "screenshot": "on_request"
}
```

The response includes `session_id`, stable `tab_id`, initial `state`,
negotiated capabilities, and the current document generation. The agent
receives the same session ID out of band from the host; opening a session is
not an agent capability.

## 8. Observe, element index, and fingerprint

### `observe.request`

Fields: `include_screenshot` (boolean), `max_elements` (optional, server
clamped), `text_limit` (optional characters, server clamped).

Observation is legal for the agent only in state `agent`. The host may
observe in any active state. In `human`, no observation result is sent to
the agent channel.

### `observe.result`

```json
{
  "type": "observe.result",
  "session_id": "s_01...",
  "request_id": "r_01...",
  "tab_id": "t_01...",
  "state": "agent",
  "url": "https://example.com/",
  "title": "Example",
  "generation": 7,
  "fingerprint": "<64 lowercase hex>",
  "viewport": {"width": 1120, "height": 712, "scale": 2.0},
  "scroll": {"x": 0, "y": 480, "height": 2400},
  "text": "bounded visible text, never raw DOM",
  "elements": [
    {
      "id": "el_17",
      "kind": "click",
      "role": "link",
      "tag": "a",
      "label": "Read more",
      "bounds": {"x": 64, "y": 220, "width": 90, "height": 24},
      "enabled": true,
      "visible": true
    }
  ],
  "focused_element_id": null,
  "screenshot_png_b64": null,
  "screenshot_hash": null
}
```

Element fields:

- `id`: snapshot-local `el_N`, assigned in document order. Raw Blitz
  `NodeId` values never cross the API.
- `kind`: one operation kind the element supports: `click`, `type`,
  `select`, or `scroll`.
- `role`: `link`, `button`, `textbox`, `searchbox`, `checkbox`, `radio`,
  `combobox`, `listbox`, `option`, or `generic`.
- `label`: accessible label in precedence order `aria-label`, associated
  label, visible text, `placeholder`, `title`, `alt`, submit `value`.
- `bounds`: CSS pixels in the current page viewport.
- `value`: present only for non-sensitive editable/select elements.
- `value_state`: `empty`, `present`, or `sensitive`; sensitive values and
  lengths are omitted unless a future broker spec explicitly changes this.
- `checked` / `selected`: present when applicable.

Only elements with a non-empty clickable/editable area are indexed. An
element is `visible` only when its bounds intersect the viewport and Blitz
does not report it as `display:none` or `visibility:hidden`.

### Generation and fingerprint

`generation` is monotonic per tab. It increments on document replacement,
navigation commit, history traversal, accepted execution, control release,
and whenever a newly computed index digest differs from the previous digest.

```text
fingerprint = sha256(
  utf8(url) || 0x00 || decimal(generation) || 0x00 || index_digest
)
index_digest = sha256(concat(length_prefixed_utf8(
  id, role, label, quantized_bounds, kind, enabled, checked, selected,
  value_state
)))
```

Bounds are quantized to two decimal places before encoding. A proposal is
stale when URL, generation, fingerprint, snapshot ID, or target no longer
matches at execution time.

## 9. Proposals, decisions, and execution

### `propose` (agent channel only)

Required: `proposal_id`, `snapshot_id`, `fingerprint`, `generation`,
`operation`, and `rationale`.

```json
{
  "type": "propose",
  "proposal_id": "p_01...",
  "snapshot_id": "o_01...",
  "fingerprint": "<64 lowercase hex>",
  "generation": 7,
  "operation": {"kind": "click", "target_id": "el_17"},
  "rationale": "Open the article the user asked for"
}
```

The browser validates shape, snapshot freshness, and target existence,
classifies the operation, stores exactly one pending proposal, emits
`proposal.pending` to the host, and enters `awaiting_decision`. A second
proposal while one is pending returns `refused/proposal_pending`.

### Operations

| Operation | Class | V1 browser support |
|---|---|---|
| `observe` via `observe.request` | observe | phase 2 |
| `scroll {dy}` | observe | phase 2 |
| `click {target_id}` | act | phase 2 |
| `navigate {url}` | act | phase 2 |
| `back`, `forward`, `reload` | act | phase 2 |
| `type {target_id,text,submit}` | act/sensitive | refused until page input phase |
| `select {target_id,value}` | act | refused until forms phase |

Sensitive classification applies to password, OTP, payment, credential,
submit-that-posts-credentials, send, purchase, and delete targets. The host
may reclassify upward; neither page text nor agent rationale may lower it.

### `decision` (host channel only)

Required: `proposal_id`, `fingerprint`, `generation`, `decision`
(`approve|reject`), `decided_by`, and `surface`.

On approval the browser revalidates all bindings immediately before
dispatch. It consumes the decision exactly once, executes at most one
operation, computes a post-operation observation digest, and emits
`executed`. On rejection it destroys the proposal and returns to `agent`.

## 10. Events, refusals, and audit

Browser events use the same envelope and a per-session event sequence.

| Type | Audience | Meaning |
|---|---|---|
| `state` | host + agent lifecycle view | Control state, URL, title, generation, fingerprint |
| `nav.started` | both | Fetch began |
| `nav.committed` | both | New document installed |
| `nav.finished` | both | Load finished or failed |
| `frame.ready` | host | New frame available; includes dimensions, scale, hash |
| `observe.result` | requester only | Snapshot result |
| `proposal.pending` | host | Full pending proposal for approval UI |
| `proposal.invalidated` | both | Pending proposal destroyed, with reason |
| `executed` | both, redacted for agent | One operation completed |
| `refused` | requester + host | Protocol/stale/authority refusal |
| `action_required` | host + lifecycle-only agent notice | Human step blocks progress |
| `error` | both | Session-level public error |

`action_required.kind` is one of `login`, `password`, `one_time_code`,
`passkey`, `biometric`, `hardware_security_key`, `payment`,
`consent`, `download`, `upload`, `permission`, `blocked`, or
`human_judgment`. The `detail` shown to the human is browser/shell copy;
page-provided text is quoted separately as untrusted `page_detail`.

Refusal reasons are closed vocabulary: `wrong_channel`, `wrong_state`,
`decision_missing`, `decision_used`, `decision_mismatch`,
`stale_fingerprint`, `stale_generation`, `target_missing`,
`target_not_visible`, `target_disabled`, `proposal_pending`,
`proposal_invalidated`, `capability_unsupported`, `sensitive_requires_human`,
`replayed_seq`, `frame_too_large`, `unsupported_message`, and `internal`.

Every proposal, decision, execution, refusal, control transfer, stop, and
attention transition appends:

```json
{
  "audit_id": "a_01...",
  "session_id": "s_01...",
  "at": "2026-11-04T12:00:00.000Z",
  "proposal_id": "p_01...",
  "operation": "click",
  "target_id": "el_17",
  "before_fingerprint": "<hex>",
  "after_fingerprint": "<hex>",
  "screenshot_hash": "<hex or null>",
  "decided_by": "user",
  "surface": "desktop-panel",
  "result": "executed"
}
```

Audit records never contain typed secrets, cookies, tokens, full page text,
or screenshot bytes. Screenshot hashes are hashes of bytes already supplied
through an authorized observation.

## 11. Supervision and desktop handoff

The host owns the session header, **Stop**, **Take control**, **Done**, close,
approval cards, and permission prompts. The browser owns its tab strip,
back/forward/reload, address pill, and page viewport. The host must not draw a
second editable address bar or navigation controls.

`control.take` semantics:

1. Increment the control epoch and block every new agent action.
2. Destroy any pending proposal and unused decision.
3. Stop observation delivery to the agent.
4. If one native input dispatch is already executing, let only that call
   return; start no queued work. Discard uncommitted fetch results.
5. Leave URL, scroll, selection, and document untouched.
6. Emit `state: human` with the latest frame identifier.

`control.release` (`Done`) returns lifecycle control without executing
anything. It increments the control epoch, requires a new observation before
the next proposal, and records the releasing surface.

`stop` is accepted regardless of a stale shell view. It destroys proposals,
cancels queued execution, requests load cancellation, and enters `stopped`.
Closing the session is separate and destroys the tab/session resources.

External handoff is not session transfer. Opening the current URL in the
user's default browser returns `session_transferred: false` because cookies,
storage, form state, and page memory do not move.

## 12. Credentials and passkeys

- A password, OTP, payment, or credential value never travels in `propose`,
  `decision`, `observe.result`, audit records, screenshots, or logs.
- V1 has no credential store. A sensitive target is classified
  `sensitive`; agent-supplied secret text is refused with
  `sensitive_requires_human` and the browser raises `action_required`.
- A future credential broker may fill one approved field inside the browser
  host boundary. The agent learns only origin, field kind, and whether the
  fill occurred.
- Passkeys require a native WebAuthn/broker design that does not exist in
  this browser. The capability remains `unsupported`; no simulated ceremony,
  injected authenticator, or fake success is permitted.

## 13. Page-facing Blitz APIs (separate phase)

Trusted AOT page bundles may receive a richer `blitz.*` host API. This API is
not part of the agent protocol and grants no supervision or credential
authority.

Phase 4 candidate surface:

- `querySelector`, `querySelectorAll`;
- node text/attribute/value reads and writes;
- bounds and resolved style reads;
- create, append, replace, and remove nodes;
- general listeners with target, current target, coordinates/keys, and
  `preventDefault` semantics;
- keyboard, input, focus/blur, and submit events;
- host-driven timers and animation-frame callbacks;
- permission-gated fetch and origin-scoped storage.

String marshaling must be UTF-8 with explicit lengths before any of this is
exposed beyond the bundled start page. The existing ASCII-only bridge is a
correctness bug for general page content.

Arbitrary remote JavaScript is not implied by these APIs. It requires a
separate interpreted-Hermes page runtime, Web API surface, origin model,
module loader, timer model, and security review.

## 14. APIs requested from the Muse shell

The browser can implement its side now, but a production Muse integration
still needs these shell-owned contracts:

1. Launch/session creation carrying task title, start URL, policy,
   capabilities, and distinct agent/host credentials.
2. Decision delivery bound to `proposal_id`, `generation`, and fingerprint,
   with `decided_by` and decision surface preserved verbatim.
3. Frame consumption: accepted dimensions, scale, cadence, color format, and
   whether the shell wants PNG snapshots, shared-memory frames, or both.
4. Credential and passkey brokers that fill or complete ceremonies without
   passing secret material through Muse, the agent, or browser logs.
5. Typed `action_required` presentation and resolution semantics shared by
   desktop, mobile, approval page, and glasses surfaces.
6. Session completion, audit read/export, retention, and deletion rules.
7. Conformance fixtures for stale pages, wrong-channel decisions, takeover
   races, transport loss, and sensitive fields.

## 15. Current implementation gaps and blockers

Verified against the tree on 2026-11-04:

1. **Runtime context:** fixed 2026-11-04. FFI entries scope activation
   around init, dispatch, eval, and destroy; `--runtimetest` proves two-tab
   AOT isolation.
2. **Tab identity:** implemented 2026-11-04. Async navigation carries a
   stable `TabId`, IDs are never reused, and document generations increment
   on document replacement. External session addressing still needs phase 2.
3. **Control surface:** implemented 2026-11-04. `--control-socket <path>`
   serves authenticated NDJSON with host/agent token roles (token files or
   `HERMES_HOST_TOKEN`/`HERMES_AGENT_TOKEN`); `session.open` accepts a
   start URL. There is still no library API; the socket is the API.
4. **Page input:** keyboard edits only the address bar. Page focus, caret,
   selection, IME, and form submission are not wired in the host.
5. **Strings:** the AOT bridge throws above ASCII 127. Replace with
   length-prefixed UTF-8 before expanding page APIs.
6. **Remote script:** unsupported by design; client-rendered sites can be
   empty. Capability reporting must keep this visible.
7. **Distribution:** no app bundle or Git repository; the binary links to a
   Hermes dylib under the user's cache through an absolute rpath.
8. **Element index:** implemented 2026-11-04 in `src/observe.rs` and
   exposed over the control socket as `observe.result`, with snapshot-local
   IDs, labels, bounds, states, visible text, and SHA-256 fingerprint.

## 16. Build sequence

### Phase 0 — correctness foundations (implemented 2026-11-04)

- Scope the active Hermes runtime; add a two-tab AOT isolation regression.
- Add stable `TabId`, document generation, and stale-result tests.
- Keep `--selftest` and `--uitest` green.

### Phase 1 — observation (internal foundation implemented 2026-11-04)

- Add a native snapshot module using Blitz traversal/selectors.
- Emit snapshot-local element IDs, labels, bounds, state, visible text,
  scroll/viewport, URL/title, generation, and fingerprint.
- Add `--observe` headless output for tests before opening a socket.

### Phase 2 — supervised local control (implemented 2026-11-04)

- Add session/control state and Unix-socket NDJSON with two token roles.
- Implement observe, propose, decision, click, scroll, navigate/history,
  screenshot, stop, take, and release.
- Add `--supervisortest` covering §17 plus HANDOFF_SPEC §9.
- Render the in-window supervision state from the same snapshot used by the
  protocol; do not invent a second UI state model.
- Ladybird-alignment step 1 (done): typed `PageCommand` page-endpoint
  vocabulary for execution, and a page-instance ID tier bound into
  snapshots and proposals.
- Ladybird-alignment step 2 (done): canonical tab (identity, session
  history, title, session binding) split from the page instance
  (document, runtime, generations, snapshots), mirroring
  `CanonicalTraversable` vs hosted pages.
- Ladybird-alignment step 3 (done): stable node references
  (`n<page>_<node>`) with a per-instance known-element registry,
  WebDriver-style staleness checks, and hit-test interactability checks at
  execution time. Snapshot-local `el_N` IDs remain for compatibility.
- Ladybird-alignment step 4 (done): one queued input pipeline for page
  clicks, wheel scroll, and approved agent commands, with monotonic input
  event IDs and host-recorded provenance (`physical_user` /
  `approved_agent`) carried on `executed` events.
- Ladybird-alignment step 5 (done): frame contract — a monotonic frame
  counter for presented and captured frames; screenshots name their
  `frame_id`, and observations report `presented_frame_id`.
- Ladybird-alignment step 6 (pending): move the page instance behind a
  child process speaking the typed page-endpoint vocabulary, with the
  canonical tab and supervision state surviving page-process crashes.

### Phase 3 — page input and forms

- Route page focus/key/text through Blitz primitives.
- Implement non-sensitive `type`, then GET forms; POST and selects follow.
- Sensitive typing remains refused until a broker exists.

#### 3.1 Form-aware observation, durable refs, and delta snapshots (extending `src/observe.rs`)

To make account creation, form navigation, form submission, and recall fast and deterministic for both Static Hermes AOT logic and small local LLMs, `observe.result` is extended with the following non-breaking fields while keeping all existing v1 fields intact:

1. **Durable Element References (`node_ref` + stable `el_N` mapping):**
   - Alongside snapshot-local `el_N`, every element carries its stable per-page-instance `node_ref` (`n<page_instance_id>_<blitz_node_id>`), and the session maintains a `DurableRefMap` keyed by `(page_instance_id, blitz_node_id)` so that in-page DOM mutations (such as inline validation errors or dynamically revealed fields) never renumber existing `el_N` references on the same document instance.
   - **New-Element Marker (`is_new: bool` / `*` prefix in compact view):** The observer compares each element's `(page_instance_id, blitz_node_id)` against the previous observation in the same session and sets `"is_new": true` (rendered as `*[el_N]` in `compact_state`) whenever a field, validation banner, MFA/OTP input, or modal appears after an action.

2. **Rich Form Control & Validation Metadata on Observed Elements:**
   - Every form-associated element (`<input>`, `<select>`, `<textarea>`, `<button>`, `<fieldset>`, `<label>`, `<form>`) includes:
     - `form_id`: stable ref of the owning `<form>` element (via ancestor walk or HTML `form="..."` attribute);
     - `form_action`: resolved submission URL of the owning `<form>`;
     - `form_method`: normalized `"GET"` or `"POST"`;
     - `name`: HTML `name` attribute (the exact key serialized on form submission);
     - `input_type`: normalized HTML input type (`text`, `email`, `password`, `tel`, `url`, `number`, `date`, `time`, `datetime-local`, `month`, `week`, `checkbox`, `radio`, `file`, `submit`, `reset`, `search`);
     - `autocomplete`: HTML `autocomplete` token (`given-name`, `family-name`, `name`, `username`, `email`, `new-password`, `current-password`, `one-time-code`, `tel`, `organization`, `street-address`, `address-level2`, `address-level1`, `postal-code`, `country`, `cc-name`, `cc-number`, etc.);
     - `placeholder`: placeholder hint when present;
     - `required`, `readonly`, `disabled`, `checked`, `multiple`: booleans;
     - `validation`: `{pattern, min, max, minlength, maxlength, step, aria_invalid, error_message}` where `error_message` is resolved from `aria-errormessage`, `aria-describedby`, or adjacent visible inline error text inside the same field container;
     - `options`: for `<select>`, `<datalist>`, and ARIA `listbox`/`combobox` controls, an ordered array of `{"value": "...", "label": "...", "selected": bool, "disabled": bool}` extracted directly from child `<option>` / `[role="option"]` nodes in Blitz's arena so the caller sees all valid choices without a separate click turn.

3. **Hidden Radio/Checkbox Promotion Inside Styled Wrappers (`promote_hidden_inputs`):**
   - Many modern signup, plan-selection, and consent forms hide native `<input type="radio">` or `<input type="checkbox">` (`display: none` or `visibility: hidden`) inside a visible `<label>` or cursor-interactive card (`cursor: pointer`, `onclick`, `tabindex`).
   - During tree traversal in `src/observe.rs`, if a visible `<label>` or interactive wrapper contains a hidden `radio` or `checkbox` descendant (or targets one via `for="..."`), the observer promotes the visible wrapper to `role: "radio"` or `"checkbox"`, attaches the inner input's `name`, `value`, `checked`, `required`, and `form_id`, and uses the wrapper's visible text as `label`.

4. **Occlusion Culling, Bounds Propagation, and Opacity-0 False-Banner Filtering:**
   - **Paint-order occlusion culling:** Elements whose interactive hit-test center is completely covered by a higher z-order modal, sticky header, or overlay are excluded from actionable elements (or marked `occluded_by`).
   - **Bounds containment propagation:** Interactive parents (`<button>`, `<a>`, `<label>`, `[role="button"]`, `[role="combobox"]`) propagate their bounds to 99%-contained decorative descendants (`<span>`, `<strong>`, `<svg>`) so inner spans do not emit duplicate clickable IDs.
   - **Opacity-0 static text filtering:** Text nodes inside ancestors with `opacity: 0`, `visibility: hidden`, `display: none`, or `aria-hidden="true"` (walking through `display: contents` wrappers) are excluded from `text` and `compact_state` so pre-rendered hidden `"Account created!"` or `"Invalid password"` banners never trick the agent or local model.

5. **Ultra-Compact `<250-Token` State Format (`compact_state`) & Myers Delta Diff (`diff_from_last`):**
   - `observe.result` includes `compact_state` (a dense, line-oriented representation optimized for `<250` tokens on local SLMs) and `diff_from_last` (a unified line diff against the prior snapshot's `compact_state`):
     ```text
     [state] Create Account — Acme | https://acme.local/signup | gen:4
     [form#f1 POST https://acme.local/signup]
       [el_1] textbox "Full Name" name="full_name" autocomplete="name" required = ""
       [el_2] email "Work Email" name="email" autocomplete="email" required = ""
       [el_3] password "Password" name="password" autocomplete="new-password" required minlength=8 = [empty]
       [el_4] select "Role" name="role" required = "eng" options=["eng:Software Engineer (selected)" | "pm:Product Manager" | "des:Designer"]
       [el_5] checkbox "I agree to Terms" name="tos" required = unchecked
       [el_6] button[submit] "Create Account"
     [/state]
     ```
   - **80% Head / 20% Tail Budget Truncation:** When `compact_state` exceeds the caller's token/character budget, the serializer keeps the top 80% and bottom 20% of lines (never truncating the tail), because a form's `Submit` / `Create Account` / `Continue` button and footer errors live at the end of the form.

#### 3.2 Unified `type`, `select`, `check`, and Form Navigation (`src/main.rs` & `src/dom_ffi.rs`)

1. **Physical & Programmatic Form Focus and Keyboard Navigation (`Form Navigation`):**
   - `App` tracks `focused_node_id: Option<usize>` in the active `Tab` alongside `address_bar_focused`.
   - Clicking an `<input>`, `<textarea>`, `<select>`, or `[contenteditable]` unfocuses the address bar, sets `focused_node_id = Some(node_id)`, and focuses the node inside Blitz.
   - **Keyboard routing in `handle_key`:**
     - When `focused_node_id` is `Some(node_id)` and `address_bar_focused` is `false`:
       - Printable characters, `Backspace`, `Delete`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, and `Cmd/Ctrl+A` route directly into Blitz's Parley `text_input_data` editor for that node AND synchronize the DOM `value` attribute so DOM queries and observations always agree.
       - **`Tab` / `Shift+Tab` Form Navigation:** Pressing `Tab` (or dispatching `{kind: "navigate_form", direction: "next"|"prev"}`) advances `focused_node_id` to the next/previous enabled, visible focusable form control in `[tabindex]` + document order within the current `<form>` (or document), scrolling the newly focused field into the viewport if needed and firing `blur` on the previous node and `focus` on the new node.
       - **`Enter` Implicit Submit:** Pressing `Enter` inside a single-line `<input>` (text, email, password, search, tel, url, number) triggers native submission of its owning `<form>` (subject to supervision policy if the form is classified `sensitive` or `commit`).

2. **Unified `type` Operation with Direct Value Assignment, Combobox Handoff, and Readback Verification:**
   - Request schema: `{"kind": "type", "target_id": "el_1", "text": "Ada Lovelace", "clear": true, "submit": false}`.
   - Execution sequence:
     1. Resolve `target_id` to its exact Blitz `node_id` and verify interactability (connected, visible, not `disabled`, not `readonly`).
     2. **Auto-Routing by Control Kind:**
        - If the target is a `<select>`, automatically delegate to the `select` matcher (matching by `value`, `label`, or unique prefix) so an agent or model that calls `type(el_4, "Software Engineer")` succeeds without needing a separate click.
        - If the target is an HTML5 compound input (`date`, `time`, `datetime-local`, `month`, `week`, `color`, `range`, `number`), validate the format (`YYYY-MM-DD`, `HH:MM`, numeric `[min, max]`) and assign the normalized value directly; if invalid, refuse with `invalid_field_format` and report the expected ISO/numeric format.
        - For standard `<input>` and `<textarea>`: focus the node, clear existing content when `clear: true`, update both the DOM `value` attribute and Blitz's Parley `text_input_data` text buffer, and fire AOT `oninput` and `onchange` handlers.
     3. **Combobox Focus Handoff Detection:** If clicking/focusing a `role="combobox"` wrapper causes an AOT handler to reveal and focus an inner/overlay `<input>`, route the text into the newly focused `<input>` node.
     4. **Mandatory Readback Verification & Concatenation Guard:** Immediately read back both the DOM `value` attribute and Parley editor text after mutation.
        - If `clear: true` was requested and the readback value contains the old value concatenated with `text`, overwrite both buffers cleanly with `text`.
        - If `maxlength` or an AOT `oninput` handler formatted/truncated the value, include `"actual_value": "<readback>"` in the `executed` result so the caller sees the exact state in memory.

3. **Self-Correcting `select` Operation (`<select>` & ARIA Listboxes):**
   - Request schema: `{"kind": "select", "target_id": "el_4", "value": "Software Engineer"}`.
   - Strips zero-width characters (`\u200B..\uFEFF`) and normalizes whitespace.
   - Matches `<option>` children (excluding `disabled` options) in priority order:
     1. Exact match on `option.value` or trimmed `option.label` / `option.textContent`;
     2. Case-insensitive normalized match on `option.value` or `option.label`;
     3. Unique case-insensitive prefix match (`label.startsWith(want)` when exactly 1 enabled option matches).
   - **Loud Refusal on Mismatch (Zero Silent No-Ops):** If 0 options match (or >1 match on prefix/normalized label), the browser **never** silently succeeds; it returns `refused/invalid_option` with the complete list of valid choices in `detail`:
     `"No option matched 'Engineer'. Available options: eng ('Software Engineer'), pm ('Product Manager'), des ('Designer')"`.
   - On match: sets `selected` on the matched `<option>`, clears `selected` on sibling `<option>`s (unless `<select multiple>`), updates the `<select>`'s displayed value, fires AOT `oninput` + `onchange`, and increments `generation`.

4. **Four-Tier `check` / `uncheck` Operation (`checkbox` & `radio`):**
   - Request schema: `{"kind": "check", "target_id": "el_5", "checked": true}` (or toggled via `click`).
   - Resolves the underlying control through a 4-tier waterfall:
     1. Target node itself is `<input type="checkbox|radio">`;
     2. Target node is `<label for="X">` or ancestor `<label>` → resolve the associated control;
     3. Target node is a cursor-interactive wrapper card → resolve descendant `input[type="checkbox"], input[type="radio"]`;
     4. Target node has `role="checkbox"` or `role="radio"` → toggle `aria-checked`.
   - For `type="radio"`, setting `checked = true` automatically unchecks all other `<input type="radio">` nodes sharing the same `name` and `form_id`.
   - Verifies post-mutation `checked` state matches the requested boolean before emitting `executed`.

#### 3.3 Atomic `fill_form` Batch Macro & Native `<form>` Submission (`Form Submission`)

1. **Single-Turn Atomic `fill_form` Operation:**
   - Instead of 8–12 serial round-trips to fill a multi-field form, the agent, host, or Static Hermes AOT runtime can propose a single atomic `fill_form` operation:
     ```json
     {
       "kind": "fill_form",
       "form_id": "el_0",
       "fields": [
         {"target_id": "el_1", "op": "type", " text": "Ada Lovelace"},
         {"target_id": "el_2", "op": "type", "text": "ada@acme.local"},
         {"target_id": "el_3", "op": "fill_secret", "secret_key": "acct_password"},
         {"target_id": "el_4", "op": "select", "value": "eng"},
         {"target_id": "el_5", "op": "check", "checked": true}
       ],
       "submit_target_id": null
     }
     ```
   - **Transactional Validation (`--bail` semantics):**
     - Before mutating the DOM, the browser validates that every `target_id` exists in the bound snapshot, is visible and enabled, and that any `<select>` option or HTML5 constraint (`pattern`, `minlength`, `maxlength`, `min`, `max`, `type=email|number|date`) is satisfied.
     - If any field fails validation or option matching, no partial submission occurs; the browser returns a structured `field_errors` map (`{"el_4": "No option matched ..."}`) so the caller repairs all errors in one turn.
     - If all fields pass, the browser applies all field mutations in document order in `<1ms`, runs AOT `oninput`/`onchange` handlers, verifies readback on every field, and updates the layout/frame in a single render pass.

2. **Two-Phase Fill-Then-Commit Supervision Rule:**
   - Populating non-submitting fields via `fill_form` (with `"submit_target_id": null`) updates the live on-screen form so the human can visually inspect every filled value, selected dropdown, and checked box (with password fields masked as `••••••••`).
   - Submitting the form (`{"kind": "submit_form", "target_id": "el_6"}` or `fill_form` with `"submit_target_id": "el_6"`) is classified as a **commit action** (`act` or `sensitive` if the form contains password/payment/account-creation fields) and requires an explicit host-recorded human `decision: approve` bound to the post-fill fingerprint.

3. **Native HTML `<form>` Serialization & `GET` / `POST` Execution (`src/browser.rs` & `src/main.rs`):**
   - Clicking a `<button type="submit">`, `<input type="submit">`, `<button>` without `type="button"`, or executing `submit_form` runs the native form submission pipeline:
     1. **Locate `<form>`:** Walk ancestors of the submitter node (or resolve its `form="..."` attribute).
     2. **HTML5 Constraint Validation:** Check all enabled submittable controls belonging to that `<form>` for `required` (non-empty / checked), `type="email"` (`@` + domain), `minlength`/`maxlength`, `min`/`max`, and `pattern`. If any control fails validation and `novalidate` / `formnovalidate` is not set:
        - Focus the first invalid control, set `aria-invalid="true"`, surface the validation message in the next observation, and return `refused/form_validation_failed` with the invalid field IDs and reasons instead of firing a network request.
     3. **AOT `onsubmit` Dispatch:** Fire the `<form>`'s `onsubmit` handler in Static Hermes if registered; if `preventDefault()` is called, stop network submission and commit the AOT DOM mutations.
     4. **Construct Form Data Set:** Walk all submittable elements in tree order:
        - Skip `disabled` elements, elements without a `name` attribute, unchecked `checkbox`/`radio` inputs, and `<button>`/`<input type="submit|button|reset">` nodes other than the activated submitter.
        - For `<select>`, emit `(name, option.value)` for every selected `<option>`.
        - For `<input>` and `<textarea>`, emit `(name, current_value)` (resolving any brokered vault value from the node's internal secret slot without exposing it in logs).
     5. **Dispatch HTTP Request via `blitz-net`:**
        - Resolve `action` attribute (or `formaction` on the submitter) against the document's base URL.
        - For `method="GET"` (default): URL-encode the form data set into the target URL's query string (`application/x-www-form-urlencoded`) and fetch via `blitz-net`.
        - For `method="POST"`: encode the form data set as `application/x-www-form-urlencoded` (or `multipart/form-data` when `enctype="multipart/form-data"`) with `Content-Type` header and `Origin`/`Referer` headers, send the `POST` request, handle `301`/`302`/`303` redirects, persist any `Set-Cookie` headers in the tab's origin-scoped cookie jar, parse the response HTML into Blitz, and increment `generation`.

#### 3.4 Account Creation & Local Encrypted Credential Broker (`Account Creation`)

1. **Local AES-256-GCM Credential & Profile Broker (`src/vault.rs`):**
   - The browser hosts an internal, origin-scoped credential & profile broker backed by an AES-256-GCM encrypted store (`~/.hermes-blitz/vault.enc`, permissions `0600`, key stored in `~/.hermes-blitz/vault.key` or OS keychain).
   - **Strict Zero-Knowledge Boundary for Agent & LLM:**
     - The agent and local LLM can **never** read secret values. They only see symbolic credential handles (`vault://profile/work_email`, `vault://account/<origin>/password`, `vault://generator/new_password`) and field `value_state: "sensitive"`.
     - When `fill_secret` executes on an approved or pre-commit field, the browser writes the secret directly into an internal `secret_values: HashMap<usize, Zeroizing<String>>` table on the `Tab` and renders masked bullets (`••••••••`) in the Blitz Parley text box. The real cleartext is substituted only inside `browser.rs` at the moment of HTTPS `POST` form serialization to the matching `origin`.

2. **Deterministic Account-Creation & Auth Field Waterfall:**
   - Adapted from `vercel-labs/agent-browser`'s `handle_auth_login` and extended for full account registration flows, the browser classifies form fields into canonical account slots using a deterministic waterfall:
     - **`email`:** `input[type=email]`, `input[autocomplete=email]`, `input[name*=email i]`, `input[id*=email i]`
     - **`username`:** `input[autocomplete=username]`, `input[name=username i]`, `input[id*=user i]`
     - **`full_name` / `given_name` / `family_name`:** `input[autocomplete=name|given-name|family-name]`, `input[name*=name i]`
     - **`new_password`:** `input[type=password][autocomplete=new-password]`, first `input[type=password]` on a form containing a signup/register submit button or confirm-password field
     - **`confirm_password`:** second `input[type=password]` in the same `<form>` or `input[type=password][name*=confirm|repeat|verify i]`
     - **`current_password`:** `input[type=password][autocomplete=current-password]` or single `input[type=password]` on a sign-in form
     - **`otp_code`:** `input[autocomplete=one-time-code]`, `input[name*=otp|code|token|pin|totp|2fa i]`, or 6-digit numeric inputs revealed after step 1
     - **`tos_consent`:** `input[type=checkbox][required]` or `checkbox` whose label matches `terms|privacy|agree|accept`

3. **End-to-End Account Creation Ceremony:**
   - **Step 1 (Observe & Classify):** When a signup/registration page loads, `observe.result` identifies the registration `<form>` and its canonical slots (`email`, `username`, `full_name`, `new_password`, `confirm_password`, `tos_consent`, `submit`).
   - **Step 2 (Credential Generation & Pre-Fill):** The broker generates a high-entropy password (satisfying the field's `minlength`, `maxlength`, and `pattern` attributes found in `observe.result.elements[i].validation`) in volatile memory (`pending_account`) and populates both `new_password` and `confirm_password` with the identical generated secret, while filling `full_name`, `email`, custom role/plan dropdowns, and `tos_consent` in one atomic `fill_form` pass.
   - **Step 3 (Human Commit Gate):** The browser enters `awaiting_decision` on the final `Create Account` submit proposal, displaying the origin, identity (`email`/`username`), filled non-sensitive fields, and `[generated strong password saved to local vault on approval]` in the supervision bar / glasses card.
   - **Step 4 (Submit, Verify, & Commit to Vault):** Upon `decision: approve`, the browser submits the `<form>`, waits for the response document to commit, verifies that no `aria-invalid="true"` or error banner appeared and that the post-submit page reflects an authenticated/welcome state, and atomically commits `(origin, username, email, password, created_at, form_recipe_id)` into the encrypted local vault. If the site presents a follow-up `one-time-code` step, `is_new` (`*`) highlights the OTP input immediately.

#### 3.5 Form & Account Recall, Replay, and Cross-Session Memory (`Recall`)

1. **Three-Layer Local Recall Store (`~/.hermes-blitz/recall.json` + `vault.enc`):**
   - **Layer 1 — Origin Account & Session Recall:** Maps `origin` → saved account records (`email`, `username`, `password` handle, session cookies) so returning to any service where an account was created logs in or restores the session in one step.
   - **Layer 2 — Canonical User Profile Recall:** Stores user-approved non-secret profile attributes (`full_name`, `given_name`, `family_name`, `email`, `phone`, `company`, `role`, `city`, `country`, `postal_code`, `github_url`, `linkedin_url`) keyed by semantic slot name.
   - **Layer 3 — Deterministic `FormRecipe` & Submission Receipt Recall (`workflow-use` pattern):**
     - Whenever a form is successfully submitted, the browser saves a `FormSubmissionReceipt` and a reusable `FormRecipe`:
       ```json
       {
         "recipe_id": "rcp_01...",
         "origin": "https://acme.local",
         "path_pattern": "/signup",
         "form_signature": "<sha256 of sorted field (tag, input_type, name, autocomplete)>",
         "bindings": [
           {"name": "full_name", "autocomplete": "name", "source": "profile:full_name"},
           {"name": "email", "autocomplete": "email", "source": "profile:email"},
           {"name": "password", "input_type": "password", "source": "vault:password"},
           {"name": "role", "tag": "select", "last_value": "eng", "last_label": "Software Engineer"}
         ],
         "last_submitted_at": "2026-11-04T18:45:00Z",
         "post_submit_url": "https://acme.local/welcome"
       }
       ```

2. **Instant `<0.5ms` Recall & One-Shot Replay:**
   - On every `observe.result`, the browser computes the `form_signature` for each `<form>` on the page and checks the local `RecallStore`:
     - **Exact Recipe Match (Same Site Return):** If `(origin, form_signature)` matches a stored `FormRecipe`, `observe.result` attaches `"recalled_recipe_id": "rcp_01..."` and a ready-to-execute `recalled_fill_form` proposal that populates the entire form in `<0.5ms` with **zero LLM calls**.
     - **Cross-Site Semantic Slot Match (New Site, Known Fields):** Even on a brand-new website never visited before, Static Hermes AOT matches any subset of fields whose `(autocomplete, input_type, name, label)` map to Layer 1/2 profile & vault keys, pre-populating all known fields immediately and leaving only novel site-specific fields for the local LLM.
     - **Explicit Recall Query (`recall.query` / `recall.apply`):** The user or agent can ask *"What account or answers did I submit on Acme?"* or *"Refill this form with what I used last time"*, and the browser returns the exact timestamped non-sensitive submission receipt (with secrets redacted to `[saved in vault]`) or applies it directly to the current form.

### Phase 4 — trusted page APIs and distribution

- Replace ASCII marshaling with UTF-8.
- Expand the AOT bridge in small capability groups with tests.
- Package a signed macOS app only after the Hermes dylib is bundled and the
  launch path no longer depends on a developer cache directory.

#### 4.1 Length-prefixed UTF-8 FFI bridge (`c_shim/blitz_bridge.h`, `c_shim/hermes_shim.cpp`, `src/dom_ffi.rs`)

- Replace null-terminated ASCII-127 string checks in `src/js/dom_runtime.ts` (`encodeAscii`) and `c_shim/hermes_shim.cpp` with explicit `(const uint8_t* ptr, size_t len)` UTF-8 byte slices so international names, currency symbols (`€`, `£`, `¥`), em-dashes, and non-ASCII form labels/values round-trip without throwing.

#### 4.2 Static Hermes AOT Form & Recall Brain (`src/js/dom_runtime.ts` compiled via `shermes -typed -emit-c`)

- Expose zero-copy C FFI externs (`$SHBuiltin.extern_c`) to the AOT-compiled `dom_runtime.ts` unit:
  - `__blitz_get_form_json(ctx_ptr, out_buf, out_cap) -> int32`
  - `__blitz_apply_form_batch(ctx_ptr, batch_json_ptr, batch_json_len) -> int32`
  - `__blitz_lookup_recall(ctx_ptr, origin_ptr, origin_len, sig_ptr, sig_len, out_buf, out_cap) -> int32`
- Compile a deterministic TypeScript form-analyzer and autofill planner (`planFormFillAOT`) directly into native machine code via `shermes -typed -exported-unit=dom_runtime -emit-c`:
  - Scores every observed form field against canonical profile/vault slots (`autocomplete` weight `1.0`, exact `type`+`name` weight `0.9`, normalized `label` token match weight `0.8`).
  - Resolves all high-confidence (`>= 0.85`) fields in `<0.5ms` inside the browser process with zero network or LLM latency, and flags only unresolved/custom fields (`unresolved_fields: [...]`) for Tier-1 local LLM completion.

### Phase 5 — local on-device LLM inference & end-to-end execution pipeline

To guarantee the browser operates at native speed with 100% local intelligence and zero cloud DOM/screenshot round-trips, the system executes across four strictly partitioned local layers:

#### 5.1 Selected Local Models (Already Cached on Disk)

1. **Tier-1 Fast Local Action & Form Model:** `mlx-community/Qwen3-4B-Instruct-2507-4bit` (`2.1 GB` on disk; paired with `mlx-community/Qwen2.5-Coder-3B-Instruct-4bit` at `1.6 GB` for ultra-low-memory mode)
   - **Runtime:** Loaded persistently in local Apple Silicon GPU unified memory via `/Users/mac/.local/bin/mlx_lm.server` on `http://127.0.0.1:8080/v1/chat/completions` (or LiteRT / `llama.cpp` on Android phone NPU/GPU).
   - **Role:** Consumes the `<250-token` `compact_state` snapshot + `unresolved_fields` list and emits a single grammar-constrained JSON `fill_form` or navigation `propose` object (`max_tokens: 160`, `temperature: 0.0`, thinking disabled for instant prefill) in `~120–180ms`.
2. **Tier-2 Escalation Local MoE Model:** `unsloth/gemma-4-26B-A4B-it-GGUF` / `macmacmacmac/gemma-4-31B-it-litert-lm` (`4B` active parameters per token)
   - **Runtime:** Local GGUF / LiteRT runner on loopback.
   - **Role:** Invoked only when Tier-1 encounters a multi-step recovery failure, a complex conditional form dependency, or cross-tab option synthesis. Because only 4B parameters activate per token, it delivers frontier-class judgment at local 4B inference speed.

#### 5.2 End-to-End Execution Placement & Latency Budget

| Stage | Component | Where It Runs | Input | Output | Latency Budget | Security Guarantee |
|---|---|---|---|---|---|---|
| **1. Fetch & Render** | `blitz-net` + `blitz-dom` + `Vello` | Browser native process (Local CPU + GPU) | HTTPS URL / Form `POST` | Resolved layout arena + 120fps GPU surface | `30–120 ms` (net) / `<2 ms` (layout) | Zero untrusted remote JS / zero JIT surface; no screenshot streaming |
| **2. Observe & Distill** | `src/observe.rs` + `DurableRefMap` | Browser native process (In-memory Rust walk) | Blitz DOM + Parley layout tree | `observe.result` + `<250-token` `compact_state` + `diff_from_last` | `<0.3 ms` | Strips `opacity:0` decoy text; redacts all sensitive values to `value_state: "sensitive"` |
| **3. Recall & Fast Autofill** | `RecallStore` + Static Hermes AOT (`dom_runtime.ts` → C) | Browser native process (AOT machine code) | `observe.result` + `RecallStore` metadata | Matched `fill_form` bindings + `unresolved_fields` | `<0.5 ms` | Zero LLM calls for known forms, logins, and profile fields |
| **4. Ambiguous Field Resolution** | `Qwen3-4B-Instruct-4bit` (Tier 1) / `Gemma-4-26B-A4B` (Tier 2) | Local Loopback GPU/NPU (`127.0.0.1`) | `<250-token` `compact_state` + user goal | Single atomic JSON `fill_form` or `click`/`navigate` proposal | `120–180 ms` | Runs 100% locally; never receives raw passwords, cookies, or vault secrets |
| **5. Pre-Fill & Secret Injection** | `execute_operation` + `VaultBroker` | Browser native process (In-memory DOM + Parley) | Validated `fill_form` batch + symbolic `secret_key`s | Populated form inputs, selected options, readback verification | `<1.0 ms` | Secrets stay in `Zeroizing<String>` host memory; viewport shows `••••••••` |
| **6. Human Commit Gate** | Supervision Bar / Companion / Glasses Card | Local Window Chrome or 100-byte BLE/Wi-Fi Card | `proposal.pending` (`submit_form` + `fingerprint`) | Host-recorded `decision: approve\|reject` | Human tap (`1 click`) | Cryptographically bound to `proposal_id`, `generation`, and `fingerprint` |
| **7. Submit & Receipt Recall** | `browser.rs` Form Submitter + `RecallStore` | Browser native process (`blitz-net` + disk) | Approved `<form>` state | HTTP `GET`/`POST` response + saved `FormRecipe` & vault entry | `50–150 ms` (net) | Commits credentials/recipes to AES-256-GCM vault only after verified success |

### Phase 6 — demo showcase workflows (account creation, form navigation, form submission, and recall)

The end-to-end demo exercises four flagship capabilities on real server-rendered and local fixture pages with zero cloud LLM calls:

1. **Workflow A — One-Shot Account Creation & Vault Commit:**
   - User gives goal: *"Create a new developer account on the portal using my work profile and Engineering role."*
   - Browser loads the signup page (`<2ms` layout), indexes the `<form>` (`el_1` Full Name, `el_2` Work Email, `el_3` New Password, `el_4` Confirm Password, `el_5` Role `<select>`, `el_6` Terms checkbox, `el_7` Create Account submit).
   - Static Hermes AOT immediately maps `el_1` (`full_name`), `el_2` (`email`), `el_3`/`el_4` (`vault://generator/new_password`), and `el_6` (`tos_consent`) in `<0.5ms`. Local `Qwen3-4B-Instruct-4bit` maps `el_5` → `"eng"` (`"Software Engineer"`) in `~140ms`.
   - Browser executes the atomic `fill_form` batch, populates all 6 controls on screen (showing `••••••••••••` in the password boxes), verifies readback, and pauses at `el_7` (`Create Account`) in `awaiting_decision`.
   - Human taps **Approve** once; browser serializes the `POST` body (injecting the generated password directly from the broker), submits the form, lands on the authenticated welcome dashboard, and saves the new account + `FormRecipe` to the local vault and recall store.
2. **Workflow B — Keyboard & Agent Form Navigation + Self-Correcting Validation:**
   - Demonstrates `Tab` / `Shift+Tab` focus traversal across inputs, live physical typing into focused Blitz Parley inputs, and loud self-correction when an invalid `<select>` option or bad date/email format is proposed (`refused/invalid_option` returning `Available options: ...`, immediately repaired on the next local step).
3. **Workflow C — Instant Cross-Session Recall & Zero-LLM Replay:**
   - User navigates back to the login page or opens a second multi-field registration/checkout form.
   - Browser computes the `form_signature`, matches the saved `FormRecipe` and vault credentials in `<0.5ms` with **0 LLM tokens**, prefills the entire form instantaneously, and answers *"What role and email did I register with?"* from the local `FormSubmissionReceipt` ledger.

## 17. Acceptance tests

The build must pass the existing `--selftest`, `--uitest`, all nine cases in
`HANDOFF_SPEC.md` §9, and these implementation-specific cases:

1. Two start-page tabs: clicking tab A increments only A; clicking tab B
   increments only B; destroying B does not disable A.
2. Close an earlier tab while a later tab's fetch is in flight; the result
   lands in the tab with the matching stable ID or is discarded if closed.
3. `observe` twice without mutation returns the same fingerprint; a DOM
   mutation that changes an indexed tuple changes the fingerprint.
4. A proposal submitted twice with the same command/proposal IDs creates one
   pending proposal and can execute at most once.
5. In `human`, `observe.request` on the agent channel returns no snapshot;
   after release, a fresh observation is required before proposing.
6. Stop during fetch prevents commit when the fetch later returns.
7. Screenshot hash in audit equals the hash of the authorized frame bytes.
8. Password field value is absent from snapshots, events, audit, and logs.
9. Unsupported type/select/passkey commands return
   `capability_unsupported`; no document mutation occurs.
10. Non-ASCII page text round-trips through snapshot after the UTF-8 bridge
    phase; before that phase the capability remains partial and the start
    page test remains ASCII-only.
11. **Durable refs & `*` new-element markers:** Mutating the DOM in-page (e.g.,
    inserting an inline error banner or OTP field) preserves existing `el_N`
    and `n<page>_<node>` IDs for unchanged controls and marks only the newly
    inserted control with `is_new: true` (`*` in `compact_state`).
12. **Hidden radio/checkbox promotion:** A `<label>` wrapping a `display:none`
    `<input type="radio">` or `<input type="checkbox">` is indexed as
    `role: "radio"|"checkbox"` with accurate `checked` state, and toggling it
    updates the inner input and unchecks sibling radios in the same group.
13. **Loud `<select>` option validation:** Proposing `select` with a
    non-existent option returns `refused/invalid_option` listing all valid
    `<option>` values/labels and leaves the current selection unchanged;
    proposing a valid value, label, or unique prefix updates `<option selected>`,
    fires `onchange`, and changes the fingerprint.
14. **Atomic `fill_form` & readback verification:** A single `fill_form`
    batch populates text, email, `<select>`, and `checkbox` controls in one
    turn, updates Blitz's Parley text layout and DOM attributes identically,
    and reports verified readback values.
15. **Native `<form>` `GET` and `POST` submission:** Approving `submit_form`
    on a `GET` form navigates to `action?name=val...`; approving `submit_form`
    on a `POST` form transmits the `application/x-www-form-urlencoded` body
    with all enabled named controls (including broker-injected password bytes
    that never appeared in `observe` or `propose`) to the local test server
    and renders the response page.
16. **Account creation & Recall replay:** Completing a signup flow stores the
    origin credential handle and `FormRecipe`; reloading or revisiting the
    form matches `form_signature` in `<1ms`, emits `recalled_recipe_id`, and
    replays the form fill with zero LLM calls.

## 18. Definition of the first valuable demo

A localhost fixture and one server-rendered public page can be driven through
the socket protocol with: open → observe → propose click → host approve →
execute once → new fingerprint → take control → human click → Done → fresh
observe → stop. Every transition is visible in the native window and every
refusal path is covered by a state assertion. That demo proves the browser
contract without claiming Chrome equivalence or remote-script support.

### 18.1 Extended Hackathon Demo (Account Creation, Form Navigation, Form Submission, and Recall)

Building directly on the foundational demo above, the flagship showcase runs
100% locally (Static Hermes AOT + Blitz + Vello GPU + local `Qwen3-4B` /
`Gemma-4-26B-A4B` over loopback) through four continuous acts:

1. **Act 1 — Instant Form Indexing & Account Creation:** Open a realistic
   multi-field account registration page (`Full Name`, `Email`, `Password`,
   `Confirm Password`, `Role` dropdown, `Plan` radio card, `Terms` checkbox).
   Observe in `<0.3ms` (`compact_state` <200 tokens with inline `<select>`
   options and promoted radio cards). Fill the entire form in one atomic
   `fill_form` batch (Static Hermes AOT matching standard profile/password
   fields in `<0.5ms` + local `Qwen3-4B` selecting custom fields in `~150ms`),
   visually verify the populated fields and masked password bullets in the
   native Vello window, approve the final `Create Account` commit in one tap,
   and watch the real HTTP `POST` submission land on the authenticated account
   page.
2. **Act 2 — Interactive Form Navigation & Self-Correction:** Navigate between
   form fields with `Tab` / `Shift+Tab` and physical typing, trigger an inline
   validation error or option mismatch, and show the browser surfacing the
   exact `Available options` / validation constraint and repairing it in one
   step.
3. **Act 3 — Zero-LLM Recall & Instant Sign-In / Replay:** Return to the sign-in
   or registration flow (or open a second form asking for the same profile
   fields), watch `RecallStore` + Static Hermes AOT match the `form_signature`
   and profile slots in `<0.5ms` with **zero LLM calls**, prefill the entire
   form instantaneously, and query the local submission receipt ledger to
   recall the exact account and form values used.
