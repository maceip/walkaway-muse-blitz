# DEMO RUNBOOK — live 10-portal campaign (coordinated 2026-11-04 evening)

Owner of this runbook execution: Partner Lane P (`pebble-mirach`) on this
machine. Zones X/Y/Z are COMPLETE per `docs/STATUS.md`. Do not start a
second campaign/fixture/Hermes stack — check processes first.

## Preconditions (in order)

1. Bus on 19099 healthy (`curl http://127.0.0.1:19099/health`); start via
   `services/hermes-host/start-transport.sh` if down. USB attached,
   `adb reverse tcp:19099 tcp:19099`.
2. Phone: Walkaway Bridge open, status `CONNECTED — WAITING`,
   DAT `registration=REGISTERED devices=1`. If `AVAILABLE`: tap
   `Connect Meta AI` and finish Meta AI registration (wiped by any
   `connectedDebugAndroidTest` run — never run instrumented tests now).
3. Glasses WORN, display awake, neural band on. Prove it with a probe
   card BEFORE launching: POST any bus message; require `dat_sent`.
   `dat_failed` + logcat `SESSION_ENDED_BY_DEVICE`/`NO_ELIGIBLE_DEVICE`
   means not worn/awake — do not launch.
4. Keep the phone app in the foreground for the whole run. Closing it
   stops polling; cards stall silently.

## Launch (single command, detached stack)

Exactly one launcher exists: `services/agent-driver/run_live_demo.py`
(the former `scripts/salvaged-demo-live/` `/tmp` copies were consolidated
into it and deleted; recover them from git `c48b782` if ever needed).

```sh
python3 services/agent-driver/run_live_demo.py --detach   # fixture 8795 + reset + Hermes + campaign
tail -f /tmp/demo-live/run_live_demo.log                  # card announcements + final result
```

Campaign runs `run_10portal_campaign(..., glasses_timeout_s=900)` via
`services/agent-driver/parallel_campaign_driver.py` (driver CLI default
timeout is 30s — too short for a human tap; do not use the CLI). Logs:
`/tmp/demo-live/run_live_demo.log` (detached launcher); per-run state in
a `walkaway-live-demo-*` temp dir (`live_demo.sqlite3`, `vault.json`,
`audit.jsonl`); disco view `http://127.0.0.1:8795/disco-workbench`.

## Human script (exactly 3 taps, glasses only)

- Card 1: SoCon account + vault commit → **Approve**
- Card 2: Qwen-repaired SoCon CFP → **Approve**
- Card 3: batch commit, portals #2–#10 → **Approve**

Touch nothing on the phone. If a card is dismissed without a decision,
the run cannot recover that card (Android dedupe is one card per bus
message): stop the stack, fix worn state, relaunch from step 3.

## Known failure modes (all observed 2026-11-04)

- Agent-session shells get SIGKILL on steering → always use the detached
  spawner above, never a foreground campaign.
- Bus renumbering below Android `last_message_id` → never delete
  `/tmp/walkaway-bus-state.json`.
- `dat_failed` fast (<1s) = glasses not worn/eligible at send time.

## Abort

Kill by exact recorded PIDs only (spawner prints them; campaign workers
are `hermes_p3..p10.sock` under the campaign temp dir). Never kill the
19099 bus or anything on 8899.
