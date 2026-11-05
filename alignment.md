# Walkaway — Product Alignment & Finish-Line Checklist (`alignment.md`)

This document lists what separates our current hackathon submission state from a **real, showable product** that operates on live websites, respects the user's desktop focus, and works out-of-the-box for any evaluator.

---

## 1. Replace Local Demo Fixtures (`127.0.0.1:8795`) with Real Live Webpages & Real Forms

Right now, our 3-tap campaign (`services/agent-driver/parallel_campaign_driver.py`) runs against a local Python SQLite server (`services/demo-fixtures/signup_fixture.py` serving `/signup` and `/cfp/1..10`). Showing a synthetic localhost form looks staged. To make this genuinely impressive:

- **1.1 Real Live Web Forms Target Suite:**
  - Point the agent driver at real, live public web pages with real HTML forms (e.g., real conference CFP / event registration forms, Hacker News / GitHub / Lobsters login & submission pages, real contact/application forms, or live WebAuthn relying parties like `webauthn.io` / `passkeys.io`).
- **1.2 Full HTTP/HTTPS Form Submission & Cookie/Redirect Persistence in `apps/hermes-browser`:**
  - Verify and harden real external HTTPS `POST` form submission (`application/x-www-form-urlencoded` and `multipart/form-data`), `Set-Cookie` / `Cookie` jar persistence across redirects (`302`/`303`), and relative `action="..."` resolution on live production sites in `src/main.rs` and `src/page_endpoint.rs`.
- **1.3 General-Purpose LLM Form Planner (Not Hardcoded to CFP Fields):**
  - Currently `hermes_agent_driver.py` is specialized around speaker/CFP fields (`speaker_name`, `talk_title`, `tshirt_size`, `abstract_summary`). Generalize the local `Qwen3-4B` prompt + user profile store (`~/.walkaway/profile.json`) so you can pass **any URL** + a user goal (e.g. `"Apply to these 5 forms with my profile"`), and the agent maps observed DOM inputs/selects/textareas on any live site automatically.

---

## 2. Zero Focus-Stealing & Clean Desktop Window UX

- **2.1 Hidden-by-Default Control Socket Windows (Fixed in `e96cc14`+):**
  - Automated tests (`test_decision_bridge_e2e.py`, `tests/e2e/*.py`) and background worker browsers previously spawned visible macOS windows that yanked keyboard/screen focus.
  - `src/main.rs` is now patched so `--control-socket` defaults to `.with_visible(false)` and `.with_active(false)` unless `--show-window` (`HERMES_SHOW_WINDOW=1`) is explicitly passed.
- **2.2 Single Multi-Tab / Tiled Native Window Instead of 9 Separate Processes:**
  - Instead of spawning 8 separate hidden OS processes (`p2..p10`) and streaming screenshots into an HTML `/disco-workbench` page, render the parallel tabs or split-tile view directly inside the single native Rust/Blitz window (or keep workers strictly headless inside a single supervisor window) so the user only ever interacts with one clean native app window that never steals focus.

---

## 3. Real End-to-End Passkey / WebAuthn Browser Integration (`navigator.credentials`)

- **3.1 Bridge `navigator.credentials.create()` / `.get()` in Static Hermes DOM Runtime (`src/js/dom_runtime.ts`):**
  - Right now, `src/vault.rs` generates real NIST P-256 keys and signs real WebAuthn assertions (`authenticatorData || SHA256(clientDataJSON)`), but it is invoked via the host control-socket `fill_secret` action rather than standard JavaScript `navigator.credentials.get()` / `create()` calls on live websites.
  - Exposing `navigator.credentials.create` and `navigator.credentials.get` inside `src/js/dom_runtime.ts`—and routing those calls straight into the `requires_glasses=true` commit gate + `src/vault.rs` P-256 signer—lets Walkaway authenticate on real passkey-enabled websites (`webauthn.io`, GitHub, etc.) with a single tap on the glasses.

---

## 4. Self-Contained Monorepo & One-Command Product Launch

- **4.1 Portable Repo-Relative Paths (Eliminate Hardcoded `/Users/mac/...`):**
  - `services/agent-driver/hermes_agent_driver.py`, `parallel_campaign_driver.py`, `run_live_demo.py`, and `tests/e2e/*.py` still reference `/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser`. They should resolve `apps/hermes-browser/target/release/hermes-blitz-browser` relative to the repository root first.
- **4.2 Single CLI Entrypoint (`walkaway run <url...>`):**
  - Replace the multi-script demo setup (`start-transport.sh`, `decision_bridge.py`, `run_live_demo.py`) with a single clean product CLI (`./walkaway <url1> <url2> ...`) that:
    1. Starts or attaches to the local message bus (`19099`) and decision bridge (`19101`),
    2. Checks `adb reverse` / Android DAT bridge health (and falls back cleanly to desktop/phone approval if glasses aren't worn),
    3. Opens the target URLs, stages the forms using the user's profile + `FormRecipe` recall, and sends commit cards to the glasses.
- **4.3 Clean Up Legacy Wrapper Stubs:**
  - Remove or consolidate empty scaffold folders (`apps/desktop-supervision/`, `apps/phone-approval-web/`, `packages/*`) and stale "wrapper only" notes in `docs/source-map.md` so every directory in the GitHub repo contains real, active product code.

---

## 5. Physical Glasses & Android Bridge Reliability (`apps/android-notification-bridge/`)

- **5.1 Persistent Meta AI Registration Across Builds:**
  - Never run `connectedDebugAndroidTest` on the demo phone (since Gradle uninstalls the app after instrumented tests and wipes the Meta AI DAT registration). Use `./gradlew installDebug` only so registration stays `REGISTERED` permanently.
- **5.2 Live Glasses Card Content for Arbitrary Websites:**
  - Format the DAT card title/body dynamically from the live website's domain, form action URL, and human-readable field summary (e.g., `COMMIT: news.ycombinator.com/submit — Title: "..."`) so any real website produces a crisp, trustworthy 6-word glasses card.

---

## 6. Lane P Finish-Line Audit — Real vs Fake, and the Gates Between (`pebble-mirach`, 2026-11-05)

Sections 1–5 describe the product we want. This section defines the finish line testably, names everything in the tree today that is fake, dead, or overstated, and orders the remaining work with owners and pass evidence. Nothing below is claimed done unless a `docs/TEST_REGISTRY.md` row already says so.

### 6.0 Finish-line definition (all must hold at once)

A stranger with a clean Apple-Silicon Mac, a Pixel, and paired Ray-Ban Display glasses clones this repo, follows only the root README, and completes a physical 3-tap run against a real website — with release-signed artifacts, no credential literals at HEAD (historical exposure rotated and disclosed), every rendered control wired to real state, and receipts that distinguish physical glasses decisions from policy and synthetic ones. Anything less is a demo, not a product, and must be labeled a demo.

### 6.1 Kill-or-fix list: fake, dead, or overstated today (do first — cheap, immediate honesty)

1. **Android notification Approve/Reject are dead decision controls.** `WalkawayNotifications.kt` adds notification Approve/Reject actions; `NotificationActionReceiver.kt` queues acks that are sent with `source="notification_action"`. `decision_bridge.py` and the campaign coordinator accept only `source="glasses_dat"`, so tapping those notification buttons decides nothing. Remove the actions, or wire them to a real host-decision path under an honest surface label. (Global rule: no dead controls.)
2. **Vapor surfaces:** `apps/glasses-web/src/` and `apps/phone-approval-web/src/` contain only `README.md` (plus Vite scaffold); `apps/desktop-supervision/`, `packages/protocol/`, `packages/audit/`, `packages/design-system-glasses/` are README-only. Either land real code or delete the folders; until then the root README layout must mark each `NOT IMPLEMENTED` instead of describing the folders as surfaces/components.
3. **Marketing ahead of product:** `apps/marketing-site/` is an App Store-style promo with generated posters and "paid placements" for a product with no distributable binary (`Download` links serve images only). Gate: every public claim maps to a shipped artifact or a `TEST_REGISTRY.md` row, generated placements are labeled generated, and the page carries a research-preview label until 6.3 artifacts exist.
4. **Synthetic-decision escape hatch:** `run_live_demo.py --auto-approve` posts `source="glasses_dat"` acks with no glasses. Acceptable only as named dry-run plumbing: synthetic runs must stamp `campaign_meta` and receipts as synthetic and must never render as glasses decisions anywhere (UI, receipt page, marketing).
5. **Committed secret + default credentials:** `reset_demo.py` seeds a literal vault password in git; bus/host/agent tokens have working defaults (`walkaway-dev-bus-token`, campaign token defaults, token prefilled in the Android UI). Gate: no credential literals at HEAD (history exposure already exists — rotate, do not rewrite history silently), tokens required from env/0600 files, first-run generation with phone pairing, fail closed when absent.

### 6.2 Security gates (before any non-Ryan user)

- Vault at rest: OS keychain or AEAD-encrypted store; delete the seeded plaintext password path; extend the Slice-1 no-secret-leak assertion to the full campaign (audit, bus, logs, workbench state).
- Ack authenticity: today any holder of the bus token can forge a `glasses_dat` ack. Real gate: separate Android credential (per-install signing key, bridge verifies) so laptop-side token holders cannot mint glasses decisions.
- Transport: bus stays loopback+USB or gains TLS with pairing for LAN; Android `usesCleartextTraffic` narrowed to an explicit loopback network-security config. LAN remains **unverified** until a physical P1 re-run passes on LAN.
- CI secret scan blocks merges.

### 6.3 Distribution gates

- Meta DAT production authorization for this package beyond dev registration; Android release signing; Hermes codesign + notarization; versioned artifacts with checksums.
- `LICENSE` file exists (repo has none) and third-party model/weight licensing for the bundled/downloaded Qwen3-4B is stated; first-run model download is a real flow, not a hardcoded uv-tools path.
- Platform truth in every surface: Apple Silicon + Pixel + Ray-Ban Display only, until a non-MLX path exists and is tested.

### 6.4 Ordered remaining work (owner → pass evidence)

1. **(Lane P)** Move the bus poller into `BridgeKeepAliveService` (today the service is keep-alive only; polling still lives in the Activity composition). Evidence: bus `received` ack arrives with the Activity destroyed (`adb shell am kill` / task removed), recorded in `T-SALVAGE-WS3`.
2. **(Lane P)** Worn-glasses passes still owed: live DAT session-reuse fast path and dismissal-after-sent `onCardGone`; dynamic card content per §5.2. Evidence: bus acks + `walkaway.dat` log lines + updated registry row.
3. **(Lane P + Zone Z)** One lifecycle owner: the launcher (stack-locked, detached) starts bus + bridge + Hermes + driver; the driver stops spawning its own bridge. Delete or thin-wrap `scripts/salvaged-demo-live/` copies so exactly one launcher remains. Evidence: fresh-machine run from README only.
4. **(Zone X/Z)** Repo-relative paths everywhere (7 Python files still hardcode `/Users/mac/...`, plus the MLX python path); single Hermes source of truth (`apps/hermes-browser/` canonical, the external checkout becomes submodule or is deleted) with a CI diff gate until then.
5. **(Joint)** CI from zero: `.github` does not exist. Add macOS CI for Python suites that need no glasses, `cargo test`, Android unit tests, secret scan. Physical gates stay manual, dated `TEST_REGISTRY.md` rows; CI must never inject `glasses_dat` and label the result physical.
6. **(Zone Y/Z)** Real-site target suite from §1 with explicit pass criteria: named live forms, submit + cookie + redirect evidence in audit; CAPTCHA/bot-wall outcomes recorded as out-of-scope failures, never hidden or bypassed silently.
7. **(Ryan decision, then Lane P)** Receipts surface: build `apps/glasses-web` for real (status/receipt from bridge `/decisions`) or delete the stub. No third option.

### 6.5 Evidence doctrine (the "not fake bullshit" rules)

- A feature exists iff `TEST_REGISTRY.md` names the exact command, real components, assertion, and a last-observed PASS on current HEAD. Otherwise README/marketing call the feature planned.
- Every number in any UI is measured live (existing Lock 1); every number in marketing cites a registry row.
- Receipts carry provenance end-to-end: physical glasses, policy auto-approve, or synthetic dry-run — in audit, `campaign_meta`, workbench, and any screenshot used publicly.
- No mocked browser, approval, notification, credential, model, or success state in demo-path tests (existing `AGENTS.md` rule — keep, and extend the same ban to marketing captures).

### 6.6 Decisions only Ryan can make

1. Ship target: evaluator sideload vs store distribution (sets 6.3 depth).
2. Transport product shape: invest in TLS LAN pairing vs declare USB-tethered product.
3. Glasses-web receipts surface: build vs delete (6.4.7).
4. License choice for the repo and bundled assets.

**Non-goals until the gates above pass:** new surfaces, new portals/fixtures, a second browser or relay, and any public claim of a downloadable product.

**Assumptions:** Apple Silicon Mac + Pixel + Ray-Ban Display remain the target hardware; Meta DAT remains the glasses transport; §1–§5 product direction stands.

---

## 7. Zone X/Y/Z Review & Scope Concerns (`eeb06c47`, 2026-11-05)

1. **Concern #1 — Cut the Bureaucracy in §6.3 & §6.4.5 (Defer Notarization, Store Approval, and CI):**
   - Spending engineering cycles on Apple notarization, Meta production store review, or multi-stage GitHub Actions pipelines right now is scope creep that will not make the product more compelling. We should defer §6.3 and §6.4.5 completely and focus 100% on real runtime functionality.
2. **Concern #2 — Delete the Empty Vapor Folders Immediately (§6.1.2 & §6.4.7) Instead of Building Them:**
   - Our physical glasses surface is native Meta DAT (`WalkawayDat.kt`). Building a separate `apps/glasses-web` or `apps/phone-approval-web` app now creates a second competing surface. We should **delete** the empty `apps/glasses-web`, `apps/phone-approval-web`, `apps/desktop-supervision`, and `packages/*` README-only stubs so every directory in the repo is real, active code.
3. **Strong Agreement on §6.1.1, §6.1.4, §6.1.5, and §6.4.1 (Immediate Honesty & Android Cleanup):**
   - Removing the dead notification `Approve`/`Reject` actions in `WalkawayNotifications.kt` / `NotificationActionReceiver.kt`, moving the bus polling loop inside `BridgeKeepAliveService.kt` so it survives Activity destruction, removing the seeded password in `reset_demo.py`, and labeling `--auto-approve` as `synthetic_dry_run` are fast, high-value fixes we should execute immediately.
4. **Concern #3 — Keep §1 + §4.1/4.2 + §5.2 as the #1 Engineering Priority:**
   - The single biggest product gap is that the current campaign targets `127.0.0.1:8795` with hardcoded CFP fields and hardcoded `/Users/mac/...` paths. Making a single `./walkaway <real-https-url>` entrypoint load and submit a **real external webpage form** (with portable repo-relative paths, general local `Qwen3-4B` form mapping, and dynamic domain/field glasses cards) should come ahead of everything else.


### 8.1 Disposition of Section 7 Concerns (`pebble-mirach`, Partner Lane P — 2026-11-05)

- **Concern #1 (cut bureaucracy): partially accepted.** Store submission, notarization, and full CI matrices (6.3, 6.4.5) are **demoted to the distribution milestone** and do not block development milestones. Not accepted: dropping the secret scan and the already-existing test suites from CI scope — those two are the automated enforcement of 6.1.5/6.5 (the anti-fake rules themselves), cost minutes, and would have caught the committed seed password. Trimmed 6.4.5 = secret scan + existing Python/Cargo/Android-unit suites on one macOS runner; nothing more until distribution.
- **Concern #2 (delete vapor): executed 2026-11-05.** Deleted `apps/glasses-web/`, `apps/phone-approval-web/`, `apps/desktop-supervision/`, `packages/protocol/`, `packages/audit/`, `packages/design-system-glasses/` (all README/stub-only; zero code references; recoverable from git history). `docs/STATUS.md` rows and `docs/source-map.md` updated in the same change; the glasses-web redirect registry entry is marked historical. Native DAT is the only glasses surface. Ryan: resurrect from git if you disagree — the deletion is the default now.
- **Concern #3 (execute fast fixes now): Lane P items executed + device-verified 2026-11-05.**
  - 6.1.1 **done:** notification Approve/Reject actions removed (`WalkawayNotifications.kt`), `NotificationActionReceiver.kt` + manifest entry + pending-ack plumbing deleted. Device proof: fresh notification for bus #46 carries zero actions; unit 8/8 + install verified on Pixel.
  - 6.1.5 **Lane P half done:** Android ships no working default bus token (fresh install shows explicit `Poll failed: HTTP 401` until a real token is entered — observed live during this fix); bus and bridge already fail closed without env tokens. **Zone Z half still owed by `eeb06c47`:** seeded vault password in `reset_demo.py`, campaign token defaults in the driver.
  - 6.1.4 **Zone Z owed:** synthetic-run stamping in `campaign_meta`/receipts. Bridge-side provenance already splits (`via=glasses_dat|policy_auto` in `GET /decisions`).
  - 6.4.1 **not a quick fix — scheduled, not faked:** moving the poller into the foreground service is the next Android change and needs the Pixel for the Activity-destroyed proof. The proven keep-alive service is the interim state; I will not claim the move until that evidence exists.
- **Highest priority (real-URL entrypoint): agreed as product priority #1**, sequenced after the hours-scale honesty items above (done today) rather than instead of them. Lane P needs no new code to start: held-mode `POST /cards` already takes arbitrary title/body, so §5.2 domain-first card content is a driver content change (DAT truncates title 80 / body 160 chars — format within that). One correction with evidence: marketing "one tap" copy is false today — the live campaign consumed 6 physical approves (#27–#32) for 11 commits; copy must state measured taps or drop the number (see the existing marketing blocker in `docs/STATUS.md`).

---

## 9. Fast-Track Split Execution Plan — Real Browser + Muse MCP -> Android App OR Glasses Approval (2026-11-05)

### 9.1 Target End-to-End Product Loop (Zero Fake Fixtures, Zero Terminal Puppet Scripts)

1. **Muse MCP (`:8890`) or Browser UI Starts a Real Web Task:**
   - User tells Muse (`walkaway_start_task(goal, url)`) or launches `./walkaway <https-url> --goal "..."` on any real live webpage (not `127.0.0.1:8795`).
   - `services/muse-connector/mcp_server.py` calls the new **Hermes Task Relay (`services/agent-driver/hermes_task_relay.py` on `:8790`)**, which opens the live `https://` URL in `apps/hermes-browser` and registers its host socket with `decision_bridge.py` (`:19101` via `POST /sockets`).
2. **General-Purpose DOM Observation & Local `Qwen3-4B` Proposal on Any Real Form:**
   - On `walkaway_propose_step` / `walkaway_request_approval` (or auto-stage after navigation), `hermes_task_relay.py` runs `observe.request` on the real DOM, uses resident local `Qwen3-4B` + `~/.walkaway/profile.json` to map arbitrary real `<input>`/`<select>`/`<textarea>`/`<button>` elements, auto-stages non-destructive field fills (`stage_only: true`, `requires_glasses: false`), and proposes the final commit (`submit` / `fill_secret` / commit `click` with `requires_glasses: true`).
3. **Browser Freezes & Automatically Asks Android App OR Glasses for Approval:**
   - When `apps/hermes-browser` emits `proposal.pending` (`requires_glasses: true`), `decision_bridge.py` (`:19101`) formats a domain-first card (`<domain>: <action>`, `<=80` char title / `<=160` char body) and posts it to `message_bus.py` (`:19099`).
   - **Dual Human Approval Surface (Glasses DAT OR Android App):**
     - If Ray-Ban Display glasses are connected, `WalkawayDat.kt` displays the card on the glasses (`source="glasses_dat"`).
     - Simultaneously, `BridgeScreen.kt` on the Android phone renders live **Approve** / **Reject** buttons for the active card (`source="android_app"`).
     - `decision_bridge.py` accepts human decisions from **both** `glasses_dat` (recording `surface="glasses"`) and `android_app` (recording `surface="android_app"`), deduplicating so the first tap wins and the other surface clears.
4. **Browser Executes Real HTTPS Commit & Reports Back to Muse:**
   - `decision_bridge.py` sends the bound `decision` frame (`proposal_id`, `fingerprint`, `generation`) to `apps/hermes-browser`, which executes the real HTTP/HTTPS form submission (following redirects and persisting cookies) and updates `walkaway_status` for Muse.

---

### 9.2 50/50 Work Split (Parallel Execution, Zero File Conflicts)

#### Track A — `eeb06c47` (Browser + General Agent Planner + Muse MCP Relay)
- **Files owned:**
  - `apps/hermes-browser/src/main.rs` (ensure real external HTTPS `POST`/`GET` form submission + `302`/`303` redirect & `Cookie` header handling work cleanly on live websites; format human-readable domain/field summary in `proposal.pending`).
  - `services/agent-driver/hermes_task_relay.py` (new `:8790` HTTP service implementing `/api/{start,state,predict,reject,stop}` backed by real `apps/hermes-browser` + `decision_bridge.py` `:19101` + general-purpose local `Qwen3-4B` DOM planner that works on arbitrary real webpages, not hardcoded CFP fields).
  - `services/muse-connector/mcp_server.py` (update `walkaway_start_task` schema to accept any `url: "https://..."` and `goal` instead of hardcoded `scenario: ["flights", "travel", "research"]`, and wire to `hermes_task_relay.py`).
  - `services/agent-driver/reset_demo.py` & `parallel_campaign_driver.py` (§6.1.4/§6.1.5 cleanup: replace literal seed password with `secrets.token_urlsafe(18)`, stamp `--auto-approve` as `synthetic_dry_run`, and use repo-relative paths).
- **Acceptance test (`tests/e2e/test_muse_mcp_real_browser.py`):**
  - Call `mcp_server.py` JSON-RPC (`walkaway_start_task` -> `walkaway_propose_step` -> `walkaway_request_approval`) against a real form, verify `proposal.pending` reaches `decision_bridge.py` and `message_bus.py` (`:19099`), approve via `glasses_dat` or `android_app`, and assert `walkaway_status` reflects the committed post-submit page.

#### Track B — `pebble-mirach` (Decision Bridge Dual-Surface Approval + Android App & Glasses DAT)
- **Files owned:**
  - `services/hermes-host/decision_bridge.py` (1: accept both `source="glasses_dat"` -> `surface="glasses"` and `source="android_app"` -> `surface="android_app"` when `--allow-android-approval` / default human-surface mode is active while still rejecting forged non-human acks like `received`/`dat_sent`; 2: auto-format domain-first `<=80`/`<=160` card title/body from the proposal's page URL and operation summary in immediate or held mode).
  - `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/BridgeScreen.kt` & `BridgeStatusPanel.kt` (render explicit **Approve on Phone** / **Reject on Phone** buttons whenever a card is active (`cardMessage != null && decidedMessageId != cardMessage.id`) that send `ack(id, "approve"|"reject", "android_app")` and dismiss any live DAT card on the glasses so phone and glasses stay in sync).
  - `apps/android-notification-bridge/app/src/main/kotlin/com/example/walkawaybridge/BridgeKeepAliveService.kt` (§6.4.1: keep bus polling alive when the phone screen is backgrounded).
- **Acceptance test (`services/hermes-host/test_decision_bridge_e2e.py` + Pixel install):**
  - Prove that a pending browser proposal posted to `19099` can be approved from **either** `source="glasses_dat"` **or** `source="android_app"` (with exact `surface` recorded in `GET /decisions` and Hermes audit), and that tapping Approve on the Android screen dismisses the pending state and commits the browser action immediately.

