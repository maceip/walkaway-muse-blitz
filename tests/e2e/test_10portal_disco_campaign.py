#!/usr/bin/env python3
"""10-Portal Disco Campaign End-to-End Acceptance Suite (`test_10portal_disco_campaign.py`).

Proves:
- Test 1 (`test_1_full_10portal_campaign_with_3_glasses_cards`):
  Runs Act 1 (`/signup` account + blind vault commit), Act 2 (`/cfp/1` validation failure ->
  resident `mlx-community/Qwen3-4B-Instruct-2507-4bit` self-correction), and Act 3 (`/cfp/2`..`/cfp/10`
  zero-LLM `FormRecipe` recall + batch manifest), gated by 3 `glasses_dat` approvals on `message_bus.py`.
  Verifies `total_llm_calls == 1`, all 10 portals committed in SQLite, intermediate `received`/`dat_sent`/`dat_failed`
  acks ignored, duplicate `approve` ack replay idempotency, and zero password leaks across all surfaces.
- Test 2 (`test_2_reject_safety_gate_on_card3_batch`):
  Verifies that rejecting Glasses Card #3 (`action: "reject", source: "glasses_dat"`) rejects all 9 staged
  batch proposals so zero of Portals #2–#10 are committed to SQLite.
- Test 3 (`test_3_deterministic_reset_gate_under_2000ms`):
  Runs `reset_demo_state` twice consecutively and asserts identical clean state and measured reset time `< 2000ms`.
"""

from __future__ import annotations

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
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "demo-fixtures"))
sys.path.insert(0, str(ROOT / "services" / "agent-driver"))

import parallel_campaign_driver  # noqa: E402
import reset_demo  # noqa: E402
import signup_fixture  # noqa: E402

HERMES_BIN = parallel_campaign_driver.HERMES_BIN
MESSAGE_BUS_SCRIPT = ROOT / "services" / "hermes-host" / "message_bus.py"


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def _http_request(
    url: str,
    payload: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
) -> tuple[int, str]:
    req_headers = dict(headers or {})
    data = None
    method = "GET"
    if payload is not None:
        req_headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
        method = "POST"
    req = urllib.request.Request(url, data=data, headers=req_headers, method=method)
    with urllib.request.urlopen(req, timeout=5.0) as resp:
        return int(resp.status), resp.read().decode("utf-8")


class TenPortalDiscoCampaignE2ETest(unittest.TestCase):
    resident_qwen: parallel_campaign_driver.ResidentQwenDriver | None = None

    @classmethod
    def setUpClass(cls) -> None:
        assert HERMES_BIN.exists(), f"Hermes binary missing at {HERMES_BIN}"
        assert MESSAGE_BUS_SCRIPT.exists(), f"Message bus missing at {MESSAGE_BUS_SCRIPT}"
        cls.resident_qwen = parallel_campaign_driver.get_resident_qwen_driver()

    @classmethod
    def tearDownClass(cls) -> None:
        if cls.resident_qwen is not None:
            cls.resident_qwen.close()
            cls.resident_qwen = None

    def _start_stack(self, tmp: Path) -> dict[str, Any]:
        db_path = tmp / "campaign.sqlite3"
        sock_path = tmp / "hermes_primary.sock"
        vault_path = tmp / "vault.json"
        recall_path = tmp / "recall.json"
        audit_path = tmp / "audit.jsonl"

        server = signup_fixture.start_server("127.0.0.1", 0, db_path)
        port = server.server_port
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        origin = f"http://127.0.0.1:{port}"

        reset_demo.reset_demo_state(
            fixture_url=origin,
            recall_path=recall_path,
            vault_path=vault_path,
            db_path=db_path,
        )

        bus_port = _free_port()
        bus_token = "walkaway-10portal-e2e-token"
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

        host_token = "host-secret-token-disco10-998877"
        agent_token = "agent-secret-token-disco10-112233"
        env = os.environ.copy()
        env["HERMES_HOST_TOKEN"] = host_token
        env["HERMES_AGENT_TOKEN"] = agent_token
        env["HERMES_VAULT_PATH"] = str(vault_path)
        # Keep the test out of the credential store the user actually uses.
        env["HERMES_PASSKEY_KEYCHAIN_SERVICE"] = "ai.zmax.hermes.browser.test.e2e.passkeys"
        env["HERMES_PASSWORD_KEYCHAIN_SERVICE"] = "ai.zmax.hermes.browser.test.e2e.credentials"
        env["HERMES_RECALL_PATH"] = str(recall_path)
        env["HERMES_AUDIT_PATH"] = str(audit_path)

        browser_proc = subprocess.Popen(
            [str(HERMES_BIN), "--control-socket", str(sock_path)],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        deadline = time.monotonic() + 10.0
        while not sock_path.exists() and time.monotonic() < deadline:
            time.sleep(0.03)
        self.assertTrue(sock_path.exists(), "Primary Hermes did not bind --control-socket")

        for _ in range(40):
            try:
                status, _ = _http_request(f"{bus_base}/health")
                if status == 200:
                    break
            except Exception:
                time.sleep(0.03)

        return {
            "db_path": db_path,
            "sock_path": sock_path,
            "vault_path": vault_path,
            "recall_path": recall_path,
            "audit_path": audit_path,
            "server": server,
            "origin": origin,
            "bus_base": bus_base,
            "bus_token": bus_token,
            "bus_proc": bus_proc,
            "browser_proc": browser_proc,
            "host_token": host_token,
            "agent_token": agent_token,
        }

    def _stop_stack(self, stack: dict[str, Any]) -> tuple[str, str]:
        browser_proc: subprocess.Popen[str] = stack["browser_proc"]
        bus_proc: subprocess.Popen[str] = stack["bus_proc"]
        server = stack["server"]

        browser_proc.terminate()
        bus_proc.terminate()
        stdout_text, stderr_text = "", ""
        try:
            stdout_text, stderr_text = browser_proc.communicate(timeout=4.0)
        except subprocess.TimeoutExpired:
            browser_proc.kill()
            stdout_text, stderr_text = browser_proc.communicate()
        try:
            bus_proc.communicate(timeout=3.0)
        except subprocess.TimeoutExpired:
            bus_proc.kill()
            bus_proc.communicate()
        server.shutdown()
        server.server_close()
        return stdout_text, stderr_text

    def test_1_full_10portal_campaign_with_3_glasses_cards(self) -> None:
        with tempfile.TemporaryDirectory(prefix="walkaway-disco10-t1-") as tmpdir:
            stack = self._start_stack(Path(tmpdir))
            origin = stack["origin"]
            bus_base = stack["bus_base"]
            bus_token = stack["bus_token"]
            bus_headers = {"X-Bus-Token": bus_token}

            staging_counts_at_card3: list[int] = []

            def handle_glasses_card(card_idx: int, msg: dict[str, Any]) -> None:
                msg_id = int(msg["id"])
                # Post intermediate non-terminal acks first; these MUST be ignored!
                for non_terminal in ("received", "dat_sent", "dat_failed"):
                    _http_request(
                        f"{bus_base}/ack",
                        payload={
                            "message_id": msg_id,
                            "action": non_terminal,
                            "source": "android_bridge",
                        },
                        headers=bus_headers,
                    )
                if card_idx == 3:
                    # Verify that before Card #3 is approved, only Portal #1 is in SQLite
                    _, cfp_raw = _http_request(f"{origin}/api/cfp_submissions")
                    cfp_rows = json.loads(cfp_raw)["cfp_submissions"]
                    staging_counts_at_card3.append(len(cfp_rows))
                    # Verify explicit batch_proposals manifest has all 9 portals (#2..#10)
                    manifest = msg.get("batch_proposals") or json.loads(msg["body"])["batch_proposals"]
                    self.assertEqual(len(manifest), 9)
                    self.assertEqual([int(m["portal_id"]) for m in manifest], list(range(2, 11)))

                # Now post the real glasses_dat approve ack
                _http_request(
                    f"{bus_base}/ack",
                    payload={
                        "message_id": msg_id,
                        "action": "approve",
                        "source": "glasses_dat",
                    },
                    headers=bus_headers,
                )

            try:
                result = parallel_campaign_driver.run_10portal_campaign(
                    fixture_origin=origin,
                    sock_path=stack["sock_path"],
                    host_token=stack["host_token"],
                    agent_token=stack["agent_token"],
                    bus_base_url=bus_base,
                    bus_token=bus_token,
                    vault_path=stack["vault_path"],
                    recall_path=stack["recall_path"],
                    audit_path=stack["audit_path"],
                    resident_qwen=self.resident_qwen,
                    on_glasses_card=handle_glasses_card,
                    include_screenshots=True,
                )

                # 1. Exactly 1 LLM call across all 10 portals (Portal #1 repair; Portals #2..#10 == 0)
                self.assertEqual(result["total_llm_calls"], 1)
                self.assertGreater(result["llm_repair_ms"], 0.0)
                self.assertGreater(result["total_recall_ms"], 0.0)

                # 2. Exactly 3 Glasses cards & 3 glasses_dat approvals completed the campaign;
                #    all 9 intermediate received/dat_sent/dat_failed acks were ignored.
                self.assertEqual(result["glasses_cards_count"], 3)
                self.assertEqual(result["glasses_approvals_count"], 3)
                self.assertEqual(result["ignored_acks_count"], 9)
                self.assertEqual(staging_counts_at_card3, [1])
                self.assertEqual(result["batch_executed_count"], 9)

                # 3. All 10 conference submissions exist in SQLite (portal_id 1..10)
                _, cfp_raw = _http_request(f"{origin}/api/cfp_submissions")
                cfp_rows = json.loads(cfp_raw)["cfp_submissions"]
                self.assertEqual(len(cfp_rows), 10)
                self.assertEqual(
                    [int(r["portal_id"]) for r in cfp_rows],
                    list(range(1, 11)),
                )

                # 4. Replaying a duplicate "approve" ack on Card #3 does NOT execute anything twice
                card3_id = int(result["card_messages"][2]["id"])
                _http_request(
                    f"{bus_base}/ack",
                    payload={
                        "message_id": card3_id,
                        "action": "approve",
                        "source": "glasses_dat",
                    },
                    headers=bus_headers,
                )
                coordinator: parallel_campaign_driver.GlassesBusCoordinator = result["coordinator"]
                dup_res = coordinator.replay_duplicate_ack_check(
                    card3_id,
                    result["batch_proposals"],
                )
                self.assertEqual(dup_res["status"], "ignored_duplicate")
                self.assertEqual(dup_res["executed_count"], 0)
                self.assertEqual(dup_res["skipped_proposals"], 9)

                _, cfp_after_dup_raw = _http_request(f"{origin}/api/cfp_submissions")
                self.assertEqual(len(json.loads(cfp_after_dup_raw)["cfp_submissions"]), 10)

                # 5. Check workbench & campaign receipt surfaces
                _, wb_state_raw = _http_request(f"{origin}/api/workbench/state")
                wb_state = json.loads(wb_state_raw)
                self.assertEqual(len(wb_state["tiles"]), 10)
                self.assertEqual(wb_state["campaign"]["total_llm_calls"], 1)

                _, wb_html = _http_request(f"{origin}/disco-workbench")
                _, receipt_json_raw = _http_request(f"{origin}/api/campaign_receipt")
                _, receipt_html = _http_request(f"{origin}/campaign-receipt")
                receipt_json = json.loads(receipt_json_raw)
                self.assertEqual(receipt_json["cfp_submissions_count"], 10)
                self.assertEqual(receipt_json["total_llm_calls"], 1)
            finally:
                stdout_text, stderr_text = self._stop_stack(stack)

            # 6. Zero password leaks across all frames, audit log, workbench, and receipt
            vault_data = json.loads(stack["vault_path"].read_text(encoding="utf-8"))
            secrets_to_check: list[str] = []
            if "socon_speaker_pw" in vault_data:
                secrets_to_check.append(str(vault_data["socon_speaker_pw"]))
            for acct in vault_data.get("accounts") or []:
                if acct.get("secret"):
                    secrets_to_check.append(str(acct["secret"]))
            self.assertTrue(secrets_to_check, "Expected at least one vault secret to verify non-leakage")

            audit_text = (
                stack["audit_path"].read_text(encoding="utf-8")
                if stack["audit_path"].exists()
                else ""
            )
            for secret in secrets_to_check:
                self.assertNotIn(secret, result["raw_wire_text"], "Secret leaked in socket frames!")
                self.assertNotIn(secret, audit_text, "Secret leaked in audit log!")
                self.assertNotIn(secret, wb_state_raw, "Secret leaked in workbench JSON!")
                self.assertNotIn(secret, wb_html, "Secret leaked in workbench HTML!")
                self.assertNotIn(secret, receipt_json_raw, "Secret leaked in campaign receipt JSON!")
                self.assertNotIn(secret, receipt_html, "Secret leaked in campaign receipt HTML!")
                self.assertNotIn(secret, stdout_text, "Secret leaked in browser stdout!")
                self.assertNotIn(secret, stderr_text, "Secret leaked in browser stderr!")

    def test_2_reject_safety_gate_on_card3_batch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="walkaway-disco10-t2-") as tmpdir:
            stack = self._start_stack(Path(tmpdir))
            origin = stack["origin"]
            bus_base = stack["bus_base"]
            bus_token = stack["bus_token"]
            bus_headers = {"X-Bus-Token": bus_token}

            def handle_glasses_card_reject_batch(card_idx: int, msg: dict[str, Any]) -> None:
                action = "approve" if card_idx in (1, 2) else "reject"
                _http_request(
                    f"{bus_base}/ack",
                    payload={
                        "message_id": int(msg["id"]),
                        "action": action,
                        "source": "glasses_dat",
                    },
                    headers=bus_headers,
                )

            try:
                result = parallel_campaign_driver.run_10portal_campaign(
                    fixture_origin=origin,
                    sock_path=stack["sock_path"],
                    host_token=stack["host_token"],
                    agent_token=stack["agent_token"],
                    bus_base_url=bus_base,
                    bus_token=bus_token,
                    vault_path=stack["vault_path"],
                    recall_path=stack["recall_path"],
                    audit_path=stack["audit_path"],
                    resident_qwen=self.resident_qwen,
                    on_glasses_card=handle_glasses_card_reject_batch,
                    include_screenshots=False,
                )

                self.assertEqual(result["batch_decision"], "reject")
                self.assertEqual(result["batch_executed_count"], 0)
                self.assertEqual(result["batch_rejected_count"], 9)

                # Verify that ONLY Portal #1 is committed in SQLite; zero of Portals #2..#10 committed
                _, cfp_raw = _http_request(f"{origin}/api/cfp_submissions")
                cfp_rows = json.loads(cfp_raw)["cfp_submissions"]
                self.assertEqual(len(cfp_rows), 1)
                self.assertEqual(int(cfp_rows[0]["portal_id"]), 1)
            finally:
                self._stop_stack(stack)

    def test_3_deterministic_reset_gate_under_2000ms(self) -> None:
        with tempfile.TemporaryDirectory(prefix="walkaway-disco10-t3-") as tmpdir:
            tmp = Path(tmpdir)
            db_path = tmp / "reset_test.sqlite3"
            vault_path = tmp / "vault.json"
            recall_path = tmp / "recall.json"

            server = signup_fixture.start_server("127.0.0.1", 0, db_path)
            port = server.server_port
            server_thread = threading.Thread(target=server.serve_forever, daemon=True)
            server_thread.start()
            origin = f"http://127.0.0.1:{port}"

            try:
                # Dirty the database, recall store, and vault
                with contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.execute(
                        "INSERT INTO cfp_submissions (portal_id, conference_name, speaker_name, email, "
                        "talk_title, track, tshirt_size, abstract_summary, live_demo, confirmation_ref, created_at) "
                        "VALUES (1, 'SoCon 2026', 'Ryan', 'ryan@example.com', 'Title', 'systems', 'L', "
                        "'Long enough abstract summary for testing reset.', 'yes', 'ref', '2026-11-05T00:00:00Z')"
                    )
                    conn.commit()
                recall_path.write_text('{"recipes": [{"recipe_id": "dirty"}]}', encoding="utf-8")
                vault_path.write_text('{"dirty": true}', encoding="utf-8")

                r1 = reset_demo.reset_demo_state(
                    fixture_url=origin,
                    recall_path=recall_path,
                    vault_path=vault_path,
                    db_path=db_path,
                )
                r2 = reset_demo.reset_demo_state(
                    fixture_url=origin,
                    recall_path=recall_path,
                    vault_path=vault_path,
                    db_path=db_path,
                )

                self.assertTrue(r1["ok"])
                self.assertTrue(r2["ok"])
                self.assertLess(r1["reset_ms"], 2000.0)
                self.assertLess(r2["reset_ms"], 2000.0)
                self.assertEqual(r1["vault_mode"], "0o600")
                self.assertEqual(r2["vault_mode"], "0o600")
                self.assertEqual(r1["recall_state"], {"recipes": []})
                self.assertEqual(r1["recall_state"], r2["recall_state"])
                self.assertGreaterEqual(len(r1["vault_state"]["socon_speaker_pw"]), 20)
                self.assertGreaterEqual(len(r2["vault_state"]["socon_speaker_pw"]), 20)
                self.assertNotEqual(
                    r1["vault_state"]["socon_speaker_pw"],
                    r2["vault_state"]["socon_speaker_pw"],
                )
                self.assertEqual(r1["vault_state"]["accounts"], [])
                self.assertEqual(r2["vault_state"]["accounts"], [])

                _, cfp_raw = _http_request(f"{origin}/api/cfp_submissions")
                self.assertEqual(json.loads(cfp_raw)["cfp_submissions"], [])

                # Also verify --reset-demo CLI invocation
                cli_proc = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "services" / "agent-driver" / "reset_demo.py"),
                        "--reset-demo",
                        "--fixture-url",
                        origin,
                        "--recall-path",
                        str(recall_path),
                        "--vault-path",
                        str(vault_path),
                        "--db-path",
                        str(db_path),
                    ],
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=5.0,
                )
                cli_out = json.loads(cli_proc.stdout)
                self.assertTrue(cli_out["ok"])
                self.assertLess(cli_out["reset_ms"], 2000.0)
                self.assertGreaterEqual(len(cli_out["vault_state"]["socon_speaker_pw"]), 20)
                self.assertEqual(cli_out["vault_state"]["accounts"], [])
            finally:
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    unittest.main(verbosity=2)
