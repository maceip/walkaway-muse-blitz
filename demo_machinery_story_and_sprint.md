# FULL UNABRIDGED TECHNICAL PLAN & STATE RECOVERY (`eeb06c47` + `pebble-mirach`)

> [!IMPORTANT]
> **For `pebble-mirach` (restarted session replacing `clover-pulsar`):**
> **Select Option 1: `Partner Lane P (Recommended)`.**
> You (`pebble-mirach`) own **Partner Lane P — Transport & Human Surfaces**:
> - `/Users/mac/walkaway-monorepo/services/hermes-host`
> - `/Users/mac/walkaway-monorepo/apps/android-notification-bridge`
> - `/Users/mac/walkaway-monorepo/apps/glasses-web`
>
> The other agent (`eeb06c47`) owns **Zones X, Y, Z** (`/Users/mac/hermes-blitz-browser`, `services/demo-fixtures`, `services/agent-driver`, `tests/e2e`).

---

## PART 1: Complete Amnesia Recovery — What You (`clover-pulsar` / `pebble-mirach`) Built in the Previous 2 Hours

1. **Walkaway Monorepo & Coordination Records Created:**
   - `AGENTS.md`, `docs/STATUS.md`, `docs/TEST_REGISTRY.md`, `docs/source-map.md`, `docs/demo-plan.md`, `docs/FINAL_LOCKED_PLAN.md`.
2. **Laptop → Android Message Bus (`services/hermes-host/message_bus.py`):**
   - Running on **port `19099`** (moved off `8899` because a pre-existing `python3 -m http.server` owns `8899` — do NOT touch `8899`).
   - Verified Android polling client receives messages and posts acknowledgements back to `19099`.
   - Proved ordinary Android notifications do **not** reach the glasses (`user saw nothing on glasses and heard nothing on phone`).
3. **Meta Wearables DAT DisplayAccess Proven & Implemented in Walkaway Bridge (`apps/android-notification-bridge`):**
   - Investigated `/Users/mac/ambient-link-meta/relay-android` as a read-only reference (did not copy relay/mesh/sync/dictation).
   - Used Ambient's registered app once to prove physical DAT pathway (`STARTED` session + `STARTED` display + content ack + human saw card).
   - Implemented minimal native Meta DAT path in `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/WalkawayDat.kt`:
     - Meta DAT SDK configuration, process-singleton initialization, registration/device monitoring, `startRegistration`, display-session lifecycle, two-button Approve/Reject card, laptop bus messages triggering DAT cards, physical DAT actions wired back toward laptop `/ack` (`{"action":"approve"|"reject","source":"glasses_dat"}`), SDK 35 alignment with Meta's sample, one-time failure callback fix, scroll fix for Android control screen.
   - **Verified Walkaway Bridge State on Pixel 11 Pro XL (`67080DLDV009AD`):**
     ```
     registration=REGISTERED
     devices=1
     connected=Meta RB Display 003F
     ```
   - **Remaining Physical Diagnosis:** Sessions reached `STARTING`, then returned `SESSION_ENDED_BY_DEVICE` + `NO_ELIGIBLE_DEVICE` when glasses were unmounted (`isMounted=false`) and band was disconnected (`peerConnectedState=Disconnected`). Glasses are now charged for the physical worn test.
4. **Meta AI Debug Web-App Redirect & Muse Connector:**
   - Added/tested Meta AI debug web-app redirect script (`apps/glasses-web/install-debug.sh`); launches/focuses Meta AI on Pixel.
   - Unblocked Muse MCP connector (`/Users/mac/walkaway-hackathon/connector/mcp_server.py`); verified bearer token against hosted MCP endpoint (`HTTP 200`, 6 tools listed).

---

## PART 2: Verbatim Synthesized Operating Model & Claim/Contract Locks

We keep the exact **Zone X / Zone Y / Zone Z** labels for Browser/Fixtures/Driver, and **Partner Lane P** for Transport & Human Surfaces (`pebble-mirach`).

### Four Mandatory Claim/Contract Locks:
1. **Live Measurement Lock:** Every speed/RAM number must be measured live (`Instant::elapsed`, OS `getrusage` RSS), never rendered as a static hardcoded string (`2.3ms`, `21MB`, etc.).
2. **Passkey Terminology & Verification Lock:** A software-generated P-256 assertion accepted by our fixture is not automatically equivalent to a platform/synced passkey. Before recording, Zone X states exactly which relying-party verification is implemented (`ECDSA P-256 over authenticatorData || SHA256(clientDataJSON)` verified against registered public key, `rpIdHash`, `challenge`, `origin`, `UP=1, UV=1`). On camera, say "passkey" only when that real WebAuthn registration/assertion verification runs for the relevant RP/origin; otherwise call it a vault-held software credential or vault signature.
3. **Batch Authorization Lock (Tap 3):** One glasses tap releasing 9 submissions requires an explicit batch-authorization manifest containing every `proposal_id`, `fingerprint`, `generation`, `session_id`, and `portal_id` being released.
4. **Folder Lock:** Zone Z may consume the `19099` contract, but `services/hermes-host` remains **Partner Lane P (`pebble-mirach`)'s exclusive implementation folder**.

---

## PART 3: Zone X — Native Hermes Browser Engine (Owner: `eeb06c47` Main Agent)

**Confidence:** Medium. Headless storyline slices are proven; the live headed socket loop, semantic overlay, and passkey work are gated below.

**Exclusive Files:**
- `/Users/mac/hermes-blitz-browser/src/main.rs`
- `/Users/mac/hermes-blitz-browser/src/chrome.rs`
- `/Users/mac/hermes-blitz-browser/src/page_endpoint.rs`
- `/Users/mac/hermes-blitz-browser/src/vault.rs`

### X0 — Freeze Verified Browser Baseline & Publish Exact Hermes Host-Socket Contract
- **Binary:** `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`
- **Baseline Test:** `python3 /Users/mac/walkaway-monorepo/tests/e2e/test_full_storyline_vertical.py` (**PASS** in `4.902s`).
- **Start Command:**
  ```bash
  /Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser \
    --control-socket <SOCK_PATH> \
    --host-token-file <HOST_TOKEN_FILE> \
    --agent-token-file <AGENT_TOKEN_FILE> \
    [--supervise-commits-only] [--xray]
  ```
- **Socket Handshake (`hello`):**
  - Client sends: `{"v": 1, "type": "hello", "request_id": "h1", "token": "<contents of token file>"}`
  - Browser replies: `{"v": 1, "type": "hello.ok", "request_id": "h1", "role": "host"|"agent", "protocol": 1, "engine": {"name": "hermes-blitz", "version": "0.1.0"}}`
- **Every subsequent request frame carries a per-connection monotonic integer `"seq": 1, 2, 3, ...`.**
- **`session.open` (Host Channel Only):**
  - Send: `{"v": 1, "seq": 1, "type": "session.open", "request_id": "open-1", "title": "Muse Speaker Campaign", "start_url": "http://127.0.0.1:<PORT>/signup", "policy": {"decision": "human_per_action"}}`
  - Receive: `{"v": 1, "type": "session.opened", "request_id": "open-1", "session_id": "s_1", "tab_id": 2, "state": "agent", "generation": 1, "capabilities": {...}, "at": "..."}`
- **`proposal.pending` (Emitted on Host Channel):**
  ```json
  {
    "v": 1,
    "type": "proposal.pending",
    "session_id": "s_1",
    "event_seq": 2,
    "state": "awaiting_decision",
    "proposal_id": "p_submit_1",
    "snapshot_id": "o_2",
    "fingerprint": "<64-char sha256 hex>",
    "generation": 3,
    "operation": {"kind": "submit", "target_id": "el_7", "target_ref": null},
    "rationale": "Submit speaker registration form",
    "classification": "sensitive",
    "requires_glasses": true,
    "at": "2026-11-04T..."
  }
  ```
  - **High-Consequence Metadata Rule:** `requires_glasses` is `true` **ONLY** for `submit` and `fill_secret` (`classification == "sensitive"`). Routine `navigate`, `fill_form` (without `fill_secret`), `replay_recipe` (without `fill_secret`), `type`, `select`, `scroll`, and `click` have `requires_glasses: false`.
- **`decision` (Sent by Host Channel to Browser):**
  ```json
  {
    "v": 1,
    "seq": 2,
    "type": "decision",
    "request_id": "dec-1",
    "session_id": "s_1",
    "proposal_id": "p_submit_1",
    "fingerprint": "<exact 64-char sha256 hex from proposal.pending>",
    "generation": 3,
    "decision": "approve",
    "decided_by": "user",
    "surface": "glasses_dat"
  }
  ```
  *(where `"decision"` is `"approve"` or `"reject"`)*
- **`executed` / Receipt Frame (Emitted on Both Host & Agent Channels After Approval):**
  ```json
  {
    "v": 1,
    "type": "executed",
    "session_id": "s_1",
    "event_seq": 3,
    "state": "agent",
    "proposal_id": "p_submit_1",
    "operation": "submit",
    "operation_detail": {"kind": "submit", "target_id": "el_7", "target_ref": null},
    "target_id": "el_7",
    "before_fingerprint": "<64-char hex>",
    "after_fingerprint": "<64-char hex>",
    "generation": 4,
    "input_event_id": 2,
    "input_provenance": "approved_agent",
    "at": "2026-11-04T..."
  }
  ```
- **Final Audit / Receipt Query (`audit.read` on Host Channel):**
  - Send: `{"v": 1, "seq": 3, "type": "audit.read", "request_id": "aud-1", "session_id": "s_1"}`
  - Receive: `{"v": 1, "type": "audit.result", "session_id": "s_1", "records": [{"audit_id": "a_1", "session_id": "s_1", "proposal_id": "...", "operation": "...", "before_fingerprint": "...", "after_fingerprint": "...", "screenshot_hash": "...", "decided_by": "user", "surface": "glasses_dat", "result": "executed"}]}`

### X1 — Minutes 0–35: Live Headed Window Socket + Commit-Only Gating
- Move/run the Host/Agent non-blocking socket loop in the live headed WGPU window (`main.rs`).
- External agent commands visibly drive the real desktop Hermes window.
- Add `--supervise-commits-only`:
  - `navigate`, `fill_form` (non-secret), and `replay_recipe` (non-secret) execute live without a glasses prompt.
  - `submit` and `fill_secret` (`requires_glasses: true`) hard-freeze at the engine boundary, emit `proposal.pending`, and resume only after the Host `decision` arrives.
- **Gate at Minute 35:** Launch headed window with `--control-socket` + `--supervise-commits-only`, send `fill_form`, assert filled inputs render in live window, send `submit`, assert window freezes before commit, send Host `approve`, assert execution resumes once.

### X2 — Minutes 35–70: Native Semantic X-Ray Overlay and Freeze Shield (`src/chrome.rs`)
- Render directly over webpage DOM bounding boxes in the chrome overlay layer:
  - Element ID and role tags (`[el_3: textbox]`)
  - Vault shield (`[el_6: BLIND VAULT — Hidden from LLM/JS]`)
  - Freeze shield (`[el_9: ENGINE COMMIT FREEZE • fp:a9c1...]`)
  - Telemetry bar (`STATIC HERMES + BLITZ ENGINE`, live-measured `Layout+Observe` ms, live-measured `RAM/RSS` MB, `FormSignature`, `Recall` count & `0 LLM calls`).
- **Gate at Minute 70:** Capture live-window PNG via `include_screenshot: true` / `--render`, verify X-Ray boxes align with DOM elements, vault shield covers secret field, freeze shield and telemetry bar are readable and do not corrupt page interaction.

### X3 — Minutes 70–100: P-256 WebAuthn Assertion Work in `vault.rs`
- Add ECDSA P-256 signing for `authenticatorData` (37 bytes: `SHA256(rp_id) || flags(UP=1, UV=1) || signCount`) and `clientDataJSON` (`{"type":"webauthn.get","challenge":...,"origin":...}`).
- Trigger signing only after the glasses-originated Host `approve` arrives.
- **Required Gate Before Demo Use:** Fixture verifies the assertion against the registered public key; wrong challenge, wrong origin/RP, replayed assertion, and rejected glasses decision all fail; secret key material and generated passwords never appear in logs, proposals, audit, screenshots, or test output.

---

## PART 4: Zone Y — 10-Portal Suite and Disco Workbench (Owner: `eeb06c47` Subagent 1)

**Exclusive Files:**
- `/Users/mac/walkaway-monorepo/services/demo-fixtures/signup_fixture.py`
- `/Users/mac/walkaway-monorepo/tests/e2e/test_zone_y_10portals_and_disco.py`

### Y1 — Minutes 0–35: 10-Viewport Disco Workbench
- Add `GET /disco-workbench` and `POST /api/workbench/state` displaying 10 live-updated tiles inside Blitz:
  - Conference name, portal status, Hermes viewport snapshot/status representation, `FormSignature` match badge, `RECIPE RECALLED • 0 LLM tokens` badge, `QWEN3-4B ABSTRACT REPAIR` badge, and commit-gate state (`FROZEN FOR GLASSES TAP` / `SUBMITTED`).
- **Gate at Minute 35:** Load `/disco-workbench` in Hermes, verify all 10 tiles render with non-zero bounding rectangles, record actual measured render/layout time.

### Y2 — Minutes 35–65: Ten Real SQLite-Backed Portals (`/cfp/1` .. `/cfp/10`)
- Conferences: `1: SoCon 2026`, `2: KubeCon NA`, `3: BlackHat USA`, `4: RustConf`, `5: NeurIPS Systems`, `6: DEF CON AI`, `7: USENIX Security`, `8: AI Engineer Fair`, `9: PyCon US`, `10: QCon SF`.
- Portal #1 (`SoCon 2026`) enforces the curveball (`#validation-error` on missing `tshirt_size` or `len(abstract_summary) < 40`).
- Portals #2–#10 share compatible speaker-form semantics so `recall.rs` matches their `FormSignature`s; every submission writes real rows to SQLite.
- Also expose `POST /api/reset` (`<10s` one-command deterministic reset) and `GET /campaign-receipt` / `GET /api/campaign_receipt` (Session ID, Proposal ID, Classification, Before/After fingerprints, Screenshot/frame hash, Decision surface, Non-sensitive account/result ID, zero passwords).
- **Gate at Minute 65:** All 10 endpoints render, validate, and write to SQLite; Portal #1 rejects invalid submission for the exact visible reason.

### Y3 — Minutes 65–90: Camera Typography and Contrast Polish
- Make every field label, filled value, vault shield, validation error, and portal status legible in compressed 1080p video without altering `FormSignature`s.

---

## PART 5: Zone Z — Parallel Driver, Qwen, E2E, and Reset Harness (Owner: `eeb06c47` Subagent 2)

**Exclusive Files:**
- `/Users/mac/walkaway-monorepo/services/agent-driver/` (`parallel_campaign_driver.py`, `reset_demo.py`)
- `/Users/mac/walkaway-monorepo/tests/e2e/` (`test_10portal_disco_campaign.py`)

### Z1 — Minutes 0–40: 10-Portal Campaign Driver and Pre-Warmed `Qwen3-4B`
- Keep `mlx-community/Qwen3-4B-Instruct-2507-4bit` resident in memory to eliminate cold loading.
- Solve Portal #1, use `Qwen3-4B` to repair Portal #1's validation failure, replay the learned recipe across Portals #2–#10, push per-portal measured timing and status to `/disco-workbench`, track total LLM calls across the full campaign.
- **Gate at Minute 40 (`tests/e2e/test_10portal_disco_campaign.py`):** All 10 SQLite submissions succeed, total LLM calls across all 10 portals == `1`, measured recall time recorded, zero secret leaks.

### Z2 — Minutes 40–75: Three-Tap Glasses Commit Batching Through Port `19099`
- Exactly 3 glasses cards:
  1. `SoCon 2026 — Create Speaker Account & Seal Vault/Passkey`
  2. `SoCon 2026 — Submit Repaired CFP`
  3. `Batch Commit — Release 9 Recalled Conference Submissions` (carries explicit `batch_proposals` manifest: `[{proposal_id, fingerprint, generation, session_id, portal_id}, ...]`)
- `received`, `dat_sent`, and `dat_failed` never authorize execution; only `approve` or `reject` with `source="glasses_dat"` can become a Hermes decision.
- **Gate at Minute 75:** Full E2E proves 3 physical-card authorizations complete the campaign, all 10 backend submissions exist once, rejecting any card prevents execution, and replaying an ack cannot execute anything twice.

### Z3 — Minutes 75–100: One-Command Camera Reset and Rehearsal Harness (`--reset-demo`)
- Wipe/reset SQLite, clear recipe cache, re-seed/reset vault state safely, pre-warm `Qwen3-4B`, and return demo to take-one condition; two consecutive resets produce identical starting state.

---

## PART 6: Partner Lane P — Transport & Human Surfaces (Owner: `pebble-mirach`, formerly `clover-pulsar`)

**Exclusive Files:**
- `/Users/mac/walkaway-monorepo/services/hermes-host`
- `/Users/mac/walkaway-monorepo/apps/android-notification-bridge`
- `/Users/mac/walkaway-monorepo/apps/glasses-web`

### Ordered Must-Do Tasks (Partner Lane P):
1. **P1 — Pass the Physical Walkaway-Native DAT Gate (30-Min Time-Box / Hard Gate 1):**
   - Glasses charged, worn, and band connected.
   - Confirm Walkaway Bridge shows: `Bus connected`, `registration=REGISTERED`, `devices=1`, `connected=Meta RB Display 003F`.
   - Send one fresh message through the `19099` laptop bus.
   - Confirm the Walkaway-native card physically appears on the glasses.
   - Tap **Approve** on the glasses -> confirm laptop receives `{"action":"approve","source":"glasses_dat"}`.
   - Repeat once with **Reject** (`{"action":"reject","source":"glasses_dat"}`).
   - **Hard stop:** If the native card does not appear within 30 minutes, use the exact session error and worn/peer state to choose one diagnostic branch or fallback explicitly; do not rewrite lifecycle speculatively.
2. **P2 — Choose and Freeze the Phone-to-Laptop Network Path:**
   - Current proof uses USB plus `adb reverse` to `127.0.0.1:19099`.
   - Either keep the USB path with cable out of the camera shot, or move the bus to the laptop LAN address and retest untethered. Record using the exact tested path.
3. **P3 — Implement the Hermes Host Decision Bridge (`services/hermes-host/`):**
   - Connect to Hermes using only the host token (`--host-token-file`).
   - Consume `proposal.pending`.
   - Auto-approve routine (`requires_glasses: false`) proposals on the laptop host bridge; convert high-consequence (`requires_glasses: true`) proposals into Android/DAT messages on `19099`.
   - Ignore Android `received`, `dat_sent`, and `dat_failed` acknowledgements for decision purposes.
   - Translate **only** physical `approve` or `reject` from `glasses_dat` into a Hermes `decision` frame bound to `proposal_id`, `fingerprint`, and `generation` (including expanding Tap 3's `batch_proposals` manifest into individual bound Hermes `decision` frames).
   - Handle stale, duplicate, rejected, and timed-out proposals without executing twice.
4. **P4 — Make Android Demo-Safe (`apps/android-notification-bridge/`):**
   - Auto-connect to the saved bus address on launch.
   - Show four unambiguous states: `Bus connected/disconnected`, `DAT registered/not registered`, `Glasses connected/not connected`, `Last card sent/approved/rejected/failed`.
   - Deduplicate bus messages so one proposal cannot produce multiple cards.
   - Keep the process alive for the duration of the recording session.
5. **P5 — Physically Verify the Meta AI Web-App Redirect (`apps/glasses-web/`):**
   - Run the Walkaway install script with the final public HTTPS URL; confirm whether the app appears on the glasses.
   - Since the DAT card is the approval surface, keep the web app limited to status/receipt (never build a second approval UI).
6. **P6 — Run the Full Transport Rehearsal Twice:**
   - Laptop proposal appears on glasses -> Approve executes -> Reject kills a second proposal without execution -> both results appear accurately in the laptop acknowledgement log.

---

## PART 7: Integration, Demo Run, Video, Submission & Five-Hour Clock

### Ordered Demo Operator Tasks:
1. **Freeze Exact 3-Minute Script:** Act 1 account creation + blind vault/passkey with 1 glasses approval -> Act 2 validation error + `Qwen3-4B` self-correction + 1 glasses approval -> Act 3 zero-LLM recipe recall + 1 glasses batch approval -> Final Receipt.
2. **Shot List (No Slides):** Laptop (Hermes performing work) -> Phone (starting task / 4-state status) -> Glasses (approval card + physical Approve) -> Laptop (execution + final state) -> Receipt close-up.
3. **One-Page Reset Checklist:** Charge glasses/band/phone -> reset fixture/recall/vault -> restart Hermes -> restart host bridge -> restart `19099` bus -> reconnect Android -> confirm `DAT REGISTERED, devices=1, connected` -> confirm no stale state.
4. **Two Timed Rehearsals (3:00–4:00):** Measure any pause `>3s`; raw target `<3m20s` before editing.
5. **Record (4:00–4:40):** Master take + close-up insert of glasses card + clean narration if needed.
6. **Verify Every Spoken Claim Against Test Record:** Never say "cloud" if USB/LAN; never say "push notification" if DAT; never say "zero-LLM recall" unless `llm_calls == 0`; never say "password never leaves laptop" unless proven by `vault.rs`.
7. **Submit with Buffer (4:40–5:00):** Final video `<3:00`, public repo checked, connector submission checked, form completed `>=45m` before deadline.

### Five-Hour Clock & Hard Gates:
- **0:00–0:30:** Physical DAT gate (`P1`) + Browser baseline (`X0`) — **Hard Gate 1 at 0:30**.
- **0:30–2:00:** `Zones X/Y/Z` (`eeb06c47`) and `Partner Lane P` (`pebble-mirach`) execute in parallel.
- **2:00–3:00:** First full integrated run and fixes — **Hard Gate 2 at 3:00** (browser code freezes after 2 integrated greens).
- **3:00–4:00:** Two timed rehearsals — **Hard Gate 3 at 4:00** (no new features after Gate 3).
- **4:00–4:40:** Record master take and inserts.
- **4:40–5:00:** Edit, export, verify claims, submit.
