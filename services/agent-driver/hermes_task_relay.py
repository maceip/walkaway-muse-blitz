#!/usr/bin/env python3
"""Hermes Task Relay (`services/agent-driver/hermes_task_relay.py` on `:8790`).

Connects the Muse MCP Connector (`services/muse-connector/mcp_server.py` on `:8890`)
and the `./walkaway` CLI directly to:
  1. `apps/hermes-browser` (real native Rust/Blitz browser via `--control-socket`),
  2. `services/hermes-host/decision_bridge.py` (`:19101` host supervision + card binding),
  3. `services/hermes-host/message_bus.py` (`:19099` Android app + Ray-Ban Display glasses DAT bus),
  4. `services/agent-driver/hermes_agent_driver.py` (general-purpose local `Qwen3-4B` DOM form planner).

Endpoints (matching `mcp_server.py` relay contract):
  - `GET  /health`      -> `{"ok": True, "service": "hermes-task-relay"}`
  - `GET  /api/state`   -> current task state, live page URL/title, pending proposal & card, last receipt
  - `POST /api/start`   -> `{"goal": "...", "url": "https://..."}` launches/navigates Hermes & attaches to bridge
  - `POST /api/predict` -> observes live DOM, runs local Qwen3-4B (or FormRecipe recall), stages fields,
                           proposes commit (`requires_glasses=True`), and posts approval card to 19099
  - `POST /api/reject`  -> rejects the currently pending card/proposal so it can never execute
  - `POST /api/stop`    -> stops the active Hermes browser session cleanly
"""

from __future__ import annotations

import argparse
import json
import os
import re
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "services" / "agent-driver"))

import hermes_agent_driver  # noqa: E402
from parallel_campaign_driver import ControlSocketClient  # noqa: E402

DECISION_BRIDGE_SCRIPT = REPO_ROOT / "services" / "hermes-host" / "decision_bridge.py"
MESSAGE_BUS_SCRIPT = REPO_ROOT / "services" / "hermes-host" / "message_bus.py"

DEFAULT_LIVE_URL = os.environ.get("WALKAWAY_DEFAULT_TASK_URL", "https://httpbin.org/forms/post")


def _http_json(
    url: str,
    payload: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
    timeout_s: float = 10.0,
) -> dict[str, Any]:
    req_headers = {"Accept": "application/json"}
    if headers:
        req_headers.update(headers)
    data = None
    method = "GET"
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        req_headers["Content-Type"] = "application/json"
        method = "POST"
    req = urllib.request.Request(url, data=data, headers=req_headers, method=method)
    with urllib.request.urlopen(req, timeout=timeout_s) as resp:
        raw = resp.read().decode("utf-8", errors="replace")
        return json.loads(raw) if raw.strip() else {}


def _extract_url_from_goal(goal: str, explicit_url: str | None = None) -> str:
    if explicit_url and explicit_url.strip():
        u = explicit_url.strip()
        if not u.startswith(("http://", "https://")):
            u = "https://" + u
        return u
    match = re.search(r"https?://[^\s,\"'>)]+", goal or "")
    if match:
        return match.group(0)
    domain_match = re.search(r"\b([a-zA-Z0-9-]+\.(?:org|com|io|net|edu|gov|build)(?:/[^\s,\"'>)]*)?)", goal or "")
    if domain_match:
        return "https://" + domain_match.group(1)
    return DEFAULT_LIVE_URL


class HermesTaskManager:
    """Manages the live Hermes browser session, Decision Bridge attachment, and local Qwen3-4B proposals."""

    def __init__(
        self,
        bus_url: str = "http://127.0.0.1:19099",
        bus_token: str = "",
        bridge_url: str = "http://127.0.0.1:19101",
        show_window: bool = False,
        user_profile: dict[str, str] | None = None,
    ) -> None:
        self.bus_url = bus_url.rstrip("/")
        self.bus_token = bus_token or os.environ.get("WALKAWAY_BUS_TOKEN", "")
        self.bridge_url = bridge_url.rstrip("/")
        self.show_window = show_window or (os.environ.get("HERMES_SHOW_WINDOW") == "1")
        self.user_profile = dict(user_profile or {})

        self._lock = threading.Lock()
        self.work_dir = Path(tempfile.mkdtemp(prefix="walkaway-task-relay-"))
        self.vault_path = Path(os.environ.get("HERMES_VAULT_PATH", str(self.work_dir / "vault.json")))
        self.recall_path = Path(os.environ.get("HERMES_RECALL_PATH", str(self.work_dir / "recall.json")))
        self.audit_path = Path(os.environ.get("HERMES_AUDIT_PATH", str(self.work_dir / "audit.jsonl")))

        self.bus_proc: subprocess.Popen[str] | None = None
        self.bridge_proc: subprocess.Popen[str] | None = None
        self.browser_proc: subprocess.Popen[str] | None = None
        self.agent_client: ControlSocketClient | None = None

        self.host_token = os.environ.get("HERMES_HOST_TOKEN") or secrets.token_urlsafe(18)
        self.agent_token = os.environ.get("HERMES_AGENT_TOKEN") or secrets.token_urlsafe(18)

        self.session_id: str | None = None
        self.status: str = "idle"
        self.goal: str = ""
        self.target_url: str = ""
        self.started_at: float = 0.0
        self.steps_acted: int = 0
        self.decision_model: str | None = None
        self.last_snapshot: dict[str, Any] = {}
        self.pending_decision: dict[str, Any] | None = None
        self.last_decision_receipt: dict[str, Any] | None = None
        self.say: str = "No active browser task."

    def _bus_headers(self) -> dict[str, str]:
        return {"X-Bus-Token": self.bus_token} if self.bus_token else {}

    def ensure_bus_and_bridge(self) -> None:
        """Ensure message_bus.py and decision_bridge.py are reachable, starting them if needed."""
        try:
            _http_json(f"{self.bus_url}/health", timeout_s=1.5)
        except Exception:
            if not self.bus_token:
                self.bus_token = secrets.token_urlsafe(18)
            parsed_bus = urlparse(self.bus_url)
            bus_port = parsed_bus.port or 19099
            b_env = os.environ.copy()
            b_env["WALKAWAY_BUS_TOKEN"] = self.bus_token
            b_env["WALKAWAY_BUS_STATE"] = str(self.work_dir / f"bus-state-{bus_port}.json")
            self.bus_proc = subprocess.Popen(
                [
                    sys.executable,
                    str(MESSAGE_BUS_SCRIPT),
                    "--host",
                    parsed_bus.hostname or "127.0.0.1",
                    "--port",
                    str(bus_port),
                ],
                env=b_env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
            )
            for _ in range(40):
                try:
                    _http_json(f"{self.bus_url}/health", timeout_s=1.0)
                    break
                except Exception:
                    time.sleep(0.05)

        try:
            _http_json(f"{self.bridge_url}/health", headers=self._bus_headers(), timeout_s=1.5)
        except Exception:
            parsed_br = urlparse(self.bridge_url)
            br_port = parsed_br.port or 19101
            br_env = os.environ.copy()
            br_env["WALKAWAY_BUS_TOKEN"] = self.bus_token
            br_env["HERMES_HOST_TOKEN"] = self.host_token
            self.bridge_proc = subprocess.Popen(
                [
                    sys.executable,
                    str(DECISION_BRIDGE_SCRIPT),
                    "--bus-url",
                    self.bus_url,
                    "--http-host",
                    parsed_br.hostname or "127.0.0.1",
                    "--http-port",
                    str(br_port),
                    "--card-mode",
                    "held",
                    "--allow-android-approval",
                ],
                env=br_env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
            )
            for _ in range(40):
                try:
                    _http_json(f"{self.bridge_url}/health", headers=self._bus_headers(), timeout_s=1.0)
                    break
                except Exception:
                    time.sleep(0.05)

    def _stop_browser_locked(self) -> None:
        if self.agent_client is not None:
            try:
                self.agent_client.close()
            except Exception:
                pass
            self.agent_client = None
        if self.browser_proc is not None:
            self.browser_proc.terminate()
            try:
                self.browser_proc.communicate(timeout=2.0)
            except Exception:
                self.browser_proc.kill()
                self.browser_proc.communicate()
            self.browser_proc = None
        self.session_id = None
        self.pending_decision = None

    def _observe_locked(self, label: str = "obs") -> dict[str, Any]:
        if self.agent_client is None or not self.session_id:
            return self.last_snapshot
        self.agent_client.send(
            {
                "type": "observe.request",
                "session_id": self.session_id,
                "request_id": f"{label}-{int(time.time() * 1000)}",
                "include_screenshot": False,
            }
        )
        snap = self.agent_client.read_until_type("observe.result", timeout_s=10.0)
        self.last_snapshot = snap
        return snap

    def _sync_bridge_decisions_locked(self) -> None:
        """Check if a pending proposal was approved or rejected on glasses_dat or android_app."""
        if not self.pending_decision:
            return
        prop_id = self.pending_decision.get("proposal_id")
        if not prop_id:
            return
        try:
            dec_resp = _http_json(
                f"{self.bridge_url}/decisions",
                headers=self._bus_headers(),
                timeout_s=3.0,
            )
        except Exception:
            return

        decisions_list = dec_resp.get("decisions") or []
        human_entry: dict[str, Any] = {}
        for d in decisions_list:
            if (
                str(d.get("proposal_id")) == str(prop_id)
                and d.get("decision") in {"approve", "reject"}
                and d.get("via") in {"glasses_dat", "android_app"}
            ):
                human_entry = d

        for entry in decisions_list:
            receipt = entry.get("receipt") or {}
            if str(entry.get("proposal_id") or receipt.get("proposal_id")) != str(prop_id):
                continue
            rtype = receipt.get("type")
            if rtype == "executed":
                # Drain any pending frames on agent socket and refresh live DOM snapshot
                try:
                    self._observe_locked("post-commit")
                except Exception:
                    pass
                self.steps_acted += 1
                via = human_entry.get("via") or receipt.get("surface") or "glasses_dat"
                surface = human_entry.get("surface") or receipt.get("surface") or via
                self.last_decision_receipt = {
                    "proposal_id": prop_id,
                    "decision": "approve",
                    "via": via,
                    "surface": surface,
                    "message_id": human_entry.get("card_message_id") or entry.get("message_id"),
                    "receipt": receipt,
                }
                self.pending_decision = None
                self.status = "committed"
                title = self.last_snapshot.get("title") or self.last_snapshot.get("url") or ""
                self.say = f"Approved via {via} and committed on {title}."
                return
            if rtype in {"proposal.invalidated", "refused"} or entry.get("decision") == "reject":
                via = human_entry.get("via") or entry.get("via") or "human"
                surface = human_entry.get("surface") or entry.get("surface") or via
                self.last_decision_receipt = {
                    "proposal_id": prop_id,
                    "decision": "reject",
                    "via": via,
                    "surface": surface,
                    "message_id": human_entry.get("card_message_id") or entry.get("message_id"),
                    "receipt": receipt,
                }
                self.pending_decision = None
                self.status = "rejected"
                self.say = f"Rejected proposal {prop_id} via {via}; browser took no action."
                return

    def start_task(self, goal: str, url: str | None = None, profile: dict[str, str] | None = None) -> dict[str, Any]:
        with self._lock:
            self._stop_browser_locked()
            self.ensure_bus_and_bridge()
            if profile:
                self.user_profile.update(profile)

            resolved_url = _extract_url_from_goal(goal, url)
            self.goal = goal or f"Complete form on {resolved_url}"
            self.target_url = resolved_url
            self.started_at = time.perf_counter()
            self.steps_acted = 0
            self.last_decision_receipt = None
            self.pending_decision = None

            sock_path = self.work_dir / f"hermes-{int(time.time() * 1000)}.sock"
            self.host_token = secrets.token_urlsafe(18)
            self.agent_token = secrets.token_urlsafe(18)

            env = os.environ.copy()
            env["HERMES_HOST_TOKEN"] = self.host_token
            env["HERMES_AGENT_TOKEN"] = self.agent_token
            env["HERMES_VAULT_PATH"] = str(self.vault_path)
            env["HERMES_RECALL_PATH"] = str(self.recall_path)
            env["HERMES_AUDIT_PATH"] = str(self.audit_path)
            if self.show_window:
                env["HERMES_SHOW_WINDOW"] = "1"
            else:
                env["HERMES_HIDDEN_WINDOW"] = "1"

            hermes_bin = hermes_agent_driver.resolve_hermes_binary()
            cmd = [
                str(hermes_bin),
                "--control-socket",
                str(sock_path),
                "--supervise-commits-only",
                "--xray",
            ]
            if not self.show_window:
                cmd.append("--hidden-window")

            self.browser_proc = subprocess.Popen(
                cmd,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            deadline = time.monotonic() + 10.0
            while not sock_path.exists() and time.monotonic() < deadline:
                time.sleep(0.04)
            if not sock_path.exists():
                raise RuntimeError("Hermes browser did not create control socket")

            attach_resp = _http_json(
                f"{self.bridge_url}/sockets",
                payload={
                    "sock_path": str(sock_path),
                    "token": self.host_token,
                    "start_url": resolved_url,
                    "session_title": f"Walkaway: {self.goal[:60]}",
                },
                headers=self._bus_headers(),
                timeout_s=10.0,
            )
            self.session_id = str(attach_resp["session_id"])

            self.agent_client = ControlSocketClient(sock_path, self.agent_token)

            # Wait for initial URL load to settle past muse://start and observe live DOM
            snap = self._observe_locked("start")
            for _ in range(60):
                cur_url = str(snap.get("url") or "")
                if cur_url not in {"", "about:blank", "muse://start"} and (
                    snap.get("elements") or snap.get("text")
                ):
                    break
                time.sleep(0.1)
                snap = self._observe_locked("start-poll")

            self.status = "ready"
            self.say = (
                f"Opened {snap.get('title') or resolved_url} in Hermes browser. "
                "Call propose_step to stage fields and request approval on glasses or Android app."
            )
            return self._public_state_locked()

    def predict_next_step(self) -> dict[str, Any]:
        with self._lock:
            self._sync_bridge_decisions_locked()
            if self.pending_decision is not None:
                return self._public_state_locked()
            if self.agent_client is None or not self.session_id:
                raise ValueError("No active browser session. Call walkaway_start_task first.")

            snap = self._observe_locked("predict-pre")
            plan = hermes_agent_driver.plan_general_web_form_with_local_llm(
                snapshot=snap,
                goal=self.goal,
                user_profile=self.user_profile,
            )
            self.decision_model = plan.get("model_id") or "FormRecipe"

            # Step 1: If there are non-destructive form fields to fill, stage them immediately
            stage_op = plan.get("stage_operation")
            if stage_op:
                stage_pid = f"stage-{int(time.time() * 1000)}"
                self.agent_client.send(
                    {
                        "type": "propose",
                        "session_id": self.session_id,
                        "request_id": f"req-{stage_pid}",
                        "proposal_id": stage_pid,
                        "snapshot_id": snap["snapshot_id"],
                        "fingerprint": snap["fingerprint"],
                        "generation": snap["generation"],
                        "operation": stage_op,
                        "rationale": f"Stage form fields for goal: {self.goal}",
                    }
                )
                # Wait for decision_bridge policy_auto execution
                deadline = time.monotonic() + 10.0
                while time.monotonic() < deadline:
                    dec_resp = _http_json(
                        f"{self.bridge_url}/decisions",
                        headers=self._bus_headers(),
                        timeout_s=3.0,
                    )
                    if any(
                        str((d.get("receipt") or {}).get("proposal_id")) == stage_pid
                        and (d.get("receipt") or {}).get("type") == "executed"
                        for d in (dec_resp.get("decisions") or [])
                    ):
                        break
                    time.sleep(0.04)
                self.steps_acted += 1
                snap = self._observe_locked("predict-post-stage")

            # Step 2: Propose the irreversible commit (submit) with requires_glasses=True
            commit_op = plan.get("commit_operation")
            if not commit_op:
                self.status = "staged"
                self.say = "Form fields staged; no submit button found on page."
                return self._public_state_locked()

            commit_pid = f"commit-{int(time.time() * 1000)}"
            self.agent_client.send(
                {
                    "type": "propose",
                    "session_id": self.session_id,
                    "request_id": f"req-{commit_pid}",
                    "proposal_id": commit_pid,
                    "snapshot_id": snap["snapshot_id"],
                    "fingerprint": snap["fingerprint"],
                    "generation": snap["generation"],
                    "operation": commit_op,
                    "rationale": f"Commit form on {snap.get('url')}: {self.goal}",
                }
            )

            # Wait for proposal to appear in decision_bridge GET /pending
            pending_item: dict[str, Any] | None = None
            deadline_p = time.monotonic() + 10.0
            while time.monotonic() < deadline_p:
                pend_resp = _http_json(
                    f"{self.bridge_url}/pending",
                    headers=self._bus_headers(),
                    timeout_s=3.0,
                )
                for item in pend_resp.get("pending") or []:
                    if (
                        str(item.get("session_id")) == str(self.session_id)
                        and str(item.get("proposal_id")) == commit_pid
                    ):
                        pending_item = item
                        break
                if pending_item is not None:
                    break
                time.sleep(0.04)

            if pending_item is None:
                raise RuntimeError(f"Commit proposal {commit_pid} did not reach decision_bridge /pending")

            card_title = plan["card_title"]
            card_body = plan["card_body"]
            manifest = [
                {
                    "session_id": self.session_id,
                    "proposal_id": commit_pid,
                    "fingerprint": pending_item["fingerprint"],
                    "generation": pending_item["generation"],
                }
            ]
            card_msg = _http_json(
                f"{self.bridge_url}/cards",
                payload={
                    "title": card_title,
                    "body": card_body,
                    "kind": "high_consequence_commit",
                    "batch_proposals": manifest,
                },
                headers=self._bus_headers(),
                timeout_s=5.0,
            )

            submit_label = (plan.get("submit_target") or {}).get("label") or "Submit"
            self.pending_decision = {
                "proposal_id": commit_pid,
                "operation": "submit",
                "label": submit_label,
                "choice": ", ".join(plan.get("fields_preview") or []),
                "card_message_id": card_msg.get("id"),
                "card_title": card_title,
                "card_body": card_body,
                "fingerprint": pending_item["fingerprint"],
                "generation": pending_item["generation"],
            }
            self.status = "awaiting_approval"
            self.say = (
                f"Staged form on {snap.get('title') or snap.get('url')} and sent approval card "
                f"#{card_msg.get('id')} ({card_title}) to your glasses and Android app."
            )
            return self._public_state_locked()

    def reject_pending_step(self) -> dict[str, Any]:
        with self._lock:
            self._sync_bridge_decisions_locked()
            if not self.pending_decision:
                self.say = "No pending proposal to reject."
                return self._public_state_locked()
            mid = self.pending_decision.get("card_message_id")
            if mid is not None:
                _http_json(
                    f"{self.bus_url}/ack",
                    payload={
                        "message_id": int(mid),
                        "action": "reject",
                        "source": "glasses_dat",
                    },
                    headers=self._bus_headers(),
                    timeout_s=5.0,
                )
                deadline = time.monotonic() + 5.0
                while time.monotonic() < deadline and self.pending_decision is not None:
                    self._sync_bridge_decisions_locked()
                    if self.pending_decision is None:
                        break
                    time.sleep(0.04)
            return self._public_state_locked()

    def stop_task(self) -> dict[str, Any]:
        with self._lock:
            self._stop_browser_locked()
            self.status = "stopped"
            self.say = "Stopped the Walkaway browser task."
            return self._public_state_locked()

    def get_state(self) -> dict[str, Any]:
        with self._lock:
            self._sync_bridge_decisions_locked()
            return self._public_state_locked()

    def _public_state_locked(self) -> dict[str, Any]:
        elapsed_ms = int((time.perf_counter() - self.started_at) * 1000.0) if self.started_at else 0
        page = {
            "title": self.last_snapshot.get("title") or "",
            "url": self.last_snapshot.get("url") or self.target_url,
        }
        return {
            "ok": True,
            "say": self.say,
            "status": self.status,
            "goal": self.goal,
            "page": page,
            "decision": self.pending_decision,
            "last_decision": self.last_decision_receipt,
            "history_count": self.steps_acted,
            "elapsed_ms": elapsed_ms,
            "decision_model": self.decision_model,
            "bus_url": self.bus_url,
            "bridge_url": self.bridge_url,
        }

    def close(self) -> None:
        with self._lock:
            self._stop_browser_locked()
            for proc in (self.bridge_proc, self.bus_proc):
                if proc is not None:
                    proc.terminate()
                    try:
                        proc.communicate(timeout=2.0)
                    except Exception:
                        proc.kill()
                        proc.communicate()
            self.bridge_proc = None
            self.bus_proc = None


class RelayHTTPServer(ThreadingHTTPServer):
    allow_reuse_address = True


def make_handler(manager: HermesTaskManager):
    class RelayHandler(BaseHTTPRequestHandler):
        def _send_json(self, code: int, payload: dict[str, Any]) -> None:
            raw = json.dumps(payload).encode("utf-8")
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def _read_json(self) -> dict[str, Any]:
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0:
                return {}
            return json.loads(self.rfile.read(length).decode("utf-8"))

        def do_GET(self) -> None:
            path = urlparse(self.path).path
            if path == "/health":
                self._send_json(200, {"ok": True, "service": "hermes-task-relay"})
                return
            if path == "/api/state":
                self._send_json(200, manager.get_state())
                return
            self._send_json(404, {"error": f"Unknown GET path {path}"})

        def do_POST(self) -> None:
            path = urlparse(self.path).path
            try:
                body = self._read_json()
                if path == "/api/start":
                    state = manager.start_task(
                        goal=str(body.get("goal") or ""),
                        url=body.get("url"),
                        profile=body.get("profile"),
                    )
                    self._send_json(200, state)
                    return
                if path == "/api/predict":
                    state = manager.predict_next_step()
                    self._send_json(200, state)
                    return
                if path == "/api/reject":
                    state = manager.reject_pending_step()
                    self._send_json(200, state)
                    return
                if path == "/api/stop":
                    state = manager.stop_task()
                    self._send_json(200, state)
                    return
                self._send_json(404, {"error": f"Unknown POST path {path}"})
            except Exception as exc:
                self._send_json(400, {"error": str(exc)})

        def log_message(self, *_args: object) -> None:
            pass

    return RelayHandler


def main() -> int:
    parser = argparse.ArgumentParser(description="Hermes Task Relay for Muse MCP & Live Webpages")
    parser.add_argument("--host", default=os.environ.get("WALKAWAY_RELAY_HOST", "127.0.0.1"))
    parser.add_argument("--port", type=int, default=int(os.environ.get("WALKAWAY_RELAY_PORT", "8790")))
    parser.add_argument("--bus-url", default=os.environ.get("WALKAWAY_BUS_URL", "http://127.0.0.1:19099"))
    parser.add_argument("--bus-token", default=os.environ.get("WALKAWAY_BUS_TOKEN", ""))
    parser.add_argument("--bridge-url", default=os.environ.get("WALKAWAY_BRIDGE_URL", "http://127.0.0.1:19101"))
    parser.add_argument("--show-window", action="store_true", help="Show the Hermes browser window")
    args = parser.parse_args()

    manager = HermesTaskManager(
        bus_url=args.bus_url,
        bus_token=args.bus_token,
        bridge_url=args.bridge_url,
        show_window=args.show_window,
    )
    server = RelayHTTPServer((args.host, args.port), make_handler(manager))
    print(f"Hermes Task Relay listening on http://{args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        manager.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
