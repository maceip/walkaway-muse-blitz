#!/usr/bin/env python3
"""Slice-3 End-to-End Acceptance Test: Zero-LLM FormRecipe Recall & Instant Sign-In / Replay.

Follows /Users/mac/walkaway-monorepo/docs/demo-plan.md Storyline #3:
- Launches the real localhost fixture (/signup and /signin) backed by SQLite (`accounts` and `logins`).
- Launches the real hermes-blitz-browser binary with `HERMES_VAULT_PATH`, `HERMES_RECALL_PATH`,
  and `HERMES_AUDIT_PATH`.
- Phase A: Creates the speaker account on `/signup` (with blind vault password generation) and
  performs the initial supervised sign-in on `/signin`, recording the structural `FormRecipe` in
  `HERMES_RECALL_PATH`.
- Phase B: Navigates to `/signin` again, observes that `observe.result` exposes `form_signature`
  and `matched_recipe_id`, runs `hermes_agent_driver.plan_recall_or_llm(...)` with
  `llm_calls == 0` (`< 5ms`), proposes `{"kind": "replay_recipe"}` + `submit`, host-approves,
  and asserts:
  1. SQLite `logins` table records both authenticated sign-ins (`len(logins) == 2`).
  2. Recall path made `0` LLM calls and completed in `< 5ms`.
  3. The plaintext password appears nowhere in `HERMES_RECALL_PATH`, `observe.result`, proposals,
     events, `HERMES_AUDIT_PATH`, stdout, or stderr.
"""

import contextlib
import json
import os
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
sys.path.insert(0, str(ROOT / "tests" / "e2e"))

import hermes_agent_driver  # noqa: E402
import signup_fixture  # noqa: E402
from test_slice1_signup import ControlClient, HERMES_BIN  # noqa: E402


class Slice3RecallAndSigninE2ETest(unittest.TestCase):
    def test_slice3_zero_llm_form_recipe_recall_and_instant_signin(self) -> None:
        self.assertTrue(HERMES_BIN.exists(), f"Hermes binary missing at {HERMES_BIN}")

        with tempfile.TemporaryDirectory(prefix="walkaway-slice3-") as tmpdir:
            tmp = Path(tmpdir)
            db_path = tmp / "accounts.sqlite3"
            sock_path = tmp / "hermes.sock"
            vault_path = tmp / "vault.json"
            recall_path = tmp / "recall.json"
            audit_path = tmp / "audit.jsonl"

            server = signup_fixture.start_server("127.0.0.1", 0, db_path)
            port = server.server_port
            server_thread = threading.Thread(target=server.serve_forever, daemon=True)
            server_thread.start()
            origin = f"http://127.0.0.1:{port}"
            signup_url = f"{origin}/signup"
            signin_url = f"{origin}/signin"

            host_token = "host-secret-token-slice3-998877"
            agent_token = "agent-secret-token-slice3-112233"

            env = os.environ.copy()
            env["HERMES_HOST_TOKEN"] = host_token
            env["HERMES_AGENT_TOKEN"] = agent_token
            env["HERMES_VAULT_PATH"] = str(vault_path)
            env["HERMES_RECALL_PATH"] = str(recall_path)
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
                    # Verify capabilities report recall_recipe as supported
                    agent.send({"type": "capabilities.get", "request_id": "caps-s3"})
                    caps = agent.read_until_type("capabilities")
                    self.assertEqual(caps.get("recall"), "supported", f"Recall capability missing: {caps}")

                    # 1. Open session at /signup and create the account in SQLite + Vault
                    host.send(
                        {
                            "type": "session.open",
                            "request_id": "open-s3",
                            "title": "DeepSec 2026 Recall & Sign-In",
                            "start_url": signup_url,
                            "capabilities": {"protocol": 1},
                            "policy": {"decision": "human_per_action"},
                        }
                    )
                    opened = host.read_until_type("session.opened")
                    session_id = opened["session_id"]

                    snap = None
                    for attempt in range(15):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"obs-s3-signup-{attempt}",
                                "include_screenshot": False,
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        els = cand.get("elements") or []
                        if any((e.get("dom_id") or e.get("name")) == "speaker_name" for e in els):
                            snap = cand
                            break
                        time.sleep(0.1)
                    self.assertIsNotNone(snap)

                    by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-signup-fill",
                            "proposal_id": "prop-s3-signup-fill",
                            "snapshot_id": snap["snapshot_id"],
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "operation": {
                                "kind": "fill_form",
                                "fields": [
                                    {"target_ref": by_id["speaker_name"]["node_ref"], "action": "type", "value": "Ryan MacArthur"},
                                    {"target_ref": by_id["email"]["node_ref"], "action": "type", "value": "ryan@zunftmax.secure.build"},
                                    {
                                        "target_ref": by_id["password"]["node_ref"],
                                        "action": "fill_secret",
                                        "origin": origin,
                                        "username": "ryan@zunftmax.secure.build",
                                    },
                                    {
                                        "target_ref": by_id["talk_title"]["node_ref"],
                                        "action": "type",
                                        "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                    },
                                    {"target_ref": by_id["track"]["node_ref"], "action": "select", "value": "sandboxing"},
                                    {"target_ref": by_id["live_demo"]["node_ref"], "action": "check", "value": "true"},
                                ],
                            },
                            "rationale": "Create speaker account",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-signup-fill",
                            "proposal_id": "prop-s3-signup-fill",
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "obs-s3-signup-staged"})
                    snap_staged = agent.read_until_type("observe.result")
                    staged_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_staged.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-signup-sub",
                            "proposal_id": "prop-s3-signup-sub",
                            "snapshot_id": snap_staged["snapshot_id"],
                            "fingerprint": snap_staged["fingerprint"],
                            "generation": snap_staged["generation"],
                            "operation": {"kind": "submit", "target_ref": staged_by_id["submit-btn"]["node_ref"]},
                            "rationale": "Submit speaker signup",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-signup-sub",
                            "proposal_id": "prop-s3-signup-sub",
                            "fingerprint": snap_staged["fingerprint"],
                            "generation": snap_staged["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # 2. Navigate to /signin for initial sign-in (which records the /signin FormRecipe)
                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "obs-s3-after-signup"})
                    snap_after_signup = agent.read_until_type("observe.result")
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-nav-signin1",
                            "proposal_id": "prop-s3-nav-signin1",
                            "snapshot_id": snap_after_signup["snapshot_id"],
                            "fingerprint": snap_after_signup["fingerprint"],
                            "generation": snap_after_signup["generation"],
                            "operation": {"kind": "navigate", "url": signin_url},
                            "rationale": "Navigate to speaker sign-in",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-nav-signin1",
                            "proposal_id": "prop-s3-nav-signin1",
                            "fingerprint": snap_after_signup["fingerprint"],
                            "generation": snap_after_signup["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    snap_signin1 = None
                    for attempt in range(15):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"obs-s3-signin1-{attempt}",
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        els = cand.get("elements") or []
                        if any((e.get("dom_id") or e.get("name")) == "signin-btn" for e in els):
                            snap_signin1 = cand
                            break
                        time.sleep(0.1)
                    self.assertIsNotNone(snap_signin1)
                    self.assertTrue(snap_signin1.get("form_signature", "").startswith("sig_"))

                    s1_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_signin1.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-signin1-fill",
                            "proposal_id": "prop-s3-signin1-fill",
                            "snapshot_id": snap_signin1["snapshot_id"],
                            "fingerprint": snap_signin1["fingerprint"],
                            "generation": snap_signin1["generation"],
                            "operation": {
                                "kind": "fill_form",
                                "fields": [
                                    {"target_ref": s1_by_id["email"]["node_ref"], "action": "type", "value": "ryan@zunftmax.secure.build"},
                                    {
                                        "target_ref": s1_by_id["password"]["node_ref"],
                                        "action": "fill_secret",
                                        "origin": origin,
                                        "username": "ryan@zunftmax.secure.build",
                                    },
                                ],
                            },
                            "rationale": "Stage speaker credentials on /signin",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-signin1-fill",
                            "proposal_id": "prop-s3-signin1-fill",
                            "fingerprint": snap_signin1["fingerprint"],
                            "generation": snap_signin1["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "obs-s3-signin1-staged"})
                    snap_signin1_staged = agent.read_until_type("observe.result")
                    s1_staged_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_signin1_staged.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-signin1-sub",
                            "proposal_id": "prop-s3-signin1-sub",
                            "snapshot_id": snap_signin1_staged["snapshot_id"],
                            "fingerprint": snap_signin1_staged["fingerprint"],
                            "generation": snap_signin1_staged["generation"],
                            "operation": {"kind": "submit", "target_ref": s1_staged_by_id["signin-btn"]["node_ref"]},
                            "rationale": "Submit speaker sign-in #1",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-signin1-sub",
                            "proposal_id": "prop-s3-signin1-sub",
                            "fingerprint": snap_signin1_staged["fingerprint"],
                            "generation": snap_signin1_staged["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Verify 1 login row so far
                    with contextlib.closing(sqlite3.connect(db_path)) as conn:
                        login_count_1 = conn.execute("SELECT COUNT(*) FROM logins").fetchone()[0]
                    self.assertEqual(login_count_1, 1)

                    # 3. Phase B: Navigate back to /signin and verify ZERO-LLM FormRecipe Recall!
                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "obs-s3-after-signin1"})
                    snap_after_signin1 = agent.read_until_type("observe.result")
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-nav-signin2",
                            "proposal_id": "prop-s3-nav-signin2",
                            "snapshot_id": snap_after_signin1["snapshot_id"],
                            "fingerprint": snap_after_signin1["fingerprint"],
                            "generation": snap_after_signin1["generation"],
                            "operation": {"kind": "navigate", "url": signin_url},
                            "rationale": "Re-open /signin to test zero-LLM recall",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-nav-signin2",
                            "proposal_id": "prop-s3-nav-signin2",
                            "fingerprint": snap_after_signin1["fingerprint"],
                            "generation": snap_after_signin1["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    snap_signin2 = None
                    for attempt in range(15):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"obs-s3-signin2-{attempt}",
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        els = cand.get("elements") or []
                        if any((e.get("dom_id") or e.get("name")) == "signin-btn" for e in els):
                            snap_signin2 = cand
                            break
                        time.sleep(0.1)
                    self.assertIsNotNone(snap_signin2)
                    self.assertIsNotNone(
                        snap_signin2.get("matched_recipe_id"),
                        f"Expected matched_recipe_id on repeat visit to /signin, got {snap_signin2}",
                    )

                    # Ask agent driver to plan the action: must return replay_recipe with 0 LLM calls!
                    plan = hermes_agent_driver.plan_recall_or_llm(
                        snapshot=snap_signin2,
                        speaker_profile={"email": "ryan@zunftmax.secure.build"},
                    )
                    self.assertEqual(plan["llm_calls"], 0, "FormRecipe recall must make 0 LLM calls")
                    self.assertLess(plan["latency_ms"], 5.0, "FormRecipe recall must be < 5ms")
                    self.assertEqual(plan["operation"]["kind"], "replay_recipe")

                    # Propose and approve replay_recipe
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-replay",
                            "proposal_id": "prop-s3-replay",
                            "snapshot_id": snap_signin2["snapshot_id"],
                            "fingerprint": snap_signin2["fingerprint"],
                            "generation": snap_signin2["generation"],
                            "operation": plan["operation"],
                            "rationale": "Zero-LLM FormRecipe replay for DeepSec 2026 sign-in",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-replay",
                            "proposal_id": "prop-s3-replay",
                            "fingerprint": snap_signin2["fingerprint"],
                            "generation": snap_signin2["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Observe staged replay and submit
                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "obs-s3-signin2-staged"})
                    snap_signin2_staged = agent.read_until_type("observe.result")
                    s2_staged_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_signin2_staged.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    self.assertEqual(s2_staged_by_id["email"].get("value"), "ryan@zunftmax.secure.build")
                    self.assertEqual(s2_staged_by_id["password"].get("value_state"), "sensitive")
                    self.assertIsNone(s2_staged_by_id["password"].get("value"))

                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s3-signin2-sub",
                            "proposal_id": "prop-s3-signin2-sub",
                            "snapshot_id": snap_signin2_staged["snapshot_id"],
                            "fingerprint": snap_signin2_staged["fingerprint"],
                            "generation": snap_signin2_staged["generation"],
                            "operation": {"kind": "submit", "target_ref": s2_staged_by_id["signin-btn"]["node_ref"]},
                            "rationale": "Submit replayed sign-in",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s3-signin2-sub",
                            "proposal_id": "prop-s3-signin2-sub",
                            "fingerprint": snap_signin2_staged["fingerprint"],
                            "generation": snap_signin2_staged["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Verify 2 authenticated logins in SQLite
                    with contextlib.closing(sqlite3.connect(db_path)) as conn:
                        conn.row_factory = sqlite3.Row
                        logins = [dict(r) for r in conn.execute("SELECT * FROM logins").fetchall()]
                    self.assertEqual(len(logins), 2, f"Expected 2 authenticated logins, got {logins}")
                    self.assertEqual(logins[1]["email"], "ryan@zunftmax.secure.build")

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

            # Verify recall store exists and plaintext password never leaked anywhere
            self.assertTrue(recall_path.exists(), "Recall store was not persisted by hermes-blitz-browser")
            vault_data = json.loads(vault_path.read_text(encoding="utf-8"))
            secret = vault_data["accounts"][0]["secret"]
            recall_text = recall_path.read_text(encoding="utf-8")
            audit_text = audit_path.read_text(encoding="utf-8") if audit_path.exists() else ""
            combined_wire = "\n".join(host.raw_lines + agent.raw_lines)

            self.assertNotIn(secret, recall_text, "Plaintext password leaked in recall.json!")
            self.assertNotIn(secret, combined_wire, "Plaintext password leaked in control socket frames!")
            self.assertNotIn(secret, audit_text, "Plaintext password leaked in audit log!")
            self.assertNotIn(secret, stdout_text, "Plaintext password leaked in browser stdout!")
            self.assertNotIn(secret, stderr_text, "Plaintext password leaked in browser stderr!")


if __name__ == "__main__":
    unittest.main(verbosity=2)
