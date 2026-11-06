#!/usr/bin/env python3
"""One-command Walkaway demo launcher: fixture + Hermes + reset + campaign.

Prereqs already running: message bus on 19099, Android bridge connected,
DAT REGISTERED, glasses worn. User script: Approve 3 cards on the glasses.
"""
import os, subprocess, sys, time, urllib.request
from pathlib import Path

ROOT = Path("/Users/mac/walkaway-monorepo")
LIVE = Path("/tmp/demo-live"); LIVE.mkdir(exist_ok=True)
SOCK = LIVE / "hermes.sock"
if SOCK.exists(): SOCK.unlink()
DB = LIVE / "demo.sqlite3"

def wait_http(url, timeout=20):
    t0 = time.monotonic()
    while time.monotonic() - t0 < timeout:
        try:
            with urllib.request.urlopen(url, timeout=2) as r:
                if r.status == 200: return
        except Exception: pass
        time.sleep(0.25)
    raise SystemExit(f"timeout: {url}")

procs = []
try:
    print("[demo] fixture -> 8795", flush=True)
    procs.append(subprocess.Popen(
        [sys.executable, str(ROOT/"services/demo-fixtures/signup_fixture.py"),
         "--port", "8795", "--db", str(DB)],
        cwd=ROOT, stdout=open(LIVE/"fixture.log","a"), stderr=subprocess.STDOUT))
    wait_http("http://127.0.0.1:8795/signup")

    print("[demo] reset state", flush=True)
    subprocess.run([sys.executable, str(ROOT/"services/agent-driver/reset_demo.py"),
                    "--reset-demo", "--fixture-url", "http://127.0.0.1:8795",
                    "--db-path", str(DB)], cwd=ROOT, check=True,
                   stdout=open(LIVE/"reset.log","w"), stderr=subprocess.STDOUT)

    env = os.environ.copy()
    env.update({
        "HERMES_HOST_TOKEN": "host-secret-token-campaign-998877",
        "HERMES_AGENT_TOKEN": "agent-secret-token-campaign-112233",
        "HERMES_VAULT_PATH": "/tmp/hermes-vault.json",
        "HERMES_RECALL_PATH": "/tmp/hermes-recall.json",
        "HERMES_AUDIT_PATH": "/tmp/hermes-audit.jsonl",
    })
    print("[demo] hermes -> socket", flush=True)
    procs.append(subprocess.Popen(
        ["/Users/mac/walkaway-monorepo/apps/hermes-browser/target/release/hermes-blitz-browser",
         "--control-socket", str(SOCK)],
        cwd=ROOT, env=env, stdout=open(LIVE/"hermes.log","a"), stderr=subprocess.STDOUT))
    t0 = time.monotonic()
    while not SOCK.exists():
        if time.monotonic() - t0 > 20: raise SystemExit("hermes socket timeout")
        time.sleep(0.1)
    time.sleep(1.0)

    print("[demo] bus check", flush=True)
    wait_http("http://127.0.0.1:19099/health", timeout=5)

    print("[demo] CAMPAIGN LIVE — Approve 3 cards on the glasses", flush=True)
    sys.path.insert(0, str(ROOT / "services" / "agent-driver"))
    import parallel_campaign_driver as drv
    announced = set()
    def on_card(n, msg):
        if n not in announced:
            announced.add(n)
            print(f"[demo] >>> CARD {n} ON GLASSES (bus #{msg.get('id')}) — TAP APPROVE <<<", flush=True)
    try:
        res = drv.run_10portal_campaign(
            fixture_origin="http://127.0.0.1:8795",
            sock_path=SOCK,
            host_token="host-secret-token-campaign-998877",
            agent_token="agent-secret-token-campaign-112233",
            bus_base_url="http://127.0.0.1:19099",
            bus_token="walkaway-dev-bus-token",
            vault_path=Path("/tmp/hermes-vault.json"),
            recall_path=Path("/tmp/hermes-recall.json"),
            audit_path=Path("/tmp/hermes-audit.jsonl"),
            on_glasses_card=on_card,
            glasses_timeout_s=600.0,
        )
        summary = {k: v for k, v in res.items() if k not in {"coordinator", "raw_wire_text"}}
        print("[demo] CAMPAIGN RESULT:", flush=True)
        import json; print(json.dumps(summary, indent=2, default=str), flush=True)
        rc = 0
    except Exception as e:
        print(f"[demo] CAMPAIGN FAILED: {type(e).__name__}: {e}", flush=True)
        rc = 1
    print(f"[demo] campaign exit={rc}", flush=True)
    raise SystemExit(rc)
finally:
    for p in reversed(procs):
        p.terminate()
    for p in reversed(procs):
        try: p.wait(timeout=5)
        except Exception: p.kill()
