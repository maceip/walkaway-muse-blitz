# Secure Browser with Approval

**Muse gets a real browser. A human keeps the brake.**

Ask Muse for browser work in plain language. A typed browser agent executes in
your own Chrome, proposes one step at a time, and nothing consequential runs
until you approve the exact step — live screenshot included — on an approval
page on your phone. The Muse Connector deliberately has no approve tool: Muse
can never authorize Muse's own browser actions.

Project control, locked decisions, and the full checklist: [PROJECT.md](PROJECT.md).
Theme: **Push the Frontier**. Track: **Indie Developer**. Brand: Walkaway.

## The 30-second version for judges

1. Tell Muse: *"Find Zurich → London flights for Friday."* Muse calls the
   **Secure Browser with Approval** connector (MCP, streamable HTTP).
2. The companion agent builds a fresh indexed table of every operable element
   on the page and returns one typed operation + one observed target per model
   call. Invented targets are rejected before anything touches the browser.
   Page fingerprints invalidate stale decisions; every decision is consumed
   once, so a retry cannot double-click.
3. Each proposal appears on your approval page with a real screenshot, the
   action line, confidence, and the deciding model's name. Approve executes
   exactly that step. Reject deletes the proposal on the companion — a
   rejected step can never execute late.
4. Meta Ray-Ban Display glasses run the same approval page as a bonus surface.
   No part of the system requires glasses.

## Why the approval split matters

Browser agents fail in two directions: unsupervised agents click the wrong
thing, and supervised agents stop being useful. Secure Browser separates
capabilities in code rather than in promises:

- **Connector (Muse side):** start, status, propose, request approval, reject,
  stop. Text-only results — screenshots never transit the connector.
- **Approval page (human side):** the only path to execution. Proposing never
  executes. Approving executes exactly one shown step.
- **Companion (machine side):** CDP browser, typed decisions, freshness checks,
  per-step screenshots.

## Meta technology used

| Surface | Use |
|---|---|
| Muse Connector Platform | Existing-MCP connector: stateless streamable-HTTP JSON-RPC, six annotated tools, bearer auth. Live at `https://muse.secure.build/mcp`; packet in `submission/SUBMISSION.md` |
| Meta Model API | Muse Spark chooser (`companion/spark_choose.py`): one `muse-spark-1.3` call at `https://api.meta.ai/v1` returns strict-JSON operation+target |
| Meta Ray-Ban Display + WebMCP | The approval page also runs as a 600×600 Web App with six `walkaway_*` tools — bonus surface, same backend |

## Verified in this repo (2026-11-03)

- Hosted loop over public HTTPS: MCP `initialize` → start → propose
  (`CLICK`, confidence 0.95 class proposals) → approve via approval-page API
  only (page navigated, history increments, real screenshot returned) → stop.
- Connector auth: unauthenticated requests return 401; relay API likewise.
- Tests: glasses tools 3/3, Spark decision parser 6/6, connector RPC 4/4 —
  including "no approve tool exists" and "screenshots never appear in
  connector results". TypeScript build clean.
- Deployment: systemd services (Chrome CDP, relay, connector) behind Caddy
  with Let's Encrypt on `walkaway.secure.build` / `muse.secure.build`.

## Honest status

| Claim | Status |
|---|---|
| Connector, relay, approval page, typed agent | ✅ working, hosted, tested |
| Muse Spark decision path | Implemented + unit-tested; live call pending `MODEL_API_KEY` (UI prints the active model verbatim; fallback is labeled TypeSafe) |
| Google Flights scenario | Wired; verified end-to-end scenario is the local research fixture |
| Glasses hardware run | Install path built (`docs/INSTALL_PIXEL.md`); hardware verification pending |
| Android driver app | Not built yet — Pixel must be connected over ADB |

## Run

Hosted stack (already running): `https://walkaway.secure.build/app/` with a
relay token (`?k=`), connector health at `https://muse.secure.build/health`.

Local stack:

```sh
bash scripts/run_all.sh
# Relay :8790 · Connector :8890/mcp · Approval page http://127.0.0.1:5173/
```

Tests: `cd glasses && yarn test && yarn build` ·
`cd companion && python3 -m unittest test_spark_choose` ·
`cd connector && python3 -m unittest test_mcp_server`

## Repo map

| Path | What |
|---|---|
| `PROJECT.md` | Locked scope, decisions, checklist — start here |
| `connector/` | Muse Connector MCP server (no approve tool, by design) |
| `companion/` | Relay + Muse Spark chooser; owns Chrome over CDP |
| `glasses/` | Approval page (phone-responsive; 600×600 WebMCP on glasses) |
| `submission/` | Connector packet + 512×512 icon |
| `site/` | Generated public pages (landing, privacy, terms, support, docs) |
| `VIDEO_SCRIPT.md` | The 3-minute demo, shot by shot |
