# Handoff spec — Muse browser ⇄ agent control

Status: **draft for review**. Owner: Muse browser shell team.
Scope: the contract between a Muse shell that renders a browser and an agent
that drives it, where a human can take the wheel at any moment.

This document does not describe the existing CDP/connector path
(`connector/`, `companion/`). That path is unchanged and remains the shipped
product. This spec covers the second path: Muse hands the user to a browser
surface it renders itself (mobile shell, docked desktop panel), and the agent
operates inside that same surface.

---

## 1. Why this needs a spec at all

The CDP path separates capabilities by *process*: the connector can propose but
not approve, the approval page can approve but not decide, the companion
executes. The human is never inside the browser.

A Muse-native browser collapses that separation. The human, the agent, and the
page are all in one window. Without an explicit contract, three things break:

1. **Authority blurs.** If the agent can act whenever the window is open, the
   approval step becomes decorative. PROJECT.md's locked rule — *the connector
   has no approve tool; execution requires a recorded decision* — must survive
   the move into the browser.
2. **Control fights.** The user types while the agent clicks. Both mutate the
   same document. Without ownership, neither can reason about the result.
3. **Evidence disappears.** Today every proposal carries a screenshot and a
   freshness check. In-surface, the temptation is to skip both because "the
   user can see it". That is exactly when they stop being checkable.

So: one control state at a time, decisions bound to proposals, freshness
enforced at execution, and an audit trail that survives the session.

---

## 2. Non-goals

- Not a new approval product. The approval page stays the authority for
  consequential actions when the human is *not* in the browser.
- Not remote control of a user's own Chrome. That is the CDP path.
- Not a permissions framework for the agent's other tools (email, calendar).
- Not a visual spec. Chrome composition is owned by the browser build; this
  spec only fixes which side owns which control.

---

## 3. Roles and trust boundaries

| Role | Owns | May | May never |
|---|---|---|---|
| **Host shell** (Muse chat, mobile, desktop panel) | Session chrome, approval UI, Stop / Take control / Done | Open/close sessions, render proposals, record the human's decision, stop the task | Execute operations, decide on the human's behalf |
| **Agent runtime** | Deciding the next step | Observe, propose typed operations | Execute, approve, or claim a decision exists |
| **Browser** (this build) | The document, its own tab strip and nav row | Execute an operation **only** with a valid decision; report state | Decide what to do next; hide a navigation from the host |
| **Human** | The brake | Take control at any time, approve, reject, stop | — |

Trust boundary: the browser trusts the host for *decisions*, never for
*operations*. A message that asks the browser to act without a decision is a
protocol violation and must be refused, not logged and ignored.

---

## 4. Control model

Exactly one control owner exists at any time. The owner is part of session
state and is broadcast on every change.

### 4.1 States

| State | Meaning | Who may execute |
|---|---|---|
| `idle` | No session | nobody |
| `agent` | Agent is working; may observe and propose | browser, on a decision |
| `awaiting_decision` | A proposal is pending; nothing runs | nobody |
| `human` | User has taken control | nobody (agent proposals paused) |
| `action_required` | Agent needs the human (login, 2FA, consent) | nobody |
| `stopped` | User stopped the task | nobody |
| `done` | Task completed | nobody |
| `error` | Unrecoverable session error | nobody |

`awaiting_decision` and `action_required` are *sub-states of agent control*: the
agent still owns the task, it is simply blocked. `human` is a transfer.

"Who may execute" means *via the protocol*. In `human`, the person acts directly
on the surface — clicking links, typing in the address bar — which is ordinary
browsing and needs no decision. What stops is agent-initiated execution.

### 4.2 Transitions

| From | Event | To | Guard |
|---|---|---|---|
| `idle` | `session.open` | `agent` | host authenticated |
| `agent` | `propose` | `awaiting_decision` | proposal is well-formed, target exists |
| `awaiting_decision` | `decision: approve` | `agent` | proposal id + fingerprint match, decision unused |
| `awaiting_decision` | `decision: reject` | `agent` | proposal deleted; never executes later |
| `awaiting_decision` | `control.take` | `human` | always allowed; pending proposal invalidated |
| `agent` | `control.take` | `human` | always allowed; in-flight proposal invalidated |
| `human` | `control.release` | `agent` | host records who released |
| `agent` / `awaiting_decision` | `action_required` | `action_required` | browser raised it |
| `action_required` | `decision: approve` | `agent` | same binding rules as any decision |
| `action_required` | `control.take` | `human` | always allowed |
| any active | `stop` | `stopped` | user or host only, never the agent |
| `agent` | `done` | `done` | no pending proposal |
| any | transport loss | `error` | see §9.6 |

Rules that are not negotiable:

- **Take control is unconditional.** No proposal, modal, or in-flight operation
  may delay it. If an operation is executing, the browser finishes that single
  operation and then yields; it never starts another.
- **Stop is unconditional and immediate.** Pending proposals are destroyed.
- **Only the host transitions to `human`.** The agent cannot request, simulate,
  or infer a transfer.

### 4.3 What "taking control" means for the page

On `control.take`:

1. Cancel any pending proposal (`proposal.invalidated`, reason `control_taken`).
2. Stop issuing `observe` results to the agent (the page is now private to the
   human until released).
3. Leave the document untouched. Taking control must never navigate, scroll, or
   mutate.

---

## 5. Message contract

Transport is out of scope of the schema; the reference binding is a
localhost WebSocket with a per-session bearer token, one JSON object per frame.
Desktop panel embedding may use native IPC with the same objects.

The agent and the host connect on **separate authenticated channels**. Host-only
message types (`decision`, `control.*`, `stop`, `session.*`) are rejected on the
agent channel — that rejection is what makes invariant 4 enforceable rather than
aspirational.

Every message:

```json
{
  "v": 1,
  "type": "propose",
  "session_id": "s_01J9Z8K3",
  "seq": 42,
  "at": "2026-11-04T12:00:00.000Z"
}
```

`seq` is monotonic per sender; receivers reject non-increasing `seq` for a given
`session_id` (replay and reordering defence).

### 5.1 Host → browser

| Type | Purpose | Required fields |
|---|---|---|
| `session.open` | Start a session | `capabilities`, `policy` |
| `session.close` | End it | `reason` |
| `observe.request` | Ask for the current page state | `include_screenshot` |
| `decision` | Record a human decision | `proposal_id`, `fingerprint`, `decision`, `decided_by`, `surface` |
| `control.take` / `control.release` | Transfer control | `by` |
| `stop` | Abort the task | `by` |

### 5.2 Browser → host

| Type | Purpose | Required fields |
|---|---|---|
| `state` | Control/nav state changed | `control`, `url`, `title`, `fingerprint`, `generation` |
| `observe.result` | Page snapshot | `elements[]`, `fingerprint`, `generation`, optional `screenshot_png_b64` |
| `action_required` | Blocked on the human | `kind`, `detail` |
| `executed` | Result of one operation | `proposal_id`, `ok`, `post_fingerprint`, `effects[]` |
| `refused` | Protocol violation or stale binding | `proposal_id`, `reason` |
| `error` | Session-level failure | `code`, `detail` |

### 5.3 Operations

```json
{ "kind": "click",    "target_id": "el_17" }
{ "kind": "type",     "target_id": "el_22", "text": "…", "submit": false }
{ "kind": "select",   "target_id": "el_31", "value": "…" }
{ "kind": "scroll",   "dy": 480 }
{ "kind": "navigate", "url": "https://…" }
{ "kind": "back" } | { "kind": "forward" } | { "kind": "reload" }
```

Classification decides whether a decision is required:

| Class | Operations | Decision |
|---|---|---|
| `observe` | `observe.request` | not required |
| `act` | `click`, `type`, `select`, `navigate`, `back`, `forward`, `reload` | **required** |
| `sensitive` | `type` into a password/payment field; `click` on a submit that posts credentials | **required + never autofilled + masked in logs** |

`scroll` is local to the viewport and never changes document state; it needs no
decision. Everything else does.

### 5.4 Proposal binding

A proposal is only executable while all four of these still match:

```
proposal_id  — the exact proposal
target_id    — still present in the element index
fingerprint  — document unchanged since observation
decision     — exists, matches proposal_id + fingerprint, and is unused
```

Any mismatch → `refused` with a specific reason
(`stale_fingerprint`, `target_missing`, `decision_missing`, `decision_used`).
The browser refuses *before* touching the document, and reports what changed.

---

## 6. Freshness and the element index

The browser maintains a per-document element index: every operable element with
a stable `id`, its role, accessible name, bounding box, and enabled state.

- `generation` increments on navigation, reload, and on any mutation that adds,
  removes, or renames an indexed element.
- `fingerprint = sha256(url ‖ generation ‖ index_digest)`, where `index_digest`
  is over the ordered `(id, role, name, bbox)` tuples.
- The agent never sees raw DOM. It sees the index, exactly as the CDP path
  does, so the "invented target" rejection rule keeps working unchanged.

---

## 7. UI ownership

Grounded in the Muse mobile and docked-desktop screenshots. This is the part
most likely to be argued about, so it is stated explicitly.

| Element | Owner | Notes |
|---|---|---|
| Session header (title, subtitle, status) | **Host** | Subtitle mirrors `state.url`; read-only |
| `Stop` | **Host** | Sends `stop` |
| `Take control` / `Done` | **Host** | Sends `control.take` / `control.release` |
| Close (×) | **Host** | Ends the session; must confirm if a task is running |
| Approval cards, "Action required", permission prompts | **Host** | The only surface that may emit `decision` |
| Tab strip, `+`, tab close | **Browser** | Local to the browser; the agent may not create or close tabs |
| Back / forward / reload | **Browser** | Agent-initiated ones are operations (§5.3) and need a decision |
| Address pill, padlock | **Browser** | Host never renders an editable URL |
| Page viewport | **Browser** | Host may display a screenshot, never drive it |

Consequence: the host must not draw its own back/forward/reload. Two sets of
navigation controls that disagree about history is a bug the user cannot
diagnose.

---

## 8. Security invariants

Each is testable and each maps to a failure seen in practice.

1. **No decision, no execution.** The browser refuses any operation without a
   decision bound per §5.4.
2. **Decisions are single-use.** Replaying a consumed `decision` returns
   `decision_used` and does nothing.
3. **Stale decisions die.** Fingerprint mismatch at execution time invalidates
   the decision; the proposal must be re-observed and re-approved.
4. **The agent cannot mint decisions.** `decision` from the agent's connection
   is refused regardless of payload.
5. **Take control wins.** On `control.take`, pending proposals are invalidated
   and no further operation starts.
6. **Credentials are never autofilled.** A `type` into a password or payment
   field requires the decision to name that field, and the value never appears
   in logs, screenshots, or the audit trail.
7. **Every execution is auditable.** `{proposal_id, operation, before_fp,
   after_fp, screenshot_hash, decided_by, at}` is recorded, including refusals.
8. **The decision surface is recorded, not inferred.** `decided_by` carries the
   surface that produced the decision (`mobile`, `desktop-panel`, `glasses`) and
   the browser stores it verbatim, so an audit can distinguish a phone approval
   from an in-panel one.

---

## 9. Conformance tests

A browser build claims conformance when these pass. Each is a state assertion,
not a screenshot comparison.

| # | Test | Expected |
|---|---|---|
| 9.1 | `execute` with no prior decision | `refused / decision_missing`, document unchanged |
| 9.2 | Replay a consumed decision | `refused / decision_used` |
| 9.3 | Mutate the page, then execute an old proposal | `refused / stale_fingerprint` |
| 9.4 | Remove the target, then execute | `refused / target_missing` |
| 9.5 | `control.take` while `awaiting_decision` | proposal invalidated, state `human`, no execution |
| 9.6 | Transport drops mid-proposal | state `error`, proposal destroyed, no execution on reconnect without a new decision |
| 9.7 | Agent sends `decision` | refused; session unaffected |
| 9.8 | `type` into a password field | requires field-scoped decision; value absent from audit log |
| 9.9 | `stop` while an operation is executing | that operation completes or is not started; no second one begins |

9.9 is the only place a race is permitted, and it must be resolved in favour of
"at most one more operation".

---

## 10. Current status in the browser build

The reference build is a separate tree (`hermes-blitz-browser`: Static Hermes +
Blitz + Vello, macOS). Honest gap analysis: it renders and navigates; it has no
agent protocol at all.

| Capability | Status |
|---|---|
| Real navigation, history, tabs, links, scrolling | built |
| Typed DOM mutation bridge (`$SHBuiltin.extern_c`) | built |
| Element index + fingerprint | **not built** |
| Session/control state machine | **not built** |
| Proposal / decision binding | **not built** |
| Audit log | **not built** |
| Screenshot capture for proposals | **not built** (offscreen renderer exists and could serve this) |
| Page JS execution | not supported by design — see README |

The element index is the first thing to build: `observe` is useless without it,
and §5.4 binding depends on it. The existing hit-testing and `get_element_by_id`
paths already give the primitives.

---

## 11. Open questions for the team

1. **Who owns the audit log?** Browser-local file, host, or the existing relay?
   Recommendation: browser writes, host reads — same as today's relay split.
2. **Does `navigate` need a decision?** It is an `act` here because navigation
   can carry credentials in a URL. If the team prefers fewer prompts, restrict
   the exemption to same-origin GETs and say so explicitly.
3. **What happens on `control.take` during an executing operation?** This spec
   says "finish one, start none". Confirm that is acceptable, or require
   cancellation — cancellation is harder to make atomic.
4. **Does the mobile shell reuse this contract?** It should; the screenshots
   show the same affordances. If mobile needs a different decision surface, the
   binding rules must still hold.
5. **Session lifetime.** Does a tab survive a session? Recommendation: yes, but
   a new session always starts in `agent` with a fresh fingerprint.

---

## 12. What I need from the shell team

To implement the browser side against this spec, I need four answers:

1. Transport and auth for `session.open` (WebSocket vs native IPC, token
   lifetime).
2. Whether the host or the browser renders the approval card for `sensitive`
   operations.
3. The exact `policy` block the host will send (which operation classes are
   auto-approved, if any).
4. Confirmation that §7 ownership is right, especially "host draws no
   navigation controls".

Until those are settled I will not add protocol code to the browser — an
unimplemented contract in the tree is worse than a written one.
