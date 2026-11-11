#!/usr/bin/env python3
"""P2 decision-bridge real-system E2E (Partner Lane P).

Real components: release Hermes binary (`--control-socket`), SQLite demo
fixture, `message_bus.py`, `decision_bridge.py`. The physical glasses tap is
represented exactly as the Android bridge emits the glasses tap: a bus ack
with `source="glasses_dat"` (the physical tap itself is proven separately by
the P1 hard gate; this test proves the bridge honors only that source).

Case A (immediate cards, 1 Hermes): policy auto-approve without a card,
forged/non-decision acks ignored, glasses reject blocks the commit, glasses
approve commits exactly once, replayed ack decides nothing twice.
Case B (held cards, 2 Hermes processes): tampered manifest -> 409; one batch
card from an explicit 2-proposal manifest + one glasses approve fans out to
two bound decisions on two sockets (accounts == 2).

Run: python3 services/hermes-host/test_decision_bridge_e2e.py
"""

import json
import os
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "demo-fixtures"))
sys.path.insert(0, str(ROOT / "tests" / "e2e"))

import signup_fixture  # noqa: E402
from test_slice1_signup import ControlClient, HERMES_BIN  # noqa: E402

BUS_SCRIPT = ROOT / "services" / "hermes-host" / "message_bus.py"
BRIDGE_SCRIPT = ROOT / "services" / "hermes-host" / "decision_bridge.py"
BUS_TOKEN = "walkaway-bridge-e2e-token"
HOST_TOKEN = "host-secret-token-bridge-9988"


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def http_json(url, payload=None, token=BUS_TOKEN):
    headers = {"X-Bus-Token": token}
    data = None
    method = "GET"
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode()
        method = "POST"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            return r.status, json.loads(r.read().decode())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode() or "{}")


def wait_until(fn, timeout=15.0, interval=0.1):
    deadline = time.monotonic() + timeout
    last_error = None
    while time.monotonic() < deadline:
        try:
            value = fn()
        except Exception as e:  # startup races: connection refused, EOF, ...
            last_error = e
            value = None
        if value:
            return value
        time.sleep(interval)
    raise TimeoutError(f"condition not met in time (last error: {last_error})")


class Harness:
    """Fixture server + bus + N Hermes processes + one bridge."""

    def __init__(self, test, tmp, hermes_count, card_mode, dynamic=False, allow_android=True,
                 start_url=None):
        self.test = test
        self.tmp = Path(tmp)
        self.db = self.tmp / "db.sqlite3"
        server = signup_fixture.start_server("127.0.0.1", 0, self.db)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.origin = f"http://127.0.0.1:{server.server_port}"
        self.bus_port = free_port()
        self.bus = f"http://127.0.0.1:{self.bus_port}"
        self.bridge_port = free_port()
        self.bridge = f"http://127.0.0.1:{self.bridge_port}"
        env = os.environ.copy()
        env["WALKAWAY_BUS_TOKEN"] = BUS_TOKEN
        env["WALKAWAY_BUS_STATE"] = str(self.tmp / "bus-state.json")
        self.procs = [
            subprocess.Popen(
                [sys.executable, str(BUS_SCRIPT), "--host", "127.0.0.1", "--port", str(self.bus_port)],
                env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
        ]
        self.agents = []
        socks = []
        for i in range(hermes_count):
            sock = self.tmp / f"h{i}.sock"
            socks.append(sock)
            henv = os.environ.copy()
            henv["HERMES_HOST_TOKEN"] = HOST_TOKEN
            henv["HERMES_HIDDEN_WINDOW"] = "1"
            henv["HERMES_AGENT_TOKEN"] = f"agent-secret-token-bridge-{i}777"
            henv["HERMES_VAULT_PATH"] = str(self.tmp / f"vault{i}.json")
            henv["HERMES_RECALL_PATH"] = str(self.tmp / f"recall{i}.json")
            henv["HERMES_AUDIT_PATH"] = str(self.tmp / f"audit{i}.jsonl")
            self.procs.append(
                subprocess.Popen(
                    [str(HERMES_BIN), "--control-socket", str(sock)],
                    env=henv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
            )
            self._agent_tokens = getattr(self, "_agent_tokens", []) + [henv["HERMES_AGENT_TOKEN"]]
        for sock in socks:
            wait_until(lambda s=sock: s.exists())
        benv = os.environ.copy()
        benv["WALKAWAY_BUS_TOKEN"] = BUS_TOKEN
        benv["HERMES_HOST_TOKEN"] = HOST_TOKEN
        cmd = [sys.executable, str(BRIDGE_SCRIPT), "--bus-url", self.bus,
               "--http-port", str(self.bridge_port), "--card-mode", card_mode,
               "--allow-android-approval" if allow_android else "--no-allow-android-approval"]
        if not dynamic:
            for sock in socks:
                cmd += ["--socket", str(sock)]
            cmd += ["--start-url", start_url or f"{self.origin}/signup"]
        self.procs.append(
            subprocess.Popen(cmd, env=benv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        )
        wait_until(lambda: http_json(f"{self.bridge}/health", token="")[1].get("ok"))
        if not dynamic:
            wait_until(
                lambda: len([s for s in http_json(f"{self.bridge}/health", token="")[1]["sessions"] if s])
                == hermes_count
            )
        self.socks = socks

    def attach(self, index=0):
        return http_json(
            f"{self.bridge}/sockets",
            {"sock_path": str(self.socks[index]), "start_url": f"{self.origin}/signup"},
        )

    def session_id(self, index=0):
        _, health = http_json(f"{self.bridge}/health", token="")
        sessions = [s for s in health["sessions"] if s]
        return sessions[index]

    def agent(self, index=0):
        while len(self.agents) <= index:
            i = len(self.agents)
            self.agents.append(ControlClient(self.socks[i], self._agent_tokens[i]))
        return self.agents[index]

    def decisions(self):
        return http_json(f"{self.bridge}/decisions")[1]["decisions"]

    def pending(self):
        return http_json(f"{self.bridge}/pending")[1]["pending"]

    def receipts(self, proposal_id, kind="executed"):
        return [
            d["receipt"] for d in self.decisions()
            if d.get("receipt", {}).get("type") == kind
            and d["receipt"].get("proposal_id") == proposal_id
        ]

    def bus_ack(self, message_id, action, source):
        return http_json(f"{self.bus}/ack", {"message_id": message_id, "action": action, "source": source})

    def audit_surfaces(self, index=0):
        out = []
        path = self.tmp / f"audit{index}.jsonl"
        if path.exists():
            for line in path.read_text().splitlines():
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if rec.get("result") == "executed":
                    out.append((rec.get("proposal_id"), rec.get("surface")))
        return out

    def account_count(self):
        with sqlite3.connect(self.db) as c:
            return c.execute("SELECT COUNT(*) FROM accounts").fetchone()[0]

    def stop(self):
        for p in reversed(self.procs):
            p.terminate()
        for p in reversed(self.procs):
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                p.kill()


def observe_until(agent, session_id, control_id, prefix):
    for attempt in range(20):
        agent.send({"type": "observe.request", "session_id": session_id,
                    "request_id": f"{prefix}-{attempt}"})
        snap = agent.read_until_type("observe.result")
        if any((e.get("dom_id") or e.get("name")) == control_id for e in snap.get("elements") or []):
            return snap
        time.sleep(0.1)
    raise AssertionError(f"control {control_id} never appeared")


def by_id(snap):
    return {
        (e.get("dom_id") or e.get("name") or e.get("id")): e
        for e in snap["elements"] if e.get("dom_id") or e.get("name") or e.get("id")
    }


def propose(agent, session_id, snap, proposal_id, operation, rationale):
    agent.send(
        {
            "type": "propose",
            "session_id": session_id,
            "request_id": proposal_id,
            "proposal_id": proposal_id,
            "snapshot_id": snap["snapshot_id"],
            "fingerprint": snap["fingerprint"],
            "generation": snap["generation"],
            "operation": operation,
            "rationale": rationale,
        }
    )


def full_signup_fields(els, origin, email):
    return [
        {"target_ref": els["speaker_name"]["node_ref"], "action": "type", "value": "Ryan MacArthur"},
        {"target_ref": els["email"]["node_ref"], "action": "type", "value": email},
        {"target_ref": els["password"]["node_ref"], "action": "fill_secret",
         "origin": origin, "username": email},
        {"target_ref": els["talk_title"]["node_ref"], "action": "type",
         "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework"},
        {"target_ref": els["track"]["node_ref"], "action": "select", "value": "sandboxing"},
        {"target_ref": els["live_demo"]["node_ref"], "action": "check", "value": "true"},
    ]


def wait_card(harness, after_id):
    def _next():
        _, payload = http_json(f"{harness.bus}/messages/next?after={after_id}")
        return payload.get("message")
    return wait_until(_next)


class BridgeImmediateE2ETest(unittest.TestCase):
    def test_policy_auto_glasses_only_reject_replay(self):
        self.assertTrue(HERMES_BIN.exists())
        with tempfile.TemporaryDirectory(prefix="walkaway-bridge-a-") as tmp:
            h = Harness(self, tmp, hermes_count=1, card_mode="immediate", allow_android=False)
            try:
                agent = h.agent(0)
                session = h.session_id(0)
                email = "ryan@zunftmax.secure.build"

                # 1. Plain fill_form (no secret) -> policy auto-approve, no card.
                snap = observe_until(agent, session, "speaker_name", "a-init")
                els = by_id(snap)
                propose(agent, session, snap, "a-fill-plain",
                        {"kind": "fill_form", "fields": [
                            {"target_ref": els["speaker_name"]["node_ref"], "action": "type",
                             "value": "Ryan MacArthur"}]},
                        "Stage speaker name")
                wait_until(lambda: h.receipts("a-fill-plain"))
                _, health = http_json(f"{h.bridge}/health", token="")
                self.assertEqual(health["cards"], 0)
                self.assertTrue(any(d.get("via") == "policy_auto" for d in h.decisions()))

                # 2. fill_form with fill_secret -> glasses card; forged acks ignored.
                snap = observe_until(agent, session, "password", "a-secret")
                propose(agent, session, snap, "a-fill-secret",
                        {"kind": "fill_form", "fields": full_signup_fields(by_id(snap), h.origin, email)},
                        "Stage account + blind vault password")
                card = wait_card(h, 0)
                self.assertEqual(card["batch_proposals"][0]["proposal_id"], "a-fill-secret")
                h.bus_ack(card["id"], "received", "android_app")
                h.bus_ack(card["id"], "dat_sent", "android_dat")
                h.bus_ack(card["id"], "approve", "android_app")  # phone-button forgery
                h.bus_ack(card["id"], "approve", "e2e-bus-bridge")  # legacy test source
                time.sleep(1.6)
                self.assertEqual(h.receipts("a-fill-secret"), [])
                self.assertEqual(len(h.pending()), 1)
                h.bus_ack(card["id"], "approve", "glasses_dat")
                wait_until(lambda: h.receipts("a-fill-secret"))

                # 3. submit -> glasses reject blocks the commit.
                snap = observe_until(agent, session, "submit-btn", "a-sub1")
                propose(agent, session, snap, "a-submit-1",
                        {"kind": "submit", "target_ref": by_id(snap)["submit-btn"]["node_ref"]},
                        "Commit speaker account")
                card2 = wait_card(h, card["id"])
                h.bus_ack(card2["id"], "reject", "glasses_dat")
                invalidated = wait_until(lambda: h.receipts("a-submit-1", kind="proposal.invalidated"))
                self.assertEqual(invalidated[0].get("reason"), "rejected")
                self.assertEqual(h.account_count(), 0)

                # 4. Re-proposed submit -> glasses approve commits once; replay no-ops.
                snap = observe_until(agent, session, "submit-btn", "a-sub2")
                propose(agent, session, snap, "a-submit-2",
                        {"kind": "submit", "target_ref": by_id(snap)["submit-btn"]["node_ref"]},
                        "Commit speaker account (retry)")
                card3 = wait_card(h, card2["id"])
                h.bus_ack(card3["id"], "approve", "glasses_dat")
                wait_until(lambda: h.receipts("a-submit-2"))
                self.assertEqual(h.account_count(), 1)
                decided_before = [d for d in h.decisions() if d.get("via") == "glasses_dat"]
                h.bus_ack(card3["id"], "approve", "glasses_dat")  # replay
                time.sleep(1.6)
                decided_after = [d for d in h.decisions() if d.get("via") == "glasses_dat"]
                self.assertEqual(len(decided_after), len(decided_before))
                self.assertEqual(h.account_count(), 1)
            finally:
                h.stop()


class BridgeHeldBatchE2ETest(unittest.TestCase):
    def test_batch_manifest_fanout_two_hermes(self):
        self.assertTrue(HERMES_BIN.exists())
        with tempfile.TemporaryDirectory(prefix="walkaway-bridge-b-") as tmp:
            h = Harness(self, tmp, hermes_count=2, card_mode="held")
            try:
                sessions = [h.session_id(0), h.session_id(1)]
                last_card_id = 0

                def card_and_approve(entries, title):
                    nonlocal last_card_id
                    status, message = http_json(
                        f"{h.bridge}/cards",
                        {"title": title, "body": title, "batch_proposals": entries},
                    )
                    self.assertEqual(status, 201, message)
                    last_card_id = message["id"]
                    h.bus_ack(message["id"], "approve", "glasses_dat")
                    return message

                def manifest_for(proposal_id):
                    pending = wait_until(
                        lambda: [p for p in h.pending() if p["proposal_id"] == proposal_id] or None
                    )
                    p = pending[0]
                    return {"proposal_id": p["proposal_id"], "fingerprint": p["fingerprint"],
                            "generation": p["generation"], "session_id": p["session_id"]}

                # Stage both signups; each fill is carded singly (Tap-1 shape).
                for i in range(2):
                    agent = h.agent(i)
                    snap = observe_until(agent, sessions[i], "password", f"b{i}-init")
                    propose(agent, sessions[i], snap, f"b{i}-fill",
                            {"kind": "fill_form",
                             "fields": full_signup_fields(by_id(snap), h.origin, f"ryan{i}@zunftmax.secure.build")},
                            f"Stage account {i}")
                    card_and_approve([manifest_for(f"b{i}-fill")], f"Approve fill {i}")
                    wait_until(lambda i=i: h.receipts(f"b{i}-fill"))

                # Propose both submits; held mode must have created no new card.
                entries = []
                for i in range(2):
                    agent = h.agent(i)
                    snap = observe_until(agent, sessions[i], "submit-btn", f"b{i}-sub")
                    propose(agent, sessions[i], snap, f"b{i}-submit",
                            {"kind": "submit", "target_ref": by_id(snap)["submit-btn"]["node_ref"]},
                            f"Commit account {i}")
                    entry = manifest_for(f"b{i}-submit")
                    entry["portal_id"] = f"portal-{i + 1}"
                    entries.append(entry)
                _, payload = http_json(f"{h.bus}/messages/next?after={last_card_id}")
                self.assertIsNone(payload.get("message"))

                # Tampered manifest -> 409, nothing carded.
                tampered = [dict(entries[0], fingerprint="f_forged"), entries[1]]
                status, _ = http_json(f"{h.bridge}/cards",
                                      {"title": "x", "body": "x", "batch_proposals": tampered})
                self.assertEqual(status, 409)

                # One batch card + one glasses approve -> two bound decisions.
                card_and_approve(entries, "Batch Commit Manifest for Portals #1-#2")
                wait_until(lambda: h.receipts("b0-submit") and h.receipts("b1-submit"))
                self.assertEqual(h.account_count(), 2)
                batch = [d for d in h.decisions()
                         if d.get("via") == "glasses_dat" and d.get("card_message_id") == last_card_id]
                self.assertEqual(len(batch), 2)
            finally:
                h.stop()


class BridgeDynamicAttachE2ETest(unittest.TestCase):
    def test_attach_socket_at_runtime_then_glasses_flow(self):
        self.assertTrue(HERMES_BIN.exists())
        with tempfile.TemporaryDirectory(prefix="walkaway-bridge-c-") as tmp:
            h = Harness(self, tmp, hermes_count=1, card_mode="immediate", dynamic=True)
            try:
                _, health = http_json(f"{h.bridge}/health", token="")
                self.assertEqual(health["sockets"], [])
                status, attached = h.attach(0)
                self.assertEqual(status, 201, attached)
                session = attached["session_id"]
                self.assertTrue(session)
                dup_status, _ = h.attach(0)
                self.assertEqual(dup_status, 409)

                agent = h.agent(0)
                snap = observe_until(agent, session, "password", "c-fill")
                propose(agent, session, snap, "c-fill",
                        {"kind": "fill_form",
                         "fields": full_signup_fields(by_id(snap), h.origin, "ryan@zunftmax.secure.build")},
                        "Stage account on dynamically attached socket")
                card = wait_card(h, 0)
                h.bus_ack(card["id"], "approve", "glasses_dat")
                receipts = wait_until(lambda: h.receipts("c-fill"))
                # Verbatim executed frame is readable by drivers (WS1 receipt contract).
                self.assertEqual(receipts[0].get("type"), "executed")
                self.assertIn("before_fingerprint", receipts[0])
            finally:
                h.stop()


class BridgeDualSurfaceE2ETest(unittest.TestCase):
    def test_android_app_and_glasses_both_commit_first_tap_wins(self):
        self.assertTrue(HERMES_BIN.exists())
        with tempfile.TemporaryDirectory(prefix="walkaway-bridge-d-") as tmp:
            h = Harness(self, tmp, hermes_count=1, card_mode="immediate", allow_android=True)
            try:
                _, health = http_json(f"{h.bridge}/health", token="")
                self.assertTrue(health["allow_android_approval"])
                agent = h.agent(0)
                session = h.session_id(0)
                email = "ryan@zunftmax.secure.build"

                # Fill approved from the PHONE surface (android_app).
                snap = observe_until(agent, session, "password", "d-fill")
                propose(agent, session, snap, "d-fill",
                        {"kind": "fill_form", "fields": full_signup_fields(by_id(snap), h.origin, email)},
                        "Stage account + blind vault password")
                card = wait_card(h, 0)
                self.assertLessEqual(len(card["title"]), 80)
                self.assertLessEqual(len(card["body"]), 160)
                self.assertIn("127.0.0.1", card["title"])  # domain-first
                h.bus_ack(card["id"], "received", "android_app")  # non-human ack still ignored
                time.sleep(1.2)
                self.assertEqual(h.receipts("d-fill"), [])
                h.bus_ack(card["id"], "approve", "android_app")
                wait_until(lambda: h.receipts("d-fill"))
                d = [x for x in h.decisions() if x.get("proposal_id") == "d-fill" and x.get("via")]
                self.assertEqual(d[0]["surface"], "android_app")

                # Submit: glasses tap wins; later phone ack on same card is ignored.
                snap = observe_until(agent, session, "submit-btn", "d-sub")
                propose(agent, session, snap, "d-submit",
                        {"kind": "submit", "target_ref": by_id(snap)["submit-btn"]["node_ref"]},
                        "Commit speaker account")
                card2 = wait_card(h, card["id"])
                h.bus_ack(card2["id"], "approve", "glasses_dat")
                wait_until(lambda: h.receipts("d-submit"))
                self.assertEqual(h.account_count(), 1)
                h.bus_ack(card2["id"], "reject", "android_app")  # loser surface: must not decide twice
                time.sleep(1.2)
                decided = [x for x in h.decisions() if x.get("proposal_id") == "d-submit" and x.get("via")]
                self.assertEqual(len(decided), 1)
                self.assertEqual(decided[0]["surface"], "glasses")
                # Hermes audit carries the same surface attribution.
                surfaces = dict(h.audit_surfaces(0))
                self.assertEqual(surfaces.get("d-fill"), "android_app")
                self.assertEqual(surfaces.get("d-submit"), "glasses")
            finally:
                h.stop()


class PasskeyPageServer:
    """A tiny HTTP server that serves one page which asks for a passkey.

    The demo fixture's own pages carry no script, so a page that really
    calls `navigator.credentials.create()` is served from here. The page is
    the point: it makes the browser start a *page-initiated* ceremony, which
    the browser must then card on the glasses rather than approve by itself.
    """

    PAGE = """<!DOCTYPE html><html><head><title>Passkey page</title></head><body>
      <div id="host">waiting</div>
      <button id="passkey-state">waiting</button>
      <script>
        navigator.credentials.create({
          publicKey: {
            rp: { id: "127.0.0.1", name: "Example" },
            user: { name: "ryan@example.com", displayName: "Ryan" },
            challenge: new Uint8Array([1, 2, 3, 4])
          }
        }).then(function(cred) {
          var status = "created:" + String(cred.id.byteLength);
          document.getElementById("host").setAttribute("data-result", status);
          document.getElementById("passkey-state").innerText = status;
        }).catch(function(err) {
          var status = "error:" + String(err && err.message ? err.message : err);
          document.getElementById("host").setAttribute("data-result", status);
          document.getElementById("passkey-state").innerText = status;
        });
      </script>
    </body></html>"""

    def __init__(self):
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

        page = self.PAGE

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):  # noqa: N802
                body = page.encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_port
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    @property
    def url(self):
        return f"http://127.0.0.1:{self.port}/passkey"

    def stop(self):
        self.server.shutdown()


class PagePasskeyGlassesE2ETest(unittest.TestCase):
    """A page-initiated passkey ceremony goes to the glasses as a card.

    The glasses DAT card is the primary surface for a passkey prompt, and
    the browser must not approve a page's own ceremony by itself. This
    drives the real chain: the page calls `navigator.credentials.create()`,
    the browser announces the ceremony as `proposal.pending`, the bridge
    cards it, and the glasses tap (a bus ack with `source="glasses_dat"`)
    decides it.
    """

    def test_page_passkey_ceremony_is_carded_and_decided_on_glasses(self):
        self.assertTrue(HERMES_BIN.exists())
        page_server = PasskeyPageServer()
        try:
            with tempfile.TemporaryDirectory(prefix="walkaway-passkey-") as tmp:
                h = Harness(self, tmp, hermes_count=1, card_mode="immediate",
                            allow_android=False, start_url=page_server.url)
                try:
                    h.agent(0)

                    # 1. The page's ceremony becomes a card, not an automatic
                    #    approval. The browser has no agent proposal here; the
                    #    ceremony is the page's own.
                    card = wait_card(h, 0)
                    entry = card["batch_proposals"][0]
                    self.assertTrue(
                        str(entry.get("proposal_id", "")).startswith("page-passkey-"),
                        f"expected a page-passkey proposal, got {entry.get('proposal_id')}",
                    )
                    self.assertEqual(card.get("kind"), "passkey")

                    # 2. Before the glasses answer, nothing is decided.
                    time.sleep(1.2)
                    decided = [d for d in h.decisions()
                               if str(d.get("proposal_id", "")).startswith("page-passkey-")]
                    self.assertEqual(decided, [],
                                     "the page's ceremony must not be decided before the glasses answer")

                    # 3. The glasses tap decides it.
                    h.bus_ack(card["id"], "approve", "glasses_dat")
                    decided = wait_until(
                        lambda: [d for d in h.decisions()
                                 if str(d.get("proposal_id", "")).startswith("page-passkey-")]
                        or None
                    )
                    self.assertEqual(len(decided), 1)
                    self.assertEqual(decided[0]["decision"], "approve")
                    self.assertEqual(decided[0]["surface"], "glasses")

                    # 4. The decision is consumed exactly once. A replayed tap
                    #    must not decide anything a second time.
                    h.bus_ack(card["id"], "approve", "glasses_dat")
                    time.sleep(1.6)
                    decided_after = [d for d in h.decisions()
                                     if str(d.get("proposal_id", "")).startswith("page-passkey-")
                                     and d.get("via") == "glasses_dat"]
                    self.assertEqual(len(decided_after), 1,
                                     "a replayed glasses tap must not decide twice")
                finally:
                    h.stop()
        finally:
            page_server.stop()

    def test_full_signup_fixture_webauthn_register_and_assert_on_glasses(self):
        self.assertTrue(HERMES_BIN.exists())
        with tempfile.TemporaryDirectory(prefix="walkaway-webauthn-full-") as tmp:
            h = Harness(self, tmp, hermes_count=1, card_mode="immediate",
                        allow_android=False)
            try:
                agent = h.agent(0)
                session = h.session_id(0)

                # Navigate to /signin?auto_passkey=register -> page runs navigator.credentials.create()
                # -> glasses card -> glasses approve -> form.submit() POSTs attestationObject to /signin
                # -> signup_fixture extracts P-256 public key from CBOR attestationObject and stores in DB!
                snap = observe_until(agent, session, "speaker_name", "w-init")
                propose(agent, session, snap, "w-nav-reg",
                        {"kind": "navigate", "url": f"{h.origin}/signin?auto_passkey=register"},
                        "Navigate to /signin to register hardware passkey")
                wait_until(lambda: h.receipts("w-nav-reg"))

                reg_card = wait_card(h, 0)
                self.assertEqual(reg_card.get("kind"), "passkey")
                self.assertIn("Register Passkey", reg_card.get("title", ""))
                h.bus_ack(reg_card["id"], "approve", "glasses_dat")

                def _cred_stored():
                    with sqlite3.connect(h.db) as c:
                        row = c.execute("SELECT credential_id, public_key_pem FROM webauthn_credentials").fetchone()
                        return row if row and "BEGIN PUBLIC KEY" in row[1] else None

                cred_row = wait_until(_cred_stored)
                self.assertTrue(cred_row[0])

                # Navigate to /signin?auto_passkey=assert -> page runs navigator.credentials.get()
                # -> glasses card -> glasses approve -> form.submit() POSTs signed assertion to /signin
                # -> signup_fixture verifies ECDSA P-256 signature via openssl and authenticates session!
                snap2 = observe_until(agent, session, "email", "w-reg-done")
                propose(agent, session, snap2, "w-nav-assert",
                        {"kind": "navigate", "url": f"{h.origin}/signin?auto_passkey=assert"},
                        "Navigate to /signin to sign in with hardware passkey")
                wait_until(lambda: h.receipts("w-nav-assert"))

                assert_card = wait_card(h, reg_card["id"])
                self.assertEqual(assert_card.get("kind"), "passkey")
                self.assertIn("Passkey Sign-In", assert_card.get("title", ""))
                h.bus_ack(assert_card["id"], "approve", "glasses_dat")

                def _assertion_verified():
                    with sqlite3.connect(h.db) as c:
                        a = c.execute("SELECT credential_id, verified, sign_count FROM webauthn_assertions WHERE verified = 1").fetchone()
                        l = c.execute("SELECT COUNT(*) FROM logins").fetchone()[0]
                        return (a, l) if a and l >= 1 else None

                verified_row, login_count = wait_until(_assertion_verified)
                self.assertEqual(verified_row[0], cred_row[0])
                self.assertEqual(verified_row[1], 1)
                self.assertGreaterEqual(verified_row[2], 1)
                self.assertEqual(login_count, 1)
            finally:
                h.stop()

    def test_held_mode_background_tab_passkey_ceremony_auto_cards_and_settles(self):
        self.assertTrue(HERMES_BIN.exists())
        page_server = PasskeyPageServer()
        try:
            with tempfile.TemporaryDirectory(prefix="walkaway-passkey-held-bg-") as tmp:
                # Live demo stack runs decision_bridge in card_mode="held" and attaches
                # background tabs via POST /sockets with background=True.
                h = Harness(self, tmp, hermes_count=1, card_mode="held",
                            dynamic=True, allow_android=False)
                try:
                    code, attached = http_json(
                        f"{h.bridge}/sockets",
                        {
                            "sock_path": str(h.socks[0]),
                            "start_url": page_server.url,
                            "background": True,
                        },
                    )
                    self.assertEqual(code, 201, attached)
                    agent = h.agent(0)
                    session = wait_until(lambda: h.session_id(0))

                    # Even in held mode (without POST /cards), a page-initiated passkey
                    # ceremony on a background tab is immediately carded to the bus.
                    card = wait_card(h, 0)
                    self.assertEqual(card.get("kind"), "passkey")
                    entry = card["batch_proposals"][0]
                    self.assertTrue(str(entry.get("proposal_id", "")).startswith("page-passkey-"))

                    # Glasses tap settles the background tab's WebAuthn promise.
                    h.bus_ack(card["id"], "approve", "glasses_dat")
                    snap = wait_until(
                        lambda: (
                            s if str(by_id(s := observe_until(agent, session, "passkey-state", "bg-pk"))
                                     .get("passkey-state", {}).get("label", "")).startswith("created:")
                            else None
                        )
                    )
                    self.assertTrue(by_id(snap)["passkey-state"]["label"].startswith("created:"))
                finally:
                    h.stop()
        finally:
            page_server.stop()


if __name__ == "__main__":
    unittest.main(verbosity=2)
