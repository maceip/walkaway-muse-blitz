#!/usr/bin/env python3
"""One-command Live Demo Launcher for Walkaway x Muse Blitz (10-Portal Disco Campaign).

Usage (Physical Ray-Ban Display Glasses on Bus 19099):
    python3 /Users/mac/walkaway-monorepo/services/agent-driver/run_live_demo.py

Usage (Dry-Run / Self-Approve Rehearsal without Glasses):
    python3 /Users/mac/walkaway-monorepo/services/agent-driver/run_live_demo.py --auto-approve --exit-on-finish
"""

from __future__ import annotations

import argparse
import json
import os
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


def _banner(action_line: str, say_line: str = "") -> None:
    line = "=" * 78
    print(f"\n{line}", flush=True)
    print(f"ACTION: {action_line}", flush=True)
    if say_line:
        print(f"\nSAY:\n{say_line}", flush=True)
    print(f"{line}\n", flush=True)


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


def main() -> int:
    parser = argparse.ArgumentParser(description="Walkaway x Muse Blitz Live Demo Launcher")
    parser.add_argument("--fixture-port", type=int, default=8795)
    parser.add_argument("--bus-url", default="http://127.0.0.1:19099")
    parser.add_argument(
        "--bus-token",
        default=os.environ.get("WALKAWAY_BUS_TOKEN", "walkaway-dev-bus-token"),
    )
    parser.add_argument(
        "--auto-approve",
        action="store_true",
        help="Automatically simulate glasses_dat approve after 1.5s per card (for dry rehearsal)",
    )
    parser.add_argument(
        "--exit-on-finish",
        action="store_true",
        help="Exit immediately after printing receipt instead of holding window open",
    )
    args = parser.parse_args()

    _banner(
        "WALKAWAY x MUSE BLITZ — PRE-FLIGHT INITIALIZATION",
        "1) Verifying 19099 Bus  2) Resetting 10-Portal Fixture  3) Pre-Warming Resident Qwen3-4B",
    )

    # 1. Verify or start message bus on 19099 (or isolated ephemeral bus for --auto-approve dry run)
    ephemeral_bus_proc: subprocess.Popen[str] | None = None
    transport_script = ROOT / "services" / "hermes-host" / "start-transport.sh"
    if args.auto_approve and args.bus_url == "http://127.0.0.1:19099":
        import socket as _sock
        with _sock.socket(_sock.AF_INET, _sock.SOCK_STREAM) as s:
            s.bind(("127.0.0.1", 0))
            eph_port = s.getsockname()[1]
        args.bus_url = f"http://127.0.0.1:{eph_port}"
        bus_script = ROOT / "services" / "hermes-host" / "message_bus.py"
        env_bus = os.environ.copy()
        env_bus["WALKAWAY_BUS_TOKEN"] = args.bus_token
        env_bus["WALKAWAY_BUS_STATE"] = str(Path(tempfile.gettempdir()) / f"walkaway-dryrun-{eph_port}.json")
        ephemeral_bus_proc = subprocess.Popen(
            [sys.executable, str(bus_script), "--host", "127.0.0.1", "--port", str(eph_port)],
            env=env_bus,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            text=True,
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
            )
            time.sleep(0.6)

    # 2. Start fixture server and run deterministic <2s reset
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

    # 3. Pre-warm Resident Qwen3-4B before launching the browser window
    print("[PRE-FLIGHT] Warming resident Qwen3-4B-Instruct-2507-4bit on Apple Silicon GPU...", flush=True)
    qwen = parallel_campaign_driver.get_resident_qwen_driver()
    print(f"[PRE-FLIGHT] Resident Qwen3-4B READY (loaded in {qwen.load_ms:.0f}ms)", flush=True)

    # 4. Launch primary Muse Blitz headed WGPU window with Semantic X-Ray HUD & Commit-Only Gating
    host_token = "host-secret-token-live-demo"
    agent_token = "agent-secret-token-live-demo"
    env = os.environ.copy()
    env["HERMES_HOST_TOKEN"] = host_token
    env["HERMES_AGENT_TOKEN"] = agent_token
    env["HERMES_VAULT_PATH"] = str(vault_path)
    env["HERMES_RECALL_PATH"] = str(recall_path)
    env["HERMES_AUDIT_PATH"] = str(audit_path)

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
                f"TAP APPROVE ON GLASSES CARD #1 (or press ENTER here) — [{title}]",
                "Tap 1 on my Ray-Ban Display glasses seals my speaker account and P-256 passkey inside the Blind Vault — the LLM and page JS never see the secret.",
            )
        elif card_idx == 2:
            _banner(
                f"TAP APPROVE ON GLASSES CARD #2 (or press ENTER here) — [{title}]",
                "Portal 1 threw a CFP validation error. Local Qwen3-4B repaired the abstract in one second. Tap 2 commits Portal 1 and compiles a reusable FormRecipe.",
            )
        elif card_idx == 3:
            _banner(
                f"TAP APPROVE ON GLASSES CARD #3 (or press ENTER here) — [{title}]",
                "Portals 2 through 10 matched that FormSignature in under 5 milliseconds with zero LLM calls. Tap 3 releases all 9 conference submissions at once.",
            )
        if args.auto_approve:
            def _delayed_ack() -> None:
                time.sleep(1.2)
                _post_ack(args.bus_url, args.bus_token, mid, "approve")
            threading.Thread(target=_delayed_ack, daemon=True).start()

    _banner(
        "ACTION! MUSE BLITZ WINDOW LIVE — CAMPAIGN RUNNING",
        f"Disco Workbench: {origin}/disco-workbench  |  Receipt: {origin}/campaign-receipt",
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
            glasses_timeout_s=600.0,
            show_workbench_in_primary=True,
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
            print("Muse Blitz window is displaying /campaign-receipt. Press Ctrl+C when recording finishes.", flush=True)
            while browser_proc.poll() is None:
                time.sleep(0.5)
    finally:
        browser_proc.terminate()
        try:
            browser_proc.communicate(timeout=3.0)
        except Exception:
            browser_proc.kill()
        if ephemeral_bus_proc is not None:
            ephemeral_bus_proc.terminate()
        qwen.close()
        server.shutdown()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
