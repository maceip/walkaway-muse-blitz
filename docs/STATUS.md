# Shared status board

Last updated: 2026-11-04

Governing execution plan: `docs/FINAL_LOCKED_PLAN.md` (sprint board: `demo_machinery_story_and_sprint.md`)

Read-only technical reference:
`/Users/mac/walkaway-hackathon/docs/spec.md`

Coordination rules: `AGENTS.md`

This file is the single shared record for ownership, current state, evidence,
and next action. Chat claims do not update project state; this file does.

## Canonical rule

New demo work lands in this monorepo only after the component is claimed
below. Existing working code remains canonical at the source-map location
until a recorded migration replaces that location. The old Chrome/Walkaway
path and the Hermes native path are intentionally separate execution paths;
never blur which browser executed an action.

## Components

| Component | Owner | State | Canonical location | Last verification | Next action |
|---|---|---|---|---|---|
| Hermes browser | Browser/Fixtures Agent (`eeb06c47` — Zone X) | Zone X (`X1`, `X2`, `X3`) COMPLETE: `--supervise-commits-only` (`requires_glasses` classification), `--xray` Semantic X-Ray overlay + live telemetry HUD (`layout_observe_ms`, OS `getrusage` `RSS MB`, `FormSignature`, `recalled_count`), and NIST P-256 ECDSA WebAuthn assertion signing in `src/vault.rs` | `/Users/mac/hermes-blitz-browser` (commit `6d2c4ef`) | `--selftest`, `--uitest`, `--runtimetest`, `--observetest`, `--supervisortest` (54/54 checks PASS), `test_zone_x_engine_superpowers.py` (PASS `1.799s`), plus Slices 1–3 & full vertical | Frozen at commit `6d2c4ef` for integrated 10-portal rehearsal with `pebble-mirach` |
| Monorepo wrapper | pebble-mirach (formerly clover-pulsar) | Scaffold created | `/Users/mac/walkaway-monorepo` | JSON manifests parse; Git status inventoried; Android describe completed | Other agent must claim components here before editing |
| Android notification bridge | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | P4: 4-state demo-safe UI + persistent dedupe; DAT RE-REGISTRATION REQUIRED (test-run uninstall wiped app data) | `apps/android-notification-bridge/` | 2026-11-04: `:app:testDebugUnitTest` PASS, `:app:connectedDebugAndroidTest` PASS on Pixel; on-device DISCONNECTED + CONNECTED-WAITING phases, zero phone Approve/Reject nodes, replay #9-#13 carded once each | Owner: open app -> Connect Meta AI (re-register DAT) before any glasses run; then P5/P6 |
| Laptop message bus | pebble-mirach (Partner Lane P) | P3 FROZEN: USB + `adb reverse tcp:19099`; NEXT_ID persisted; `batch_proposals` passthrough | `services/hermes-host/message_bus.py` (+ `start-transport.sh`) | Recovery verified 2026-11-04: tunnel removed -> `start-transport.sh` restored -> message #12 `received` ~0.5s; LAN fallback documented UNVERIFIED in README (needs P1 re-run on LAN before any demo use) | Demo startup is exactly `services/hermes-host/start-transport.sh`; never use port 8899 |
| Glasses web surface | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Wrapper only in monorepo; older Walkaway app exists separately | `apps/glasses-web/` wrapper; `/Users/mac/walkaway-hackathon/glasses` existing app | No monorepo glasses UI verified | Claim before creating or migrating any UI |
| Phone approval surface | UNASSIGNED | Wrapper only | `apps/phone-approval-web/` | No implementation verified | Claim before work; do not duplicate glasses approval logic |
| Desktop supervision | Browser/Fixtures Agent (`eeb06c47`) | Implemented inside Hermes | `/Users/mac/hermes-blitz-browser/src/chrome.rs` | Supervision bar exercised by `--supervisortest` and screenshot capture | No standalone desktop client unless demo requires separation |
| Muse connector | pebble-mirach (Partner Lane P) | Deployed; Meta listed six tools in user-operated form | `/Users/mac/walkaway-hackathon/connector/mcp_server.py` | Live `tools/list` with connector bearer returned HTTP 200; request logging enabled | Owner completes submission; do not create a second connector |
| Meta DAT reference path | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physical glasses card verified through existing Ambient app; not copied into Walkaway | `/Users/mac/ambient-link-meta/relay-android` (reference only) | Ambient debug APK installed; DAT session `STARTED`, display `STARTED`, display content ack received; user selected “Saw the card” | Extract only DAT lifecycle/registration learnings; do not use Ambient relay/mesh/sync |
| Walkaway native DAT | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physically verified (P1/P2); registration currently AVAILABLE after instrumented-test uninstall wiped app data | `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/WalkawayDat.kt` | Registration check 2026-11-04 post-`connectedDebugAndroidTest`: `registrationState=AVAILABLE devices=0`; `dat_failed` fast path observed | NEVER run connected tests on demo day without re-registering after; owner re-registers via Connect Meta AI, confirm REGISTERED + devices=1 |
| Hermes host bridge | pebble-mirach (Partner Lane P) | P2 PASS incl. full physical chain (Hermes -> bridge -> glasses tap -> executed -> SQLite) | `services/hermes-host/decision_bridge.py` (+ `message_bus.py`) | `test_decision_bridge_e2e.py` PASS x2 (8.2s/8.3s); physical rerun 2026-11-04 `P2 PHYSICAL CHAIN PASS`: cards #10/#11 `approve`/`glasses_dat` (01:21:57Z/01:22:10Z), audit `executed` `surface=glasses`, `accounts == 1` | Zone Z integration: driver reads `GET /health` sessions + `GET /pending`, posts Tap1/2/3 via `POST /cards` (19101, held mode); Lane P next: P3-P6 rehearsals |
| Agent driver | Browser/Fixtures Agent (`eeb06c47` — Zone Z) | Zone Z (`Z1`, `Z2`, `Z3`) COMPLETE: `parallel_campaign_driver.py` (resident `mlx-community/Qwen3-4B-Instruct-2507-4bit`, 10-portal campaign, 3-tap glasses coordinator, Tap-3 `batch_proposals` manifest) + `reset_demo.py` (`--reset-demo` in `~3ms`) | `services/agent-driver/{hermes_agent_driver.py, parallel_campaign_driver.py, reset_demo.py}` | `test_10portal_disco_campaign.py` (3/3 tests PASS in `16.695s`: `total_llm_calls==1`, 3 glasses cards, 10 SQLite submissions, Card #3 reject safety gate, `<2000ms` reset gate, `0` secret leaks) | Ready for physical glasses rehearsal with `pebble-mirach` |
| Demo fixture | Browser/Fixtures Agent (`eeb06c47` — Zone Y) | Zone Y (`Y1`, `Y2`, `Y3`) COMPLETE: `/cfp/1`..`/cfp/10`, `/disco-workbench` 10-viewport live Blitz grid (`#tile-1`..`#tile-10`), `/api/webauthn/{challenge,register,verify,assertions}` (real OpenSSL P-256 verification), `/campaign-receipt`, and `POST /api/reset` | `services/demo-fixtures/signup_fixture.py` | `test_zone_y_10portals_and_disco.py` (PASS `4.871s`) + full 9-test E2E suite (`python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` — 9/9 PASS in `37.271s`) | Frozen for rehearsal and recording |
| Protocol contracts | UNASSIGNED | Living in implementations/spec, not yet copied | `packages/protocol/` | Hermes spec + suites are current evidence | Claim before extracting shared schemas |
| Marketing surface | MCode (`minimax-code`) | Built: static, no-build marketing site for "Walkaway × Muse Blitz — Supervised Batch Browser & Glasses Commit Gate". Hero + hero art, 3-act story accordion, security model, film (intro/outro/demo frame), App Store asset kit with downloads, 6 paid placements as inline SVG creatives with slot filter + SVG export, test-registry proof cards, FAQ, CTA | `apps/marketing-site/` (no build step; `index.html` + `assets/`) | `python3 scripts/verify-marketing-site.py` PASS (24 local refs resolved, 4 font urls resolved, 37 banner text nodes fitted to canvas, 6 non-ascii classes covered); local render `python3 -m http.server 8099 --bind 127.0.0.1` in `apps/marketing-site/` — 0 console errors, 0 failed/4xx/5xx requests | Publish to a public URL; drop the final 3-minute demo master into `apps/marketing-site/assets/video/` and point the film frame at it. KNOWN BLOCKER: no demo video file exists in the repo yet, so the film section ships as a poster frame only |

## Live-process conflicts and known duplicates

- Port `8899` is already occupied by a pre-existing `python3 -m http.server`
  serving `/Users/mac/labrea/agent-relay-surface/public`. Do not kill or
  replace that process. Walkaway message bus now uses `19099`.
- Existing Walkaway Chrome relay and example web processes are active from
  earlier work. They are not the Hermes execution path.
- Hosted `muse.secure.build` runs the existing connector. Local stale
  connector processes must not be treated as the deployed connector.
- Port `8099` is held by the marketing-site preview
  (`python3 -m http.server 8099 --bind 127.0.0.1` rooted at
  `apps/marketing-site/`). The port was confirmed unused before the preview
  started and it is not part of the demo path. Stop it when the preview is no
  longer needed; never reuse it for the message bus or the host bridge.

## Current hard stop

Do not build another browser, approval UI, connector, relay, Android bridge,
host bridge, or message bus. Resolve ownership here first, then add the
smallest real-system test for the next demo slice.
