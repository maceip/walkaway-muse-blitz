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
| Android notification bridge | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | P1 PASS: physical DAT Approve + Reject from worn glasses | `apps/android-notification-bridge/` | 2026-11-04: bus #7 `approve`/`glasses_dat` 00:40:55Z, #8 `reject`/`glasses_dat` 00:41:26Z (bus 19099, display STARTED, `DAT card sent`) | P3/P4: freeze USB path; 4-state demo-safe UI + dedupe pass |
| Laptop message bus | pebble-mirach (Partner Lane P) | Running on 19099; NEXT_ID persisted (`WALKAWAY_BUS_STATE`, default `/tmp/walkaway-bus-state.json`); `batch_proposals` manifest passthrough | `services/hermes-host/message_bus.py` | P1 cards #7-#9 delivered after seeding next_id past Android `last_message_id=6` cursor | Keep on 19099; never restart without persisted state (Android cursor skips renumbered ids) |
| Glasses web surface | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Wrapper only in monorepo; older Walkaway app exists separately | `apps/glasses-web/` wrapper; `/Users/mac/walkaway-hackathon/glasses` existing app | No monorepo glasses UI verified | Claim before creating or migrating any UI |
| Phone approval surface | UNASSIGNED | Wrapper only | `apps/phone-approval-web/` | No implementation verified | Claim before work; do not duplicate glasses approval logic |
| Desktop supervision | Browser/Fixtures Agent (`eeb06c47`) | Implemented inside Hermes | `/Users/mac/hermes-blitz-browser/src/chrome.rs` | Supervision bar exercised by `--supervisortest` and screenshot capture | No standalone desktop client unless demo requires separation |
| Muse connector | pebble-mirach (Partner Lane P) | Deployed; Meta listed six tools in user-operated form | `/Users/mac/walkaway-hackathon/connector/mcp_server.py` | Live `tools/list` with connector bearer returned HTTP 200; request logging enabled | Owner completes submission; do not create a second connector |
| Meta DAT reference path | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physical glasses card verified through existing Ambient app; not copied into Walkaway | `/Users/mac/ambient-link-meta/relay-android` (reference only) | Ambient debug APK installed; DAT session `STARTED`, display `STARTED`, display content ack received; user selected “Saw the card” | Extract only DAT lifecycle/registration learnings; do not use Ambient relay/mesh/sync |
| Walkaway native DAT | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | PHYSICALLY VERIFIED in Walkaway own app (P1 Hard Gate 1 PASS) | `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/WalkawayDat.kt` | Worn retry 2026-11-04: session/display STARTED on `Meta RB Display 003F`, cards #7/#8/#9 `dat_sent`, physical Approve (#7) + Reject (#8) acks `source=glasses_dat` | No package-authorization change needed; worn+band was the missing eligibility |
| Hermes host bridge | pebble-mirach (Partner Lane P) | P2 BUILT + socket-E2E PASS x2; physical chain rerun pending glasses tap window | `services/hermes-host/decision_bridge.py` (+ `message_bus.py`) | `python3 services/hermes-host/test_decision_bridge_e2e.py` PASS x2 (8.2s/8.3s): policy auto-approve, forged acks ignored, glasses reject blocks commit, replay no-op, 2-Hermes batch fan-out (accounts==2, tampered manifest 409). Physical: card #9 `dat_sent` 00:46:36Z from real Hermes proposal, timed out awaiting tap (no `executed`, accounts==0) | Rerun `python3 /tmp/p2_physical.py` (600s windows) with wearer tapping both cards; Zone Z consumes `GET /pending` + `POST /cards` on 19101, sees `GET /health` sessions |
| Agent driver | Browser/Fixtures Agent (`eeb06c47` — Zone Z) | Zone Z (`Z1`, `Z2`, `Z3`) COMPLETE: `parallel_campaign_driver.py` (resident `mlx-community/Qwen3-4B-Instruct-2507-4bit`, 10-portal campaign, 3-tap glasses coordinator, Tap-3 `batch_proposals` manifest) + `reset_demo.py` (`--reset-demo` in `~3ms`) | `services/agent-driver/{hermes_agent_driver.py, parallel_campaign_driver.py, reset_demo.py}` | `test_10portal_disco_campaign.py` (3/3 tests PASS in `16.695s`: `total_llm_calls==1`, 3 glasses cards, 10 SQLite submissions, Card #3 reject safety gate, `<2000ms` reset gate, `0` secret leaks) | Ready for physical glasses rehearsal with `pebble-mirach` |
| Demo fixture | Browser/Fixtures Agent (`eeb06c47` — Zone Y) | Zone Y (`Y1`, `Y2`, `Y3`) COMPLETE: `/cfp/1`..`/cfp/10`, `/disco-workbench` 10-viewport live Blitz grid (`#tile-1`..`#tile-10`), `/api/webauthn/{challenge,register,verify,assertions}` (real OpenSSL P-256 verification), `/campaign-receipt`, and `POST /api/reset` | `services/demo-fixtures/signup_fixture.py` | `test_zone_y_10portals_and_disco.py` (PASS `4.871s`) + full 9-test E2E suite (`python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` — 9/9 PASS in `37.271s`) | Frozen for rehearsal and recording |
| Protocol contracts | UNASSIGNED | Living in implementations/spec, not yet copied | `packages/protocol/` | Hermes spec + suites are current evidence | Claim before extracting shared schemas |

## Live-process conflicts and known duplicates

- Port `8899` is already occupied by a pre-existing `python3 -m http.server`
  serving `/Users/mac/labrea/agent-relay-surface/public`. Do not kill or
  replace that process. Walkaway message bus now uses `19099`.
- Existing Walkaway Chrome relay and example web processes are active from
  earlier work. They are not the Hermes execution path.
- Hosted `muse.secure.build` runs the existing connector. Local stale
  connector processes must not be treated as the deployed connector.

## Current hard stop

Do not build another browser, approval UI, connector, relay, Android bridge,
host bridge, or message bus. Resolve ownership here first, then add the
smallest real-system test for the next demo slice.
