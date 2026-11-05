# Shared status board

Last updated: 2026-11-04 21:07 PT (Post-submission salvage & full integration WS1/WS2/WS4 COMPLETE & GREEN in `AGENT_SALVAGE_COORDINATION.md`; WS3 Android code + unit tests complete, device verification awaits Pixel USB reconnect; evening demo runbook is `docs/DEMO_RUN.md`)

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


## LIVE DEMO RUN — COMPLETE 2026-11-04 19:15 PT

- Terminal evidence (run dir
  `/var/folders/8j/5mwkcdz55xbcg1pg_54tlcf00000gn/T/walkaway-live-demo-psa_o5bz/`):
  `campaign_meta` status `COMMITTED`, `llm_calls=1`; SQLite
  `accounts == 1` (Ryan MacArthur), `cfp_submissions == 10`; 11 executed
  receipts all `decision_surface=glasses_dat` (act1 signup, act2 repaired
  CFP, act3 batch p2-p10); bus glasses approves on #27-#32
  (03:12:35Z-03:15:45Z); Hermes audit `executed/glasses_dat x11`.
- Path to completion (honest sequence): Lane P run #14/#15 committed Act 1
  then Card 2 hit the driver CLI 30s window after a dismissed card; Lane P
  detached rerun (bus #26, 900s windows) failed with
  `ConnectionResetError` when the 19099 bus was restarted mid-run
  (run3.log `CAMPAIGN FAILED`); the subsequent `muse_blitz.sock` run
  completed all 10 portals over the same Lane P transport (bus 19099,
  DAT, dedupe, 4-state UI `CARD ON GLASSES` observed live).
- Runbook: `docs/DEMO_RUN.md`. No demo stack needs to stay running;
  fixture 8795 `/campaign-receipt` serves the receipt page.

## Components

| Component | Owner | State | Canonical location | Last verification | Next action |
|---|---|---|---|---|---|
| Hermes browser | Browser/Fixtures Agent (`eeb06c47` — Zone X) | Zone X (`X1`, `X2`, `X3`) + Salvage WS1/WS4 COMPLETE: `--supervise-commits-only` (`requires_glasses` classification with `stage_only` intermediate fill/probe support), `--xray` Semantic X-Ray overlay + live telemetry HUD (`layout_observe_ms`, OS `getrusage` `RSS MB`, `FormSignature`, `recalled_count`), and NIST P-256 ECDSA WebAuthn assertion signing in `src/vault.rs`; synced byte-for-byte with `apps/hermes-browser/src/` | `/Users/mac/hermes-blitz-browser` & `apps/hermes-browser` | `--selftest`, `--uitest`, `--runtimetest`, `--observetest`, `--supervisortest` (54/54 checks PASS), `test_zone_x_engine_superpowers.py` (PASS), plus full 9-test `tests/e2e` suite (9/9 PASS in `29.783s`) | Complete & verified |
| Monorepo wrapper | pebble-mirach (formerly clover-pulsar) | Scaffold created | `/Users/mac/walkaway-monorepo` | JSON manifests parse; Git status inventoried; Android describe completed | Other agent must claim components here before editing |
| Android notification bridge | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | WS3 code complete (FGS keep-alive, guarded DAT reuse, card-gone + Resend, dedupe forget); device verification BLOCKED (USB disconnected) | `apps/android-notification-bridge/` | `:app:testDebugUnitTest` 8/8 PASS + assembleDebug fresh APK 2026-11-04 20:59 PT; earlier P4 live states + full-demo transport PASS | Reconnect Pixel: installDebug, verify FGS + background poll + Resend, run connected tests (wipes DAT reg — re-register after) |
| Laptop message bus | pebble-mirach (Partner Lane P) | P3 FROZEN + Salvage WS2 COMPLETE: USB + `adb reverse tcp:19099`; NEXT_ID persisted; `batch_proposals` passthrough; `/tmp/walkaway-bus-<port>.lock` flock + `allow_reuse_address = False` (live on `PID 12901`) | `services/hermes-host/message_bus.py` (+ `start-transport.sh`) | `test_message_bus_lock.py` PASS (`0.172s`); recovery verified 2026-11-04: tunnel removed -> `start-transport.sh` restored -> message #12 `received` ~0.5s | Demo startup is `services/hermes-host/start-transport.sh` or `run_live_demo.py`; never use port 8899 |
| Glasses web surface | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Wrapper only in monorepo; older Walkaway app exists separately | `apps/glasses-web/` wrapper; `/Users/mac/walkaway-hackathon/glasses` existing app | No monorepo glasses UI verified | Claim before creating or migrating any UI |
| Phone approval surface | UNASSIGNED | Wrapper only | `apps/phone-approval-web/` | No implementation verified | Claim before work; do not duplicate glasses approval logic |
| Desktop supervision | Browser/Fixtures Agent (`eeb06c47`) | Implemented inside Hermes | `/Users/mac/hermes-blitz-browser/src/chrome.rs` | Supervision bar exercised by `--supervisortest` and screenshot capture | No standalone desktop client unless demo requires separation |
| Muse connector | pebble-mirach (Partner Lane P) | Deployed; Meta listed six tools in user-operated form | `/Users/mac/walkaway-hackathon/connector/mcp_server.py` | Live `tools/list` with connector bearer returned HTTP 200; request logging enabled | Owner completes submission; do not create a second connector |
| Meta DAT reference path | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physical glasses card verified through existing Ambient app; not copied into Walkaway | `/Users/mac/ambient-link-meta/relay-android` (reference only) | Ambient debug APK installed; DAT session `STARTED`, display `STARTED`, display content ack received; user selected “Saw the card” | Extract only DAT lifecycle/registration learnings; do not use Ambient relay/mesh/sync |
| Walkaway native DAT | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physically verified (P1/P2); registration currently AVAILABLE after instrumented-test uninstall wiped app data | `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/WalkawayDat.kt` | Registration check 2026-11-04 post-`connectedDebugAndroidTest`: `registrationState=AVAILABLE devices=0`; `dat_failed` fast path observed | NEVER run connected tests on demo day without re-registering after; owner re-registers via Connect Meta AI, confirm REGISTERED + devices=1 |
| Hermes host bridge | pebble-mirach (Partner Lane P) | P2 PASS + salvage WS1/WS2 COMPLETE: `POST /sockets` dynamic attach, bus per-port flock + reuse-addr off, integrated end-to-end with `parallel_campaign_driver.py` | `services/hermes-host/decision_bridge.py` (+ `message_bus.py`) | `test_decision_bridge_e2e.py` 3-case PASS (`9.061s`) + `test_10portal_disco_campaign.py` 3-case PASS (`14.466s`) | Complete & verified |
| Agent driver | Browser/Fixtures Agent (`eeb06c47` — Zone Z) | Zone Z (`Z1`, `Z2`, `Z3`) + Salvage WS1/WS2 COMPLETE: `parallel_campaign_driver.py` routed 100% through `decision_bridge.py` (`19101` held mode, `POST /sockets`, `GET /pending`, `POST /cards`, `GET /decisions`; duplicate host socket client deleted) + `run_live_demo.py` (`/tmp/walkaway-demo-stack.lock` flock + `--detach` mode) + `reset_demo.py` (`--reset-demo` in `~3ms`) | `services/agent-driver/{hermes_agent_driver.py, parallel_campaign_driver.py, run_live_demo.py, reset_demo.py}` | `test_10portal_disco_campaign.py` (3/3 tests PASS in `14.466s` via `decision_bridge.py` held mode: `total_llm_calls==1`, 3 glasses cards, 10 SQLite submissions, Card #3 reject safety gate, `<2000ms` reset gate, `0` secret leaks) | Complete & verified |
| Demo fixture | Browser/Fixtures Agent (`eeb06c47` — Zone Y) | Zone Y (`Y1`, `Y2`, `Y3`) + Salvage WS1 COMPLETE: `/cfp/1`..`/cfp/10`, `/disco-workbench` 10-viewport live Blitz grid (`#tile-1`..`#tile-10`), `/api/webauthn/{challenge,register,verify,assertions}`, `/campaign-receipt`, `POST /api/reset`, and `HighBacklogThreadingHTTPServer` (`request_queue_size=128`) for 9-worker fanout | `services/demo-fixtures/signup_fixture.py` | `test_zone_y_10portals_and_disco.py` + full 9-test E2E suite (`python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` — 9/9 PASS in `29.783s`) | Complete & verified |
| Protocol contracts | UNASSIGNED | Living in implementations/spec, not yet copied | `packages/protocol/` | Hermes spec + suites are current evidence | Claim before extracting shared schemas |
| Marketing surface | MCode (`minimax-code`) | Built and verified; NOT published. Static, no-build marketing site for "Walkaway × Muse Blitz — Supervised Batch Browser & Glasses Commit Gate". Hero + hero art, 3-act story accordion, security model, **THE ADVERSARY threat band** (threat-actor avatar + 6 named threat/control pairs sourced from `docs/TEST_REGISTRY.md`), film (intro/outro/demo frame), App Store asset kit with downloads, 6 paid placements as inline SVG creatives with slot filter + SVG export, test-registry proof cards, FAQ, CTA | `apps/marketing-site/` (no build step; `index.html` + `assets/`) | `python3 scripts/verify-marketing-site.py` PASS (26 local refs resolved, 4 font urls resolved, 37 banner text nodes fitted to canvas, 6 non-ascii classes covered); local render `python3 -m http.server 8099 --bind 127.0.0.1` in `apps/marketing-site/` — 0 console errors, 0 failed/4xx/5xx requests | 2026-11-04: user declined public publication at the confirmation gate; site stays local until the owner asks for a deploy. Remaining: drop the final 3-minute demo master into `apps/marketing-site/assets/video/` and point the film frame at it. KNOWN BLOCKER: no demo video file exists in the repo yet, so the film section ships as a poster frame only |

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
