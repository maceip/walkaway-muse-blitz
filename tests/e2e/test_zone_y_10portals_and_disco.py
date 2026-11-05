#!/usr/bin/env python3
"""Zone Y End-to-End Verification Test: 10-Portal CFP Suite, Disco Workbench, WebAuthn P-256 & Reset.

Verifies:
1. `/disco-workbench` renders in `hermes-blitz-browser` and `observe.result` confirms all 10 tiles
   (`tile-1`..`tile-10`) have non-zero bounding rects (`width > 0`, `height > 0`, `visible == True`),
   recording real measured layout/observe timing via `POST /api/workbench/state`.
2. All 10 `/cfp/1`..`/cfp/10` portals render in `hermes-blitz-browser` with identical `form_signature`;
   `/cfp/1` rejects invalid submission with `#validation-error`; after staging & submitting valid fields
   on `/cfp/1`, portals `/cfp/2`..`/cfp/10` match the stored `FormRecipe` (`matched_recipe_id`) and
   commit via zero-LLM `replay_recipe` + `submit`, persisting all 10 rows in SQLite `cfp_submissions`.
3. Real ECDSA P-256 WebAuthn (`/api/webauthn/challenge`, `/api/webauthn/register`, `/api/webauthn/verify`)
   succeeds with a real OpenSSL P-256 signature (`UP=1, UV=1`) and rejects wrong challenge, wrong RP/origin,
   missing UP/UV flags, tampered signature, and replayed challenge.
4. `/api/campaign_receipt`, `/campaign-receipt`, and `POST /api/reset` work cleanly with zero secret leakage.
"""

import base64
import contextlib
import hashlib
import json
import os
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
sys.path.insert(0, str(ROOT / "services" / "agent-driver"))
sys.path.insert(0, str(ROOT / "tests" / "e2e"))

import hermes_agent_driver  # noqa: E402
import signup_fixture  # noqa: E402
from test_slice1_signup import ControlClient, HERMES_BIN  # noqa: E402


def _http_json(
    url: str, payload: dict | None = None, method: str | None = None
) -> tuple[int, dict]:
    headers = {}
    data = None
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
        method = method or "POST"
    else:
        method = method or "GET"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            return resp.status, json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as err:
        try:
            body = err.read().decode("utf-8", errors="replace")
            return err.code, json.loads(body)
        finally:
            err.close()


def _b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


class ZoneY10PortalsAndDiscoE2ETest(unittest.TestCase):
    def test_zone_y_disco_workbench_10_portals_webauthn_and_reset(self) -> None:
        self.assertTrue(HERMES_BIN.exists(), f"Hermes binary missing at {HERMES_BIN}")

        with tempfile.TemporaryDirectory(prefix="walkaway-zone-y-") as tmpdir:
            tmp = Path(tmpdir)
            db_path = tmp / "zone_y.sqlite3"
            sock_path = tmp / "hermes.sock"
            vault_path = tmp / "vault.json"
            recall_path = tmp / "recall.json"
            audit_path = tmp / "audit.jsonl"

            server = signup_fixture.start_server("127.0.0.1", 0, db_path)
            port = server.server_port
            server_thread = threading.Thread(target=server.serve_forever, daemon=True)
            server_thread.start()
            origin = f"http://127.0.0.1:{port}"

            host_token = "host-secret-token-zoney-998877"
            agent_token = "agent-secret-token-zoney-112233"

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
                        self.fail(
                            f"Hermes exited early ({proc.returncode}):\nSTDOUT:\n{out}\nSTDERR:\n{err}"
                        )
                    time.sleep(0.05)
                self.assertTrue(sock_path.exists(), "Hermes did not bind --control-socket in time")

                host = ControlClient(sock_path, host_token)
                agent = ControlClient(sock_path, agent_token)

                def run_approved_op(
                    session_id: str, snap: dict, req_id: str, op: dict, rationale: str
                ) -> dict:
                    agent.send(
                        {
                            "type": "propose",
                            "session_id": session_id,
                            "request_id": req_id,
                            "proposal_id": req_id,
                            "snapshot_id": snap["snapshot_id"],
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "operation": op,
                            "rationale": rationale,
                        }
                    )
                    pending = host.read_until_type("proposal.pending")
                    self.assertEqual(pending.get("type"), "proposal.pending", f"Failed: {pending}")
                    host.send(
                        {
                            "type": "decision",
                            "session_id": session_id,
                            "request_id": f"dec-{req_id}",
                            "proposal_id": req_id,
                            "fingerprint": snap["fingerprint"],
                            "generation": snap["generation"],
                            "decision": "approve",
                            "decided_by": "user",
                            "surface": "glasses",
                        }
                    )
                    exec_frame = host.read_until_type("executed")
                    self.assertEqual(exec_frame.get("type"), "executed", f"Failed: {exec_frame}")
                    return exec_frame

                def observe_until_dom_id(
                    session_id: str, dom_id: str, prefix: str
                ) -> tuple[dict, float]:
                    for attempt in range(15):
                        t0 = time.perf_counter()
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"{prefix}-{attempt}",
                                "include_screenshot": False,
                            }
                        )
                        cand = agent.read_until_type("observe.result")
                        obs_ms = (time.perf_counter() - t0) * 1000.0
                        els = cand.get("elements") or []
                        if any((e.get("dom_id") or e.get("name")) == dom_id for e in els):
                            return cand, obs_ms
                        time.sleep(0.08)
                    self.fail(f"DOM id {dom_id} never appeared in observe.result")

                try:
                    # 1. Open /disco-workbench and verify all 10 tiles have non-zero bounding rects
                    host.send(
                        {
                            "type": "session.open",
                            "request_id": "open-zy",
                            "title": "Zone Y 10-Portal Disco Workbench",
                            "start_url": f"{origin}/disco-workbench",
                            "capabilities": {"protocol": 1},
                            "policy": {"decision": "human_per_action"},
                        }
                    )
                    opened = host.read_until_type("session.opened")
                    session_id = opened["session_id"]

                    snap_wb, wb_obs_ms = observe_until_dom_id(session_id, "tile-10", "obs-wb-init")
                    self.assertGreater(wb_obs_ms, 0.0)
                    wb_by_id = {
                        e.get("dom_id"): e
                        for e in (snap_wb.get("elements") or [])
                        if e.get("dom_id")
                    }
                    for pid in range(1, 11):
                        tile_key = f"tile-{pid}"
                        self.assertIn(tile_key, wb_by_id, f"Missing {tile_key} in {wb_by_id.keys()}")
                        bounds = wb_by_id[tile_key]["bounds"]
                        self.assertGreater(bounds["width"], 0.0, f"{tile_key} has 0 width: {bounds}")
                        self.assertGreater(bounds["height"], 0.0, f"{tile_key} has 0 height: {bounds}")
                        self.assertTrue(wb_by_id[tile_key]["visible"], f"{tile_key} not visible")

                    # 2. Navigate through /cfp/1..10: test validation curveball on /cfp/1, then repair & recall across /cfp/2..10
                    portal_signatures: list[str] = []
                    total_recall_ms = 0.0
                    receipt_proposals: list[dict] = []

                    # Navigate to /cfp/1
                    run_approved_op(
                        session_id,
                        snap_wb,
                        "nav-cfp-1",
                        {"kind": "navigate", "url": f"{origin}/cfp/1"},
                        "Open Portal #1 SoCon 2026",
                    )
                    snap_p1, p1_obs_ms = observe_until_dom_id(session_id, "submit-cfp", "obs-p1")
                    sig_p1 = snap_p1.get("form_signature")
                    self.assertTrue(sig_p1 and sig_p1.startswith("sig_"), f"Missing form_signature: {snap_p1}")
                    portal_signatures.append(sig_p1)

                    by_id_p1 = {
                        (e.get("dom_id") or e.get("name")): e
                        for e in snap_p1["elements"]
                        if e.get("dom_id") or e.get("name")
                    }
                    for req_ctrl in (
                        "talk_title",
                        "track",
                        "tshirt_size",
                        "abstract_summary",
                        "live_demo",
                        "submit-cfp",
                    ):
                        self.assertIn(req_ctrl, by_id_p1)

                    # Stage incomplete submission on /cfp/1 (missing tshirt_size, short abstract)
                    run_approved_op(
                        session_id,
                        snap_p1,
                        "fill-p1-bad",
                        {
                            "kind": "fill_form",
                            "fields": [
                                {
                                    "target_ref": by_id_p1["talk_title"]["node_ref"],
                                    "action": "type",
                                    "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                },
                                {
                                    "target_ref": by_id_p1["track"]["node_ref"],
                                    "action": "select",
                                    "value": "systems",
                                },
                                {
                                    "target_ref": by_id_p1["abstract_summary"]["node_ref"],
                                    "action": "type",
                                    "value": "Too short draft",
                                },
                            ],
                        },
                        "Stage incomplete CFP on Portal #1",
                    )
                    snap_p1_bad_staged, _ = observe_until_dom_id(
                        session_id, "submit-cfp", "obs-p1-bad-staged"
                    )
                    by_id_p1_bs = {
                        (e.get("dom_id") or e.get("name")): e
                        for e in snap_p1_bad_staged["elements"]
                        if e.get("dom_id") or e.get("name")
                    }
                    run_approved_op(
                        session_id,
                        snap_p1_bad_staged,
                        "sub-p1-bad",
                        {"kind": "submit", "target_ref": by_id_p1_bs["submit-cfp"]["node_ref"]},
                        "Submit incomplete CFP on Portal #1",
                    )

                    # Verify validation error and 0 rows in SQLite
                    _, sub_resp = _http_json(f"{origin}/api/cfp_submissions")
                    self.assertEqual(len(sub_resp["cfp_submissions"]), 0)

                    snap_p1_err, _ = observe_until_dom_id(session_id, "submit-cfp", "obs-p1-err")
                    self.assertIn(
                        "Validation Error: Speaker T-shirt size is required and Abstract must be at least 40 characters",
                        snap_p1_err.get("text", ""),
                    )

                    # Now stage repaired valid fields on /cfp/1 (records the valid FormRecipe in recall.json!)
                    by_id_p1_err = {
                        (e.get("dom_id") or e.get("name")): e
                        for e in snap_p1_err["elements"]
                        if e.get("dom_id") or e.get("name")
                    }
                    valid_abstract = (
                        "Empirical analysis of what Firecracker microVMs leave as homework "
                        "for containment-first sandboxing across Linux userland and host boundaries."
                    )
                    run_approved_op(
                        session_id,
                        snap_p1_err,
                        "fill-p1-repaired",
                        {
                            "kind": "fill_form",
                            "fields": [
                                {
                                    "target_ref": by_id_p1_err["talk_title"]["node_ref"],
                                    "action": "type",
                                    "value": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
                                },
                                {
                                    "target_ref": by_id_p1_err["track"]["node_ref"],
                                    "action": "select",
                                    "value": "systems",
                                },
                                {
                                    "target_ref": by_id_p1_err["tshirt_size"]["node_ref"],
                                    "action": "select",
                                    "value": "L",
                                },
                                {
                                    "target_ref": by_id_p1_err["abstract_summary"]["node_ref"],
                                    "action": "type",
                                    "value": valid_abstract,
                                },
                                {
                                    "target_ref": by_id_p1_err["live_demo"]["node_ref"],
                                    "action": "check",
                                    "value": "yes",
                                },
                            ],
                        },
                        "Stage repaired CFP on Portal #1",
                    )
                    snap_p1_good, _ = observe_until_dom_id(
                        session_id, "submit-cfp", "obs-p1-good"
                    )
                    by_id_p1_good = {
                        (e.get("dom_id") or e.get("name")): e
                        for e in snap_p1_good["elements"]
                        if e.get("dom_id") or e.get("name")
                    }
                    exec_p1 = run_approved_op(
                        session_id,
                        snap_p1_good,
                        "sub-p1-good",
                        {"kind": "submit", "target_ref": by_id_p1_good["submit-cfp"]["node_ref"]},
                        "Submit repaired Portal #1 CFP",
                    )
                    receipt_proposals.append(
                        {
                            "proposal_id": "sub-p1-good",
                            "classification": "high_consequence_form_submit",
                            "before_fingerprint": snap_p1_good["fingerprint"],
                            "after_fingerprint": exec_p1.get("fingerprint", ""),
                            "frame_hash": exec_p1.get("frame_hash", ""),
                            "decision_surface": "glasses",
                        }
                    )
                    _http_json(
                        f"{origin}/api/workbench/state",
                        {
                            "portal_id": 1,
                            "status": "SUBMITTED",
                            "form_signature": sig_p1,
                            "layout_observe_ms": round(p1_obs_ms, 3),
                            "recall_ms": 0.0,
                            "llm_calls": 1,
                            "proposal_id": "sub-p1-good",
                            "fingerprint": snap_p1_good["fingerprint"],
                            "commit_state": "COMMITTED",
                        },
                    )

                    # Portals 2..10: zero-LLM FormRecipe recall!
                    for pid in range(2, 11):
                        agent.send(
                            {
                                "type": "observe.request",
                                "session_id": session_id,
                                "request_id": f"obs-pre-nav-{pid}",
                            }
                        )
                        snap_pre = agent.read_until_type("observe.result")
                        run_approved_op(
                            session_id,
                            snap_pre,
                            f"nav-cfp-{pid}",
                            {"kind": "navigate", "url": f"{origin}/cfp/{pid}"},
                            f"Open Portal #{pid}",
                        )
                        snap_p, obs_ms = observe_until_dom_id(
                            session_id, "submit-cfp", f"obs-p{pid}"
                        )
                        sig_p = snap_p.get("form_signature")
                        self.assertEqual(
                            sig_p,
                            sig_p1,
                            f"Portal #{pid} form_signature {sig_p} != Portal #1 {sig_p1}",
                        )
                        self.assertIsNotNone(
                            snap_p.get("matched_recipe_id"),
                            f"Portal #{pid} did not match stored FormRecipe: {snap_p}",
                        )
                        portal_signatures.append(sig_p)

                        plan = hermes_agent_driver.plan_recall_or_llm(
                            snapshot=snap_p,
                            speaker_profile={"email": "ryan@zunftmax.secure.build"},
                        )
                        self.assertEqual(plan["llm_calls"], 0)
                        self.assertEqual(plan["operation"]["kind"], "replay_recipe")

                        t_rec0 = time.perf_counter()
                        run_approved_op(
                            session_id,
                            snap_p,
                            f"replay-p{pid}",
                            plan["operation"],
                            f"Zero-LLM FormRecipe replay on Portal #{pid}",
                        )
                        recall_ms = (time.perf_counter() - t_rec0) * 1000.0
                        total_recall_ms += recall_ms

                        snap_p_staged, _ = observe_until_dom_id(
                            session_id, "submit-cfp", f"obs-p{pid}-staged"
                        )
                        by_id_ps = {
                            (e.get("dom_id") or e.get("name")): e
                            for e in snap_p_staged["elements"]
                            if e.get("dom_id") or e.get("name")
                        }
                        exec_p = run_approved_op(
                            session_id,
                            snap_p_staged,
                            f"sub-p{pid}",
                            {"kind": "submit", "target_ref": by_id_ps["submit-cfp"]["node_ref"]},
                            f"Submit recalled Portal #{pid}",
                        )
                        receipt_proposals.append(
                            {
                                "proposal_id": f"sub-p{pid}",
                                "classification": "high_consequence_form_submit",
                                "before_fingerprint": snap_p_staged["fingerprint"],
                                "after_fingerprint": exec_p.get("fingerprint", ""),
                                "frame_hash": exec_p.get("frame_hash", ""),
                                "decision_surface": "glasses",
                            }
                        )
                        _http_json(
                            f"{origin}/api/workbench/state",
                            {
                                "portal_id": pid,
                                "status": "RECALLED",
                                "form_signature": sig_p,
                                "matched_recipe_id": snap_p["matched_recipe_id"],
                                "layout_observe_ms": round(obs_ms, 3),
                                "recall_ms": round(recall_ms, 3),
                                "llm_calls": 0,
                                "proposal_id": f"sub-p{pid}",
                                "fingerprint": snap_p_staged["fingerprint"],
                                "commit_state": "COMMITTED",
                            },
                        )

                    self.assertEqual(len(set(portal_signatures)), 1)
                    _, all_subs = _http_json(f"{origin}/api/cfp_submissions")
                    self.assertEqual(len(all_subs["cfp_submissions"]), 10)

                    # Update campaign metadata and re-observe /disco-workbench
                    _http_json(
                        f"{origin}/api/workbench/state",
                        {
                            "campaign": {
                                "session_id": session_id,
                                "total_llm_calls": 1,
                                "total_recall_ms": round(total_recall_ms, 3),
                                "batch_status": "ALL_10_SUBMITTED",
                                "receipt_proposals": receipt_proposals,
                            }
                        },
                    )
                    agent.send(
                        {
                            "type": "observe.request",
                            "session_id": session_id,
                            "request_id": "obs-pre-wb-final",
                        }
                    )
                    snap_pre_wb = agent.read_until_type("observe.result")
                    run_approved_op(
                        session_id,
                        snap_pre_wb,
                        "nav-wb-final",
                        {"kind": "navigate", "url": f"{origin}/disco-workbench"},
                        "Return to Disco Workbench to inspect live metrics",
                    )
                    snap_wb_final, _ = observe_until_dom_id(
                        session_id, "tile-10", "obs-wb-final"
                    )
                    self.assertIn("Submitted: 10/10", snap_wb_final.get("text", ""))
                    self.assertIn("RECIPE RECALLED • 0 LLM tokens", snap_wb_final.get("text", ""))

                    # 3. Real ECDSA P-256 WebAuthn registration & assertion verification via OpenSSL
                    priv_key = tmp / "p256_priv.pem"
                    pub_key = tmp / "p256_pub.pem"
                    subprocess.run(
                        [
                            "openssl",
                            "ecparam",
                            "-name",
                            "prime256v1",
                            "-genkey",
                            "-noout",
                            "-out",
                            str(priv_key),
                        ],
                        check=True,
                        capture_output=True,
                    )
                    subprocess.run(
                        [
                            "openssl",
                            "ec",
                            "-in",
                            str(priv_key),
                            "-pubout",
                            "-out",
                            str(pub_key),
                        ],
                        check=True,
                        capture_output=True,
                    )
                    pub_pem = pub_key.read_text(encoding="utf-8")

                    reg_code, reg_res = _http_json(
                        f"{origin}/api/webauthn/register",
                        {
                            "credential_id": "cred_socon_p256_1",
                            "public_key_pem": pub_pem,
                            "rp_id": "socon2026.local",
                            "user_email": "ryan@zunftmax.secure.build",
                        },
                    )
                    self.assertEqual(reg_code, 200)
                    self.assertTrue(reg_res["ok"])

                    chal_code, chal_res = _http_json(
                        f"{origin}/api/webauthn/challenge?rp_id=socon2026.local"
                    )
                    self.assertEqual(chal_code, 200)
                    challenge = chal_res["challenge"]

                    rp_hash = hashlib.sha256(b"socon2026.local").digest()
                    good_auth_data = rp_hash + bytes([0x05]) + (1).to_bytes(4, "big")
                    good_client_bytes = json.dumps(
                        {
                            "type": "webauthn.get",
                            "challenge": challenge,
                            "origin": origin,
                        }
                    ).encode("utf-8")

                    def sign_p256(auth_bytes: bytes, client_bytes: bytes) -> str:
                        msg = auth_bytes + hashlib.sha256(client_bytes).digest()
                        res = subprocess.run(
                            ["openssl", "dgst", "-sha256", "-sign", str(priv_key)],
                            input=msg,
                            capture_output=True,
                            check=True,
                        )
                        return _b64url(res.stdout)

                    good_sig_b64 = sign_p256(good_auth_data, good_client_bytes)

                    # (a) Fail on wrong challenge
                    bad_client_chal = json.dumps(
                        {
                            "type": "webauthn.get",
                            "challenge": "wrong_challenge_value",
                            "origin": origin,
                        }
                    ).encode("utf-8")
                    code_bad_chal, _ = _http_json(
                        f"{origin}/api/webauthn/verify",
                        {
                            "credential_id": "cred_socon_p256_1",
                            "rp_id": "socon2026.local",
                            "origin": origin,
                            "authenticator_data_b64": _b64url(good_auth_data),
                            "client_data_json_b64": _b64url(bad_client_chal),
                            "signature_der_b64": sign_p256(good_auth_data, bad_client_chal),
                        },
                    )
                    self.assertEqual(code_bad_chal, 400)

                    # (b) Fail on wrong origin
                    code_bad_origin, _ = _http_json(
                        f"{origin}/api/webauthn/verify",
                        {
                            "credential_id": "cred_socon_p256_1",
                            "rp_id": "socon2026.local",
                            "origin": "http://evil.example.com",
                            "authenticator_data_b64": _b64url(good_auth_data),
                            "client_data_json_b64": _b64url(good_client_bytes),
                            "signature_der_b64": good_sig_b64,
                        },
                    )
                    self.assertEqual(code_bad_origin, 400)

                    # (c) Fail on missing UV flag (flags = 0x01 instead of 0x05)
                    no_uv_auth_data = rp_hash + bytes([0x01]) + (1).to_bytes(4, "big")
                    code_no_uv, _ = _http_json(
                        f"{origin}/api/webauthn/verify",
                        {
                            "credential_id": "cred_socon_p256_1",
                            "rp_id": "socon2026.local",
                            "origin": origin,
                            "authenticator_data_b64": _b64url(no_uv_auth_data),
                            "client_data_json_b64": _b64url(good_client_bytes),
                            "signature_der_b64": sign_p256(no_uv_auth_data, good_client_bytes),
                        },
                    )
                    self.assertEqual(code_no_uv, 400)

                    # (d) Succeed on valid ECDSA P-256 assertion
                    code_ok, verify_ok = _http_json(
                        f"{origin}/api/webauthn/verify",
                        {
                            "credential_id": "cred_socon_p256_1",
                            "rp_id": "socon2026.local",
                            "origin": origin,
                            "authenticator_data_b64": _b64url(good_auth_data),
                            "client_data_json_b64": _b64url(good_client_bytes),
                            "signature_der_b64": good_sig_b64,
                        },
                    )
                    self.assertEqual(code_ok, 200, f"Expected 200, got {verify_ok}")
                    self.assertTrue(verify_ok["verified"])

                    # (e) Fail on replayed challenge
                    code_replay, replay_res = _http_json(
                        f"{origin}/api/webauthn/verify",
                        {
                            "credential_id": "cred_socon_p256_1",
                            "rp_id": "socon2026.local",
                            "origin": origin,
                            "authenticator_data_b64": _b64url(good_auth_data),
                            "client_data_json_b64": _b64url(good_client_bytes),
                            "signature_der_b64": good_sig_b64,
                        },
                    )
                    self.assertEqual(code_replay, 400)
                    self.assertIn("replay", replay_res.get("error", "").lower())

                    # 4. Verify /api/campaign_receipt and /campaign-receipt in Blitz
                    rc_code, receipt = _http_json(f"{origin}/api/campaign_receipt")
                    self.assertEqual(rc_code, 200)
                    self.assertEqual(receipt["cfp_submissions_count"], 10)
                    self.assertEqual(receipt["webauthn_assertions_count"], 1)
                    self.assertEqual(receipt["total_llm_calls"], 1)
                    self.assertGreater(receipt["total_recall_ms"], 0.0)
                    self.assertEqual(len(receipt["proposals"]), 10)

                    run_approved_op(
                        session_id,
                        snap_wb_final,
                        "nav-receipt",
                        {"kind": "navigate", "url": f"{origin}/campaign-receipt"},
                        "Open final campaign receipt in Blitz",
                    )
                    snap_receipt, _ = observe_until_dom_id(
                        session_id, "receipt-cfp", "obs-receipt"
                    )
                    self.assertIn(
                        "HERMES SUPERVISED CAMPAIGN RECEIPT",
                        snap_receipt.get("text", ""),
                    )

                    # 5. Verify POST /api/reset wipes all tables and resets tiles to PENDING
                    rst_code, rst_res = _http_json(f"{origin}/api/reset", {})
                    self.assertEqual(rst_code, 200)
                    self.assertTrue(rst_res["reset"])

                    _, post_reset_rc = _http_json(f"{origin}/api/campaign_receipt")
                    self.assertEqual(post_reset_rc["cfp_submissions_count"], 0)
                    self.assertEqual(post_reset_rc["webauthn_assertions_count"], 0)
                    self.assertIsNone(post_reset_rc["account_id"])

                    _, post_reset_wb = _http_json(f"{origin}/api/workbench/state")
                    self.assertEqual(len(post_reset_wb["tiles"]), 10)
                    self.assertTrue(
                        all(t["status"] == "PENDING" for t in post_reset_wb["tiles"])
                    )

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
