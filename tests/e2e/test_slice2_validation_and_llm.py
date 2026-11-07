#!/usr/bin/env python3
"""Slice-2 End-to-End Acceptance Test: Interactive Form Navigation & Self-Correcting Validation with Real Local Qwen3-4B.

Follows /Users/mac/walkaway-monorepo/docs/demo-plan.md Storyline #2:
- Launches the real localhost CFP fixture (/cfp-socon, modeled on Ryan's SO-CON 2027 CFP portal)
  backed by a SQLite `cfp_submissions` table.
- Launches the real hermes-blitz-browser binary over --control-socket.
- Step 1 (Initial incomplete attempt): Agent stages the CFP form missing `tshirt_size` and with a
  too-short `abstract_summary`, and host approves `submit`.
- Asserts that:
  1. The server refuses to commit the invalid submission (`0` rows in `cfp_submissions`).
  2. The browser renders the validation error banner (`#validation-error`) in the live DOM.
- Step 2 (Self-correcting validation via real local Qwen3-4B-Instruct-2507-4bit):
  3. `hermes_agent_driver.py` observes the validation error banner + form controls in Hermes and
     invokes the real local `mlx-community/Qwen3-4B-Instruct-2507-4bit` model (`llm_calls == 1`)
     to synthesize the corrective `fill_form` fields (`tshirt_size="L"` and full >=40-char abstract).
  4. Agent proposes the corrective `fill_form` and `submit`, host approves both, and the SQLite
     `cfp_submissions` table records exactly 1 valid submission (`tshirt_size == "L"`,
     `len(abstract_summary) >= 40`, `confirmation_ref == "socon2027=cfp-accepted"`).
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

import signup_fixture  # noqa: E402
from test_slice1_signup import ControlClient, HERMES_BIN  # noqa: E402


class Slice2ValidationAndLlmE2ETest(unittest.TestCase):
    def test_slice2_interactive_validation_and_real_qwen_self_correction(self) -> None:
        self.assertTrue(HERMES_BIN.exists(), f"Hermes binary missing at {HERMES_BIN}")

        import hermes_agent_driver  # noqa: F401

        with tempfile.TemporaryDirectory(prefix="walkaway-slice2-") as tmpdir:
            tmp = Path(tmpdir)
            db_path = tmp / "accounts.sqlite3"
            sock_path = tmp / "hermes.sock"
            vault_path = tmp / "vault.json"
            audit_path = tmp / "audit.jsonl"

            server = signup_fixture.start_server("127.0.0.1", 0, db_path)
            port = server.server_port
            server_thread = threading.Thread(target=server.serve_forever, daemon=True)
            server_thread.start()
            cfp_url = f"http://127.0.0.1:{port}/cfp-socon"

            host_token = "host-secret-token-slice2-998877"
            agent_token = "agent-secret-token-slice2-112233"

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
                    # 1. Host opens supervised session on /cfp-socon
                    host.send(
                        {
                            "type": "session.open",
                            "request_id": "open-s2",
                            "title": "SO-CON 2027 CFP Submission",
                            "start_url": cfp_url,
                            "capabilities": {"protocol": 1},
                            "policy": {"decision": "human_per_action"},
                        }
                    )
                    opened = host.read_until_type("session.opened")
                    self.assertEqual(opened.get("type"), "session.opened")
                    session_id = opened["session_id"]

                    # Wait for /cfp-socon form controls to load
                    snap = None
                    for attempt in range(15):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"obs-s2-init-{attempt}",
                                "include_screenshot": False,
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        els = cand.get("elements") or []
                        if any((e.get("dom_id") or e.get("name")) == "tshirt_size" for e in els):
                            snap = cand
                            break
                        time.sleep(0.1)

                    self.assertIsNotNone(snap, "SO-CON 2027 CFP form controls were never indexed")
                    by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    for req in ("speaker_name", "email", "talk_title", "abstract_summary", "tshirt_size", "submit-cfp"):
                        self.assertIn(req, by_id, f"Missing control {req} in {list(by_id.keys())}")

                    # 2. Attempt 1: Agent submits incomplete form (missing tshirt_size, short abstract)
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s2-fill-1",
                            "proposal_id": "prop-s2-fill-1",
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
                                        "target_ref": by_id["talk_title"]["node_ref"],
                                        "action": "type",
                                        "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                    },
                                    {
                                        "target_ref": by_id["abstract_summary"]["node_ref"],
                                        "action": "type",
                                        "value": "Too short abstract",
                                    },
                                ],
                            },
                            "rationale": "Initial CFP draft (missing tshirt_size)",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s2-fill-1",
                            "proposal_id": "prop-s2-fill-1",
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Observe staged and submit incomplete form
                    agent.send(
                        {
                            "type": "observe.request",
                            "session_id": session_id,
                            "request_id": "obs-s2-staged-1",
                            "include_screenshot": False,
                        }
                    )
                    snap_staged1 = agent.read_until_type("observe.result")
                    staged1_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_staged1.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s2-sub-1",
                            "proposal_id": "prop-s2-sub-1",
                            "snapshot_id": snap_staged1["snapshot_id"],
                            "fingerprint": snap_staged1["fingerprint"],
                            "generation": snap_staged1["generation"],
                            "operation": {
                                "kind": "submit",
                                "target_ref": staged1_by_id["submit-cfp"]["node_ref"],
                            },
                            "rationale": "Submit initial CFP draft",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s2-sub-1",
                            "proposal_id": "prop-s2-sub-1",
                            "fingerprint": snap_staged1["fingerprint"],
                            "generation": snap_staged1["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Verify 0 rows in cfp_submissions because validation failed
                    with contextlib.closing(sqlite3.connect(db_path)) as conn:
                        count_after_fail = conn.execute("SELECT COUNT(*) FROM cfp_submissions").fetchone()[0]
                    self.assertEqual(count_after_fail, 0, "Invalid CFP submission must not be committed")

                    # Observe the validation error page
                    agent.send(
                        {
                            "type": "observe.request",
                            "session_id": session_id,
                            "request_id": "obs-s2-err-1",
                            "include_screenshot": False,
                        }
                    )
                    snap_err = agent.read_until_type("observe.result")
                    self.assertIn("Validation Error", snap_err.get("text", ""), f"Expected Validation Error in page text: {snap_err}")
                    self.assertIn("tshirt_size", snap_err.get("text", ""))

                    # 3. Invoke real local Qwen3-4B-Instruct-2507-4bit model via hermes_agent_driver
                    # to synthesize the corrective fill_form operation from the observed validation error!
                    speaker_profile = {
                        "speaker_name": "Ryan MacArthur",
                        "email": "ryan@zunftmax.secure.build",
                        "talk_title": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                        "abstract_summary": (
                            "Empirical analysis of what Firecracker microVMs leave as homework "
                            "for containment-first sandboxing across Linux userland and host boundaries."
                        ),
                        "tshirt_size": "L",
                    }
                    correction = hermes_agent_driver.plan_validation_repair_with_local_llm(
                        snapshot=snap_err,
                        speaker_profile=speaker_profile,
                    )
                    self.assertEqual(correction["llm_calls"], 1)
                    self.assertEqual(correction["model_id"], "mlx-community/Qwen3-4B-Instruct-2507-4bit")
                    repair_op = correction["operation"]
                    self.assertEqual(repair_op["kind"], "fill_form")

                    # 4. Propose and approve the LLM-repaired fill_form + submit
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s2-fill-2",
                            "proposal_id": "prop-s2-fill-2",
                            "snapshot_id": snap_err["snapshot_id"],
                            "fingerprint": snap_err["fingerprint"],
                            "generation": snap_err["generation"],
                            "operation": repair_op,
                            "rationale": "Self-correct validation errors (tshirt_size=L and full abstract) via Qwen3-4B",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s2-fill-2",
                            "proposal_id": "prop-s2-fill-2",
                            "fingerprint": snap_err["fingerprint"],
                            "generation": snap_err["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Observe repaired form and submit
                    agent.send(
                        {
                            "type": "observe.request",
                            "session_id": session_id,
                            "request_id": "obs-s2-staged-2",
                            "include_screenshot": False,
                        }
                    )
                    snap_staged2 = agent.read_until_type("observe.result")
                    staged2_by_id = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in (snap_staged2.get("elements") or [])
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "prop-s2-sub-2",
                            "proposal_id": "prop-s2-sub-2",
                            "snapshot_id": snap_staged2["snapshot_id"],
                            "fingerprint": snap_staged2["fingerprint"],
                            "generation": snap_staged2["generation"],
                            "operation": {
                                "kind": "submit",
                                "target_ref": staged2_by_id["submit-cfp"]["node_ref"],
                            },
                            "rationale": "Submit repaired SO-CON 2027 CFP form",
                        }
                    )
                    self.assertEqual(host.read_until_type("proposal.pending").get("type"), "proposal.pending")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": "dec-s2-sub-2",
                            "proposal_id": "prop-s2-sub-2",
                            "fingerprint": snap_staged2["fingerprint"],
                            "generation": snap_staged2["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "desktop-panel",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                    # Verify 1 committed CFP row in SQLite
                    with contextlib.closing(sqlite3.connect(db_path)) as conn:
                        conn.row_factory = sqlite3.Row
                        rows = [dict(r) for r in conn.execute("SELECT * FROM cfp_submissions").fetchall()]
                    self.assertEqual(len(rows), 1, f"Expected 1 committed CFP submission after repair, got {rows}")
                    cfp = rows[0]
                    self.assertEqual(cfp["speaker_name"], "Ryan MacArthur")
                    self.assertEqual(cfp["email"], "ryan@zunftmax.secure.build")
                    self.assertEqual(cfp["tshirt_size"], "L")
                    self.assertGreaterEqual(len(cfp["abstract_summary"]), 40)
                    self.assertEqual(cfp["confirmation_ref"], "socon2027=cfp-accepted")

                finally:
                    host.close()
                    agent.close()
            finally:
                proc.terminate()
                try:
                    proc.communicate(timeout=4)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.communicate()
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    unittest.main(verbosity=2)
