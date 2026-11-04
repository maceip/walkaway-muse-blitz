# Muse Connector Platform — Walkaway submission packet

Status: **not submitted — all assets live 2026-11-03, awaiting owner form submission.** Paste-ready values for https://platform.muse.ai/submission
(Overview → Technical specs → Review). Owner-only fields are marked ❌; nothing in
this packet is invented. Form shape verified against the live form screenshot
(2026-11-03) and public submitter packets.

## Step 1 — Overview

| Field | Value |
|---|---|
| Connector name (≤80) | `Local Browser for Muse` |
| Company or developer (≤120) | `Ryan MacArthur` |
| Product website | `https://walkaway.secure.build/` ✅ live |
| Connector icon | `submission/icon-localbrowser.png` — 512×512 PNG, 64,004 bytes (limit 256 KiB) ✅ browser-window + home mark |
| Payments | **Does not accept payments** (no payment tools exist) |
| Your name | `Ryan MacArthur` |
| Work email | `ryan.macarthur@gmail.com` (same account as Muse sign-in) |
| Support email or URL | `https://walkaway.secure.build/support` ✅ live (ryan.macarthur@gmail.com) |
| Privacy policy | `https://walkaway.secure.build/privacy` ✅ live |
| Terms of service | `https://walkaway.secure.build/terms` ✅ live |

**Example prompts** (each maps to a real connector tool)

```text
My internet keeps dropping — open my router page at home and check whether a firmware update is waiting.
Go to my Synology dashboard and tell me which drive is almost full.
I'm already signed in to my clinic's patient portal on this computer — check if an earlier appointment opened up this week.
Test the signup flow on my localhost build and stop at the first step that breaks.
Open Home Assistant and check why the living-room lights missed tonight's schedule.
Check my kid's school portal — I'm signed in here — is tomorrow an early dismissal?
What step is the agent waiting on? Send it to my approval page.
Stop the task and close the browser — I'll take it from here.
```

**Anything else?**

```text
Muse already has a secure cloud browser. Local Browser for Muse (by
Walkaway) covers the web that browser cannot reach: the user's own Chrome,
with the user's existing signed-in sessions, local network, and localhost dev
servers. The connector also structurally cannot act without the user: it
removes the execute capability from the connector entirely: there is no approve tool for Muse to call, mis-call, or be
talked into calling by injected page content. Muse proposes one typed step at
a time — an operation plus one element actually observed on the page — and the
step executes only after you approve the exact step on a separate approval
page showing a live screenshot, the action, and the deciding model. Rejecting
deletes the proposal on the companion, so a rejected step can never run late.
Decisions are consumed once and invalidated when the page changes, so retries
cannot double-click and stale actions cannot fire. The agent runs in the
user's own Chrome with the user's existing sessions — no credentials are
handed to a cloud browser — and screenshots never pass through the connector.
The same approval page also runs on Meta Ray-Ban Display glasses (WebMCP).
Self-hosted and open source; the developer operates no servers and receives no
user data.
```

## Step 2 — Technical specs

| Field | Value |
|---|---|
| Connection type | **Existing MCP** |
| Endpoint | `https://muse.secure.build/mcp` ✅ live, verified initialize/tools/list/tools/call 2026-11-03 |
| Transport | Stateless MCP over streamable HTTP, JSON-RPC 2.0, protocol `2025-03-26` |
| Auth methods | API key (Bearer token, constant-time compare; required on non-loopback binds) |
| API/MCP documentation | `https://muse.secure.build/docs` ✅ live |
| Access requirements | User runs the open-source companion + connector on their own machine; requires Chrome and the user's own model API key. Read-only use is possible via `walkaway_status` alone. |
| Tools (6, annotated) | `walkaway_start_task` (write, open-world), `walkaway_status` (read-only), `walkaway_propose_step` (write, open-world, never executes), `walkaway_request_approval` (write; cannot approve), `walkaway_reject_step` (write, safe-direction), `walkaway_stop_task` (destructive) |
| Test account | None needed centrally: reviewer runs the repo locally; fixtures (`travel`, `research`) need no third-party account. Hosted review endpoint is live; review bearer token is in the access field of the submitted form (rotated after review). |

## Step 3 — Review

Attestations are owner-only. Pre-submission checklist:

- [ ] Publish repo; fill product website + support URL
- [ ] Host PRIVACY.md and TERMS.md at live URLs; paste URLs
- [ ] Host connector over HTTPS with `WALKAWAY_CONNECTOR_TOKEN` set; paste endpoint
- [ ] `curl` the hosted endpoint: `initialize`, `tools/list`, one `tools/call` (see connector tests)
- [ ] Owner enters name, work email, payments stance (`Does not accept payments`)
