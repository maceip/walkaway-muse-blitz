#!/usr/bin/env python3
"""Hermes host decision bridge (Partner Lane P, task P2).

Connects to one or more Hermes browsers over the host control socket and
routes human decisions:

- `proposal.pending` with `requires_glasses == false` is approved immediately
  as policy (`decided_by="policy"`, `surface="commit_gate"`, the same labels
  Hermes uses for its internal `--supervise-commits-only` gate). No bus card.
- `proposal.pending` with `requires_glasses == true` is registered as pending.
  In `immediate` card mode a glasses card is posted to the message bus at
  once. In `held` card mode the campaign driver creates cards explicitly via
  `POST /cards` (single proposal or an explicit batch manifest).
- Only bus acks with `source == "glasses_dat"` and `action` of `approve` or
  `reject` become Hermes `decision` frames, bound to the exact
  `(session_id, proposal_id, fingerprint, generation)` captured from
  `proposal.pending`. `received` / `dat_sent` / `dat_failed` acks and every
  other source are ignored. Consumed cards and decided proposals are never
  decided twice (Hermes `decision_used` is the backstop).

HTTP API (default 127.0.0.1:19101, `X-Bus-Token` required except /health):
- `GET /health`    -> sockets, session ids, counts
- `GET /pending`   -> live glasses-pending bindings (build manifests from this)
- `GET /decisions` -> decision + receipt log
- `POST /cards`    -> {"title","body","batch_proposals":[{proposal_id,
  fingerprint, generation, session_id, portal_id?}, ...]} posts ONE bus card
  bound to every listed proposal; all-or-nothing validation (409 on any
  unknown, stale, decided, or already-carded binding).

Run:
  WALKAWAY_BUS_TOKEN=... HERMES_HOST_TOKEN=... \
  python3 services/hermes-host/decision_bridge.py \
    --socket /tmp/hermes.sock --start-url http://127.0.0.1:PORT/signup
"""

import argparse
import hmac
import json
import os
import socket
import threading
import time
import urllib.request
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BUS_TOKEN = os.environ.get("WALKAWAY_BUS_TOKEN", "")


def now():
    return datetime.now(timezone.utc).isoformat()


def log(msg):
    print(f"[bridge] {msg}", flush=True)


class Bus:
    def __init__(self, base_url, token):
        self.base = base_url.rstrip("/")
        self.token = token

    def _req(self, path, payload=None):
        headers = {"X-Bus-Token": self.token}
        data = None
        method = "GET"
        if payload is not None:
            headers["Content-Type"] = "application/json"
            data = json.dumps(payload).encode()
            method = "POST"
        req = urllib.request.Request(self.base + path, data=data, headers=headers, method=method)
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read().decode())

    def post_card(self, title, body, kind, batch_proposals=None):
        payload = {"title": title, "body": body, "kind": kind}
        if batch_proposals is not None:
            payload["batch_proposals"] = batch_proposals
        return self._req("/messages", payload)

    def acks(self):
        return self._req("/acks").get("acks", [])


class HostClient:
    """One Hermes host-channel connection (NDJSON over a Unix socket)."""

    def __init__(self, bridge, sock_path, token):
        self.bridge = bridge
        self.sock_path = sock_path
        self.token = token
        self.seq = 0
        self.session_id = None
        self._send_lock = threading.Lock()
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(sock_path))
        self.reader = self.sock.makefile("r", encoding="utf-8")
        self.send({"type": "hello", "request_id": "bridge-hello", "token": token})

    def send(self, payload):
        with self._send_lock:
            self.seq += 1
            frame = {"v": 1, "seq": self.seq, **payload}
            self.sock.sendall((json.dumps(frame) + "\n").encode())

    def open_session(self, title, start_url):
        self.send(
            {
                "type": "session.open",
                "request_id": "bridge-open",
                "title": title,
                "start_url": start_url,
                "capabilities": {"protocol": 1},
                "policy": {"decision": "human_per_action"},
            }
        )

    def decide(self, binding, decision, decided_by, surface):
        self.send(
            {
                "type": "decision",
                "session_id": binding["session_id"],
                "request_id": f"bridge-dec-{binding['proposal_id']}",
                "proposal_id": binding["proposal_id"],
                "fingerprint": binding["fingerprint"],
                "generation": binding["generation"],
                "decision": decision,
                "decided_by": decided_by,
                "surface": surface,
            }
        )

    def run(self):
        log(f"host channel connected: {self.sock_path}")
        while True:
            line = self.reader.readline()
            if not line:
                log(f"host channel closed: {self.sock_path}")
                return
            try:
                frame = json.loads(line)
            except json.JSONDecodeError:
                continue
            self.bridge.on_host_frame(self, frame)


class Bridge:
    def __init__(self, bus, card_mode):
        self.bus = bus
        self.card_mode = card_mode
        self.lock = threading.Lock()
        self.clients = []            # HostClient per Hermes socket
        self.pending = {}            # (session_id, proposal_id) -> binding
        self.cards = {}              # bus message_id -> {"keys": [...], "consumed": bool}
        self.decided = set()         # (session_id, proposal_id) already decided
        self.carded = set()          # (session_id, proposal_id) bound to a card
        self.decisions = []          # audit log for GET /decisions
        self.seen_acks = set()       # (message_id, action, source, received_at)
        self.ignored_acks = 0

    # -- Hermes host frames -------------------------------------------------
    def on_host_frame(self, client, frame):
        ftype = frame.get("type")
        if ftype == "session.opened":
            client.session_id = frame.get("session_id")
            log(f"session.opened {client.session_id} on {client.sock_path}")
            return
        if ftype in ("executed", "proposal.invalidated", "refused"):
            with self.lock:
                self.decisions.append({"at": now(), "receipt": frame})
            log(f"receipt {ftype} proposal={frame.get('proposal_id')}")
            return
        if ftype != "proposal.pending":
            return
        proposal = frame.get("proposal") if isinstance(frame.get("proposal"), dict) else frame
        binding = {
            "session_id": frame.get("session_id") or proposal.get("session_id") or client.session_id,
            "proposal_id": proposal.get("proposal_id"),
            "fingerprint": proposal.get("fingerprint"),
            "generation": proposal.get("generation"),
            "classification": proposal.get("classification", "act"),
            "rationale": proposal.get("rationale", "Approve action"),
            "operation_kind": (proposal.get("operation") or {}).get("kind", "act"),
            "requires_glasses": bool(proposal.get("requires_glasses", False)),
            "client": client,
        }
        key = (binding["session_id"], binding["proposal_id"])
        if not binding["requires_glasses"]:
            with self.lock:
                if key in self.decided:
                    return
                self.decided.add(key)
                self.decisions.append(
                    {"at": now(), "via": "policy_auto", "decision": "approve",
                     "session_id": key[0], "proposal_id": key[1]}
                )
            client.decide(binding, "approve", "policy", "commit_gate")
            log(f"auto-approved (no glasses) {key}")
            return
        with self.lock:
            self.pending[key] = binding
        log(f"glasses-pending {key} kind={binding['operation_kind']}")
        if self.card_mode == "immediate":
            try:
                self.create_card(
                    title=f"Approve {binding['operation_kind']}",
                    body=binding["rationale"],
                    entries=[self._manifest_entry(binding)],
                )
            except ValueError as e:
                log(f"immediate card failed for {key}: {e}")

    @staticmethod
    def _manifest_entry(binding, portal_id=None):
        entry = {
            "proposal_id": binding["proposal_id"],
            "fingerprint": binding["fingerprint"],
            "generation": binding["generation"],
            "session_id": binding["session_id"],
        }
        if portal_id is not None:
            entry["portal_id"] = portal_id
        return entry

    # -- Cards --------------------------------------------------------------
    def create_card(self, title, body, entries):
        """Validate an explicit manifest, post ONE bus card, bind proposals."""
        if not entries:
            raise ValueError("batch_proposals is empty")
        keys = []
        with self.lock:
            for entry in entries:
                key = (entry.get("session_id"), entry.get("proposal_id"))
                binding = self.pending.get(key)
                if binding is None:
                    raise ValueError(f"unknown or stale proposal {key}")
                if key in self.decided:
                    raise ValueError(f"proposal already decided {key}")
                if key in self.carded:
                    raise ValueError(f"proposal already carded {key}")
                if entry.get("fingerprint") != binding["fingerprint"] or entry.get("generation") != binding["generation"]:
                    raise ValueError(f"binding mismatch for {key}")
                keys.append(key)
            kind = self.pending[keys[0]]["classification"] if len(keys) == 1 else "batch_commit"
        message = self.bus.post_card(title, body, kind, batch_proposals=entries)
        with self.lock:
            self.cards[message["id"]] = {"keys": keys, "consumed": False}
            self.carded.update(keys)
        log(f"card #{message['id']} bound to {len(keys)} proposal(s)")
        return message

    def poll_acks_once(self):
        for ack in self.bus.acks():
            stamp = (ack.get("message_id"), ack.get("action"), ack.get("source"), ack.get("received_at"))
            with self.lock:
                if stamp in self.seen_acks:
                    continue
                self.seen_acks.add(stamp)
            if ack.get("source") != "glasses_dat" or ack.get("action") not in ("approve", "reject"):
                with self.lock:
                    self.ignored_acks += 1
                continue
            self._apply_glasses_ack(ack)

    def _apply_glasses_ack(self, ack):
        with self.lock:
            card = self.cards.get(ack.get("message_id"))
            if card is None or card["consumed"]:
                log(f"glasses ack for unknown/consumed card #{ack.get('message_id')} ignored")
                return
            card["consumed"] = True
            bindings = []
            for key in card["keys"]:
                binding = self.pending.pop(key, None)
                if binding is not None and key not in self.decided:
                    self.decided.add(key)
                    bindings.append(binding)
                    self.decisions.append(
                        {"at": now(), "via": "glasses_dat", "decision": ack["action"],
                         "card_message_id": ack.get("message_id"),
                         "session_id": key[0], "proposal_id": key[1]}
                    )
        for binding in bindings:
            binding["client"].decide(binding, ack["action"], "user", "glasses")
            log(f"glasses {ack['action']} -> decision {(binding['session_id'], binding['proposal_id'])}")

    def ack_loop(self):
        while True:
            try:
                self.poll_acks_once()
            except Exception as e:  # bus may restart during demo reset
                log(f"ack poll failed: {e}")
            time.sleep(0.5)

    def snapshot_pending(self):
        with self.lock:
            return [
                {k: v for k, v in b.items() if k != "client"} for b in self.pending.values()
            ]

    def health(self):
        with self.lock:
            return {
                "ok": True,
                "server": "hermes-decision-bridge",
                "card_mode": self.card_mode,
                "sockets": [str(c.sock_path) for c in self.clients],
                "sessions": [c.session_id for c in self.clients],
                "pending": len(self.pending),
                "cards": len(self.cards),
                "decisions": len(self.decisions),
                "ignored_acks": self.ignored_acks,
            }


def make_handler(bridge):
    class Handler(BaseHTTPRequestHandler):
        def send_json(self, status, payload):
            content = json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(content)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(content)

        def authorized(self):
            return bool(BUS_TOKEN) and hmac.compare_digest(
                self.headers.get("X-Bus-Token", ""), BUS_TOKEN
            )

        def do_GET(self):
            if self.path == "/health":
                return self.send_json(200, bridge.health())
            if not self.authorized():
                return self.send_json(401, {"error": "Unauthorized"})
            if self.path == "/pending":
                return self.send_json(200, {"pending": bridge.snapshot_pending()})
            if self.path == "/decisions":
                with bridge.lock:
                    return self.send_json(200, {"decisions": list(bridge.decisions)})
            return self.send_json(404, {"error": "Not found"})

        def do_POST(self):
            if not self.authorized():
                return self.send_json(401, {"error": "Unauthorized"})
            if self.path != "/cards":
                return self.send_json(404, {"error": "Not found"})
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0 or length > 262144:
                return self.send_json(413, {"error": "Invalid request size"})
            try:
                payload = json.loads(self.rfile.read(length))
            except json.JSONDecodeError:
                return self.send_json(400, {"error": "Invalid JSON"})
            try:
                message = bridge.create_card(
                    title=str(payload.get("title") or "Approve")[:120],
                    body=str(payload.get("body") or "Approve"),
                    entries=payload.get("batch_proposals") or [],
                )
            except ValueError as e:
                return self.send_json(409, {"error": str(e)})
            except Exception as e:
                return self.send_json(502, {"error": f"bus post failed: {e}"})
            return self.send_json(201, message)

        def log_message(self, *_args):
            pass

    return Handler


def read_host_token(args, sock_path):
    if args.host_token_file:
        with open(args.host_token_file) as f:
            return f.read().strip()
    sibling = f"{sock_path}.host.token"
    if os.path.exists(sibling):
        with open(sibling) as f:
            return f.read().strip()
    return os.environ.get("HERMES_HOST_TOKEN", "")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--socket", action="append", required=True, dest="sockets")
    parser.add_argument("--host-token-file", default=None)
    parser.add_argument("--bus-url", default="http://127.0.0.1:19099")
    parser.add_argument("--http-host", default="127.0.0.1")
    parser.add_argument("--http-port", type=int, default=19101)
    parser.add_argument("--card-mode", choices=["immediate", "held"], default="immediate")
    parser.add_argument("--start-url", default=None, help="open a session on the first socket")
    parser.add_argument("--session-title", default="Walkaway Hermes Host Bridge")
    args = parser.parse_args()
    if not BUS_TOKEN:
        raise SystemExit("WALKAWAY_BUS_TOKEN is required")

    bridge = Bridge(Bus(args.bus_url, BUS_TOKEN), args.card_mode)
    for sock_path in args.sockets:
        token = read_host_token(args, sock_path)
        if not token:
            raise SystemExit(f"No Hermes host token for {sock_path}")
        client = HostClient(bridge, sock_path, token)
        bridge.clients.append(client)
        threading.Thread(target=client.run, daemon=True).start()
    if args.start_url:
        for client in bridge.clients:
            client.open_session(args.session_title, args.start_url)
    threading.Thread(target=bridge.ack_loop, daemon=True).start()
    server = ThreadingHTTPServer((args.http_host, args.http_port), make_handler(bridge))
    log(f"decision bridge http://{args.http_host}:{args.http_port} card_mode={args.card_mode}")
    server.serve_forever()


if __name__ == "__main__":
    main()
