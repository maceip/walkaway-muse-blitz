#!/usr/bin/env python3
"""Slice-1 End-to-End Acceptance Test (Real Fixture + Real Hermes Binary + Real Vault + Host Approval).

Follows /Users/mac/walkaway-monorepo/docs/demo-plan.md lines 25-34:
- Launches the real localhost signup fixture with a mutable SQLite account backend.
- Launches the real hermes-blitz-browser binary over --control-socket with distinct
  host and agent tokens.
- Observes the real signup form through Hermes on the agent channel.
- Proposes and host-approves supervised form filling + blind vault secret injection
  (fill_form + vault.create_account) and final form submission (submit).
- Asserts directly against the fixture SQLite backend that:
  1. Nothing is committed before host approval of the submit proposal.
  2. Exactly 1 account row is created after host approval of submit, with the exact
     non-sensitive fields and a >=16-char password whose SHA-256 matches the vault's
     generated secret.
  3. The plaintext password never appears anywhere in observe snapshots, proposals,
     events, audit records, or browser stdout/stderr logs.
"""

import contextlib
import hashlib
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
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "demo-fixtures"))
sys.path.insert(0, str(ROOT / "services" / "agent-driver"))
import hermes_agent_driver  # noqa: E402
import signup_fixture  # noqa: E402

HERMES_BIN = hermes_agent_driver.resolve_hermes_binary()


class ControlClient:
    """Line-delimited NDJSON Unix-socket client for hermes-blitz-browser."""

    def __init__(self, sock_path: Path, token: str):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(8.0)
        self.sock.connect(str(sock_path))
        self.reader = self.sock.makefile("r", encoding="utf-8")
        self.seq = 0
        self.frames: list[dict] = []
        self.raw_lines: list[str] = []
        self.send({"type": "hello", "request_id": "hello-1", "token": token})
        hello_ok = self.read_until_type("hello.ok")
        if hello_ok.get("type") != "hello.ok":
            raise RuntimeError(f"Handshake failed: {hello_ok}")

    def send(self, payload: dict) -> None:
        self.seq += 1
        frame = {"v": 1, "seq": self.seq, **payload}
        line = json.dumps(frame) + "\n"
        self.sock.sendall(line.encode("utf-8"))

    def read_frame(self) -> dict:
        line = self.reader.readline()
        if not line:
            raise EOFError("Control socket closed by browser")
        self.raw_lines.append(line)
        frame = json.loads(line)
        self.frames.append(frame)
        return frame

    def read_until_type(self, expected_types: str | set[str], timeout_s: float = 8.0) -> dict:
        if isinstance(expected_types, str):
            expected_types = {expected_types}
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            frame = self.read_frame()
            if frame.get("type") in expected_types or frame.get("type") == "refused":
                return frame
        raise TimeoutError(f"Timed out waiting for {expected_types}")

    def close(self) -> None:
        try:
            self.reader.close()
        finally:
            self.sock.close()


class Slice1SignupE2ETest(unittest.TestCase):
    def test_slice1_supervised_signup_and_blind_vault_commit(self) -> None:
        self.assertTrue(HERMES_BIN.exists(), f"Hermes binary missing at {HERMES_BIN}")

        with tempfile.TemporaryDirectory(prefix="walkaway-slice1-") as tmpdir:
            tmp = Path(tmpdir)
            db_path = tmp / "accounts.sqlite3"
            sock_path = tmp / "hermes.sock"
            vault_path = tmp / "vault.json"
            audit_path = tmp / "audit.jsonl"

            server = signup_fixture.start_server("127.0.0.1", 0, db_path)
            port = server.server_port
            server_thread = threading.Thread(target=server.serve_forever, daemon=True)
            server_thread.start()
            signup_url = f"http://127.0.0.1:{port}/signup"

            host_token = "host-secret-token-slice1-998877"
            agent_token = "agent-secret-token-slice1-112233"

            env = os.environ.copy()
            env["HERMES_HOST_TOKEN"] = host_token
            env["HERMES_AGENT_TOKEN"] = agent_token
            env["HERMES_VAULT_PATH"] = str(vault_path)
            # Keep the test out of the credential store the user actually uses.
            env["HERMES_PASSKEY_KEYCHAIN_SERVICE"] = "ai.zmax.hermes.browser.test.e2e.passkeys"
            env["HERMES_PASSWORD_KEYCHAIN_SERVICE"] = "ai.zmax.hermes.browser.test.e2e.credentials"
            env["HERMES_AUDIT_PATH"] = str(audit_path)

            proc = subprocess.Popen(
                [str(HERMES_BIN), "--control-socket", str(sock_path)],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            try:
                deadline = time.monotonic() + 8.0
                while not sock_path.exists() and time.monotonic() < deadline:
                    if proc.poll() is not None:
                        out, err = proc.communicate(timeout=2)
                        self.fail(f"Hermes exited early ({proc.returncode}):\nSTDOUT:\n{out}\nSTDERR:\n{err}")
                    time.sleep(0.05)
                self.assertTrue(sock_path.exists(), "Hermes did not bind --control-socket in time")

                host = ControlClient(sock_path, host_token)
                agent = ControlClient(sock_path, agent_token)

                try:
                    # 1. Host opens a supervised session at the real localhost signup URL.
                    host.send(
                        {
                            "type": "session.open",
                            "request_id": "open-1",
                            "title": "DeepSec 2026 Signup",
                            "start_url": signup_url,
                            "capabilities": {"protocol": 1},
                            "policy": {"decision": "human_per_action"},
                        }
                    )
                    opened = host.read_until_type("session.opened")
                    self.assertEqual(opened.get("type"), "session.opened", f"Unexpected open reply: {opened}")
                    session_id = opened["session_id"]

                    # Wait for the initial navigation to /signup to settle and observe the form.
                    snap = None
                    for attempt in range(15):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"obs-init-{attempt}",
                                "include_screenshot": False,
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        self.assertEqual(cand.get("type"), "observe.result", f"Observe failed: {cand}")
                        els = cand.get("elements") or []
                        if any(e.get("id") == "speaker_name" or e.get("name") == "speaker_name" or e.get("label") == "Speaker Full Name" for e in els):
                            snap = cand
                            break
                        time.sleep(0.1)

                    self.assertIsNotNone(
                        snap,
                        f"Signup form controls were never indexed in snapshot. Last snapshot: {cand}",
                    )

                    # Verify capabilities report form filling, select, submit, and vault as supported.
                    agent.send({"type": "capabilities.get", "request_id": "caps-1"})
                    caps = agent.read_until_type("capabilities")
                    self.assertEqual(caps.get("text_input"), "supported", f"Capabilities not ready: {caps}")
                    self.assertEqual(caps.get("select"), "supported", f"Capabilities not ready: {caps}")
                    self.assertEqual(caps.get("fill_form"), "supported", f"Capabilities not ready: {caps}")
                    self.assertEqual(caps.get("submit"), "supported", f"Capabilities not ready: {caps}")
                    self.assertEqual(caps.get("vault"), "supported", f"Capabilities not ready: {caps}")

                    # Lookup durable refs for each form control from the observed snapshot.
                    by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    for required_id in ("speaker_name", "email", "password", "talk_title", "track", "live_demo", "submit-btn"):
                        self.assertIn(required_id, by_id, f"Control #{required_id} missing from snapshot elements: {by_id.keys()}")

                    # 2. Agent proposes atomic fill_form (including vault blind secret generation for #password).
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-fill-1",
                            "proposal_id": "prop-fill-1",
                            "snapshot_id": snap["snapshot_id"],
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "operation": {
                                "kind": "fill_form",
                                "fields": [
                                    {
                                        "target_ref": by_id["speaker_name"]["node_ref"],
                                        "action": "type",
                                        "value": "Ryan MacArthur",
                                    },
                                    {
                                        "target_ref": by_id["email"]["node_ref"],
                                        "action": "type",
                                        "value": "ryan@zunftmax.secure.build",
                                    },
                                    {
                                        "target_ref": by_id["password"]["node_ref"],
                                        "action": "fill_secret",
                                        "origin": f"http://127.0.0.1:{port}",
                                        "username": "ryan@zunftmax.secure.build",
                                    },
                                    {
                                        "target_ref": by_id["talk_title"]["node_ref"],
                                        "action": "type",
                                        "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                    },
                                    {
                                        "target_ref": by_id["track"]["node_ref"],
                                        "action": "select",
                                        "value": "sandboxing",
                                    },
                                    {
                                        "target_ref": by_id["live_demo"]["node_ref"],
                                        "action": "check",
                                        "value": "true",
                                    },
                                ],
                            },
                            "rationale": "Stage speaker signup and CFP fields with blind vault password",
                        }
                    )
                    pending_fill = host.read_until_type("proposal.pending")
                    self.assertEqual(pending_fill.get("type"), "proposal.pending", f"fill_form proposal failed: {pending_fill}")

                    # Host approves the fill_form staging step.
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-fill-1",
                            "proposal_id": "prop-fill-1",
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    exec_fill = host.read_until_type("executed")
                    self.assertEqual(exec_fill.get("type"), "executed", f"fill_form execution failed: {exec_fill}")

                    # Verify that staging fill_form did NOT submit the form yet (0 accounts in DB).
                    with contextlib.closing(sqlite3.connect(db_path)) as conn:
                        count_before_submit = conn.execute("SELECT COUNT(*) FROM accounts").fetchone()[0]
                    self.assertEqual(count_before_submit, 0, "fill_form must stage fields without submitting the form")

                    # 3. Agent observes the staged form and proposes final submit.
                    agent.send(
                        {
                            "type": "observe.request",
                            "session_id": session_id,
                            "request_id": "obs-staged-1",
                            "include_screenshot": False,
                        }
                    )
                    snap_staged = agent.read_until_type("observe.result")
                    self.assertEqual(snap_staged.get("type"), "observe.result")
                    staged_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_staged.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }

                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-submit-1",
                            "proposal_id": "prop-submit-1",
                            "snapshot_id": snap_staged["snapshot_id"],
                            "fingerprint": snap_staged["fingerprint"],
                            "generation": snap_staged["generation"],
                            "operation": {
                                "kind": "submit",
                                "target_ref": staged_by_id["submit-btn"]["node_ref"],
                            },
                            "rationale": "Submit staged DeepSec 2026 speaker account and CFP form",
                        }
                    )
                    pending_submit = host.read_until_type("proposal.pending")
                    self.assertEqual(pending_submit.get("type"), "proposal.pending", f"submit proposal failed: {pending_submit}")

                    # Host approves the final submit.
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-submit-1",
                            "proposal_id": "prop-submit-1",
                            "fingerprint": snap_staged["fingerprint"],
                            "generation": snap_staged["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    exec_submit = host.read_until_type("executed")
                    self.assertEqual(exec_submit.get("type"), "executed", f"submit execution failed: {exec_submit}")

                    # Wait briefly for the POST /signup navigation to commit to the SQLite DB.
                    rows = []
                    for _ in range(20):
                        with contextlib.closing(sqlite3.connect(db_path)) as conn:
                            conn.row_factory = sqlite3.Row
                            rows = [dict(r) for r in conn.execute("SELECT * FROM accounts").fetchall()]
                        if rows:
                            break
                        time.sleep(0.1)

                    self.assertEqual(len(rows), 1, f"Expected exactly 1 created account in fixture DB, got {rows}")
                    acct = rows[0]
                    self.assertEqual(acct["speaker_name"], "Ryan MacArthur")
                    self.assertEqual(acct["email"], "ryan@zunftmax.secure.build")
                    self.assertEqual(
                        acct["talk_title"],
                        "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                    )
                    self.assertEqual(acct["track"], "sandboxing")
                    self.assertEqual(acct["live_demo"], "yes")
                    self.assertEqual(acct["confirmation_ref"], "yoink=926849de55")
                    self.assertGreaterEqual(acct["password_len"], 16)

                    # 4. Read the generated secret from the vault file ONLY inside the test to verify:
                    #    (a) its SHA-256 matches the password submitted to the fixture DB, and
                    #    (b) the plaintext password NEVER appeared in any socket frame, audit log, or stdio log.
                    self.assertTrue(vault_path.exists(), "Vault file was not persisted by hermes-blitz-browser")
                    vault_data = json.loads(vault_path.read_text(encoding="utf-8"))
                    plaintext_secret = vault_data["TestPlaintextOrSecretForVerification"] if "TestPlaintextOrSecretForVerification" in vault_data else vault_data["accounts"][0]["secret"]
                    self.assertEqual(
                        hashlib.sha256(plaintext_secret.encode("utf-8")).hexdigest(),
                        acct["password_sha256"],
                        "Submitted password SHA-256 does not match vault secret",
                    )

                    # Observe confirmation page and close.
                    agent.send(
                        {
                            "type": "observe.request",
                            "session_id": session_id,
                            "request_id": "obs-final-1",
                            "include_screenshot": False,
                        }
                    )
                    snap_final = agent.read_until_type("observe.result")
                    self.assertEqual(snap_final.get("type"), "observe.result")

                finally:
                    host.close()
                    agent.close()
            finally:
                proc.terminate()
                try:
                    stdout_text, stderr_text = proc.communicate(timeout=4)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    stdout_text, stderr_text = proc.communicate()
                server.shutdown()
                server.server_close()

            # Assert plaintext secret never leaked into any frame, audit log, stdout, or stderr.
            if vault_path.exists():
                vault_data = json.loads(vault_path.read_text(encoding="utf-8"))
                accounts_list = vault_data.get("accounts") or []
                if accounts_list and "secret" in accounts_list[0]:
                    secret = accounts_list[0]["secret"]
                    combined_wire = "\n".join(host.raw_lines + agent.raw_lines)
                    audit_text = audit_path.read_text(encoding="utf-8") if audit_path.exists() else ""
                    self.assertNotIn(secret, combined_wire, "Plaintext password leaked in control socket frames!")
                    self.assertNotIn(secret, audit_text, "Plaintext password leaked in audit log!")
                    self.assertNotIn(secret, stdout_text, "Plaintext password leaked in browser stdout!")
                    self.assertNotIn(secret, stderr_text, "Plaintext password leaked in browser stderr!")


if __name__ == "__main__":
    unittest.main(verbosity=2)
