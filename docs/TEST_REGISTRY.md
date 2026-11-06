# Real-system test registry

A test belongs here only when the exact command, real components, assertion,
and last observed result are known. Passing a unit test never proves a demo
hop works across devices.

## T-BROWSER-ICON — supplied app artwork (2026-11-06)

- Owner: ZCode, `feat/browser-app-icon`.
- Sources: six owner-supplied 1024px PNG exports copied unchanged into
  `apps/hermes-browser/resources/icons/`. Primary `default.png` is the blue
  Default export. Other appearance assets are preserved, not auto-switched.
- Command: `bash apps/hermes-browser/scripts/build-app-icon.sh
  /tmp/Walkaway-AppIcon.icns`; `bash -n` on builder and root DMG script.
- Real components: macOS `sips`, `iconutil`, full 16px–1024px iconset, generated
  ICNS decoded back to PNG. Assertion: valid ICNS and byte-identical 1024px
  round-trip of the supplied default. Invalid/missing input is a hard failure.
- Observed **PASS**: all six assets byte-identical to supplied exports;
  generation/round-trip and shell syntax pass. Signed local DMG rerun passes
  required icon generation with no former ICNS failure, mounted resource-byte
  comparison and `CFBundleIconFile=AppIcon`. Strict deep app signature verifies;
  mounted selftest/supervisor/UI/debugger28/native72/tabhost suites PASS.
  Command: `./scripts/build-macos-dmg.sh --release --no-notarize --identity
  'Developer ID Application: Zunftmax UG (haftungsbeschrankt) (D74F5NNMRQ)'`.
  Local artifact is signed but not notarized; no installed app overwrite.

## T-ADDRESS-IN-TAB — canonical Safari-style mode (2026-11-06)

- Owner: ZCode, existing `feat/netscape-gooey-debugger` task branch.
- User correction: address is inside each original Swift tab. Components:
  unmodified CompactTabStrip `.address` mode, original cell field/delegate,
  native bridge, Rust stable tab routing, Modern toolbar without Location row.
- Commands: release `--nativetabtest`, `--tabhosttest`, `cargo test --release`,
  `--capture-window /tmp/walkaway-address-tabs.png --exit-after 2`; signed local
  DMG mounted suites. No mocks or substitute address widget accepted.
- Acceptance: no standalone address field or Location row; tab row y74..112,
  page offset112; CmdL/select-all/Enter/Escape/clipboard use original selected
  cell; distinct URLs/drafts do not leak across tabs; Search/reload/loading
  bind stable tab IDs; supervised tabs cannot regain native editing via layout;
  native pixels and teardown remain correct, original sources unchanged.
- Observed **PASS 2026-11-06**: release build; **72 real AppKit assertions**
  in `--nativetabtest`; all **35 Cargo tests**; **10 host checks** including
  rejection of stale inactive-tab Search; actual native+Vello capture at
  `/tmp/walkaway-address-tabs.png` shows original in-tab placeholder/address,
  no Location row, native slot74..112 and page112. Vendor originals remain
  byte-identical. Signed mounted-image `--selftest`, `--supervisortest`,
  `--uitest`, `--debuggertest` (28), `--nativetabtest` (72), `--tabhosttest`
  (10) PASS. Local Developer ID/Hardened Runtime DMG is not notarized; existing
  optional icon-generation warning remains nonfatal.
- Native timing sample (not input/display latency): unchanged update CPU p95
  0.0045ms/1000, selection CPU p95 0.6489ms/100, 120Hz RunLoop interval p95
  8.392ms/max15.024ms across35 intervals. Live capture first submission345.41ms;
  no cold-cache/perceptual parity claim.

## T-NATIVE-TAB-UX — shortcut removal continuation (2026-11-06)

- Owner: ZCode, existing `feat/netscape-gooey-debugger` task branch.
- Components: ORIGINAL vendored Swift CompactTabStrip (`native-tabs/VENDOR.json`),
  NSHostingView in the existing winit NSWindow, Rust `native_tabs.rs` callback
  bridge and `window_host.rs` real tab/runtime transfers; persistent Blitz
  debugger-pane snapshot updates.
- Commands: `cargo build --release --manifest-path apps/hermes-browser/Cargo.toml`;
  binary `--nativetabtest`, `--tabhosttest`; `cargo test --release`;
  `--selftest`, `--uitest`, `--supervisortest`, `--debuggertest`; signed local
  DMG with mounted native/component/host suites added to packaging script.
- Acceptance: live macOS has exactly one native original tab strip; no Rust
  imitation or HTML regeneration in native animation. Real select/close/new,
  move/pin, transfer/detach callbacks preserve stable tab/document/runtime and
  history; protected supervised tabs never transfer approval authority.
  Native pixels are included in host captures; original tracking/hosting
  objects detach before window destruction. Debugger snapshots retain actual
  source/details scroll nodes and focus instead of reparsing shell HTML.
- Final observed **PASS 2026-11-06**: release build and all **34 Cargo tests**;
  original five Swift files byte-identical to pinned revision (build verifies
  SHA256); native component **56 real AppKit assertions**; real host-transfer
  **9 checks**; debugger **28 real backend assertions**. Includes original
  close-width freeze/release, move/pin/detach/transfer routing, native keyboard
  and visibility, actual capture/teardown, address-display/Stop cancellation.
  Native host integration capture `/tmp/walkaway-modern-native-final.png`
  succeeded with actual AppKit tab/address pixels plus Vello page/chrome.
- Final signed packaging command: `./scripts/build-macos-dmg.sh --release
  --no-notarize --identity 'Developer ID Application: Zunftmax UG
  (haftungsbeschrankt) (D74F5NNMRQ)'`. Mounted-DMG `--selftest`,
  `--supervisortest`, `--uitest`, `--debuggertest`, `--nativetabtest`,
  `--tabhosttest` PASS. Swift stdlib scan and license/provenance bundling included.
  Artifact is Developer ID/Hardened Runtime signed but explicitly NOT notarized.
  Existing optional ICNS generation still fails nonfatally.
- Timing scope: component update CPU and native RunLoop timer intervals only,
  not display/pointer latency. Final mounted run: selection CPU p95 **0.5005ms**
  (100 updates), no-op p95 **0.0001ms** (1000 updates), 120Hz timer interval
  p95 **9.082ms**, max **9.296ms** (35 intervals). No end-to-end smoothness
  parity claim. Physical gesture automation was blocked by the standalone
  process's unreadable accessibility window; no substitute automation used.
- Address continuation: original NSWindow retained NSTextField/field editor,
  real editing/selection/clipboard/IME, draft preservation, Enter/Escape,
  supervision gating, native load spinner and visibility under dropdown menus.
  Modern Location geometry fixed x64/y78/W-74/h24; no visible muse:// on start.
  Autocomplete is a future native text-change/anchor integration, no fake list.
- Startup changes: eliminate provisional/full duplicate chrome builds before
  final native/viewport initialization; report main-entry-to-first-submitted
  frame separately from actual presentation/cold filesystem cache. No unsupported
  Metal pipeline cache or claimed unmeasured speedup. Five fresh-process runs
  (`--exit-after 1`, unpurged caches) reported **390.49,476.80,336.95,273.01,
  277.03ms**; median **336.95ms**. Final native capture run **291.83ms**.
  No cold-boot/reboot or before/after speedup claim is made.

## Browser chrome / debugger verification — 2026-11-06

- Owner: ZCode, `feat/netscape-gooey-debugger`.
- Build command: `cargo build --release --manifest-path apps/hermes-browser/Cargo.toml`.
  Uses the debugger-enabled Static Hermes cache at
  `~/.cache/hermes-static_h/build-walkaway-debugger`.
- Real-component checks: `apps/hermes-browser/target/release/hermes-blitz-browser`
  with `--selftest`, `--uitest`, `--runtimetest`, `--observetest`,
  `--supervisortest`, `--debuggertest`;
  `cargo test --release --manifest-path apps/hermes-browser/Cargo.toml` for
  geometry, native rendering and backend assertions.
- Assertions: native chrome hit-testing and navigation remain real; gooey
  neck/spring geometry is finite, converges and preserves tab selection;
  debugger toolbar reserves viewport space and opens the real Hermes pane;
  interpreted runtime hits real line breakpoints, exposes stack/locals, steps,
  evaluates expressions, captures output/errors and interrupts safely.
- Scope: local scratchpad only, not AOT page debugging or Android/glasses
  verification. No mocked debugger or substituted success state is accepted.
- Last observed: **PASS 2026-11-06**. Debugger-enabled CMake/Ninja and browser
  release build pass; all six release CLI suites pass; debugger reports **28
  real backend assertions**; Cargo reports **18 passed / 0 failed** (2.47s).
  Gooey geometry separately passed 8/8, combined chrome/geometry 13/13.
- Rendering command: release binary `--render /tmp/walkaway-netscape.png`,
  `--render /tmp/walkaway-debugger.png --debugger`, and
  `--render /tmp/walkaway-dock.png --dark --menu --size 560x760`.
  Observed: actual Blitz/Vello output paints curved shoulders, beveled tools,
  recessed URL and debugger source/output pane. Native UI hit-tests verify
  editor focus, real breakpoint and viewport restoration. The open dropdown
  intentionally occludes underlying navigation tools; tests check visible
  toolbar controls and every dropdown action rather than clicking through it.
- Packaging command: `./scripts/build-macos-dmg.sh --release --no-notarize
  --identity 'Developer ID Application: Zunftmax UG (haftungsbeschrankt) (D74F5NNMRQ)'`.
  Observed: Developer ID / Hardened Runtime signatures verify; mounted-DMG
  `--selftest`, `--supervisortest`, `--uitest`, `--debuggertest` all pass.
  New DMG is **not notarized** (explicit local-only build). Existing optional
  ICNS generation logged a nonfatal failure; packaging and suites succeeded.
- Final formatting: task-owned Rust files pass `rustfmt --edition 2024
  --config skip_children=true --check` and edited shell scripts pass `bash -n`.
  Repository-wide `cargo fmt -- --check` still reports pre-existing formatting
  in untouched `observe.rs`, `page_endpoint.rs`, `recall.rs`, `vault.rs`; those
  unrelated files were not reformatted.
- Desktop automation limitation: Computer Use could list the task-owned native
  process but could not capture a readable accessibility tree/window. No
  physical GUI click verification is claimed. Native event-handler hit-tests
  and actual renderer evidence above passed; no substituted browser was used.

## Verified

### T-HERMES-SUITES

- Command: run `--selftest`, `--uitest`, `--runtimetest`, `--observetest`,
  and `--supervisortest` on the Hermes release binary.
- Real components: Hermes browser, Blitz rendering, Static Hermes AOT page,
  native observe, supervision state machine, supervision bar.
- Asserts: rendering/input foundations, tab/runtime isolation, indexed
  snapshots, proposal/decision execution, takeover/release/stop, audit and
  secret exclusion.
- Last observed: pass; supervisor suite reported 50 checks.

### T-HERMES-SOCKET-E2E

- Command: `python3 /tmp/socket_e2e.py` against the real Hermes GUI process.
- Real components: Unix socket, host/agent token roles, session, observe,
  propose, host approve, execute, stop.
- Asserts: the real counter increments once after host approval; the agent
  cannot approve its own proposal.
- Last observed: pass.
- Gap: the script lives outside the monorepo. Move or replace the script only
  under a claimed test task; do not create a second incompatible E2E.

### T-ANDROID-BUS-001

- Command: start `services/hermes-host/message_bus.py`, connect the installed
  Android bridge, POST one message to `/messages`, then inspect `/acks` and
  Android Notification Manager state.
- Real components: laptop bus, ADB reverse transport, installed Android app,
  Android Notification Manager.
- Asserts so far: message #1 was received by Android; Android returned a
  `received` ack; Notification Manager recorded a high-importance
  notification with Approve and Reject actions.
- Last observed: partial pass.
- Not yet asserted: the notification appeared on the glasses, and a physical
  Approve action returned to the laptop. The user had the glasses off during
  message #1.
- Current blocker: port `8899` collides with a pre-existing unrelated HTTP
  server. Walkaway moved to `19099`; message test 2 was received and acked by Android at 2026-11-04T19:58:49Z, with a high-importance notification recorded. Physical glasses appearance/action is still unconfirmed.


### T-GLASSES-DAT-REFERENCE-001

- Command: install the existing Ambient Link debug APK, connect Meta AI until
  green, then broadcast
  `com.lowkey.ambientlink.DEBUG_HUD_YANK` to package
  `com.lowkey.ambientlink`.
- Real components: Ambient Android app, Meta Wearables DAT SDK registration,
  Meta AI connection, paired Ray-Ban Display glasses.
- Asserts: DAT session and display reach `STARTED`; display request is sent;
  display content acknowledgement is received; the user physically sees the
  card on the glasses.
- Last observed: pass on 2026-11-04. Logs showed `createSession`,
  `addDisplay SUCCESS`, `display.state -> STARTED`,
  `Display request sent successfully`, and `Display content ack received`;
  the user confirmed “Saw the card.”
- Boundary: this proves the registered Ambient app's Android-to-glasses DAT
  hop. It does not prove Walkaway Bridge can push cards, because Walkaway has
  different package/credentials and no DAT integration yet. No Ambient sync,
  relay, or mesh code was copied.

### T-CONNECTOR-TOOLS-LIST

- Command: authenticated MCP `tools/list` POST to
  `https://muse.secure.build/mcp`.
- Real components: Caddy, hosted connector service, connector bearer auth.
- Asserts: HTTP 200 and a tools result.
- Last observed: pass. The user then saw six tools listed in Meta's form.


### T-MARKETING-SURFACE-001

- Command: `/tmp/fontenv/bin/python scripts/verify-marketing-site.py` from the
  repo root (fontTools 4.68.0 + brotli 1.2.1 required for the metric checks;
  plain `python3` runs the asset checks only and warns).
- Real components: `apps/marketing-site/index.html`, `assets/css/site.css`,
  the four shipped `assets/fonts/*.woff2` faces, and all 16 generated
  creatives in `apps/marketing-site/assets/posters/` and
  `apps/marketing-site/assets/banners/`.
- Asserts: every local `src`/`href` in `index.html` exists on disk; every
  `@font-face` URL resolves; each creative's `viewBox` matches the intrinsic
  size advertised for it in `index.html`; no `<text>` node overflows its
  canvas horizontally or vertically, measured with the real advance widths of
  the shipped faces and per-glyph fallback through the declared font stack;
  every non-ASCII character on the page and in the creatives is covered by at
  least one shipped font.
- Last observed: pass — 61 local references resolved, 4 font URLs resolved, 16
  creative viewBoxes matched, 89 creative text nodes fitted, 2 non-ASCII
  classes covered. Negative control: inflating one display line's `font-size`
  by 1.6x makes the script exit 1 naming the offending label
  (`'A LOCAL' (width 1888 > available 1800)`).
- Scope: static-asset integrity for the marketing surface only. It is not part
  of the demo path and proves nothing about a device hop. The browser-side
  check is a local render at `http://127.0.0.1:8099/` (preview server,
  `python3 -m http.server 8099 --bind 127.0.0.1` rooted at
  `apps/marketing-site/`): in-browser console query returned 0 entries and the
  in-browser network query returned 44/44 requests at HTTP 200 with 0
  failed/4xx/5xx, and the paid-placement filter was verified to show only the
  Square creatives when the Square tab is active.
- Responsive overflow sweep: the page is loaded in a same-origin iframe of
  fixed width (360, 390, 480, 768, 1024, 1280, 1600 CSS px) and every
  element's `scrollWidth`/`clientWidth` and bounding right edge are compared
  against the frame width. Last observed: zero overflowing elements and
  `documentElement.scrollWidth == clientWidth` at all seven widths (re-verified
  after the copy, artwork and mobile pass, and again after the mascot family and
  the baked hero square were added). Display
  type is sized by container-query units (`cqw`) from the headline's measured
  Druk Wide advance width, so it scales to its container instead of being
  guessed; measured hero size at a 390 px viewport is 33.02 px
  (0.94 × 346 px ÷ 9.851 em).


- Browser-measured creative fit: all 16 creatives are loaded into Chrome and
  every `<text>` node is measured with `getComputedTextLength()` and
  `getBBox()`. Last observed: 89 text nodes measured, 0 nodes with ink outside
  the artboard canvas in either axis (the largest deviations are 3-5 px of
  ascender / side-bearing ink past the declared `data-box`, which is ink
  overhang, not overflow). This is a real browser engine check, not a model of
  the font metrics.
- Hero artwork build: `/tmp/fontenv/bin/python scripts/build-hero-art.py` from
  the repo root (Pillow required). Real components:
  `apps/marketing-site/assets/download/hero-winged-chips.png` (source of the
  real Hermes browser crop), `muse-mascot.png`, `muse-chips.png`,
  `mark-walkaway.png`. Asserts: the script writes `hero-muse.png` (1920x1080),
  `hero-muse-flight.png` (1920x1080), `og-walkaway.png` (1200x630) and
  `square-muse.png` (1080x1080) into `assets/download/`, and every hero PNG
  re-rendered in Chrome shows the real Hermes window with the mascot and chips
  and no overlap on the baked green supervision square. Last observed: pass,
  6 files written, all four heroes re-rendered in Chrome without overlap.


### T-WALKAWAY-DAT-001

- Command: build/install Walkaway Bridge, start the laptop bus on `19099`,
  connect Android, POST a message to `/messages`, then inspect `/acks`,
  Walkaway DAT logs, and the physical glasses.
- Real components: laptop bus, Walkaway Android app, Walkaway Meta DAT
  integration, Meta AI registration, paired Ray-Ban Display glasses.
- Earlier observed (glasses not worn): partial. Display-session creation
  reached `STARTING`, then returned `SESSION_ENDED_BY_DEVICE` and
  `NO_ELIGIBLE_DEVICE` before `STARTED`; Meta companion device state reported
  `peerConnectedState=Disconnected` and `isMounted=false`.
- Last observed: **PASS (P1 Hard Gate 1)** on 2026-11-04 with glasses worn and
  neural band connected. Bus message #7 (`P1 Gate: Approve`): Android
  `received` 00:40:46Z, `dat_sent` 00:40:49Z, logcat `session.state=STARTED` +
  `display.state=STARTED` + `DAT card sent`, physical glasses ack
  `{"message_id": 7, "action": "approve", "source": "glasses_dat"}` at
  00:40:55Z. Bus message #8 (`P1 Gate: Reject`): physical glasses ack
  `{"message_id": 8, "action": "reject", "source": "glasses_dat"}` at
  00:41:26Z. Prerequisite found during the run: the bus must not renumber
  message ids below the Android persisted `last_message_id` cursor (was 6);
  `message_bus.py` now persists NEXT_ID (`WALKAWAY_BUS_STATE`).

### T-GLASSES-WEB-REDIRECT-001

- Command:
  `apps/glasses-web/install-debug.sh --name Walkaway --url https://walkaway.secure.build/app/`.
- Note 2026-11-05: `apps/glasses-web/` deleted as vapor stub; this entry is historical (script recoverable from git history).
- Real components: ADB, Pixel, Meta AI (`com.facebook.stella`).
- Last observed: deep link launched successfully and Meta AI became the
  focused app. In-app confirmation and glasses-side installation were not
  physically asserted.

## Required before any demo claim

### T-GLASSES-DELIVERY-001

- Real components: laptop bus on the recorded Walkaway port, installed
  Android bridge, Pixel notification system, paired Ray-Ban Display glasses.
- Asserts: a new laptop message causes a Walkaway DAT card on the glasses; a physical Approve or Reject action reaches the laptop `/acks` log.
- Status: **PASS** on 2026-11-04 (see T-WALKAWAY-DAT-001: #7 Approve + #8
  Reject, both `source="glasses_dat"`). Ordinary Android notification posting
  remains a non-accepted delivery path; DAT is the accepted path.

### T-DEMO-VERTICAL-001 (Slice 1 — One-Shot Account Creation & Blind Vault Commit)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_slice1_signup.py`
- Real components: `services/demo-fixtures/signup_fixture.py` (localhost HTTP + SQLite backend), `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser` (`--control-socket`, `src/vault.rs`, `src/page_endpoint.rs`, `src/observe.rs`), real host and agent socket channels.
- Asserts:
  1. Initial RED failure recorded before Slice 1 implementation (`AssertionError: 'unsupported' != 'supported'` on `capabilities.text_input` / `forms`).
  2. Staging `fill_form` (including `action: "fill_secret"` from `vault.rs`) does not submit the form (`0` rows in SQLite before `submit` approval).
  3. Host approval of `submit` creates exactly `1` account row in SQLite with the exact speaker/CFP fields (`Ryan MacArthur`, `ryan@zunftmax.secure.build`, `Containment-First Sandboxing: What Firecracker Leaves as Homework`, `sandboxing`, `live_demo="yes"`, `confirmation_ref="yoink=926849de55"`).
  4. The SHA-256 of the generated 24-char secret in `HERMES_VAULT_PATH` matches `password_sha256` stored by the server, and the plaintext password appears nowhere in `observe.result`, proposals, events, `HERMES_AUDIT_PATH`, stdout, or stderr.
- Last observed: **PASS** (2 consecutive runs: `1.210s` and `1.426s` on 2026-11-04).

### T-DEMO-VERTICAL-002 (Slice 2 — Interactive Form Navigation & Self-Correcting Validation via Real Local Qwen3-4B)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_slice2_validation_and_llm.py`
- Real components: `services/demo-fixtures/signup_fixture.py` (`/cfp-socon` + SQLite `cfp_submissions` table), `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`, `services/agent-driver/hermes_agent_driver.py`, real local `mlx-community/Qwen3-4B-Instruct-2507-4bit` running via MLX on Apple Silicon GPU.
- Asserts:
  1. Initial RED failure recorded before Slice 2 implementation (`ModuleNotFoundError: No module named 'hermes_agent_driver'`).
  2. Submitting an incomplete CFP form (missing required `tshirt_size` and with `< 40`-char `abstract_summary`) commits `0` rows to `cfp_submissions` and renders `#validation-error` in the live Blitz DOM.
  3. `hermes_agent_driver.plan_validation_repair_with_local_llm` observes the validation error in Hermes and executes real local `mlx-community/Qwen3-4B-Instruct-2507-4bit` inference (`llm_calls == 1`) to synthesize the corrective `fill_form` operation (`tshirt_size="L"`, `len(abstract_summary) >= 40`).
  4. Host approval of the repaired `fill_form` + `submit` commits `1` verified row in `cfp_submissions` (`confirmation_ref="socon2027=cfp-accepted"`).
- Last observed: **PASS** (2 consecutive runs: `3.846s` and `3.455s` on 2026-11-04).

### T-DEMO-VERTICAL-003 (Slice 3 — Zero-LLM FormRecipe Recall & Instant Sign-In / Replay)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_slice3_recall_and_signin.py`
- Real components: `services/demo-fixtures/signup_fixture.py` (`/signup` + `/signin` + SQLite `accounts` & `logins` tables), `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser` (`src/recall.rs` + `src/vault.rs`), `services/agent-driver/hermes_agent_driver.py`.
- Asserts:
  1. Initial RED failure recorded before Slice 3 implementation (`AssertionError: None != 'supported' : Recall capability missing`).
  2. After initial supervised sign-in on `/signin`, `HERMES_RECALL_PATH` persists a `FormRecipe` keyed by `(origin, form_signature)` containing zero plaintext secrets.
  3. On repeat visit to `/signin`, `observe.result` exposes `form_signature` and `matched_recipe_id`; `hermes_agent_driver.plan_recall_or_llm` selects `{"kind": "replay_recipe"}` with `llm_calls == 0` in `< 5ms`.
  4. Host approval of `replay_recipe` + `submit` stages `#email` + blind-injects the vault password into `#password` and records a second authenticated login row in SQLite (`len(logins) == 2`), with zero secret leaks across `HERMES_RECALL_PATH`, `HERMES_AUDIT_PATH`, socket frames, stdout, and stderr.
- Last observed: **PASS** (2 consecutive runs: `2.006s` and `2.047s` on 2026-11-04).

### T-DEMO-VERTICAL-FULL (All 3 Acts Continuous Session + `message_bus.py` Approval Routing)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_full_storyline_vertical.py`
- Real components: `services/demo-fixtures/signup_fixture.py`, `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`, `services/agent-driver/hermes_agent_driver.py`, real local `mlx-community/Qwen3-4B-Instruct-2507-4bit`, and `services/hermes-host/message_bus.py`.
- Asserts:
  1. Executes Act 1 (`/signup` account creation + blind vault commit), Act 2 (`/cfp-socon` validation error + real local `Qwen3-4B-Instruct-2507-4bit` self-correction), and Act 3 (`/signin` initial login + `<5ms` zero-LLM `FormRecipe` recall replay) in one continuous Hermes session, completing with `task.done` (`state == "done"`).
  2. Routes all 13 proposal approvals through `services/hermes-host/message_bus.py` (`POST /messages` -> `GET /messages/next` -> `POST /ack` -> host `decision: "approve"`), asserting `len(acks) == 13` and SQLite counts `accounts == 1`, `cfp_submissions == 1`, `logins == 2`.
- Last observed: **PASS** (2 consecutive runs: `4.319s` and `4.485s` on 2026-11-04).


### T-BRIDGE-E2E-001 (P2 Decision Bridge — Socket-Level Real-System E2E)

- Command: `python3 /Users/mac/walkaway-monorepo/services/hermes-host/test_decision_bridge_e2e.py`
- Real components: release Hermes binary (`--control-socket`), SQLite demo
  fixture, `message_bus.py`, `decision_bridge.py`. Bus acks with
  `source="glasses_dat"` stand in for the physical tap exactly as the Android
  bridge emits the tap; the physical tap itself is T-WALKAWAY-DAT-001.
- Asserts:
  1. `requires_glasses == false` proposal is policy-approved by the bridge
     (`decided_by=policy`, `surface=commit_gate`) with no bus card created.
  2. `received` / `dat_sent` acks and forged `approve` acks from
     `android_app` and `e2e-bus-bridge` sources produce no Hermes execution.
  3. Glasses `reject` yields `proposal.invalidated` (`reason=rejected`) and
     `accounts == 0`; a re-proposed submit approved from `glasses_dat`
     commits exactly once (`accounts == 1`); a replayed glasses ack decides
     nothing twice.
  4. Held mode with 2 Hermes processes: a tampered batch manifest returns
     409; one explicit 2-proposal manifest card + one glasses approve fans
     out to two bound decisions on two sockets (`accounts == 2`).
- Last observed: **PASS** (2 consecutive runs: 8.205s and 8.291s on
  2026-11-04). Regression: `test_slice1_signup.py` still PASS (1.424s).

### T-BRIDGE-PHYSICAL-001 (P2 Decision Bridge — Full Physical Chain)

- Command: start bus on 19099, Android bridge connected, then
  `python3 /tmp/p2_physical.py` (real Hermes + bridge immediate mode +
  fixture `/signup`); wearer approves both DAT cards on the glasses.
- Real components: everything in T-BRIDGE-E2E-001 plus the physical Android
  DAT hop and worn Ray-Ban Display glasses.
- Asserts: `executed` receipts for `p2-fill` and `p2-submit` in bridge
  `GET /decisions`, and one account row in SQLite.
- Earlier observed: partial. Bridge bound card #9 to the real `p2-fill`
  proposal; bus recorded `received` 00:46:33Z and `dat_sent` 00:46:36Z; the
  run then timed out waiting for the glasses tap (150s window), so no
  `executed` receipt and `accounts == 0`.
- Last observed: **PASS** on 2026-11-04 (rerun with 600s windows, wearer
  tapping both cards). Card #10 (`p2-fill`): `dat_sent` 01:17:11Z, physical
  ack `approve`/`glasses_dat` 01:21:57Z. Card #11 (`p2-submit`): `dat_sent`
  01:22:01Z, physical ack `approve`/`glasses_dat` 01:22:10Z. Bridge logged
  `receipt executed` for both proposals; Hermes audit records both as
  `result=executed, decided_by=user, surface=glasses` (01:21:57.695Z and
  01:22:10.881Z); SQLite `accounts == 1` (`Ryan MacArthur`,
  `ryan@zunftmax.secure.build`). Script result: `P2 PHYSICAL CHAIN PASS`.

### T-ZONE-X-SUPERPOWERS (Zone X — Headed Commit-Only Gating, Semantic X-Ray HUD, and P-256 WebAuthn Vault)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_zone_x_engine_superpowers.py`
- Real components: `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser` (`--control-socket`, `--supervise-commits-only`, `--xray`, `src/vault.rs`, `src/chrome.rs`), `services/demo-fixtures/signup_fixture.py`.
- Asserts:
  1. Under `--supervise-commits-only`, routine non-secret `fill_form` auto-executes via `commit_gate` (`requires_glasses=False`) without blocking on a host decision.
  2. `fill_secret` and `submit` emit `proposal.pending` with `requires_glasses=True` and hard-freeze before commit (`0` rows in SQLite before approval).
  3. Rejecting `fill_secret` destroys the proposal and creates no vault credentials or WebAuthn signatures.
  4. Approving `fill_secret` generates a 24-char secret plus NIST P-256 keypair in `HERMES_VAULT_PATH`; the signed WebAuthn assertion (`authenticatorData || SHA256(clientDataJSON)`) passes RP verification (`/api/webauthn/verify`), while wrong origin and replayed single-use challenges return HTTP 400.
  5. Live-window screenshot (`include_screenshot=True`) renders the Semantic X-Ray overlay and telemetry bar (`#xray-telemetry-bar`, `#xray-vault-shield`, `#xray-freeze-shield`); zero secrets or private keys leak anywhere.
- Last observed: **PASS** (`1.799s` on 2026-11-04; `--supervisortest` 54/54 checks PASS).

### T-ZONE-Y-10PORTALS-DISCO (Zone Y — 10 Conference Portals, Disco 10-Viewport Workbench, WebAuthn RP, and Reset)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_zone_y_10portals_and_disco.py`
- Real components: `services/demo-fixtures/signup_fixture.py`, `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`.
- Asserts:
  1. All 10 conference portals (`/cfp/1`..`/cfp/10`) render inside Hermes with identical `FormSignature` (`sig_c31d6574c15e07f6`); Portal #1 enforces the `< 40`-char abstract + `tshirt_size` curveball (`#validation-error`) while Portals #2–#10 accept replayed `FormRecipe` submissions.
  2. `/disco-workbench` renders all 10 live tiles (`#tile-1`..`#tile-10`) inside Blitz with non-zero bounding rects and live-measured timings/telemetry.
  3. `/api/webauthn/{challenge,register,verify}` verifies real OpenSSL ECDSA P-256 signatures and rejects wrong RP, wrong challenge, and replayed assertions.
  4. `/campaign-receipt` and `POST /api/reset` verify 10/10 submissions and `<1s` deterministic state reset.
- Last observed: **PASS** (`4.871s` on 2026-11-04).

### T-ZONE-Z-10PORTAL-CAMPAIGN (Zone Z — Full 10-Portal Disco Campaign, Resident Qwen3-4B, 3-Tap Glasses Coordinator & Reset Gate)

- Command: `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_10portal_disco_campaign.py`
- Real components: `services/agent-driver/parallel_campaign_driver.py`, `services/agent-driver/reset_demo.py`, `services/demo-fixtures/signup_fixture.py`, `services/hermes-host/message_bus.py`, `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`, real local `mlx-community/Qwen3-4B-Instruct-2507-4bit`.
- Asserts:
  1. Executes the full 10-portal campaign (`SoCon 2026` through `QCon SF`) with resident `Qwen3-4B-Instruct-2507-4bit` (`total_llm_calls == 1` across all 10 portals; Portals #2–#10 replayed via `FormRecipe` with `0` LLM calls).
  2. Emits exactly 3 glasses cards to `message_bus.py` (`Card #1` Passkey/Vault Auth, `Card #2` Portal #1 Repaired CFP Commit, `Card #3` Batch Commit 9 Conference CFPs carrying the explicit 9-portal `batch_proposals` manifest).
  3. Ignores non-glasses acks (`received`, `dat_sent`, `dat_failed`) and duplicate acks; rejecting Card #3 leaves Portals #2–#10 uncommitted (`total_submissions == 1`), while approving Card #3 commits all 10 portals (`total_submissions == 10`).
  4. `reset_demo.py --reset-demo` resets SQLite, workbench state, vault, recall, and audit logs in `~3ms` (`< 2000ms` gate), with zero secret or private-key leaks.
- Last observed: **PASS** (3 tests in `16.695s` on 2026-11-04; full 9-test discovery suite `python3 -m unittest discover -s tests/e2e -p "test_*.py" -v` PASS in `37.271s`).


### T-TRANSPORT-FREEZE-001 (P3 Frozen USB Transport + Recovery)

- Command: `adb reverse --remove tcp:19099`, then
  `services/hermes-host/start-transport.sh`, then POST one bus message.
- Real components: laptop bus on 19099, USB `adb reverse`, installed
  Walkaway Bridge on Pixel `67080DLDV009AD`.
- Asserts: script restores `tcp:19099 -> tcp:19099`, reports the single
  device and installed app, and the next message is `received` by Android.
- Last observed: **PASS** 2026-11-04. Message #12 `received` ~0.5s after
  posting; `dat_failed` followed with `SESSION_ENDED_BY_DEVICE` because the
  glasses were not worn (eligibility signal, not a transport failure).
- Frozen path: USB + adb reverse only. LAN procedure is documented in
  `services/hermes-host/README.md` as UNVERIFIED and requires a P1 re-run
  on LAN before any demo use.

### T-ANDROID-P4-001 (P4 4-State Demo-Safe UI + Dedupe)

- Commands: `./gradlew :app:testDebugUnitTest` and
  `./gradlew :app:connectedDebugAndroidTest` in
  `apps/android-notification-bridge/`, plus install/launch on the Pixel.
- Real components: `BridgeState.kt` (phase derivation + `IdDedupe`),
  `BridgeStatusPanel.kt`, rewired `BridgeScreen.kt`, laptop bus on 19099.
- Asserts:
  1. Unit: all 4 `BridgePhase` mappings; dedupe marks each id exactly once
     across instances over the same persisted set; eligibility hint mapping
     (`SESSION_ENDED_BY_DEVICE` / `NO_ELIGIBLE_DEVICE`).
  2. Instrumented: all 4 phase labels render; no `Approve`/`Reject` nodes
     exist in the panel composition.
  3. On-device: real app shows `DISCONNECTED`, then `CONNECTED — WAITING`
     after Connect; `Latest card #13` renders with zero phone decision
     buttons; after the test-run data wipe, bus backlog #9-#13 was carded
     exactly once each (one `dat_failed` per id in the new install).
- Last observed: **PASS** 2026-11-04 (unit + instrumented BUILD SUCCESSFUL;
  device states read via UIAutomator). `CARD_ON_GLASSES` and `DECIDED` on
  the real screen are NOT yet observed: DAT re-registration (wiped by the
  instrumented run) plus a worn-glasses tap are required; the phase logic
  and labels for those states are covered by the unit + instrumented tests.
- Cleanup in the same change: the unreachable template stack
  (`Navigation.kt`, `NavigationKeys.kt`, `ui/main/*`, `data/*` + template
  tests) was deleted; `MainActivity` renders `BridgeScreen` directly and
  nothing referenced the template stack except its own tests.


### T-LIVE-DEMO-001 (Full 10-Portal Physical Campaign)

- Command: `docs/DEMO_RUN.md` sequence (bus 19099 + fixture 8795 + Hermes
  `--control-socket` + `parallel_campaign_driver.run_10portal_campaign`),
  wearer approving glasses DAT cards. Final completing run used Hermes
  socket `muse_blitz.sock` with DB `live_demo.sqlite3` in
  `/var/folders/8j/5mwkcdz55xbcg1pg_54tlcf00000gn/T/walkaway-live-demo-psa_o5bz/`.
- Real components: release Hermes (primary + 8 workers), SQLite fixture
  (10 portals + disco workbench), resident Qwen3-4B, message bus 19099,
  Walkaway Android DAT bridge, worn Ray-Ban Display glasses.
- Last observed: **PASS** 2026-11-04 19:15 PT. `campaign_meta=COMMITTED`,
  `llm_calls=1`, `accounts == 1`, `cfp_submissions == 10`, 11 executed
  receipts with `decision_surface=glasses_dat`, bus `approve/glasses_dat`
  acks #27-#32. Earlier Lane P attempts in the same hour failed honestly:
  30s CLI tap-window timeout after a dismissed Card 2, and a
  `ConnectionResetError` when the bus restarted mid-run.


### T-SALVAGE-WS1WS2 (Salvage WS1 Dynamic Attach & Unified Campaign Driver + WS2 Bus/Stack Single-Instance)

- Commands:
  - `python3 services/hermes-host/test_decision_bridge_e2e.py` (3 cases incl. dynamic-attach case)
  - `python3 services/hermes-host/test_message_bus_lock.py`
  - `python3 tests/e2e/test_10portal_disco_campaign.py` (3 cases routing 100% of host supervision through `decision_bridge.py` `19101` held mode)
- Asserts:
  1. Bridge boots with zero sockets; `POST /sockets` attaches real Hermes host channels (primary + 8 parallel workers, HTTP 201 + live `session_id`), duplicate attach is 409, intermediate `stage_only: true` fills/probes are auto-approved by bridge policy (`requires_glasses=false`), and the 11 commit proposals pause in `GET /pending`, emit 3 glasses cards via `POST /cards`, and return verbatim `executed` / `proposal.invalidated` receipts (`webauthn_*`, `before/after_fingerprint`) via `GET /decisions`.
  2. Bus: a second instance on the same port via `0.0.0.0` (the 19:07 dual-bind shape) exits non-zero on the per-port flock (`/tmp/walkaway-bus-<port>.lock`); first bus untouched.
  3. Campaign driver + `HighBacklogThreadingHTTPServer` (`request_queue_size=128`): Card #3 fanout across 9 concurrent worker browsers commits all 10 SQLite CFP rows (`cfp_submissions == 10`), while rejecting Card #3 leaves Portals #2–#10 uncommitted (`cfp_submissions == 1`).
- Last observed: **PASS** 2026-11-04 21:06 PT (`test_decision_bridge_e2e.py` 3/3 in `9.061s`; `test_message_bus_lock.py` 1/1 in `0.172s`; `test_10portal_disco_campaign.py` 3/3 in `14.466s`; full 9-test `tests/e2e` discovery suite 9/9 in `29.783s`; `--supervisortest` 54/54 PASS).

### T-SALVAGE-WS3 (Android Hardening — Device Verification)

- Commands: `./gradlew :app:testDebugUnitTest :app:assembleDebug
  :app:installDebug :app:connectedDebugAndroidTest` in
  `apps/android-notification-bridge/` + ADB-driven checks on Pixel
  `67080DLDV009AD` against the live 19099 bus.
- Last observed: **PASS** 2026-11-05 07:23-07:26 PT.
  1. Install/launch: `installDebug` (upgrade in place, DAT registration
     preserved), auto-connect shows `CONNECTED — WAITING`.
  2. Foreground service: `dumpsys activity services` shows
     `BridgeKeepAliveService isForeground=true foregroundId=4101`
     (channel `walkaway_bridge_service`).
  3. Background polling: with the launcher in front, bus message #45 was
     `received` ~0.2s after posting; process state `cur=FGS cached=false
     isFrozen=false` (Android 15 freezer defeated while connected).
  4. Resend: #45 `dat_failed` (glasses not eligible) surfaced
     `Resend card #45`; tapping Resend produced a second `dat_failed`
     attempt for #45 (dedupe `forget` works; Resend re-offered).
  5. Instrumented: `:app:connectedDebugAndroidTest` BUILD SUCCESSFUL
     (panel: 4 phase labels, zero Approve/Reject nodes). Unit:
     `BridgeStateTest` 8/8.
- Aftermath (expected): the instrumented run uninstalls the app, so DAT
  registration read `AVAILABLE devices=0` after reinstall. One user
  re-registration via Connect Meta AI is owed before any glasses card.
- Not exercised (needs worn glasses): live DAT session REUSE path
  (both-STARTED predicate is unit-covered) and dismissal-after-sent
  `onCardGone` (send-failure Resend path above IS device-proven).

### T-ALIGNMENT-KILL-001 (Dead Controls + Vapor Folders Removal)

- Commands: `./gradlew :app:testDebugUnitTest :app:installDebug` in
  `apps/android-notification-bridge/` + ADB verification on Pixel
  `67080DLDV009AD` against live bus 19099.
- Last observed: **PASS** 2026-11-05. Notification for bus #46 carries zero
  actions (`dumpsys notification`); receiver + `notification_action` ack
  path deleted (no source references remain). Fresh-install blank bus
  token fails visibly (`HTTP 401` in `walkaway.poll` log + status);
  seeding `shared_prefs/walkaway_bus.xml` restored polling (#46
  `received` + `dat_failed`, DAT then `AVAILABLE`). Vapor folders
  `apps/glasses-web`, `apps/phone-approval-web`,
  `apps/desktop-supervision`, `packages/*` deleted with zero code
  references; STATUS/source-map updated same change. Diagnostic poll
  logging that found the 401 was reduced to effect-start + failure lines.

### T-TRACKB-DUAL-001 (Track B Dual-Surface Approval: Glasses OR Phone)

- Commands: `HERMES_HIDDEN_WINDOW=1 python3 services/hermes-host/test_decision_bridge_e2e.py`
  (4 cases) and `HERMES_HIDDEN_WINDOW=1 python3 /tmp/trackb_phone_e2e.py`
  (real Pixel UI taps) plus
  `apps/android-notification-bridge/scripts/run-device-tests-preserve-data.sh`.
- Last observed: **PASS** 2026-11-05.
  - Bridge suite 4/4 (12.738s): glasses-only mode still rejects
    `android_app`; dual mode commits from `android_app` and from
    `glasses_dat`, first tap wins (loser ack ignored), domain-first card
    title <=80 / body <=160, Hermes audit surfaces asserted per proposal.
  - Phone E2E: `TRACK B PHONE SURFACE PASS` — cards #47/#48 decided by
    tapping the real **Approve on Phone** Compose button via ADB input
    with glasses unused (DAT unavailable); `accounts == 1`; audit +
    bridge surfaces both `android_app`.
  - Preserve-data runner: instrumentation `OK (1 test)`,
    `firstInstallTime` unchanged, `dat_registration.xml` intact — device
    tests no longer wipe Meta DAT registration. Identity pin:
    `scripts/verify-app-identity.sh` PASS (package + cert SHA-256
    `abdf2c3a...`, keystore created 2026-05-02, unchanged).
- Not exercised: phone-dismisses-live-DAT-card sync with glasses worn
  (`dismissActiveCard` shipped; needs a worn-glasses pass).

### T-TRACKB-DUAL-002 (Track B Phone Approve + Reject, separate live cards)

- Command: `HERMES_HIDDEN_WINDOW=1 python3 /tmp/trackb_phone_reject_e2e.py`
  (throwaway harness over the maintained bridge/Hermes/Android surfaces;
  real Compose buttons tapped through ADB input on Pixel `67080DLDV009AD`).
- Last observed: **PASS** 2026-11-05. Card #52 fill approved on phone;
  card #53 rejected on phone (`proposal.invalidated`, bridge decision
  `reject`/`android_app`, audit `result=rejected surface=android_app`,
  accounts stayed 0); card #54 re-proposed and approved on phone
  (`executed`, accounts became 1). Per-tap responsiveness was sampled over
  successive post-tap UI frames: input dispatch 146-152 ms, with the second
  button-absent frame observed at 2,598-2,649 ms under the 250 ms dump cadence;
  screenshots in `/tmp/trackb_reject/` show both live phone buttons and the
  final `Card #54 decided via android_app: approve` state. No product code
  changed during the verification rerun; one over-broad harness assertion was
  narrowed from all audit rows to the audit row whose result is `rejected`.

### T-MUSE-MCP-REAL-BROWSER (Track A — Muse MCP -> Real Hermes Browser + Live HTTPS Webpage + Glasses/Phone Dual Approval)

- Command: `HERMES_HIDDEN_WINDOW=1 python3 /Users/mac/walkaway-monorepo/tests/e2e/test_muse_mcp_real_browser.py`
- Real components: `services/muse-connector/mcp_server.py`, `services/agent-driver/hermes_task_relay.py` (`:8790`), `services/agent-driver/hermes_agent_driver.py` (`plan_general_web_form_with_local_llm` + local `mlx-community/Qwen3-4B-Instruct-2507-4bit`), `services/hermes-host/decision_bridge.py` (`--allow-android-approval`), `services/hermes-host/message_bus.py`, and release `apps/hermes-browser` (`--control-socket`, hidden window).
- Asserts:
  1. `test_muse_mcp_drives_real_hermes_browser_and_commits_on_glasses_or_android_approval`: Invokes `mcp_server.py` JSON-RPC `tools/call` (`walkaway_start_task` with `url` + `goal`, `walkaway_get_state`), verifies real Hermes navigates, stages form fields via local `Qwen3-4B` (`llm_calls == 1`), pauses at `waiting_for_human` with a domain-first card (`<=80` title, `<=160` body), and commits only after either a `glasses_dat` (`surface="glasses"`) or `android_app` (`surface="android_app"`) approval ack (`0` rows before approval, `1` row after approval).
  2. `test_live_internet_https_httpbin_form_via_muse_mcp`: Drives live external `https://httpbin.org/forms/post` over HTTPS through `mcp_server.py` (`walkaway_start_task`), fills customer/pizza fields via local `Qwen3-4B`, emits `COMMIT httpbin.org: ...`, approves via `source="android_app"`, and verifies the real HTTPS POST response from `https://httpbin.org/post` inside Hermes (`"custname": "Ryan MacArthur"`).
- Last observed: **PASS** 2026-11-05 (`2/2` tests in `10.704s`, 100% headless with zero visible desktop windows).

### T-FGS-CRASH-001 (BridgeKeepAliveService dataSync Crash Loop — Fix)

- Observed failure 2026-11-05 22:29 PT: `ForegroundServiceDidNotStopInTimeException`
  (Android 15 dataSync quota timeout, service never stopped itself), then
  `START_STICKY` restart into `ForegroundServiceStartNotAllowedException:
  Time limit already exhausted for foreground service type dataSync` at
  `BridgeKeepAliveService.kt:40` — app crash-loop, process dead.
- Fix: type `dataSync` -> `connectedDevice` (manifest permission + service
  type + runtime `FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE`),
  `START_STICKY` -> `START_NOT_STICKY`, `onTimeout()` stops cleanly,
  `startForeground` and companion `start()` are failure-tolerant
  (degraded polling, never crash), `onDestroy` removes foreground state.
- Last observed: **PASS** 2026-11-05 22:36 PT. Preserve-data device tests
  green; dumpsys shows `isForeground=true types=0x00000010` on pid 13565;
  crash buffer clean after reinstall + relaunch + restart attempts;
  process stable. Not exercised: full Disconnect->Connect UI cycle in
  this round (user held phone foreground; service start/stop calls are
  unchanged from the previously proven path + new onDestroy guard).
