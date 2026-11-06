# Shared status board

Last updated: 2026-11-05 13:56 PT (Track A `eeb06c47` + Track B `pebble-mirach` Fast-Track Product Alignment Plan in `alignment.md` §9 COMPLETE & E2E/DEVICE VERIFIED; recorded in `AGENT_SALVAGE_COORDINATION.md` Entries #8–#11 and `docs/TEST_REGISTRY.md` `T-TRACKB-DUAL-001`, `T-TRACKB-DUAL-002`, `T-MUSE-MCP-REAL-BROWSER`)

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

## Tab UX hardening continuation — 2026-11-06

- Owner: ZCode, existing task branch `feat/netscape-gooey-debugger`, isolated
  worktree `/Users/mac/walkaway-browser-netscape`; no other browser build or
  Swift process observed in the process table before starting.
- User acceptance: remove implementation shortcuts that degrade the original
  Swift tab strip's responsiveness and interactions. Prior passing geometry
  tests do not establish perceptual or behavioral parity.
- Claimed scope: existing browser chrome/render/input/tab integration, exact
  Swift reference parity audit, real performance/behavior tests and docs.
  No changes to Android, host services or external frozen browser checkout.
- Current milestone: auditing original Swift implementation and retained-render
  options. No performance parity claim and no automatic deletion of feature;
  owner decides whether to remove it after reviewing the improved result.
- Next action: remove whole-document animation rebuilds and other confirmed
  UX shortcuts; implement/test essential missing interactions or state gaps
  honestly. New platform integration must be recorded in source-map if needed.
- Decision: replace the partial Rust tab approximation on macOS with the
  ORIGINAL MIT Swift `CompactTabStrip` hosted in the existing window/tab row.
  Canonical source `/Users/mac/tab-strip-port` revision `f23ba18`; no external
  package dependencies. Rust owns tabs/windows, Swift owns original interaction
  and animation. Additions recorded in source-map. No native parity claim until
  real build, transfer, teardown, screenshot and behavior checks complete.
- Observed milestone: original Swift bridge builds and `--nativetabtest` PASS
  31 real AppKit assertions; `--tabhosttest` PASS9 (same runtime/document/history
  moves without reconstruction); Cargo release tests PASS26. Native animation
  no longer drives whole-document chrome reparsing. Debugger retained updates
  preserve real scroll/focus/selection across repeated snapshots (7/7 tests).
- User scope extension: Netscape 6 MODERN (not Classic), standard responsive
  address field with future autocomplete anchor/popover seam, no visible
  `muse://`, real loading indicator/Stop action, improve measured cold startup.
  Native field keeps actual AppKit editing/IME/clipboard rather than appending
  keystrokes into rebuilt HTML. Native address and Modern chrome owners are
  disjoint files under this same task claim.
- Final milestone: original native tabs/address and Netscape 6 MODERN chrome
  complete; no native animation-time HTML parser/rebuild loop. Native field
  standard editing, loading spinner, autocomplete seam, hidden internal start
  URL, cancellable Reload/Stop verified. Release Cargo **34/34 PASS**; native
  **56**, real host-transfer **9**, debugger **28** checks PASS. Final signed
  local DMG and all six mounted suites PASS; not notarized. Actual native+Vello
  screenshot captured; main-entry→first-submission final run **291.83ms**, five
  fresh-process runs median **336.95ms** (unpurged caches, no cold-boot claim).
- Next action: final diff review, task-owned commit/push, snapshot merge check.
  Owner controls acceptance/perceptual review and merging; no auto-removal.
- Known limits: Computer Use cannot read standalone accessibility window, so
  no physical gesture/perceptual parity claim. Native captures omit separate
  transient hover/drag panels; steady-state native chrome is real. Optional
  ICNS generation remains a pre-existing nonfatal failure.

## Browser chrome / debugger task — 2026-11-06

- Owner: ZCode (`feat/netscape-gooey-debugger`, isolated worktree
  `/Users/mac/walkaway-browser-netscape`). User-requested continuation of the
  completed Zone X browser work, not a second browser implementation.
- Claimed scope: `apps/hermes-browser` chrome, gooey tabs, JavaScript debugger,
  browser-specific tests and documentation. The prior Zone X demo baseline is
  complete; this task does not edit the external frozen demo checkout.
- Canonical release source for this task: `apps/hermes-browser` (the source
  built by `.github/workflows/ci.yml`). Existing Android edits in the primary
  checkout are owned by another task and remain untouched.
- Verification: `git status --short --branch`, `git worktree list`, and
  `ps -axo pid,ppid,etime,command` inspected before work. No active browser,
  cargo, DMG packaging, or notarization process observed. No server started.
- Observed result: task branch isolated at `dc57f60`; debugger-enabled Hermes
  built successfully in `~/.cache/hermes-static_h/build-walkaway-debugger`.
  Existing `build` cache and frozen external browser remain untouched.
  Native Netscape 6 Classic chrome, actual gooey neck/spring tab geometry and
  isolated Hermes source debugger are complete. Exact verification commands:
  `cargo build --release --manifest-path apps/hermes-browser/Cargo.toml`;
  `cargo test --release --manifest-path apps/hermes-browser/Cargo.toml`;
  release binary `--selftest --uitest --runtimetest --observetest
  --supervisortest --debuggertest` (each flag run separately).
  Observed: build PASS, 18/18 Cargo tests PASS, all six CLI suites PASS,
  28 real debugger backend assertions PASS. Real Vello light/dark/compact
  renders verify curved tabs and readable native debugger controls.
- Local artifact: `build/Walkaway.dmg`, Developer ID `D74F5NNMRQ` + Hardened
  Runtime, bundled debugger-enabled Hermes/OpenSSL and MIT notices. Exact
  packaging command and mounted-image checks are in `docs/TEST_REGISTRY.md`;
  all mounted checks pass. Local build is explicitly not notarized; no Apple
  submission, release publication or merge was requested/performed.
- Next action: commit/push the task-owned patch and perform an isolated merge
  assessment against freshly fetched remote main; owner controls merging.
- Known verification limitations: Computer Use cannot read this canvas window's
  accessibility tree, so no physical GUI click claim; native hit-testing and
  actual rendering passed. Existing optional ICNS generation fails nonfatally.
- Known constraint: Static Hermes AOT frames cannot support bytecode stepping.
  The real debugger is explicitly an isolated local JS scratchpad, with no DOM,
  vault, network or approval bindings and no automatic remote script execution.

## Components

| Component | Owner | State | Canonical location | Last verification | Next action |
|---|---|---|---|---|---|
| Hermes browser | Browser/Fixtures Agent (`eeb06c47` — Zone X) | Zone X (`X1`, `X2`, `X3`) + Salvage WS1/WS4 COMPLETE: `--supervise-commits-only` (`requires_glasses` classification with `stage_only` intermediate fill/probe support), `--xray` Semantic X-Ray overlay + live telemetry HUD (`layout_observe_ms`, OS `getrusage` `RSS MB`, `FormSignature`, `recalled_count`), and NIST P-256 ECDSA WebAuthn assertion signing in `src/vault.rs`; synced byte-for-byte with `apps/hermes-browser/src/` | `/Users/mac/hermes-blitz-browser` & `apps/hermes-browser` | `--selftest`, `--uitest`, `--runtimetest`, `--observetest`, `--supervisortest` (54/54 checks PASS), `test_zone_x_engine_superpowers.py` (PASS), plus full 9-test `tests/e2e` suite (9/9 PASS in `29.783s`) | Complete & verified |
| Monorepo wrapper | pebble-mirach (formerly clover-pulsar) | Scaffold created | `/Users/mac/walkaway-monorepo` | JSON manifests parse; Git status inventoried; Android describe completed | Other agent must claim components here before editing |
| Android notification bridge | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | WS3 DEVICE-VERIFIED PASS (FGS, background poll, Resend, instrumented); DAT re-registration owed after instrumented wipe | `apps/android-notification-bridge/` | 2026-11-05: installDebug + `isForeground=true` FGS, background `received` ~0.2s `cur=FGS isFrozen=false`, Resend #45 second attempt, connected tests PASS; post-test DAT `AVAILABLE` | Owner: tap Connect Meta AI once (re-register DAT) before next glasses use; live session-reuse + dismissal paths still need a worn-glasses pass |
| Laptop message bus | pebble-mirach (Partner Lane P) | P3 FROZEN + Salvage WS2 COMPLETE: USB + `adb reverse tcp:19099`; NEXT_ID persisted; `batch_proposals` passthrough; `/tmp/walkaway-bus-<port>.lock` flock + `allow_reuse_address = False` (live on `PID 12901`) | `services/hermes-host/message_bus.py` (+ `start-transport.sh`) | `test_message_bus_lock.py` PASS (`0.172s`); recovery verified 2026-11-04: tunnel removed -> `start-transport.sh` restored -> message #12 `received` ~0.5s | Demo startup is `services/hermes-host/start-transport.sh` or `run_live_demo.py`; never use port 8899 |
| Glasses web surface | pebble-mirach (Partner Lane P) | DELETED 2026-11-05 (vapor stub; native DAT is the glasses surface; recoverable from git history) | — | Deletion executed per alignment §7 Concern #2 + §6.1.2 | Rebuild only on Ryan decision (alignment §6.6.3) as real code, never as stub |
| Phone approval surface | — | DELETED 2026-11-05 (README-only stub; phone shows state only, decisions are glasses-only) | — | Deletion executed per alignment §7 Concern #2 | None |
| Desktop supervision | Browser/Fixtures Agent (`eeb06c47`) | Implemented inside Hermes (standalone stub folder deleted 2026-11-05) | `/Users/mac/hermes-blitz-browser/src/chrome.rs` | Supervision bar exercised by `--supervisortest` and screenshot capture | No standalone desktop client unless product requires separation |
| Muse connector | pebble-mirach (Partner Lane P) | Deployed; Meta listed six tools in user-operated form | `/Users/mac/walkaway-hackathon/connector/mcp_server.py` | Live `tools/list` with connector bearer returned HTTP 200; request logging enabled | Owner completes submission; do not create a second connector |
| Meta DAT reference path | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physical glasses card verified through existing Ambient app; not copied into Walkaway | `/Users/mac/ambient-link-meta/relay-android` (reference only) | Ambient debug APK installed; DAT session `STARTED`, display `STARTED`, display content ack received; user selected “Saw the card” | Extract only DAT lifecycle/registration learnings; do not use Ambient relay/mesh/sync |
| Walkaway native DAT | pebble-mirach (Partner Lane P: Transport & Human Surfaces) | Physically verified (P1/P2); registration currently AVAILABLE after instrumented-test uninstall wiped app data | `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/WalkawayDat.kt` | Registration check 2026-11-04 post-`connectedDebugAndroidTest`: `registrationState=AVAILABLE devices=0`; `dat_failed` fast path observed | NEVER run connected tests on demo day without re-registering after; owner re-registers via Connect Meta AI, confirm REGISTERED + devices=1 |
| Hermes host bridge | pebble-mirach (Partner Lane P) | P2 PASS + salvage WS1/WS2 COMPLETE: `POST /sockets` dynamic attach, bus per-port flock + reuse-addr off, integrated end-to-end with `parallel_campaign_driver.py` | `services/hermes-host/decision_bridge.py` (+ `message_bus.py`) | `test_decision_bridge_e2e.py` 3-case PASS (`9.061s`) + `test_10portal_disco_campaign.py` 3-case PASS (`14.466s`) | Complete & verified |
| Agent driver | Browser/Fixtures Agent (`eeb06c47` — Zone Z) | Zone Z (`Z1`, `Z2`, `Z3`) + Salvage WS1/WS2 COMPLETE: `parallel_campaign_driver.py` routed 100% through `decision_bridge.py` (`19101` held mode, `POST /sockets`, `GET /pending`, `POST /cards`, `GET /decisions`; duplicate host socket client deleted) + `run_live_demo.py` (`/tmp/walkaway-demo-stack.lock` flock + `--detach` mode) + `reset_demo.py` (`--reset-demo` in `~3ms`) | `services/agent-driver/{hermes_agent_driver.py, parallel_campaign_driver.py, run_live_demo.py, reset_demo.py}` | `test_10portal_disco_campaign.py` (3/3 tests PASS in `14.466s` via `decision_bridge.py` held mode: `total_llm_calls==1`, 3 glasses cards, 10 SQLite submissions, Card #3 reject safety gate, `<2000ms` reset gate, `0` secret leaks) | Complete & verified |
| Demo fixture | Browser/Fixtures Agent (`eeb06c47` — Zone Y) | Zone Y (`Y1`, `Y2`, `Y3`) + Salvage WS1 COMPLETE: `/cfp/1`..`/cfp/10`, `/disco-workbench` 10-viewport live Blitz grid (`#tile-1`..`#tile-10`), `/api/webauthn/{challenge,register,verify,assertions}`, `/campaign-receipt`, `POST /api/reset`, and `HighBacklogThreadingHTTPServer` (`request_queue_size=128`) for 9-worker fanout | `services/demo-fixtures/signup_fixture.py` | `test_zone_y_10portals_and_disco.py` + full 9-test E2E suite (`python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` — 9/9 PASS in `29.783s`) | Complete & verified |
| Protocol contracts | — | README-only `packages/*` stubs DELETED 2026-11-05; contracts live in implementations + `docs/TEST_REGISTRY.md` | implementations | Hermes spec + suites are current evidence | Recreate only with real schema files + tests |
| Marketing surface | MCode (`minimax-code`) | Built and verified; NOT published. Static, no-build marketing site for **Walkaway, a local intelligent browser**, restyled into a flat Swiss/editorial poster system (flat colour fields, Druk Wide Heavy display caps, hairline rules, zero radius, massive negative space, mono corner tags; palette `ink #000000` / `paper #EFEDE2` / `paper-2 #E3E0D2` / `green #008942` / `cyan #09B1C7` / `orange #FF9D00` / `solar #FFC400`). Sections: hero + composed artwork (real Hermes screenshot, winged Muse mascot, winged AI chips, baked green supervision square), 3-block index (Local / Reachable / Composes, each with a campaign art accent), black statement band (peace of mind in one tap / it has your secrets), film frame with the mascot flying into it, 10-poster set, 6 paid placements with slot filter, 22-cell asset kit, footer. Copy rules: no em dashes, no use of the word "portals", interruption model stated as one tap, benefit stated as peace of mind, no made-up shorthand. CTA is "See it work". Nothing animates on its own: the scroll reveal is a plain fade, and the stylesheet has no keyframe animations at all. Copy states the real product story (runs on your machine; your phone and remote assistant talk to it; unlimited access to your secrets; composes/fetches/finishes long work; trusted glasses line that asks for one tap only when it has to). **Threat-model band, threat-actor dossier, FAQ, acts accordion and proof cards were removed** — the ghost avatar is now a small unlabelled mark in "Reachable" and in the kit ("Agent mark"). The hero green supervision square is baked into the artwork (it used to be a CSS overlay that hung over the panel corner and merged with the black field) | `apps/marketing-site/` (no build step; `index.html` + `assets/`); creatives from `scripts/build-posters.py`; hero art from `scripts/build-hero-art.py` | `/tmp/fontenv/bin/python scripts/verify-marketing-site.py` PASS (61 local refs resolved, 4 font urls resolved, 16 creative viewBoxes matched, 89 creative text nodes fitted to canvas in both axes, 2 non-ascii classes covered); negative test: inflating one display line makes it exit 1 with the offending label. Live render `python3 -m http.server 8099 --bind 127.0.0.1` in `apps/marketing-site/` — in-browser console query returned 0 entries, network query returned 44/44 requests at HTTP 200 with 0 failed/4xx/5xx; placement filter verified (Square shows only the 2 square creatives); responsive overflow sweep over 360/390/480/768/1024/1280/1600 px found 0 overflowing elements and `scrollWidth == clientWidth` at every width | 2026-11-04: user declined public publication at the confirmation gate; site stays local until the owner asks for a deploy. Remaining: drop the final 3-minute demo master into `apps/marketing-site/assets/video/` and point the film frame at it. KNOWN BLOCKER: no demo video file exists in the repo yet, so the film section ships as a poster frame only. KNOWN BLOCKER: the page now says one tap, while `docs/DEMO_RUN.md` and `docs/TEST_REGISTRY.md` T-ZONE-Z still describe a 3-tap human script — flagged to the owner, not reconciled unilaterally |
| Poster & banner set | MCode (`minimax-code`) | Generated, verified, deterministic. 10 posters (1920×1080) + 6 paid placements (1200×628, 1080×1080, 1080×1920, 300×250, 1200×628, 1080×1080), flat poster style, fonts embedded as base64, one `feTurbulence` grain overlay at 0.08 opacity, no gradients/scripts/external refs. Copy follows the campaign rules: poster 04 = "PEACE OF MIND. / IN ONE TAP.", poster 10 = "NEVER SEE A / BROKEN FORM / AGAIN.", poster 08 line "FORM ID" (not "PORTAL ID"), banner 02 = "ONE TAP. / PEACE OF MIND." | `scripts/build-posters.py` → `apps/marketing-site/assets/posters/*.svg` + `assets/banners/*.svg` | `python3 scripts/build-posters.py` PASS ("generated 16 artboards, 10 posters + 6 banners", "every text node measured inside its declared box"); identical sha256 across re-runs and across all three invocation forms; accent-field collision check 0 across all 16. Fit re-checked in a real browser engine: all 16 creatives loaded in Chrome and every one of the 89 text nodes measured with `getComputedTextLength()` + `getBBox()` — 0 nodes overflow the artboard canvas | Next: export to PNG if a placement needs raster |
| Hero artwork build | MCode (`minimax-code`) | Owns the composed campaign artwork and the mascot family. `scripts/build-hero-art.py` composites the real Hermes browser crop out of `assets/download/hero-winged-chips.png` (box 230,5,1403,874) with the transparent mascot and chip cutouts, bakes the green supervision square, and writes `hero-muse.png` (1920x1080), `hero-muse-flight.png` (1920x1080, mirrored scene), `og-walkaway.png` (1200x630, also the `og:image`) and `square-muse.png` (1080x1080). Mascot family, all cut to transparent alpha: `muse-mascot`, `muse-chips`, `muse-form`, `muse-tap`, `muse-glasses`, `muse-rest`. The mascot and chips are campaign drawings; the browser window in every hero is a real screenshot | `scripts/build-hero-art.py` → `apps/marketing-site/assets/download/*.png` + `assets/img/*.webp` | `/tmp/fontenv/bin/python scripts/build-hero-art.py` PASS (6 files written, deterministic); every hero PNG re-rendered in Chrome shows the real Hermes window, the mascot and the chips with no overlap on the baked square | Next: none; extend only if the owner asks for more mascot poses |

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
