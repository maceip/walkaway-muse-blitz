# Real-system test registry

A test belongs here only when the exact command, real components, assertion,
and last observed result are known. Passing a unit test never proves a demo
hop works across devices.

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
- Last observed: **PARTIAL (not passed)** on 2026-11-04. Bridge bound card
  #9 to the real `p2-fill` proposal; bus recorded `received` 00:46:33Z and
  `dat_sent` 00:46:36Z; the run then timed out waiting for the glasses tap
  (150s window), so no `executed` receipt and `accounts == 0`. Rerun with
  the script's 600s windows and the wearer tapping both cards.

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
