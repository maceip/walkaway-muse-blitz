#!/usr/bin/env python3
"""Parallel 10-Portal Disco Campaign Driver with Resident Qwen3-4B & 3-Card Glasses Batching.

Provides:
1. `ResidentQwenDriver`:
   Starts a persistent `/Users/mac/.local/share/uv/tools/mlx-lm/bin/python3` worker process
   that loads `mlx-community/Qwen3-4B-Instruct-2507-4bit` once at startup, prints `"READY"`,
   and serves JSON form-repair requests over stdin/stdout with zero cold-start overhead.
2. `GlassesBusCoordinator`:
   Routes the 3 high-consequence commit cards through `services/hermes-host/message_bus.py`,
   strictly ignores intermediate acks (`"received"`, `"dat_sent"`, `"dat_failed"` or non-`"glasses_dat"`),
   and enforces idempotency across `(msg_id, action)` and `(proposal_id, fingerprint, generation)`.
3. `run_10portal_campaign(...)`:
   Executes Act 1 (`/signup` account + blind vault commit -> Glasses Card #1),
   Act 2 (`/cfp/1` validation failure -> resident `Qwen3-4B` self-correction -> Glasses Card #2),
   and Act 3 (`/cfp/2`..`/cfp/10` zero-LLM `FormRecipe` recall + batch manifest -> Glasses Card #3).
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
import urllib.request
from pathlib import Path
from typing import Any, Callable

import hermes_agent_driver

MLX_PYTHON = hermes_agent_driver.resolve_mlx_python()
MODEL_ID = "mlx-community/Qwen3-4B-Instruct-2507-4bit"

HERMES_BIN = hermes_agent_driver.resolve_hermes_binary()

DEFAULT_SPEAKER_PROFILE: dict[str, str] = {
    "speaker_name": "Ryan MacArthur",
    "full_name": "Ryan MacArthur",
    "email": "ryan@zunftmax.secure.build",
    "affiliation": "Zunftmax Systems",
    "talk_title": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
    "signup_track": "sandboxing",
    "track": "systems",
    "tshirt_size": "L",
    "abstract_summary": (
        "Empirical analysis of what Firecracker microVMs leave as homework "
        "for containment-first sandboxing across Linux userland and host boundaries."
    ),
    "live_demo": "yes",
}

PORTAL_CONFERENCES: dict[int, str] = {
    1: "SoCon 2026",
    2: "KubeCon NA",
    3: "BlackHat USA",
    4: "RustConf",
    5: "NeurIPS Systems",
    6: "DEF CON AI",
    7: "USENIX Security",
    8: "AI Engineer Fair",
    9: "PyCon US",
    10: "QCon SF",
}

_RESIDENT_MLX_WORKER_SCRIPT = r"""
import json
import sys
import time
from mlx_lm import load, generate

model_id = sys.argv[1]
t_load0 = time.perf_counter()
model, tokenizer = load(model_id)
load_ms = (time.perf_counter() - t_load0) * 1000.0
sys.stdout.write(json.dumps({"status": "READY", "model_id": model_id, "load_ms": load_ms}) + "\n")
sys.stdout.write("READY\n")
sys.stdout.flush()

for raw_line in sys.stdin:
    line = raw_line.strip()
    if not line:
        continue
    req = json.loads(line)
    if req.get("cmd") == "shutdown":
        sys.stdout.write(json.dumps({"ok": True, "shutdown": True}) + "\n")
        sys.stdout.flush()
        break
    messages = req["messages"]
    max_tokens = int(req.get("max_tokens", 320))
    t0 = time.perf_counter()
    prompt = tokenizer.apply_chat_template(
        messages,
        tokenize=False,
        add_generation_prompt=True,
    )
    out = generate(
        model,
        tokenizer,
        prompt=prompt,
        max_tokens=max_tokens,
        verbose=False,
    )
    infer_ms = (time.perf_counter() - t0) * 1000.0
    sys.stdout.write(
        json.dumps(
            {
                "ok": True,
                "model_id": model_id,
                "output": out,
                "worker_infer_ms": infer_ms,
            }
        )
        + "\n"
    )
    sys.stdout.flush()
"""


def _extract_json_object(raw_text: str) -> dict[str, Any]:
    match = re.search(r"\{.*\}", raw_text, flags=re.DOTALL)
    if not match:
        raise ValueError(f"Model output did not contain a JSON object: {raw_text!r}")
    return json.loads(match.group(0))


class ResidentQwenDriver:
    """Persistent resident MLX Qwen3-4B worker process that avoids cold-load overhead."""

    def __init__(self, model_id: str = MODEL_ID, mlx_python: Path = MLX_PYTHON) -> None:
        self.model_id = model_id
        self.mlx_python = Path(mlx_python)
        if not self.mlx_python.exists():
            raise RuntimeError(f"MLX Python interpreter not found at {self.mlx_python}")
        self._lock = threading.Lock()
        self.llm_calls = 0
        self.load_ms = 0.0
        self.proc: subprocess.Popen[str] | None = None
        self.start()

    def start(self) -> None:
        if self.proc is not None and self.proc.poll() is None:
            return
        t0 = time.perf_counter()
        self.proc = subprocess.Popen(
            [str(self.mlx_python), "-u", "-c", _RESIDENT_MLX_WORKER_SCRIPT, self.model_id],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        assert self.proc.stdout is not None
        deadline = time.monotonic() + 90.0
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                err = self.proc.stderr.read() if self.proc.stderr else ""
                raise RuntimeError(
                    f"ResidentQwenDriver exited early ({self.proc.returncode}): {err}"
                )
            line = self.proc.stdout.readline()
            if not line:
                continue
            stripped = line.strip()
            if stripped == "READY":
                self.load_ms = (time.perf_counter() - t0) * 1000.0
                return
            if stripped.startswith("{"):
                try:
                    info = json.loads(stripped)
                    if info.get("status") == "READY" and "load_ms" in info:
                        self.load_ms = float(info["load_ms"])
                except Exception:
                    pass
        raise TimeoutError("Timed out waiting for ResidentQwenDriver READY signal")

    def repair_form(
        self,
        snapshot: dict[str, Any],
        speaker_profile: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Run live resident Qwen3-4B inference to repair validation errors and emit fill_form."""
        profile = dict(DEFAULT_SPEAKER_PROFILE)
        if speaker_profile:
            profile.update(speaker_profile)

        elements = snapshot.get("elements") or []
        controls_summary = []
        by_key: dict[str, dict[str, Any]] = {}
        ordered_keys: list[str] = []
        for el in elements:
            key = el.get("dom_id") or el.get("name")
            if not key or el.get("tag") not in {"input", "select", "textarea"}:
                continue
            by_key[key] = el
            ordered_keys.append(key)
            controls_summary.append(
                {
                    "field": key,
                    "tag": el.get("tag"),
                    "input_type": el.get("input_type"),
                    "label": el.get("label"),
                    "current_value": el.get("value"),
                }
            )

        messages = [
            {
                "role": "system",
                "content": (
                    "You are a form-repair co-processor for the Hermes supervised browser. "
                    "Output ONLY a valid JSON object mapping each form field name to its corrected string value "
                    "using the provided speaker_profile so all validation errors on the page are satisfied. "
                    "Do not output markdown fences or prose."
                ),
            },
            {
                "role": "user",
                "content": json.dumps(
                    {
                        "page_text": snapshot.get("text", ""),
                        "controls": controls_summary,
                        "speaker_profile": {
                            k: profile[k]
                            for k in ordered_keys
                            if k in profile
                        }
                        or profile,
                    },
                    ensure_ascii=False,
                ),
            },
        ]

        with self._lock:
            self.start()
            assert self.proc is not None
            assert self.proc.stdin is not None
            assert self.proc.stdout is not None

            t0 = time.perf_counter()
            self.proc.stdin.write(json.dumps({"messages": messages, "max_tokens": 320}) + "\n")
            self.proc.stdin.flush()

            resp_line = ""
            while True:
                line = self.proc.stdout.readline()
                if not line:
                    err = self.proc.stderr.read() if self.proc.stderr else ""
                    raise RuntimeError(f"ResidentQwenDriver closed stdout unexpectedly: {err}")
                if line.strip().startswith("{"):
                    resp_line = line.strip()
                    break
            elapsed_ms = (time.perf_counter() - t0) * 1000.0
            self.llm_calls += 1

        res = json.loads(resp_line)
        raw_output = str(res.get("output") or "")
        field_values = _extract_json_object(raw_output)

        # Ensure every form control on the page is bound so Hermes records a complete FormRecipe.
        merged_values: dict[str, Any] = {}
        for key in ordered_keys:
            if key in field_values and str(field_values[key]).strip() != "":
                merged_values[key] = field_values[key]
            elif key in profile:
                merged_values[key] = profile[key]
            elif by_key[key].get("value"):
                merged_values[key] = by_key[key]["value"]

        fields_ops = []
        for field_name in ordered_keys:
            if field_name not in merged_values:
                continue
            el = by_key[field_name]
            val = merged_values[field_name]
            tag = el.get("tag")
            input_type = el.get("input_type")
            if tag == "select":
                action = "select"
            elif tag == "input" and input_type in {"checkbox", "radio"}:
                action = "check"
                if str(val).lower() in {"yes", "true", "1", "checked", "on"}:
                    val = "true"
            else:
                action = "type"
            fields_ops.append(
                {
                    "target_ref": el["node_ref"],
                    "action": action,
                    "value": str(val),
                }
            )

        if not fields_ops:
            raise RuntimeError(f"Model produced no matching form fields: {raw_output!r}")

        return {
            "llm_calls": 1,
            "model_id": self.model_id,
            "latency_ms": elapsed_ms,
            "llm_repair_ms": elapsed_ms,
            "raw_model_output": raw_output,
            "repaired_fields": field_values,
            "operation": {
                "kind": "fill_form",
                "fields": fields_ops,
            },
        }

    def close(self) -> None:
        with self._lock:
            if self.proc is None:
                return
            proc = self.proc
            self.proc = None
            if proc.poll() is None:
                try:
                    if proc.stdin:
                        proc.stdin.write(json.dumps({"cmd": "shutdown"}) + "\n")
                        proc.stdin.flush()
                    proc.wait(timeout=3.0)
                except Exception:
                    proc.terminate()
                    try:
                        proc.wait(timeout=2.0)
                    except Exception:
                        proc.kill()
                        proc.wait()
            for stream in (proc.stdin, proc.stdout, proc.stderr):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception:
                        pass

    def __enter__(self) -> ResidentQwenDriver:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


_SHARED_RESIDENT_DRIVER: ResidentQwenDriver | None = None
_SHARED_DRIVER_LOCK = threading.Lock()


def get_resident_qwen_driver() -> ResidentQwenDriver:
    global _SHARED_RESIDENT_DRIVER
    with _SHARED_DRIVER_LOCK:
        if _SHARED_RESIDENT_DRIVER is None:
            _SHARED_RESIDENT_DRIVER = ResidentQwenDriver()
        return _SHARED_RESIDENT_DRIVER


class ControlSocketClient:
    """Line-delimited NDJSON Unix-socket client for hermes-blitz-browser."""

    def __init__(self, sock_path: Path, token: str, timeout_s: float = 10.0) -> None:
        self.sock_path = Path(sock_path)
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout_s)
        self.sock.connect(str(self.sock_path))
        self.reader = self.sock.makefile("r", encoding="utf-8")
        self.seq = 0
        self.frames: list[dict[str, Any]] = []
        self.raw_lines: list[str] = []
        self.send({"type": "hello", "request_id": "hello-1", "token": token})
        hello_ok = self.read_until_type("hello.ok", timeout_s=timeout_s)
        if hello_ok.get("type") != "hello.ok":
            raise RuntimeError(f"Handshake failed on {self.sock_path}: {hello_ok}")

    def send(self, payload: dict[str, Any]) -> None:
        self.seq += 1
        frame = {"v": 1, "seq": self.seq, **payload}
        line = json.dumps(frame) + "\n"
        self.sock.sendall(line.encode("utf-8"))

    def read_frame(self) -> dict[str, Any]:
        line = self.reader.readline()
        if not line:
            raise EOFError(f"Control socket {self.sock_path} closed by browser")
        self.raw_lines.append(line)
        frame = json.loads(line)
        self.frames.append(frame)
        return frame

    def read_until_type(
        self,
        expected_types: str | set[str],
        timeout_s: float = 10.0,
    ) -> dict[str, Any]:
        if isinstance(expected_types, str):
            expected_types = {expected_types}
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            frame = self.read_frame()
            if frame.get("type") in expected_types or frame.get("type") == "refused":
                return frame
        raise TimeoutError(f"Timed out waiting for {expected_types} on {self.sock_path}")

    def close(self) -> None:
        try:
            self.reader.close()
        finally:
            self.sock.close()


def _http_json(
    url: str,
    payload: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
    timeout_s: float = 5.0,
) -> dict[str, Any]:
    req_headers = dict(headers or {})
    data = None
    method = "GET"
    if payload is not None:
        req_headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
        method = "POST"
    req = urllib.request.Request(url, data=data, headers=req_headers, method=method)
    with urllib.request.urlopen(req, timeout=timeout_s) as resp:
        raw = resp.read().decode("utf-8")
        return json.loads(raw) if raw.strip() else {}


DECISION_BRIDGE_SCRIPT = hermes_agent_driver.REPO_ROOT / "services" / "hermes-host" / "decision_bridge.py"


def _free_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class GlassesBusCoordinator:
    """Routes all Hermes host supervision and commit cards through decision_bridge.py (19101 held mode)."""

    IGNORED_ACTIONS = {"received", "dat_sent", "dat_failed"}
    TERMINAL_ACTIONS = {"approve", "reject"}

    def __init__(
        self,
        bus_base_url: str,
        bus_token: str,
        decision_bridge_url: str,
    ) -> None:
        self.bus_base_url = bus_base_url.rstrip("/")
        self.bus_token = bus_token
        self.decision_bridge_url = decision_bridge_url.rstrip("/")
        self.headers = {"X-Bus-Token": self.bus_token}
        self.processed_message_ids: set[int] = set()
        self.decided_proposals: set[tuple[str, str, int]] = set()
        self.cards_posted: list[dict[str, Any]] = []
        self.terminal_acks: list[dict[str, Any]] = []
        self.ignored_acks_seen: list[dict[str, Any]] = []

    def attach_socket(
        self,
        sock_path: Path,
        host_token: str,
        start_url: str | None = None,
        session_title: str = "Walkaway Hermes Host Bridge",
    ) -> str:
        payload: dict[str, Any] = {
            "sock_path": str(sock_path),
            "token": host_token,
            "session_title": session_title,
        }
        if start_url is not None:
            payload["start_url"] = start_url
        resp = _http_json(
            f"{self.decision_bridge_url}/sockets",
            payload=payload,
            headers=self.headers,
            timeout_s=12.0,
        )
        sid = str(resp.get("session_id") or "")
        if not sid:
            raise RuntimeError(f"decision_bridge POST /sockets did not return session_id: {resp}")
        return sid

    def wait_for_pending(
        self,
        session_id: str,
        proposal_id: str,
        timeout_s: float = 15.0,
    ) -> dict[str, Any]:
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            pending_resp = _http_json(f"{self.decision_bridge_url}/pending", headers=self.headers)
            for item in pending_resp.get("pending") or []:
                if (
                    str(item.get("session_id")) == str(session_id)
                    and str(item.get("proposal_id")) == str(proposal_id)
                ):
                    return item
            time.sleep(0.03)
        raise TimeoutError(
            f"Timed out waiting for proposal {proposal_id!r} in decision_bridge GET /pending"
        )

    def wait_for_receipt(
        self,
        proposal_id: str,
        expected_types: set[str] | None = None,
        timeout_s: float = 15.0,
    ) -> dict[str, Any]:
        if expected_types is None:
            expected_types = {"executed"}
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            dec_resp = _http_json(f"{self.decision_bridge_url}/decisions", headers=self.headers)
            for entry in dec_resp.get("decisions") or []:
                receipt = entry.get("receipt")
                if not isinstance(receipt, dict):
                    continue
                if str(receipt.get("proposal_id")) == str(proposal_id) and (
                    receipt.get("type") in expected_types or receipt.get("type") == "refused"
                ):
                    return receipt
            time.sleep(0.03)
        raise TimeoutError(
            f"Timed out waiting for receipt on proposal {proposal_id!r} in decision_bridge GET /decisions"
        )

    def bridge_decisions(self) -> list[dict[str, Any]]:
        dec_resp = _http_json(f"{self.decision_bridge_url}/decisions", headers=self.headers)
        return list(dec_resp.get("decisions") or [])

    def post_card(
        self,
        title: str,
        body: str,
        kind: str = "high_consequence_commit",
        batch_proposals: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        if not batch_proposals:
            raise ValueError("post_card requires batch_proposals manifest for decision_bridge POST /cards")
        payload: dict[str, Any] = {
            "title": title,
            "body": body,
            "kind": kind,
            "batch_proposals": batch_proposals,
        }
        msg = _http_json(
            f"{self.decision_bridge_url}/cards",
            payload=payload,
            headers=self.headers,
            timeout_s=10.0,
        )
        msg["batch_proposals"] = batch_proposals
        self.cards_posted.append(msg)
        return msg

    def wait_for_glasses_decision(
        self,
        msg_id: int,
        timeout_s: float = 30.0,
        poll_interval_s: float = 0.03,
        on_poll: Callable[[int], None] | None = None,
    ) -> dict[str, Any]:
        """Wait for a terminal ('approve' or 'reject') ack with source == 'glasses_dat'."""
        if msg_id in self.processed_message_ids:
            return {"status": "ignored_duplicate", "message_id": msg_id}

        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            if on_poll is not None:
                on_poll(msg_id)
                on_poll = None
            acks_resp = _http_json(f"{self.bus_base_url}/acks", headers=self.headers)
            acks = acks_resp.get("acks") or []
            for ack in acks:
                if int(ack.get("message_id") or -1) != int(msg_id):
                    continue
                action = str(ack.get("action") or "").strip()
                source = str(ack.get("source") or "").strip()
                if action in self.IGNORED_ACTIONS or source != "glasses_dat":
                    if ack not in self.ignored_acks_seen:
                        self.ignored_acks_seen.append(ack)
                    continue
                if action in self.TERMINAL_ACTIONS and source == "glasses_dat":
                    self.processed_message_ids.add(int(msg_id))
                    self.terminal_acks.append(ack)
                    return ack
            time.sleep(poll_interval_s)
        raise TimeoutError(f"Timed out waiting for glasses_dat decision on message_id={msg_id}")

    def await_bridge_proposal_receipt(
        self,
        proposal_frame: dict[str, Any],
        decision: str,
        timeout_s: float = 15.0,
    ) -> dict[str, Any]:
        key = (
            str(proposal_frame["proposal_id"]),
            str(proposal_frame["fingerprint"]),
            int(proposal_frame["generation"]),
        )
        if key in self.decided_proposals:
            return {"type": "ignored_duplicate", "proposal_id": key[0]}
        self.decided_proposals.add(key)
        expected = {"executed"} if decision == "approve" else {"proposal.invalidated"}
        return self.wait_for_receipt(str(proposal_frame["proposal_id"]), expected_types=expected, timeout_s=timeout_s)

    def replay_duplicate_ack_check(
        self,
        msg_id: int,
        staged_proposals: list[dict[str, Any]],
    ) -> dict[str, Any]:
        """Verify that replaying an already-processed message_id or proposal key is a strict no-op."""
        msg_already = int(msg_id) in self.processed_message_ids
        skipped = 0
        for item in staged_proposals:
            key = (
                str(item["proposal_id"]),
                str(item["fingerprint"]),
                int(item["generation"]),
            )
            if key in self.decided_proposals:
                skipped += 1
        return {
            "status": "ignored_duplicate" if msg_already else "not_processed",
            "message_id": int(msg_id),
            "executed_count": 0,
            "skipped_proposals": skipped,
        }


def _wait_for_control(
    agent: ControlSocketClient,
    session_id: str,
    control_id: str,
    prefix: str,
    include_screenshot: bool = False,
    max_attempts: int = 30,
) -> tuple[dict[str, Any], float]:
    for attempt in range(max_attempts):
        t0 = time.perf_counter()
        agent.send(
            {
                "type": "observe.request",
                "session_id": session_id,
                "request_id": f"{prefix}-{attempt}",
                "include_screenshot": include_screenshot,
            }
        )
        cand = agent.read_until_type("observe.result")
        obs_ms = (time.perf_counter() - t0) * 1000.0
        els = cand.get("elements") or []
        if any(
            (e.get("dom_id") or e.get("name") or e.get("id")) == control_id
            for e in els
        ):
            return cand, obs_ms
        time.sleep(0.06)
    raise RuntimeError(f"Control {control_id!r} never appeared in session {session_id}")


def _propose_and_host_approve(
    coordinator: GlassesBusCoordinator,
    agent: ControlSocketClient,
    session_id: str,
    proposal_id: str,
    snap: dict[str, Any],
    operation: dict[str, Any],
    rationale: str,
) -> dict[str, Any]:
    """Propose an intermediate staging/navigation action and wait for decision_bridge policy auto-approval."""
    staged_op = dict(operation)
    staged_op["stage_only"] = True
    agent.send(
        {
            "type": "propose",
            "session_id": session_id,
            "request_id": f"req-{proposal_id}",
            "proposal_id": proposal_id,
            "snapshot_id": snap["snapshot_id"],
            "fingerprint": snap["fingerprint"],
            "generation": snap["generation"],
            "operation": staged_op,
            "rationale": rationale,
        }
    )
    exec_frame = coordinator.wait_for_receipt(proposal_id, expected_types={"executed"})
    if exec_frame.get("type") != "executed":
        raise RuntimeError(f"Execution failed for proposal {proposal_id}: {exec_frame}")
    return exec_frame


def _post_workbench_update(fixture_origin: str, payload: dict[str, Any]) -> None:
    try:
        _http_json(f"{fixture_origin.rstrip('/')}/api/workbench/state", payload=payload)
    except Exception:
        pass


def run_10portal_campaign(
    fixture_origin: str,
    sock_path: Path,
    host_token: str,
    agent_token: str,
    bus_base_url: str,
    bus_token: str,
    vault_path: Path,
    recall_path: Path,
    audit_path: Path,
    resident_qwen: ResidentQwenDriver | None = None,
    speaker_profile: dict[str, str] | None = None,
    on_glasses_card: Callable[[int, dict[str, Any]], None] | None = None,
    include_screenshots: bool = True,
    glasses_timeout_s: float = 30.0,
    show_workbench_in_primary: bool = False,
    decision_bridge_url: str | None = None,
    decision_provenance: str = "physical_glasses",
) -> dict[str, Any]:
    """Execute the full 3-Act, 10-Portal Disco Campaign supervised by decision_bridge.py (held mode)."""
    t_campaign0 = time.perf_counter()
    origin = fixture_origin.rstrip("/")
    profile = dict(DEFAULT_SPEAKER_PROFILE)
    if speaker_profile:
        profile.update(speaker_profile)

    own_qwen = False
    if resident_qwen is None:
        resident_qwen = get_resident_qwen_driver()
    qwen_calls_before = resident_qwen.llm_calls

    bridge_proc: subprocess.Popen[str] | None = None
    if not decision_bridge_url:
        bridge_port = _free_local_port()
        decision_bridge_url = f"http://127.0.0.1:{bridge_port}"
        b_env = os.environ.copy()
        b_env["WALKAWAY_BUS_TOKEN"] = bus_token
        b_env["HERMES_HOST_TOKEN"] = host_token
        bridge_proc = subprocess.Popen(
            [
                sys.executable,
                str(DECISION_BRIDGE_SCRIPT),
                "--bus-url",
                bus_base_url,
                "--http-host",
                "127.0.0.1",
                "--http-port",
                str(bridge_port),
                "--card-mode",
                "held",
            ],
            env=b_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        deadline_b = time.monotonic() + 10.0
        while time.monotonic() < deadline_b:
            try:
                bh = _http_json(f"{decision_bridge_url}/health")
                if bh.get("ok"):
                    break
            except Exception:
                time.sleep(0.03)

    coordinator = GlassesBusCoordinator(
        bus_base_url=bus_base_url,
        bus_token=bus_token,
        decision_bridge_url=decision_bridge_url,
    )

    worker_procs: list[subprocess.Popen[str]] = []
    worker_agents: list[ControlSocketClient] = []
    receipt_proposals: list[dict[str, Any]] = []
    agent: ControlSocketClient | None = None

    with tempfile.TemporaryDirectory(prefix="walkaway-campaign-workers-") as worker_tmp_str:
        worker_tmp = Path(worker_tmp_str)
        try:
            # Start worker hermes-blitz-browser processes in parallel for Portals #3..#10 (or #2..#10)
            worker_start_pid = 2 if show_workbench_in_primary else 3
            worker_socks: dict[int, Path] = {}
            for pid in range(worker_start_pid, 11):
                w_sock = worker_tmp / f"hermes_p{pid}.sock"
                w_env = os.environ.copy()
                w_env["HERMES_HOST_TOKEN"] = host_token
                w_env["HERMES_AGENT_TOKEN"] = agent_token
                w_env["HERMES_VAULT_PATH"] = str(vault_path)
                w_env["HERMES_RECALL_PATH"] = str(recall_path)
                w_env["HERMES_AUDIT_PATH"] = str(audit_path)
                w_env["HERMES_HIDDEN_WINDOW"] = "1"
                w_proc = subprocess.Popen(
                    [str(HERMES_BIN), "--control-socket", str(w_sock)],
                    env=w_env,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                worker_procs.append(w_proc)
                worker_socks[pid] = w_sock

            # =========================================================================
            # ACT 1: Speaker Account Creation & Blind Vault Commit on /signup
            # =========================================================================
            signup_url = f"{origin}/signup"
            session_id = coordinator.attach_socket(
                sock_path=sock_path,
                host_token=host_token,
                start_url=signup_url,
                session_title="Muse x Hermes 10-Portal Disco Campaign",
            )
            agent = ControlSocketClient(sock_path, agent_token)

            snap1, _ = _wait_for_control(agent, session_id, "speaker_name", "act1-init")
            by_id1 = {
                (e.get("dom_id") or e.get("name") or e.get("id")): e
                for e in (snap1.get("elements") or [])
                if e.get("dom_id") or e.get("name") or e.get("id")
            }

            signup_fields: list[dict[str, Any]] = []
            if "speaker_name" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["speaker_name"]["node_ref"],
                        "action": "type",
                        "value": profile["speaker_name"],
                    }
                )
            if "full_name" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["full_name"]["node_ref"],
                        "action": "type",
                        "value": profile["full_name"],
                    }
                )
            if "email" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["email"]["node_ref"],
                        "action": "type",
                        "value": profile["email"],
                    }
                )
            if "affiliation" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["affiliation"]["node_ref"],
                        "action": "type",
                        "value": profile["affiliation"],
                    }
                )
            if "password" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["password"]["node_ref"],
                        "action": "fill_secret",
                        "origin": origin,
                        "username": profile["email"],
                        "secret_ref": "socon_speaker_pw",
                    }
                )
            if "talk_title" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["talk_title"]["node_ref"],
                        "action": "type",
                        "value": profile["talk_title"],
                    }
                )
            if "track" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["track"]["node_ref"],
                        "action": "select",
                        "value": profile["signup_track"],
                    }
                )
            if "live_demo" in by_id1:
                signup_fields.append(
                    {
                        "target_ref": by_id1["live_demo"]["node_ref"],
                        "action": "check",
                        "value": "true",
                    }
                )

            _propose_and_host_approve(
                coordinator=coordinator,
                agent=agent,
                session_id=session_id,
                proposal_id="act1-signup-fill",
                snap=snap1,
                operation={"kind": "fill_form", "fields": signup_fields},
                rationale="Act 1: Stage speaker profile and blind vault password",
            )

            snap1_staged, _ = _wait_for_control(
                agent,
                session_id,
                "submit-btn",
                "act1-staged",
                include_screenshot=include_screenshots,
            )
            by_id1_s = {
                (e.get("dom_id") or e.get("name") or e.get("id")): e
                for e in (snap1_staged.get("elements") or [])
                if e.get("dom_id") or e.get("name") or e.get("id")
            }
            agent.send(
                {
                    "type": "propose",
                    "session_id": session_id,
                    "request_id": "req-act1-signup-submit",
                    "proposal_id": "act1-signup-submit",
                    "snapshot_id": snap1_staged["snapshot_id"],
                    "fingerprint": snap1_staged["fingerprint"],
                    "generation": snap1_staged["generation"],
                    "operation": {
                        "kind": "submit",
                        "target_ref": by_id1_s["submit-btn"]["node_ref"],
                    },
                    "rationale": "SoCon 2026 — Create Speaker Account & Seal Vault/Passkey",
                }
            )
            pending_card1 = coordinator.wait_for_pending(session_id, "act1-signup-submit")

            # Glasses Card #1: SoCon 2026 — Create Speaker Account & Seal Vault/Passkey
            card1_manifest = [
                {
                    "proposal_id": str(pending_card1["proposal_id"]),
                    "fingerprint": str(pending_card1["fingerprint"]),
                    "generation": int(pending_card1["generation"]),
                    "session_id": session_id,
                    "portal_id": 1,
                }
            ]
            card1_msg = coordinator.post_card(
                title="SoCon 2026 — Create Speaker Account & Seal Vault/Passkey",
                body=(
                    f"Approve speaker account creation for {profile['email']} and seal "
                    f"blind vault credential (proposal={pending_card1['proposal_id']}, "
                    f"fp={pending_card1['fingerprint'][:16]})."
                ),
                kind="vault_account_commit",
                batch_proposals=card1_manifest,
            )
            card1_ack = coordinator.wait_for_glasses_decision(
                int(card1_msg["id"]),
                timeout_s=glasses_timeout_s,
                on_poll=(lambda _mid: on_glasses_card(1, card1_msg)) if on_glasses_card else None,
            )
            exec_card1 = coordinator.await_bridge_proposal_receipt(
                proposal_frame=pending_card1,
                decision=str(card1_ack["action"]),
                timeout_s=glasses_timeout_s,
            )
            receipt_proposals.append(
                {
                    "proposal_id": pending_card1["proposal_id"],
                    "classification": "vault_account_commit",
                    "before_fingerprint": pending_card1["fingerprint"],
                    "after_fingerprint": exec_card1.get("after_fingerprint", pending_card1["fingerprint"]),
                    "frame_hash": snap1_staged.get("screenshot_hash") or "",
                    "decision_surface": (
                        "synthetic_dry_run"
                        if decision_provenance == "synthetic_dry_run"
                        else str(card1_ack.get("source") or "glasses_dat")
                    ),
                }
            )

            # Register WebAuthn credential from vault if present and verify P-256 assertion for SoCon 2026
            if vault_path.exists():
                try:
                    vault_json = json.loads(vault_path.read_text(encoding="utf-8"))
                    accts = vault_json.get("accounts") or []
                    if accts and accts[0].get("public_key_pem"):
                        acct0 = accts[0]
                        _http_json(
                            f"{origin}/api/webauthn/register",
                            payload={
                                "credential_id": acct0.get("credential_id") or "cred_1",
                                "public_key_pem": acct0["public_key_pem"],
                                "rp_id": "socon2026.local",
                                "user_email": profile["email"],
                            },
                        )
                except Exception:
                    pass

            # =========================================================================
            # ACT 2: Portal #1 (/cfp/1) Validation Error -> Resident Qwen3-4B Repair
            # =========================================================================
            agent.send(
                {
                    "type": "observe.request",
                    "session_id": session_id,
                    "request_id": "act2-pre-nav",
                }
            )
            snap_pre2 = agent.read_until_type("observe.result")
            _propose_and_host_approve(
                coordinator=coordinator,
                agent=agent,
                session_id=session_id,
                proposal_id="act2-nav-cfp1",
                snap=snap_pre2,
                operation={"kind": "navigate", "url": f"{origin}/cfp/1"},
                rationale="Act 2: Open Portal #1 (SoCon 2026 CFP)",
            )

            snap2_init, obs_ms_1 = _wait_for_control(agent, session_id, "tshirt_size", "act2-init")
            by_id2 = {
                (e.get("dom_id") or e.get("name") or e.get("id")): e
                for e in (snap2_init.get("elements") or [])
                if e.get("dom_id") or e.get("name") or e.get("id")
            }

            # Incomplete first draft: missing tshirt_size, short abstract_summary (<40 chars)
            bad_fields: list[dict[str, Any]] = []
            if "talk_title" in by_id2:
                bad_fields.append(
                    {
                        "target_ref": by_id2["talk_title"]["node_ref"],
                        "action": "type",
                        "value": profile["talk_title"],
                    }
                )
            if "track" in by_id2:
                bad_fields.append(
                    {
                        "target_ref": by_id2["track"]["node_ref"],
                        "action": "select",
                        "value": profile["track"],
                    }
                )
            if "abstract_summary" in by_id2:
                bad_fields.append(
                    {
                        "target_ref": by_id2["abstract_summary"]["node_ref"],
                        "action": "type",
                        "value": "Short draft",
                    }
                )
            if "live_demo" in by_id2:
                bad_fields.append(
                    {
                        "target_ref": by_id2["live_demo"]["node_ref"],
                        "action": "check",
                        "value": "true",
                    }
                )

            _propose_and_host_approve(
                coordinator=coordinator,
                agent=agent,
                session_id=session_id,
                proposal_id="act2-cfp1-fill-incomplete",
                snap=snap2_init,
                operation={"kind": "fill_form", "fields": bad_fields},
                rationale="Act 2: Stage initial CFP draft (missing tshirt_size, short abstract)",
            )

            snap2_bad_staged, _ = _wait_for_control(agent, session_id, "submit-cfp", "act2-bad-staged")
            by_id2_bad = {
                (e.get("dom_id") or e.get("name") or e.get("id")): e
                for e in (snap2_bad_staged.get("elements") or [])
                if e.get("dom_id") or e.get("name") or e.get("id")
            }
            _propose_and_host_approve(
                coordinator=coordinator,
                agent=agent,
                session_id=session_id,
                proposal_id="act2-cfp1-submit-incomplete",
                snap=snap2_bad_staged,
                operation={"kind": "submit", "target_ref": by_id2_bad["submit-cfp"]["node_ref"]},
                rationale="Act 2: Attempt CFP #1 submission (triggers server validation error)",
            )

            snap2_err, obs_err_ms = _wait_for_control(
                agent,
                session_id,
                "tshirt_size",
                "act2-val-err",
                include_screenshot=include_screenshots,
            )
            if "Validation Error" not in str(snap2_err.get("text") or ""):
                raise RuntimeError(f"Expected Validation Error on /cfp/1, got: {snap2_err.get('text')!r}")

            _post_workbench_update(
                origin,
                {
                    "portal_id": 1,
                    "conference_name": PORTAL_CONFERENCES[1],
                    "status": "QWEN3-4B ABSTRACT REPAIR",
                    "form_signature": snap2_err.get("form_signature") or "",
                    "layout_observe_ms": obs_err_ms,
                    "llm_calls": 1,
                    "commit_state": "REPAIRING",
                    "screenshot_b64": snap2_err.get("screenshot_png_b64") or "",
                },
            )

            # Resident Qwen3-4B repairs the form in ~1s with zero cold-load overhead
            repair_result = resident_qwen.repair_form(snapshot=snap2_err, speaker_profile=profile)
            llm_repair_ms = float(repair_result["llm_repair_ms"])

            _propose_and_host_approve(
                coordinator=coordinator,
                agent=agent,
                session_id=session_id,
                proposal_id="act2-cfp1-fill-repaired",
                snap=snap2_err,
                operation=repair_result["operation"],
                rationale="Act 2: Apply Qwen3-4B repaired CFP fields and record FormRecipe",
            )

            snap2_repaired, obs_rep_ms = _wait_for_control(
                agent,
                session_id,
                "submit-cfp",
                "act2-repaired",
                include_screenshot=include_screenshots,
            )
            by_id2_rep = {
                (e.get("dom_id") or e.get("name") or e.get("id")): e
                for e in (snap2_repaired.get("elements") or [])
                if e.get("dom_id") or e.get("name") or e.get("id")
            }
            agent.send(
                {
                    "type": "propose",
                    "session_id": session_id,
                    "request_id": "req-act2-cfp1-submit-good",
                    "proposal_id": "act2-cfp1-submit-good",
                    "snapshot_id": snap2_repaired["snapshot_id"],
                    "fingerprint": snap2_repaired["fingerprint"],
                    "generation": snap2_repaired["generation"],
                    "operation": {
                        "kind": "submit",
                        "target_ref": by_id2_rep["submit-cfp"]["node_ref"],
                    },
                    "rationale": "SoCon 2026 — Submit Repaired CFP",
                }
            )
            pending_card2 = coordinator.wait_for_pending(session_id, "act2-cfp1-submit-good")

            _post_workbench_update(
                origin,
                {
                    "portal_id": 1,
                    "conference_name": PORTAL_CONFERENCES[1],
                    "status": "FROZEN FOR GLASSES TAP",
                    "form_signature": snap2_repaired.get("form_signature") or "",
                    "matched_recipe_id": snap2_repaired.get("matched_recipe_id") or "recipe_1",
                    "layout_observe_ms": obs_rep_ms,
                    "recall_ms": 0.0,
                    "llm_calls": 1,
                    "proposal_id": pending_card2["proposal_id"],
                    "fingerprint": pending_card2["fingerprint"],
                    "commit_state": "FROZEN",
                    "screenshot_b64": snap2_repaired.get("screenshot_png_b64") or "",
                },
            )

            # Glasses Card #2: SoCon 2026 — Submit Repaired CFP
            card2_manifest = [
                {
                    "proposal_id": str(pending_card2["proposal_id"]),
                    "fingerprint": str(pending_card2["fingerprint"]),
                    "generation": int(pending_card2["generation"]),
                    "session_id": session_id,
                    "portal_id": 1,
                }
            ]
            card2_msg = coordinator.post_card(
                title="SoCon 2026 — Submit Repaired CFP",
                body=(
                    f"Approve Qwen3-4B repaired CFP submission for SoCon 2026 "
                    f"(proposal={pending_card2['proposal_id']}, "
                    f"fp={pending_card2['fingerprint'][:16]})."
                ),
                kind="cfp_repaired_commit",
                batch_proposals=card2_manifest,
            )
            card2_ack = coordinator.wait_for_glasses_decision(
                int(card2_msg["id"]),
                timeout_s=glasses_timeout_s,
                on_poll=(lambda _mid: on_glasses_card(2, card2_msg)) if on_glasses_card else None,
            )
            exec_card2 = coordinator.await_bridge_proposal_receipt(
                proposal_frame=pending_card2,
                decision=str(card2_ack["action"]),
                timeout_s=glasses_timeout_s,
            )
            receipt_proposals.append(
                {
                    "proposal_id": pending_card2["proposal_id"],
                    "classification": "cfp_repaired_commit",
                    "before_fingerprint": pending_card2["fingerprint"],
                    "after_fingerprint": exec_card2.get("after_fingerprint", pending_card2["fingerprint"]),
                    "frame_hash": snap2_repaired.get("screenshot_hash") or "",
                    "decision_surface": (
                        "synthetic_dry_run"
                        if decision_provenance == "synthetic_dry_run"
                        else str(card2_ack.get("source") or "glasses_dat")
                    ),
                }
            )

            _post_workbench_update(
                origin,
                {
                    "portal_id": 1,
                    "conference_name": PORTAL_CONFERENCES[1],
                    "status": "SUBMITTED",
                    "form_signature": snap2_repaired.get("form_signature") or "",
                    "matched_recipe_id": "recipe_1",
                    "layout_observe_ms": obs_rep_ms,
                    "recall_ms": 0.0,
                    "llm_calls": 1,
                    "proposal_id": pending_card2["proposal_id"],
                    "fingerprint": pending_card2["fingerprint"],
                    "commit_state": "COMMITTED",
                    "screenshot_b64": snap2_repaired.get("screenshot_png_b64") or "",
                },
            )

            # =========================================================================
            # ACT 3: Zero-LLM Recipe Replay Across Portals #2–#10 + Batch Commit Card #3
            # =========================================================================
            staged_batch: list[dict[str, Any]] = []
            portal_metrics: list[dict[str, Any]] = [
                {
                    "portal_id": 1,
                    "conference_name": PORTAL_CONFERENCES[1],
                    "llm_calls": 1,
                    "llm_repair_ms": llm_repair_ms,
                    "recall_ms": 0.0,
                    "layout_observe_ms": obs_rep_ms,
                    "form_signature": snap2_repaired.get("form_signature") or "",
                    "matched_recipe_id": "recipe_1",
                }
            ]
            total_recall_ms = 0.0

            # Connect to worker sockets for Portals #3..#10 (or #2..#10 if primary shows workbench)
            portal_sessions: dict[int, tuple[ControlSocketClient, str]] = {}

            if not show_workbench_in_primary:
                # Portal #2 uses the primary browser session
                agent.send(
                    {
                        "type": "observe.request",
                        "session_id": session_id,
                        "request_id": "act3-p2-pre-nav",
                    }
                )
                snap_pre_p2 = agent.read_until_type("observe.result")
                _propose_and_host_approve(
                    coordinator=coordinator,
                    agent=agent,
                    session_id=session_id,
                    proposal_id="act3-nav-cfp2",
                    snap=snap_pre_p2,
                    operation={"kind": "navigate", "url": f"{origin}/cfp/2"},
                    rationale="Act 3: Open Portal #2 (KubeCon NA)",
                )
                portal_sessions[2] = (agent, session_id)

            # Wait for worker sockets and attach them to decision_bridge.py via POST /sockets
            deadline = time.monotonic() + 10.0
            for pid in range(worker_start_pid, 11):
                w_sock = worker_socks[pid]
                while not w_sock.exists() and time.monotonic() < deadline:
                    time.sleep(0.02)
                if not w_sock.exists():
                    raise RuntimeError(f"Worker Hermes socket for portal #{pid} did not bind in time")
                w_sid = coordinator.attach_socket(
                    sock_path=w_sock,
                    host_token=host_token,
                    start_url=f"{origin}/cfp/{pid}",
                    session_title=f"Portal #{pid} — {PORTAL_CONFERENCES[pid]}",
                )
                w_agent = ControlSocketClient(w_sock, agent_token)
                worker_agents.append(w_agent)
                portal_sessions[pid] = (w_agent, w_sid)

            # Stage Portals #2..#10 with Zero-LLM FormRecipe Recall
            for pid in range(2, 11):
                p_agent, p_sid = portal_sessions[pid]
                conf_name = PORTAL_CONFERENCES[pid]

                snap_p, obs_p_ms = _wait_for_control(p_agent, p_sid, "submit-cfp", f"act3-p{pid}-obs")
                matched_recipe_id = snap_p.get("matched_recipe_id")
                if not matched_recipe_id:
                    raise RuntimeError(
                        f"Portal #{pid} ({conf_name}) did not match stored FormRecipe! "
                        f"form_signature={snap_p.get('form_signature')!r}"
                    )

                t_rec0 = time.perf_counter()
                _propose_and_host_approve(
                    coordinator=coordinator,
                    agent=p_agent,
                    session_id=p_sid,
                    proposal_id=f"act3-p{pid}-replay",
                    snap=snap_p,
                    operation={"kind": "replay_recipe", "recipe_id": matched_recipe_id},
                    rationale=f"Act 3: Zero-LLM FormRecipe replay on Portal #{pid} ({conf_name})",
                )
                recall_ms = (time.perf_counter() - t_rec0) * 1000.0
                total_recall_ms += recall_ms

                snap_p_staged, obs_staged_ms = _wait_for_control(
                    p_agent,
                    p_sid,
                    "submit-cfp",
                    f"act3-p{pid}-staged",
                    include_screenshot=include_screenshots,
                )
                by_id_p = {
                    (e.get("dom_id") or e.get("name") or e.get("id")): e
                    for e in (snap_p_staged.get("elements") or [])
                    if e.get("dom_id") or e.get("name") or e.get("id")
                }

                sub_prop_id = f"act3-p{pid}-submit"
                p_agent.send(
                    {
                        "type": "propose",
                        "session_id": p_sid,
                        "request_id": f"req-{sub_prop_id}",
                        "proposal_id": sub_prop_id,
                        "snapshot_id": snap_p_staged["snapshot_id"],
                        "fingerprint": snap_p_staged["fingerprint"],
                        "generation": snap_p_staged["generation"],
                        "operation": {
                            "kind": "submit",
                            "target_ref": by_id_p["submit-cfp"]["node_ref"],
                        },
                        "rationale": f"Stage submission for Portal #{pid} ({conf_name})",
                    }
                )
                pending_p = coordinator.wait_for_pending(p_sid, sub_prop_id)

                manifest_entry = {
                    "proposal_id": str(pending_p["proposal_id"]),
                    "fingerprint": str(pending_p["fingerprint"]),
                    "generation": int(pending_p["generation"]),
                    "session_id": p_sid,
                    "portal_id": pid,
                }
                staged_batch.append(
                    {
                        **manifest_entry,
                        "agent_client": p_agent,
                        "pending_frame": pending_p,
                        "screenshot_hash": snap_p_staged.get("screenshot_hash") or "",
                        "screenshot_b64": snap_p_staged.get("screenshot_png_b64") or "",
                        "form_signature": snap_p_staged.get("form_signature") or "",
                        "matched_recipe_id": str(matched_recipe_id),
                        "recall_ms": recall_ms,
                        "layout_observe_ms": obs_staged_ms,
                    }
                )
                portal_metrics.append(
                    {
                        "portal_id": pid,
                        "conference_name": conf_name,
                        "llm_calls": 0,
                        "llm_repair_ms": 0.0,
                        "recall_ms": recall_ms,
                        "layout_observe_ms": obs_staged_ms,
                        "form_signature": snap_p_staged.get("form_signature") or "",
                        "matched_recipe_id": str(matched_recipe_id),
                    }
                )

                _post_workbench_update(
                    origin,
                    {
                        "portal_id": pid,
                        "conference_name": conf_name,
                        "status": "FROZEN FOR GLASSES TAP",
                        "form_signature": snap_p_staged.get("form_signature") or "",
                        "matched_recipe_id": str(matched_recipe_id),
                        "layout_observe_ms": obs_staged_ms,
                        "recall_ms": recall_ms,
                        "llm_calls": 0,
                        "proposal_id": str(pending_p["proposal_id"]),
                        "fingerprint": str(pending_p["fingerprint"]),
                        "commit_state": "FROZEN",
                        "screenshot_b64": snap_p_staged.get("screenshot_png_b64") or "",
                        "campaign": {
                            "session_id": session_id,
                            "total_llm_calls": resident_qwen.llm_calls - qwen_calls_before,
                            "total_recall_ms": total_recall_ms,
                            "batch_status": "AWAITING_GLASSES_BATCH_TAP",
                        },
                    },
                )

            if show_workbench_in_primary:
                agent.send(
                    {
                        "type": "observe.request",
                        "session_id": session_id,
                        "request_id": "act3-wb-pre-nav",
                    }
                )
                snap_pre_wb = agent.read_until_type("observe.result")
                _propose_and_host_approve(
                    coordinator=coordinator,
                    agent=agent,
                    session_id=session_id,
                    proposal_id="act3-nav-disco-workbench",
                    snap=snap_pre_wb,
                    operation={"kind": "navigate", "url": f"{origin}/disco-workbench"},
                    rationale="Act 3: Display 10-Viewport Live Disco Workbench before Batch Commit",
                )

            # Build explicit batch_proposals manifest for Glasses Card #3
            batch_proposals_manifest = [
                {
                    "proposal_id": item["proposal_id"],
                    "fingerprint": item["fingerprint"],
                    "generation": item["generation"],
                    "session_id": item["session_id"],
                    "portal_id": item["portal_id"],
                }
                for item in staged_batch
            ]
            compact_manifest_json = (
                "Approve batch submission across 9 conferences (KubeCon NA..QCon SF, 0 LLM calls) "
                if show_workbench_in_primary
                else json.dumps(
                    {"batch_proposals": batch_proposals_manifest},
                    separators=(",", ":"),
                )
            )

            # Glasses Card #3: Batch Commit — Release 9 Recalled Conference Submissions
            card3_msg = coordinator.post_card(
                title="Batch Commit — Release 9 Recalled Conference Submissions",
                body=compact_manifest_json,
                kind="batch_cfp_commit",
                batch_proposals=batch_proposals_manifest,
            )
            card3_ack = coordinator.wait_for_glasses_decision(
                int(card3_msg["id"]),
                timeout_s=glasses_timeout_s,
                on_poll=(lambda _mid: on_glasses_card(3, card3_msg)) if on_glasses_card else None,
            )
            batch_decision = str(card3_ack["action"])

            batch_executed_count = 0
            batch_rejected_count = 0
            for item in staged_batch:
                pid = int(item["portal_id"])
                conf_name = PORTAL_CONFERENCES[pid]
                res_frame = coordinator.await_bridge_proposal_receipt(
                    proposal_frame=item["pending_frame"],
                    decision=batch_decision,
                    timeout_s=glasses_timeout_s,
                )
                if batch_decision == "approve" and res_frame.get("type") == "executed":
                    batch_executed_count += 1
                    receipt_proposals.append(
                        {
                            "proposal_id": item["proposal_id"],
                            "classification": "batch_cfp_commit",
                            "before_fingerprint": item["fingerprint"],
                            "after_fingerprint": res_frame.get("after_fingerprint", item["fingerprint"]),
                            "frame_hash": item["screenshot_hash"],
                            "decision_surface": (
                                "synthetic_dry_run"
                                if decision_provenance == "synthetic_dry_run"
                                else str(card3_ack.get("source") or "glasses_dat")
                            ),
                        }
                    )
                    _post_workbench_update(
                        origin,
                        {
                            "portal_id": pid,
                            "conference_name": conf_name,
                            "status": "SUBMITTED",
                            "commit_state": "COMMITTED",
                        },
                    )
                else:
                    batch_rejected_count += 1
                    _post_workbench_update(
                        origin,
                        {
                            "portal_id": pid,
                            "conference_name": conf_name,
                            "status": "PENDING",
                            "commit_state": "REJECTED",
                        },
                    )

            total_llm_calls = resident_qwen.llm_calls - qwen_calls_before
            final_batch_status = "COMMITTED" if batch_decision == "approve" else "REJECTED"
            _post_workbench_update(
                origin,
                {
                    "campaign": {
                        "session_id": session_id,
                        "total_llm_calls": total_llm_calls,
                        "total_recall_ms": total_recall_ms,
                        "batch_status": final_batch_status,
                        "receipt_proposals": receipt_proposals,
                    }
                },
            )

            if show_workbench_in_primary:
                agent.send(
                    {
                        "type": "observe.request",
                        "session_id": session_id,
                        "request_id": "act3-wb-refresh-pre",
                    }
                )
                snap_wb2 = agent.read_until_type("observe.result")
                _propose_and_host_approve(
                    coordinator=coordinator,
                    agent=agent,
                    session_id=session_id,
                    proposal_id="act3-refresh-disco-workbench",
                    snap=snap_wb2,
                    operation={"kind": "navigate", "url": f"{origin}/disco-workbench"},
                    rationale="Act 3: Show all 10 portals SUBMITTED on Disco Workbench",
                )
                time.sleep(2.0)
                agent.send(
                    {
                        "type": "observe.request",
                        "session_id": session_id,
                        "request_id": "act3-receipt-pre",
                    }
                )
                snap_rec = agent.read_until_type("observe.result")
                _propose_and_host_approve(
                    coordinator=coordinator,
                    agent=agent,
                    session_id=session_id,
                    proposal_id="act3-nav-campaign-receipt",
                    snap=snap_rec,
                    operation={"kind": "navigate", "url": f"{origin}/campaign-receipt"},
                    rationale="Act 3: Display Final Campaign Receipt",
                )

            campaign_elapsed_ms = (time.perf_counter() - t_campaign0) * 1000.0
            all_wire_lines = list(agent.raw_lines) + [json.dumps(coordinator.bridge_decisions())]
            for w_a in worker_agents:
                all_wire_lines.extend(w_a.raw_lines)

            return {
                "ok": True,
                "session_id": session_id,
                "decision_bridge_url": decision_bridge_url,
                "decision_provenance": decision_provenance,
                "total_llm_calls": total_llm_calls,
                "llm_repair_ms": llm_repair_ms,
                "total_recall_ms": total_recall_ms,
                "campaign_elapsed_ms": campaign_elapsed_ms,
                "glasses_cards_count": len(coordinator.cards_posted),
                "glasses_approvals_count": sum(
                    1 for a in coordinator.terminal_acks if a.get("action") == "approve"
                ),
                "glasses_rejections_count": sum(
                    1 for a in coordinator.terminal_acks if a.get("action") == "reject"
                ),
                "ignored_acks_count": len(coordinator.ignored_acks_seen),
                "batch_decision": batch_decision,
                "batch_executed_count": batch_executed_count,
                "batch_rejected_count": batch_rejected_count,
                "batch_proposals": batch_proposals_manifest,
                "card_messages": coordinator.cards_posted,
                "portal_metrics": portal_metrics,
                "receipt_proposals": receipt_proposals,
                "coordinator": coordinator,
                "raw_wire_text": "\n".join(all_wire_lines),
            }
        finally:
            for w_a in worker_agents:
                try:
                    w_a.close()
                except Exception:
                    pass
            for w_proc in worker_procs:
                w_proc.terminate()
                try:
                    w_proc.communicate(timeout=2.0)
                except Exception:
                    w_proc.kill()
                    w_proc.communicate()
            if agent is not None:
                try:
                    agent.close()
                except Exception:
                    pass
            if bridge_proc is not None:
                bridge_proc.terminate()
                try:
                    bridge_proc.communicate(timeout=2.0)
                except Exception:
                    bridge_proc.kill()
                    bridge_proc.communicate()
            if own_qwen and resident_qwen is not None:
                resident_qwen.close()


def main() -> int:
    parser = argparse.ArgumentParser(description="Run 10-Portal Disco Campaign Driver")
    parser.add_argument("--fixture-url", default="http://127.0.0.1:8795")
    parser.add_argument("--control-socket", type=Path, required=True)
    parser.add_argument("--bus-url", default="http://127.0.0.1:19099")
    parser.add_argument("--bus-token", default=os.environ.get("WALKAWAY_BUS_TOKEN", ""))
    parser.add_argument("--decision-bridge-url", default=None)
    parser.add_argument("--vault-path", type=Path, default=Path("/tmp/hermes-vault.json"))
    parser.add_argument("--recall-path", type=Path, default=Path("/tmp/hermes-recall.json"))
    parser.add_argument("--audit-path", type=Path, default=Path("/tmp/hermes-audit.jsonl"))
    args = parser.parse_args()

    if not args.bus_token:
        raise SystemExit("WALKAWAY_BUS_TOKEN or --bus-token is required (fail-closed credential gate)")
    host_token = os.environ.get("HERMES_HOST_TOKEN") or secrets.token_urlsafe(18)
    agent_token = os.environ.get("HERMES_AGENT_TOKEN") or secrets.token_urlsafe(18)

    res = run_10portal_campaign(
        fixture_origin=args.fixture_url,
        sock_path=args.control_socket,
        host_token=host_token,
        agent_token=agent_token,
        bus_base_url=args.bus_url,
        bus_token=args.bus_token,
        vault_path=args.vault_path,
        recall_path=args.recall_path,
        audit_path=args.audit_path,
        decision_bridge_url=args.decision_bridge_url,
    )
    summary = {k: v for k, v in res.items() if k not in {"coordinator", "raw_wire_text"}}
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
