#!/usr/bin/env python3
"""Full 3-Act Demo Storyline End-to-End Acceptance Test (`T-DEMO-VERTICAL-FULL`).

Executes all 3 acts of `/Users/mac/walkaway-monorepo/docs/demo-plan.md` in a single
continuous supervised Hermes browser session, wired through the real local
`mlx-community/Qwen3-4B-Instruct-2507-4bit` model and the real `services/hermes-host/message_bus.py`
approval/ack bus:
- Act 1: One-shot account creation & blind vault commit on `/signup` (`accounts == 1`).
- Act 2: Interactive form navigation & self-correcting validation on `/cfp-socon` using
  real local Qwen3-4B inference (`cfp_submissions == 1`, `llm_calls == 1`).
- Act 3: Zero-LLM `FormRecipe` recall & instant sign-in replay on `/signin` (`logins == 2`,
  recall `llm_calls == 0`, `< 5ms`), routed through `services/hermes-host/message_bus.py`
  (`POST /messages` -> `GET /messages/next` -> `POST /ack` -> host `decision: "approve"`).
"""

import contextlib
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
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "demo-fixtures"))
sys.path.insert(0, str(ROOT / "services" / "agent-driver"))
sys.path.insert(0, str(ROOT / "tests" / "e2e"))

import hermes_agent_driver  # noqa: E402
import signup_fixture  # noqa: E402
from test_slice1_signup import ControlClient, HERMES_BIN  # noqa: E402

MESSAGE_BUS_SCRIPT = ROOT / "services" / "hermes-host" / "message_bus.py"


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _bus_request(url: str, token: str, payload: dict | None = None) -> dict:
    headers = {"X-Bus-Token": token}
    data = None
    method = "GET"
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
        method = "POST"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=5) as resp:
        return json.loads(resp.read().decode("utf-8"))


class FullStorylineVerticalE2ETest(unittest.TestCase):
    def test_full_three_act_demo_storyline_end_to_end(self) -> None:
        self.assertTrue(HERMES_BIN.exists(), f"Hermes binary missing at {HERMES_BIN}")
        self.assertTrue(MESSAGE_BUS_SCRIPT.exists(), f"Message bus script missing at {MESSAGE_BUS_SCRIPT}")

        with tempfile.TemporaryDirectory(prefix="walkaway-full-demo-") as tmpdir:
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
            cfp_url = f"{origin}/cfp-socon"
            signin_url = f"{origin}/signin"

            bus_port = _free_port()
            bus_token = "walkaway-e2e-bus-token"
            bus_base = f"http://127.0.0.1:{bus_port}"
            bus_env = os.environ.copy()
            bus_env["WALKAWAY_BUS_TOKEN"] = bus_token
            bus_proc = subprocess.Popen(
                [sys.executable, str(MESSAGE_BUS_SCRIPT), "--host", "127.0.0.1", "--port", str(bus_port)],
                env=bus_env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            host_token = "host-secret-token-full-998877"
            agent_token = "agent-secret-token-full-112233"

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
                    time.sleep(0.05)
                self.assertTrue(sock_path.exists(), "Hermes did not bind --control-socket in time")

                # Wait for message_bus health
                for _ in range(30):
                    try:
                        with urllib.request.urlopen(f"{bus_base}/health", timeout=1) as r:
                            if r.status == 200:
                                break
                    except Exception:
                        time.sleep(0.05)

                host = ControlClient(sock_path, host_token)
                agent = ControlClient(sock_path, agent_token)

                def approve_via_bus(session_id: str, proposal_frame: dict, req_id: str) -> None:
                    """Routes the pending Hermes proposal through message_bus.py and applies the ack."""
                    msg = _bus_request(
                        f"{bus_base}/messages",
                        bus_token,
                        {
                            "title": f"Hermes Proposal {proposal_frame['proposal_id']}",
                            "body": proposal_frame.get("rationale", "Approve action"),
                            "kind": proposal_frame.get("classification", "act"),
                        },
                    )
                    polled = _bus_request(f"{bus_base}/messages/next?after={msg['id'] - 1}", bus_token)
                    self.assertEqual(polled["message"]["id"], msg["id"])
                    ack = _bus_request(
                        f"{bus_base}/ack",
                        bus_token,
                        {"message_id": msg["id"], "action": "approve", "source": "e2e-bus-bridge"},
                    )
                    self.assertEqual(ack["action"], "approve")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": req_id,
                            "proposal_id": proposal_frame["proposal_id"],
                            "fingerprint": proposal_frame["fingerprint"],
                            "generation": proposal_frame["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "glasses-bus",
                        }
                    )
                    self.assertEqual(host.read_until_type("executed").get("type"), "executed")

                def wait_for_control(session_id: str, control_id: str, prefix: str) -> dict:
                    for attempt in range(15):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"{prefix}-{attempt}",
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        els = cand.get("elements") or []
                        if any((e.get("dom_id") or e.get("name")) == control_id for e in els):
                            return cand
                        time.sleep(0.1)
                    self.fail(f"Control {control_id} never appeared")

                try:
                    # ACT 1: One-shot account creation & blind vault commit
                    host.send(
                        {
                            "type": "session.open",
                            "request_id": "open-full",
                            "title": "Muse x Hermes Batch Co-Processor",
                            "start_url": signup_url,
                            "capabilities": {"protocol": 1},
                            "policy": {"decision": "human_per_action"},
                        }
                    )
                    session_id = host.read_until_type("session.opened")["session_id"]
                    snap1 = wait_for_control(session_id, "speaker_name", "act1-init")
                    by_id1 = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap1["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act1-fill",
                            "proposal_id": "act1-fill",
                            "snapshot_id": snap1["snapshot_id"],
                            "fingerprint": snap1["fingerprint"],
                            "generation": snap1["generation"],
                            "operation": {
                                "kind": "fill_form",
                                "fields": [
                                    {"target_ref": by_id1["speaker_name"]["node_ref"], "action": "type", "value": "Ryan MacArthur"},
                                    {"target_ref": by_id1["email"]["node_ref"], "action": "type", "value": "ryan@zunftmax.secure.build"},
                                    {
                                        "target_ref": by_id1["password"]["node_ref"],
                                        "action": "fill_secret",
                                        "origin": origin,
                                        "username": "ryan@zunftmax.secure.build",
                                    },
                                    {
                                        "target_ref": by_id1["talk_title"]["node_ref"],
                                        "action": "type",
                                        "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                    },
                                    {"target_ref": by_id1["track"]["node_ref"], "action": "select", "value": "sandboxing"},
                                    {"target_ref": by_id1["live_demo"]["node_ref"], "action": "check", "value": "true"},
                                ],
                            },
                            "rationale": "Act 1: Stage speaker account + blind vault password",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act1-fill")

                    snap1_staged = wait_for_control(session_id, "submit-btn", "act1-staged")
                    by_id1_s = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap1_staged["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act1-sub",
                            "proposal_id": "act1-sub",
                            "snapshot_id": snap1_staged["snapshot_id"],
                            "fingerprint": snap1_staged["fingerprint"],
                            "generation": snap1_staged["generation"],
                            "operation": {"kind": "submit", "target_ref": by_id1_s["submit-btn"]["node_ref"]},
                            "rationale": "Act 1: Commit DeepSec 2026 speaker account",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act1-sub")

                    # ACT 2: Navigate to /cfp-socon, trigger validation error, and self-correct with Qwen3-4B
                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "act2-pre-nav"})
                    snap_pre2 = agent.read_until_type("observe.result")
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act2-nav",
                            "proposal_id": "act2-nav",
                            "snapshot_id": snap_pre2["snapshot_id"],
                            "fingerprint": snap_pre2["fingerprint"],
                            "generation": snap_pre2["generation"],
                            "operation": {"kind": "navigate", "url": cfp_url},
                            "rationale": "Act 2: Open SO-CON 2027 CFP portal",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act2-nav")

                    snap2 = wait_for_control(session_id, "tshirt_size", "act2-init")
                    by_id2 = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap2["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act2-fill-bad",
                            "proposal_id": "act2-fill-bad",
                            "snapshot_id": snap2["snapshot_id"],
                            "fingerprint": snap2["fingerprint"],
                            "generation": snap2["generation"],
                            "operation": {
                                "kind": "fill_form",
                                "fields": [
                                    {"target_ref": by_id2["speaker_name"]["node_ref"], "action": "type", "value": "Ryan MacArthur"},
                                    {"target_ref": by_id2["email"]["node_ref"], "action": "type", "value": "ryan@zunftmax.secure.build"},
                                    {
                                        "target_ref": by_id2["talk_title"]["node_ref"],
                                        "action": "type",
                                        "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                    },
                                    {"target_ref": by_id2["abstract_summary"]["node_ref"], "action": "type", "value": "Short draft"},
                                ],
                            },
                            "rationale": "Act 2: Initial CFP draft missing tshirt_size",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act2-fill-bad")

                    snap2_staged = wait_for_control(session_id, "submit-cfp", "act2-staged1")
                    by_id2_s = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap2_staged["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act2-sub-bad",
                            "proposal_id": "act2-sub-bad",
                            "snapshot_id": snap2_staged["snapshot_id"],
                            "fingerprint": snap2_staged["fingerprint"],
                            "generation": snap2_staged["generation"],
                            "operation": {"kind": "submit", "target_ref": by_id2_s["submit-cfp"]["node_ref"]},
                            "rationale": "Act 2: Submit incomplete CFP draft",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act2-sub-bad")

                    snap2_err = wait_for_control(session_id, "tshirt_size", "act2-err")
                    self.assertIn("Validation Error", snap2_err.get("text", ""))
                    repair = hermes_agent_driver.plan_validation_repair_with_local_llm(
                        snapshot=snap2_err,
                        speaker_profile={
                            "speaker_name": "Ryan MacArthur",
                            "email": "ryan@zunftmax.secure.build",
                            "talk_title": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                            "abstract_summary": (
                                "Empirical analysis of what Firecracker microVMs leave as homework "
                                "for containment-first sandboxing across Linux userland and host boundaries."
                            ),
                            "tshirt_size": "L",
                        },
                    )
                    self.assertEqual(repair["llm_calls"], 1)
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act2-fill-good",
                            "proposal_id": "act2-fill-good",
                            "snapshot_id": snap2_err["snapshot_id"],
                            "fingerprint": snap2_err["fingerprint"],
                            "generation": snap2_err["generation"],
                            "operation": repair["operation"],
                            "rationale": "Act 2: Qwen3-4B self-corrected CFP fields",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act2-fill-good")

                    snap2_repaired = wait_for_control(session_id, "submit-cfp", "act2-repaired")
                    by_id2_r = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap2_repaired["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act2-sub-good",
                            "proposal_id": "act2-sub-good",
                            "snapshot_id": snap2_repaired["snapshot_id"],
                            "fingerprint": snap2_repaired["fingerprint"],
                            "generation": snap2_repaired["generation"],
                            "operation": {"kind": "submit", "target_ref": by_id2_r["submit-cfp"]["node_ref"]},
                            "rationale": "Act 2: Submit repaired SO-CON 2027 CFP",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act2-sub-good")

                    # ACT 3: Sign-in and Zero-LLM FormRecipe Recall
                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "act3-pre-nav1"})
                    snap_pre3 = agent.read_until_type("observe.result")
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act3-nav1",
                            "proposal_id": "act3-nav1",
                            "snapshot_id": snap_pre3["snapshot_id"],
                            "fingerprint": snap_pre3["fingerprint"],
                            "generation": snap_pre3["generation"],
                            "operation": {"kind": "navigate", "url": signin_url},
                            "rationale": "Act 3: Open speaker sign-in",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act3-nav1")

                    snap3_1 = wait_for_control(session_id, "signin-btn", "act3-signin1")
                    by_id3_1 = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap3_1["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act3-fill1",
                            "proposal_id": "act3-fill1",
                            "snapshot_id": snap3_1["snapshot_id"],
                            "fingerprint": snap3_1["fingerprint"],
                            "generation": snap3_1["generation"],
                            "operation": {
                                "kind": "fill_form",
                                "fields": [
                                    {"target_ref": by_id3_1["email"]["node_ref"], "action": "type", "value": "ryan@zunftmax.secure.build"},
                                    {
                                        "target_ref": by_id3_1["password"]["node_ref"],
                                        "action": "fill_secret",
                                        "origin": origin,
                                        "username": "ryan@zunftmax.secure.build",
                                    },
                                ],
                            },
                            "rationale": "Act 3: Initial sign-in (records FormRecipe)",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act3-fill1")

                    snap3_1s = wait_for_control(session_id, "signin-btn", "act3-signin1-s")
                    by_id3_1s = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap3_1s["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act3-sub1",
                            "proposal_id": "act3-sub1",
                            "snapshot_id": snap3_1s["snapshot_id"],
                            "fingerprint": snap3_1s["fingerprint"],
                            "generation": snap3_1s["generation"],
                            "operation": {"kind": "submit", "target_ref": by_id3_1s["signin-btn"]["node_ref"]},
                            "rationale": "Act 3: Submit sign-in #1",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act3-sub1")

                    # Re-open /signin and execute Zero-LLM FormRecipe Recall
                    agent.send({"type": "observe.request", "session_id": session_id, "request_id": "act3-pre-nav2"})
                    snap_pre3_2 = agent.read_until_type("observe.result")
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act3-nav2",
                            "proposal_id": "act3-nav2",
                            "snapshot_id": snap_pre3_2["snapshot_id"],
                            "fingerprint": snap_pre3_2["fingerprint"],
                            "generation": snap_pre3_2["generation"],
                            "operation": {"kind": "navigate", "url": signin_url},
                            "rationale": "Act 3: Re-open /signin for zero-LLM recall",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act3-nav2")

                    snap3_2 = wait_for_control(session_id, "signin-btn", "act3-signin2")
                    recall_plan = hermes_agent_driver.plan_recall_or_llm(
                        snapshot=snap3_2,
                        speaker_profile={"email": "ryan@zunftmax.secure.build"},
                    )
                    self.assertEqual(recall_plan["llm_calls"], 0)
                    self.assertLess(recall_plan["latency_ms"], 5.0)
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act3-replay",
                            "proposal_id": "act3-replay",
                            "snapshot_id": snap3_2["snapshot_id"],
                            "fingerprint": snap3_2["fingerprint"],
                            "generation": snap3_2["generation"],
                            "operation": recall_plan["operation"],
                            "rationale": "Act 3: Zero-LLM FormRecipe replay",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act3-replay")

                    snap3_2s = wait_for_control(session_id, "signin-btn", "act3-signin2-s")
                    by_id3_2s = {
                        (e.get("dom_id") or e.get("name") or e.get("id")): e
                        for e in snap3_2s["elements"]
                        if e.get("dom_id") or e.get("name") or e.get("id")
                    }
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": "act3-sub2",
                            "proposal_id": "act3-sub2",
                            "snapshot_id": snap3_2s["snapshot_id"],
                            "fingerprint": snap3_2s["fingerprint"],
                            "generation": snap3_2s["generation"],
                            "operation": {"kind": "submit", "target_ref": by_id3_2s["signin-btn"]["node_ref"]},
                            "rationale": "Act 3: Submit recalled sign-in #2",
                        }
                    )
                    approve_via_bus(session_id, host.read_until_type("proposal.pending"), "dec-act3-sub2")

                    # Complete the supervised session cleanly with task.done
                    agent.send({"type": "task.done", "session_id": session_id, "request_id": "done-full"})
                    done_state = None
                    for _ in range(10):
                        frame = host.read_until_type("state")
                        if frame.get("state") == "done":
                            done_state = frame
                            break
                    self.assertIsNotNone(done_state, "Session did not reach 'done' state after task.done")

                    # Verify SQLite state across all 3 tables + message_bus acks
                    with contextlib.closing(sqlite3.connect(db_path)) as conn:
                        n_accounts = conn.execute("SELECT COUNT(*) FROM accounts").fetchone()[0]
                        n_cfp = conn.execute("SELECT COUNT(*) FROM cfp_submissions").fetchone()[0]
                        n_logins = conn.execute("SELECT COUNT(*) FROM logins").fetchone()[0]
                    self.assertEqual(n_accounts, 1)
                    self.assertEqual(n_cfp, 1)
                    self.assertEqual(n_logins, 2)

                    acks = _bus_request(f"{bus_base}/acks", bus_token)["acks"]
                    self.assertEqual(len(acks), 13, f"Expected 13 approved actions on message_bus, got {len(acks)}")

                finally:
                    host.close()
                    agent.close()
            finally:
                proc.terminate()
                bus_proc.terminate()
                for p in (proc, bus_proc):
                    try:
                        p.communicate(timeout=4)
                    except subprocess.TimeoutExpired:
                        p.kill()
                        p.communicate()
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    unittest.main(verbosity=2)
