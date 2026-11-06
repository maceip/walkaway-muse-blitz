# Walkaway — Wearable-Supervised Parallel Browser Agent
<img width="701" alt="Blitz Lightspeed Browser Demo" src="https://github.com/user-attachments/assets/bdaa2a50-5acb-4d5c-9f77-2dbfdb24a85f" />

## The Problem
Today's browser agents force an impossible choice between reckless autonomy and babysitting a screen: either you hand over raw credentials and let an unverified model click submit on live web forms, or you sit trapped at your desk approving every trivial keystroke until approval fatigue sets in. On top of that, shipping an agent inside a heavyweight Chromium instance makes parallel multi-site workflows painfully slow, bloated, and impossible to supervise once you step away from your laptop.

## The Solution
Walkaway is a lightweight, native supervised browser and wearable commit gate that lets an AI agent fill, validate, and stage dozens of web workflows in parallel while keeping you in physical command of every irreversible action from your smart glasses. Routine form staging, local error recovery, and learned layout replay execute automatically at native speed, while high-consequence commits—like signing with a blind vault passkey or submitting a batch of ten conference proposals at once—hard-freeze at the browser boundary until you tap once on your glasses to approve.

---

## How Walkaway Works (The 3-Tap Wearable Workflow)

1. **Tap 1 — Blind Vault & Hardware-Style Passkey Commit (`/signup`):**
   - The agent stages account registration fields at native speed, while secret generation and NIST P-256 ECDSA WebAuthn assertion signing (`src/vault.rs`) are isolated inside the native browser vault.
   - The browser hard-freezes on the final submit proposal (`requires_glasses=true`) and dispatches Card #1 to the wearer's Ray-Ban Meta Display glasses over the Meta Wearables DAT SDK (`WalkawayDat.kt`). One tap on the glasses executes the commit without exposing plaintext secrets or private keys to the LLM, DOM, or logs.
2. **Tap 2 — Local On-Device Self-Correction (`/cfp/1`):**
   - When Portal #1 (`SoCon 2026`) rejects an incomplete submission with a live DOM `#validation-error` (missing `tshirt_size` and `<40`-char abstract summary), the resident local Apple Silicon model (`mlx-community/Qwen3-4B-Instruct-2507-4bit`) inspects the structured DOM observation, synthesizes the repair in a single local inference (`llm_calls == 1`), re-stages the form, and pauses for Tap #2 on the glasses.
3. **Tap 3 — Zero-LLM `FormRecipe` Fanout Across 9 Parallel Portals (`/cfp/2` .. `/cfp/10`):**
   - After Portal #1 succeeds, the browser persists a deterministic `FormRecipe` keyed by `FormSignature` (`sig_c31d6574c15e07f6`).
   - Eight parallel worker browsers (`p2..p10`) replay that recipe across 9 additional conference portals with **zero LLM calls** (`<5ms` planning per portal) and stream live screenshots + RSS/layout telemetry to the 10-viewport `/disco-workbench`.
   - All 9 pending commits are bundled into an explicit cryptographic `batch_proposals` manifest (`proposal_id`, `action_hash`, `before_fingerprint`). One final tap on Card #3 fans out 9 verified commit decisions simultaneously.

---

## Architecture & Repository Layout

- [`apps/hermes-browser/`](apps/hermes-browser/) — Native Rust/Blitz browser engine with supervised host/agent Unix socket channels, `--supervise-commits-only` commit gating, `--xray` Semantic X-Ray HUD + live `getrusage` RSS/layout telemetry, `src/vault.rs` blind secret + NIST P-256 WebAuthn passkey signer, and `src/recall.rs` zero-LLM `FormRecipe` replay.
- [`apps/android-notification-bridge/`](apps/android-notification-bridge/) — Native Android bridge integrating the Meta Wearables Device Access Toolkit (`WalkawayDat.kt`), foreground keep-alive service (`BridgeKeepAliveService.kt`), 4-state glasses status UI (`BridgeScreen.kt`), and persistent message deduplication (`BridgeState.kt`).
- [`services/hermes-host/`](services/hermes-host/) — Trusted host supervision plane:
  - [`decision_bridge.py`](services/hermes-host/decision_bridge.py) — Binds to Hermes host sockets (`POST /sockets`), auto-approves routine staging (`requires_glasses=false`), holds high-consequence commit proposals (`GET /pending`), binds single and batch `batch_proposals` manifests (`POST /cards`), and enforces `source == "glasses_dat"` before executing decisions.
  - [`message_bus.py`](services/hermes-host/message_bus.py) — Single-instance (`flock`) HTTP bus on port `19099` bridging laptop proposals and physical glasses DAT acks over `adb reverse tcp:19099`.
- [`services/agent-driver/`](services/agent-driver/) — Campaign orchestration and local MLX driver:
  - [`parallel_campaign_driver.py`](services/agent-driver/parallel_campaign_driver.py) — Orchestrates the 10-portal campaign across primary + 8 parallel worker browsers through `decision_bridge.py` (`19101` held mode).
  - [`hermes_agent_driver.py`](services/agent-driver/hermes_agent_driver.py) — Resident `mlx-community/Qwen3-4B-Instruct-2507-4bit` validation repair planner and `FormRecipe` selector.
  - [`run_live_demo.py`](services/agent-driver/run_live_demo.py) & [`reset_demo.py`](services/agent-driver/reset_demo.py) — Single-instance locked launcher (`--detach`) and `<5ms` deterministic state reset utility.
- [`services/demo-fixtures/`](services/demo-fixtures/) — [`signup_fixture.py`](services/demo-fixtures/signup_fixture.py) serving `/signup`, `/signin`, `/cfp/1`..`/cfp/10`, the 10-viewport `/disco-workbench`, `/campaign-receipt`, and OpenSSL P-256 `/api/webauthn/{challenge,register,verify,assertions}` backed by SQLite.
- [`tests/e2e/`](tests/e2e/) — Full-stack real-process E2E verification suite (zero mocks).

---

## Quickstart & Verification

```bash
# 1. Run the native browser supervisor & engine self-test suites (54/54 checks)
apps/hermes-browser/target/release/hermes-blitz-browser --supervisortest

# 2. Run the host decision bridge & single-instance bus lock suites
python3 services/hermes-host/test_message_bus_lock.py
python3 services/hermes-host/test_decision_bridge_e2e.py

# 3. Run the full 9-test real-system E2E suite (Slices 1-3, Zone X/Y/Z, 10-portal campaign)
python3 -m unittest discover -s tests/e2e -p "test_*.py" -v

# 4. Launch the live physical glasses demo stack (bus 19099 + bridge 19101 + fixture 8795 + 10-portal campaign)
python3 services/agent-driver/run_live_demo.py --detach
```

See [`docs/DEMO_RUN.md`](docs/DEMO_RUN.md) for the physical glasses runbook, [`docs/STATUS.md`](docs/STATUS.md) for component ownership and live run evidence, and [`docs/TEST_REGISTRY.md`](docs/TEST_REGISTRY.md) for the complete verification log.
