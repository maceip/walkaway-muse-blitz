"""Minimal Walkaway laptop <-> Android message bus proof.

Run:
  WALKAWAY_BUS_TOKEN=walkaway-dev-bus-token \
  python3 services/hermes-host/message_bus.py --host 127.0.0.1 --port 19099

Send a message:
  curl -X POST http://127.0.0.1:19099/messages \
    -H 'Content-Type: application/json' \
    -H 'X-Bus-Token: walkaway-dev-bus-token' \
    -d '{"title":"Walkaway test","body":"Hello from the laptop","kind":"test"}'

The Android app polls /messages/next, posts an Android notification, and
returns /ack. State is intentionally in memory for the transport proof.
"""

import argparse
import fcntl
import hmac
import json
import os
import threading
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

TOKEN = os.environ.get("WALKAWAY_BUS_TOKEN", "")
STATE_PATH = os.environ.get("WALKAWAY_BUS_STATE", "/tmp/walkaway-bus-state.json")
LOCK = threading.Lock()
MESSAGES = []
ACKS = []
NEXT_ID = 1


def load_next_id():
    global NEXT_ID
    try:
        with open(STATE_PATH) as f:
            NEXT_ID = max(1, int(json.load(f).get("next_id", 1)))
    except (OSError, ValueError, json.JSONDecodeError):
        NEXT_ID = 1
    return NEXT_ID


def save_next_id():
    try:
        with open(STATE_PATH, "w") as f:
            json.dump({"next_id": NEXT_ID}, f)
    except OSError:
        pass


def now():
    return datetime.now(timezone.utc).isoformat()


class Handler(BaseHTTPRequestHandler):
    def send_json(self, status, payload):
        content = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(content)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(content)

    def authorized(self):
        return bool(TOKEN) and hmac.compare_digest(
            self.headers.get("X-Bus-Token", ""), TOKEN
        )

    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path == "/health":
            return self.send_json(200, {"ok": True, "server": "walkaway-message-bus"})
        if not self.authorized():
            return self.send_json(401, {"error": "Unauthorized"})
        if parsed.path == "/messages/next":
            try:
                after = int(parse_qs(parsed.query).get("after", ["0"])[0])
            except ValueError:
                after = 0
            with LOCK:
                effective_after = 0 if (MESSAGES and after >= NEXT_ID) else after
                message = next((m for m in MESSAGES if m["id"] > effective_after), None)
            return self.send_json(200, {"message": message})
        if parsed.path == "/acks":
            with LOCK:
                return self.send_json(200, {"acks": list(ACKS)})
        return self.send_json(404, {"error": "Not found"})

    def do_POST(self):
        parsed = urlparse(self.path)
        if not self.authorized():
            return self.send_json(401, {"error": "Unauthorized"})
        length = int(self.headers.get("Content-Length", "0"))
        if length <= 0 or length > 131072:
            return self.send_json(413, {"error": "Invalid request size"})
        try:
            payload = json.loads(self.rfile.read(length))
        except json.JSONDecodeError:
            return self.send_json(400, {"error": "Invalid JSON"})
        global NEXT_ID
        if parsed.path == "/messages":
            title = str(payload.get("title") or "Walkaway").strip()
            body = str(payload.get("body") or "").strip()
            if not body:
                return self.send_json(400, {"error": "body is required"})
            batch = payload.get("batch_proposals")
            if not isinstance(batch, list):
                batch = None
            with LOCK:
                message = {
                    "id": NEXT_ID,
                    "title": title[:120],
                    "body": body[:2000],
                    "kind": str(payload.get("kind") or "message")[:60],
                    "created_at": now(),
                }
                if batch is not None:
                    message["batch_proposals"] = batch[:32]
                # Optional purchase-card fields (Muse buy-approval demo).
                image_url = str(payload.get("image_url") or "").strip()
                price = str(payload.get("price") or "").strip()
                detail = str(payload.get("detail") or "").strip()
                if image_url:
                    message["image_url"] = image_url[:500]
                if price:
                    message["price"] = price[:40]
                if detail:
                    message["detail"] = detail[:240]
                # Base64 page-preview crop from the browser (protocol budget:
                # max 320px wide, 48k base64 chars — see hermes_task_relay).
                image_b64 = "".join(str(payload.get("image_b64") or "").split())
                if image_b64:
                    message["image_b64"] = image_b64[:48000]
                NEXT_ID += 1
                save_next_id()
                MESSAGES.append(message)
            print(f"[bus] queued #{message['id']} {message['title']!r}: {message['body']!r}", flush=True)
            return self.send_json(201, message)
        if parsed.path == "/ack":
            ack = {
                "message_id": payload.get("message_id"),
                "action": str(payload.get("action") or "received")[:60],
                "source": str(payload.get("source") or "android")[:80],
                "received_at": now(),
            }
            with LOCK:
                ACKS.append(ack)
            print(
                f"[bus] ack message={ack['message_id']} action={ack['action']} source={ack['source']}",
                flush=True,
            )
            return self.send_json(201, ack)
        return self.send_json(404, {"error": "Not found"})

    def log_message(self, *_args):
        pass


class BusServer(ThreadingHTTPServer):
    # SO_REUSEADDR allowed 0.0.0.0:19099 and 127.0.0.1:19099 to dual-bind on
    # macOS during the 2026-11-04 live run; the flock in main() is the hard
    # guarantee, this removes the bind-level half of the trap.
    allow_reuse_address = False


_LOCK_FD = None


def acquire_bus_lock(port):
    """Exclusive per-port flock; exits non-zero if another bus owns the port."""
    global _LOCK_FD
    lock_path = os.environ.get("WALKAWAY_BUS_LOCK", f"/tmp/walkaway-bus-{port}.lock")
    fd = os.open(lock_path, os.O_WRONLY | os.O_CREAT, 0o644)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        os.close(fd)
        raise SystemExit(
            f"Refusing to start: another message bus holds the lock for port {port} ({lock_path})"
        )
    os.ftruncate(fd, 0)
    os.write(fd, f"{os.getpid()}\n".encode())
    _LOCK_FD = fd
    return lock_path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8899)
    args = parser.parse_args()
    if not TOKEN:
        raise SystemExit("WALKAWAY_BUS_TOKEN is required")
    lock_path = acquire_bus_lock(args.port)
    print(f"Bus next_id starts at {load_next_id()} (state: {STATE_PATH}, lock: {lock_path})", flush=True)
    server = BusServer((args.host, args.port), Handler)
    print(f"Walkaway message bus: http://{args.host}:{args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
