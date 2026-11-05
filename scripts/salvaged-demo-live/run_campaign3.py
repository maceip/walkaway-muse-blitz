
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
