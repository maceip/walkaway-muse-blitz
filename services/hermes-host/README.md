# Hermes host bridge

Canonical implementation (Partner Lane P):

- `decision_bridge.py` — trusted host-channel bridge + bridge HTTP API.
- `message_bus.py` — laptop <-> Android bus on port `19099`.
- `test_decision_bridge_e2e.py` — real-system E2E (`T-BRIDGE-E2E-001`).

## Boundary

- Connects to Hermes using the host token (`--host-token-file`,
  `<socket>.host.token`, or `HERMES_HOST_TOKEN`).
- Opens one supervised session per `--socket` when `--start-url` is given.
  Session ids are published at `GET /health` for agent drivers.
- Never exposes the Hermes host token to Android, web, or agent processes.
  The bus token (`WALKAWAY_BUS_TOKEN`) guards both HTTP APIs.

## Decision rules

- `proposal.pending` with `requires_glasses == false`: approved immediately
  as `decided_by="policy"`, `surface="commit_gate"` (matching Hermes'
  internal `--supervise-commits-only` labels). No bus card.
- `requires_glasses == true`: registered as pending. Card mode `immediate`
  posts a glasses card at once; card mode `held` waits for `POST /cards`.
- Only bus acks with `source == "glasses_dat"` and action `approve`/`reject`
  become bound Hermes `decision` frames (`decided_by="user"`,
  `surface="glasses"`). `received`/`dat_sent`/`dat_failed` and all other
  sources are ignored. Consumed cards never decide twice.

## Bridge HTTP API (default `127.0.0.1:19101`)

- `GET /health` — sockets, session ids, pending/card/decision counts.
- `GET /pending` — live glasses-pending bindings; build manifests from this.
- `GET /decisions` — decision log + Hermes receipts (`executed`,
  `proposal.invalidated`, `refused`).
- `POST /cards` — `{title, body, batch_proposals: [{proposal_id,
  fingerprint, generation, session_id, portal_id?}, ...]}` posts ONE bus
  card bound to every listed proposal. All-or-nothing: 409 on any unknown,
  stale, decided, already-carded, or fingerprint/generation-mismatched
  entry. Works for a single proposal (Tap 1/2) and for the Tap 3 batch.
- `POST /sockets` — `{sock_path, token?, start_url?, session_title?}`
  attaches a Hermes host channel at runtime (WS1: campaign worker sockets
  spawned mid-run), opens a session when `start_url` is given, returns
  `{socket, session_id}` (201; 409 if already attached). Token falls back
  to `<sock_path>.host.token` then `HERMES_HOST_TOKEN`. `--socket` CLI args
  are optional; the bridge can boot HTTP-only and attach everything later.
  `GET /decisions` returns executed frames verbatim (including any
  `webauthn_*` fields Hermes emits).

## Run (demo shape)

```sh
WALKAWAY_BUS_TOKEN=... python3 services/hermes-host/message_bus.py \
  --host 127.0.0.1 --port 19099
WALKAWAY_BUS_TOKEN=... HERMES_HOST_TOKEN=... \
python3 services/hermes-host/decision_bridge.py \
  --socket /tmp/hermes.sock --bus-url http://127.0.0.1:19099 \
  --card-mode held --start-url http://127.0.0.1:PORT/signup
```

Bus note: message ids persist across restarts (`WALKAWAY_BUS_STATE`,
default `/tmp/walkaway-bus-state.json`). The Android bridge stores a
`last_message_id` cursor; a bus that renumbers below that cursor delivers
nothing. Do not delete the state file during a demo day.

## Frozen transport (P3)

Demo path, frozen: **USB cable + `adb reverse tcp:19099 tcp:19099`**. The
Android bridge polls `http://127.0.0.1:19099` on the phone; adb reverse
tunnels the phone loopback to the laptop bus. Startup/recovery is one
idempotent command:

```sh
services/hermes-host/start-transport.sh            # start bus if down, set tunnel, verify
services/hermes-host/start-transport.sh --check-only
```

Recovery verified 2026-11-04: tunnel removed, script restored the tunnel,
next bus message was `received` by Android ~0.5s later. A `dat_failed` ack
with `SESSION_ENDED_BY_DEVICE` in the `walkaway.dat` log means the glasses
are not worn/eligible at send time — a glasses-state signal, not a
transport failure.

LAN fallback (**unverified, not the demo path**): restart the bus with
`--host 0.0.0.0`, set the phone app's bus URL to
`http://<laptop-lan-ip>:19099`, keep the same bus token. No physical
glasses decision has been observed over LAN; do not use LAN for the demo
without re-running the P1 gate on LAN first. Port `8899` belongs to a
foreign pre-existing server and must never be used or killed.
