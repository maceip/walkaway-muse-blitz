#!/usr/bin/env python3
"""One-command Live Demo Launcher for Walkaway x Muse Blitz (10-Portal Disco Campaign).

Coordinated WS1/WS2 implementation:
- Acquires an exclusive `fcntl.flock` lock on `/tmp/walkaway-demo-stack.lock` (fail-fast).
- Reuses or starts `services/hermes-host/message_bus.py` on port 19099 via `start-transport.sh`.
- Starts `services/hermes-host/decision_bridge.py --card-mode held` on port 19101.
- Routes all Hermes host supervision and 3-tap glasses cards through `decision_bridge.py` (`19101`).
- Supports `--detach` (`start_new_session=True`) so agent-session shell signals cannot kill the live stack.
- `--auto-approve` runs on an isolated ephemeral bus/bridge for dry rehearsals only (never recorded as physical evidence).
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path("/Users/mac/walkaway-monorepo")
sys.path.insert(0, str(ROOT / "services" / "demo-fixtures"))
sys.path.insert(0, str(ROOT / "services" / "agent-driver"))

import parallel_campaign_driver  # noqa: E402
import reset_demo  # noqa: E402
import signup_fixture  # noqa: E402

STACK_LOCK_PATH = Path("/tmp/walkaway-demo-stack.lock")
DECISION_BRIDGE_SCRIPT = ROOT / "services" / "hermes-host" / "decision_bridge.py"


def _banner(action_line: str, say_line: str = "") -> None:
    line = "=" * 78
    print(f"\n{line}", flush=True)
    print(f"ACTION: {action_line}", flush=True)
    if say_line:
        print(f"\nSAY:\n{say_line}", flush=True)
    print(f"{line}\n", flush=True)


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def _post_ack(bus_url: str, bus_token: str, message_id: int, action: str = "approve") -> None:
    req = urllib.request.Request(
        f"{bus_url.rstrip('/')}/ack",
        data=json.dumps(
            {
                "message_id": int(message_id),
                "action": action,
                "source": "glasses_dat",
            }
        ).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "X-Bus-Token": bus_token,
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=5.0) as resp:
        resp.read()


def _acquire_stack_lock() -> Any:
    lock_fh = open(STACK_LOCK_PATH, "w", encoding="utf-8")
    try:
        fcntl.flock(lock_fh.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError as exc:
        lock_fh.close()
        raise SystemExit(
            f"STACK LOCK BUSY: Another Walkaway demo stack holds {STACK_LOCK_PATH} ({exc}). "
            "Stop the existing stack before starting a second one."
        )
    lock_fh.write(f"{os.getpid()}\n")
    lock_fh.flush()
    return lock_fh


def main() -> int:
    parser = argparse.ArgumentParser(description="Walkaway x Muse Blitz Live Demo Launcher")
    parser.add_argument("--fixture-port", type=int, default=8795)
    parser.add_argument("--bus-url", default="http://127.0.0.1:19099")
    parser.add_argument("--bridge-port", type=int, default=19101)
    parser.add_argument(
        "--bus-token",
        default=os.environ.get("WALKAWAY_BUS_TOKEN", "walkaway-dev-bus-token"),
    )
    parser.add_argument(
        "--detach",
        action="store_true",
        help="Re-exec this launcher in a detached session (start_new_session=True) logging to /tmp/demo-live/run_live_demo.log",
    )
    parser.add_argument(
        "--auto-approve",
        action="store_true",
        help="Dry-run rehearsal only: simulate glasses_dat approve on an isolated ephemeral bus (never physical evidence)",
    )
    parser.add_argument(
        "--exit-on-finish",
        action="store_true",
        help="Exit immediately after printing receipt instead of holding window open",
    )
    args = parser.parse_args()

    if args.detach and os.environ.get("WALKAWAY_DETACHED_CHILD") != "1":
        live_dir = Path("/tmp/demo-live")
        live_dir.mkdir(exist_ok=True)
        log_path = live_dir / "run_live_demo.log"
        child_env = os.environ.copy()
        child_env["WALKAWAY_DETACHED_CHILD"] = "1"
        child_argv = [sys.executable, str(Path(__file__).resolve())] + [
            a for a in sys.argv[1:] if a != "--detach"
        ]
        with open(log_path, "w", encoding="utf-8") as log_fh:
            proc = subprocess.Popen(
                child_argv,
                cwd=str(ROOT),
                env=child_env,
                stdout=log_fh,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        print(f"[detach] Spawned detached live demo pid={proc.pid} log={log_path}", flush=True)
        return 0

    stack_lock_fh = _acquire_stack_lock()

    _banner(
        "WALKAWAY x MUSE BLITZ — PRE-FLIGHT INITIALIZATION",
        "1) Exclusive Stack Lock  2) 19099 Bus + 19101 Decision Bridge  3) 10-Portal Fixture + Resident Qwen3-4B",
    )

    ephemeral_bus_proc: subprocess.Popen[str] | None = None
    transport_script = ROOT / "services" / "hermes-host" / "start-transport.sh"
    if args.auto_approve and args.bus_url == "http://127.0.0.1:19099":
        eph_port = _free_port()
        args.bus_url = f"http://127.0.0.1:{eph_port}"
        args.bridge_port = _free_port()
        bus_script = ROOT / "services" / "hermes-host" / "message_bus.py"
        env_bus = os.environ.copy()
        env_bus["WALKAWAY_BUS_TOKEN"] = args.bus_token
        env_bus["WALKAWAY_BUS_STATE"] = str(
            Path(tempfile.gettempdir()) / f"walkaway-dryrun-{eph_port}.json"
        )
        ephemeral_bus_proc = subprocess.Popen(
            [sys.executable, str(bus_script), "--host", "127.0.0.1", "--port", str(eph_port)],
            env=env_bus,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            text=True,
            start_new_session=True,
        )
        time.sleep(0.4)
    elif not args.auto_approve and transport_script.exists():
        subprocess.run([str(transport_script)], check=False)
    else:
        try:
            with urllib.request.urlopen(f"{args.bus_url}/health", timeout=2.0) as r:
                r.read()
        except Exception:
            bus_script = ROOT / "services" / "hermes-host" / "message_bus.py"
            env_bus = os.environ.copy()
            env_bus["WALKAWAY_BUS_TOKEN"] = args.bus_token
            subprocess.Popen(
                [sys.executable, str(bus_script), "--host", "127.0.0.1", "--port", "19099"],
                env=env_bus,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            time.sleep(0.6)

    demo_dir = Path(tempfile.mkdtemp(prefix="walkaway-live-demo-"))
    db_path = demo_dir / "live_demo.sqlite3"
    vault_path = demo_dir / "vault.json"
    recall_path = demo_dir / "recall.json"
    audit_path = demo_dir / "audit.jsonl"
    sock_path = demo_dir / "muse_blitz.sock"

    try:
        server = signup_fixture.start_server("127.0.0.1", args.fixture_port, db_path)
    except OSError:
        server = signup_fixture.start_server("127.0.0.1", 0, db_path)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    origin = f"http://127.0.0.1:{server.server_port}"

    reset_info = reset_demo.reset_demo_state(
        fixture_url=origin,
        recall_path=recall_path,
        vault_path=vault_path,
        db_path=db_path,
    )
    print(f"[PRE-FLIGHT] Fixture ready at {origin} (reset in {reset_info['reset_ms']:.1f}ms)", flush=True)

    print("[PRE-FLIGHT] Warming resident Qwen3-4B-Instruct-2507-4bit on Apple Silicon GPU...", flush=True)
    qwen = parallel_campaign_driver.get_resident_qwen_driver()
    print(f"[PRE-FLIGHT] Resident Qwen3-4B READY (loaded in {qwen.load_ms:.0f}ms)", flush=True)

    host_token = "host-secret-token-live-demo"
    agent_token = "agent-secret-token-live-demo"
    env = os.environ.copy()
    env["HERMES_HOST_TOKEN"] = host_token
    env["HERMES_AGENT_TOKEN"] = agent_token
    env["HERMES_VAULT_PATH"] = str(vault_path)
    env["HERMES_RECALL_PATH"] = str(recall_path)
    env["HERMES_AUDIT_PATH"] = str(audit_path)

    # Start decision_bridge.py (--card-mode held) on bridge_port (default 19101)
    bridge_port = args.bridge_port
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("127.0.0.1", bridge_port))
    except OSError:
        bridge_port = _free_port()
    decision_bridge_url = f"http://127.0.0.1:{bridge_port}"
    bridge_env = os.environ.copy()
    bridge_env["WALKAWAY_BUS_TOKEN"] = args.bus_token
    bridge_env["HERMES_HOST_TOKEN"] = host_token
    bridge_proc = subprocess.Popen(
        [
            sys.executable,
            str(DECISION_BRIDGE_SCRIPT),
            "--bus-url",
            args.bus_url,
            "--http-host",
            "127.0.0.1",
            "--http-port",
            str(bridge_port),
            "--card-mode",
            "held",
        ],
        env=bridge_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    for _ in range(40):
        try:
            with urllib.request.urlopen(f"{decision_bridge_url}/health", timeout=1.0) as r:
                if r.status == 200:
                    break
        except Exception:
            time.sleep(0.05)

    browser_proc = subprocess.Popen(
        [
            str(parallel_campaign_driver.HERMES_BIN),
            "--control-socket",
            str(sock_path),
            "--supervise-commits-only",
            "--xray",
        ],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )

    deadline = time.monotonic() + 8.0
    while not sock_path.exists() and time.monotonic() < deadline:
        time.sleep(0.05)

    current_waiting_mid: list[int] = []

    def _stdin_enter_fallback() -> None:
        while True:
            try:
                line = sys.stdin.readline()
                if not line:
                    break
                if current_waiting_mid:
                    mid = current_waiting_mid[-1]
                    print(f"[HOTKEY] Enter pressed -> sending Approve for Card #{mid}", flush=True)
                    _post_ack(args.bus_url, args.bus_token, mid, "approve")
            except Exception:
                break

    if not args.auto_approve and sys.stdin.isatty():
        threading.Thread(target=_stdin_enter_fallback, daemon=True).start()

    def on_card(card_idx: int, msg: dict[str, Any]) -> None:
        mid = int(msg["id"])
        current_waiting_mid.append(mid)
        title = msg.get("title", "")
        if card_idx == 1:
            _banner(
                f"TAP APPROVE ON GLASSES CARD #1 (bus #{mid}) — [{title}]",
                "Tap one creates my speaker account and saves my passkey to the vault.",
            )
        elif card_idx == 2:
            _banner(
                f"TAP APPROVE ON GLASSES CARD #2 (bus #{mid}) — [{title}]",
                "The AI fixes a form error on the first conference, and tap two submits it and saves the form template.",
            )
        elif card_idx == 3:
            _banner(
                f"TAP APPROVE ON GLASSES CARD #3 (bus #{mid}) — [{title}]",
                "The other 9 conferences reuse that template instantly with zero AI calls, and tap three submits all 9 at once.",
            )
        if args.auto_approve:
            def _delayed_ack() -> None:
                time.sleep(1.2)
                _post_ack(args.bus_url, args.bus_token, mid, "approve")
            threading.Thread(target=_delayed_ack, daemon=True).start()

    _banner(
        "ACTION! MUSE BLITZ WINDOW LIVE — CAMPAIGN RUNNING",
        f"Decision Bridge: {decision_bridge_url}  |  Disco Workbench: {origin}/disco-workbench  |  Receipt: {origin}/campaign-receipt",
    )

    try:
        res = parallel_campaign_driver.run_10portal_campaign(
            fixture_origin=origin,
            sock_path=sock_path,
            host_token=host_token,
            agent_token=agent_token,
            bus_base_url=args.bus_url,
            bus_token=args.bus_token,
            vault_path=vault_path,
            recall_path=recall_path,
            audit_path=audit_path,
            resident_qwen=qwen,
            on_glasses_card=on_card,
            include_screenshots=True,
            glasses_timeout_s=900.0,
            show_workbench_in_primary=True,
            decision_bridge_url=decision_bridge_url,
        )
        _banner(
            "CAMPAIGN COMPLETE — 10/10 CONFERENCES SUBMITTED!",
            (
                f"Total LLM Calls: {res['total_llm_calls']}  |  "
                f"Glasses Taps: {res['glasses_approvals_count']}  |  "
                f"9-Portal Recall Time: {res['total_recall_ms']:.1f}ms  |  "
                f"Receipt URL: {origin}/campaign-receipt"
            ),
        )
        if not args.exit_on_finish:
            print("Muse Blitz window is displaying /campaign-receipt. Press Ctrl+C when finished.", flush=True)
            while browser_proc.poll() is None:
                time.sleep(0.5)
    finally:
        browser_proc.terminate()
        try:
            browser_proc.communicate(timeout=3.0)
        except Exception:
            browser_proc.kill()
        bridge_proc.terminate()
        try:
            bridge_proc.communicate(timeout=3.0)
        except Exception:
            bridge_proc.kill()
        if ephemeral_bus_proc is not None:
            ephemeral_bus_proc.terminate()
        qwen.close()
        server.shutdown()
        server.server_close()
        try:
            fcntl.flock(stack_lock_fh.fileno(), fcntl.LOCK_UN)
            stack_lock_fh.close()
        except Exception:
            pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
