#!/usr/bin/env python3
"""Detached demo spawner: starts Hermes + campaign in a new session and exits.
Fixture on 8795 is reused if healthy. Logs: /tmp/demo-live/run3.log"""
import os, subprocess, sys, time, urllib.request, json
from pathlib import Path

ROOT = Path("/Users/mac/walkaway-monorepo")
LIVE = Path("/tmp/demo-live"); LIVE.mkdir(exist_ok=True)
SOCK = LIVE / "hermes.sock"
if SOCK.exists(): SOCK.unlink()
DB = LIVE / "demo.sqlite3"

def healthy(url):
    try:
        with urllib.request.urlopen(url, timeout=2) as r: return r.status == 200
    except Exception: return False

def wait(fn, what, timeout=25):
    t0 = time.monotonic()
    while time.monotonic() - t0 < timeout:
        if fn(): return
        time.sleep(0.25)
    raise SystemExit(f"timeout: {what}")

if not healthy("http://127.0.0.1:8795/signup"):
    subprocess.Popen([sys.executable, str(ROOT/"services/demo-fixtures/signup_fixture.py"),
                      "--port", "8795", "--db", str(DB)], cwd=ROOT,
                     stdout=open(LIVE/"fixture.log","a"), stderr=subprocess.STDOUT,
                     start_new_session=True)
    wait(lambda: healthy("http://127.0.0.1:8795/signup"), "fixture")
print("[spawn] fixture healthy", flush=True)

subprocess.run([sys.executable, str(ROOT/"services/agent-driver/reset_demo.py"),
                "--reset-demo", "--fixture-url", "http://127.0.0.1:8795",
                "--db-path", str(DB)], cwd=ROOT, check=True,
               stdout=open(LIVE/"reset.log","w"), stderr=subprocess.STDOUT)
print("[spawn] reset done", flush=True)

env = os.environ.copy()
env.update({"HERMES_HOST_TOKEN": "host-secret-token-campaign-998877",
            "HERMES_AGENT_TOKEN": "agent-secret-token-campaign-112233",
            "HERMES_VAULT_PATH": "/tmp/hermes-vault.json",
            "HERMES_RECALL_PATH": "/tmp/hermes-recall.json",
            "HERMES_AUDIT_PATH": "/tmp/hermes-audit.jsonl"})
hermes = subprocess.Popen(
    ["/Users/mac/hermes-blitz-browser/target/release/hermes-blitz-browser",
     "--control-socket", str(SOCK)], cwd=ROOT, env=env,
    stdout=open(LIVE/"hermes.log","a"), stderr=subprocess.STDOUT, start_new_session=True)
wait(lambda: SOCK.exists(), "hermes socket")
time.sleep(1.0)
print(f"[spawn] hermes pid={hermes.pid}", flush=True)

runner = LIVE / "run_campaign3.py"
runner.write_text('''
import sys, json
from pathlib import Path
sys.path.insert(0, "/Users/mac/walkaway-monorepo/services/agent-driver")
import parallel_campaign_driver as drv
announced = set()
def on_card(n, msg):
    if n not in announced:
        announced.add(n)
        print(f">>> CARD {n} ON GLASSES (bus #{msg.get('id')}) — TAP APPROVE <<<", flush=True)
try:
    res = drv.run_10portal_campaign(
        fixture_origin="http://127.0.0.1:8795",
        sock_path=Path("/tmp/demo-live/hermes.sock"),
        host_token="host-secret-token-campaign-998877",
        agent_token="agent-secret-token-campaign-112233",
        bus_base_url="http://127.0.0.1:19099",
        bus_token="walkaway-dev-bus-token",
        vault_path=Path("/tmp/hermes-vault.json"),
        recall_path=Path("/tmp/hermes-recall.json"),
        audit_path=Path("/tmp/hermes-audit.jsonl"),
        on_glasses_card=on_card, glasses_timeout_s=900.0)
    summary = {k: v for k, v in res.items() if k not in {"coordinator", "raw_wire_text"}}
    print("CAMPAIGN RESULT:", json.dumps(summary, indent=2, default=str), flush=True)
    print("CAMPAIGN EXIT 0", flush=True)
except Exception as e:
    import traceback; traceback.print_exc()
    print(f"CAMPAIGN FAILED: {type(e).__name__}: {e}", flush=True)
''')
camp = subprocess.Popen([sys.executable, str(runner)], cwd=ROOT,
                        stdout=open(LIVE/"run3.log","w"), stderr=subprocess.STDOUT,
                        start_new_session=True)
print(f"[spawn] campaign pid={camp.pid} log=/tmp/demo-live/run3.log", flush=True)
print("[spawn] spawner exiting; demo stack detached", flush=True)
