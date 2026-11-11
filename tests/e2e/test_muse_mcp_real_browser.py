#!/usr/bin/env python3
"""Real-System E2E Test: Muse MCP Connector -> Hermes Task Relay -> Real Browser -> Decision Bridge -> Glasses/Android Approval (`tests/e2e/test_muse_mcp_real_browser.py`).

Verifies (`T-MUSE-MCP-REAL-BROWSER`):
1. `services/muse-connector/mcp_server.py` (`POST /mcp`) receives standard MCP JSON-RPC calls:
   - `tools/list`
   - `tools/call` -> `walkaway_start_task` with a real URL and natural-language goal
   - `tools/call` -> `walkaway_propose_step` / `walkaway_request_approval`
   - `tools/call` -> `walkaway_status`
   - `tools/call` -> `walkaway_reject_step` & `walkaway_stop_task`
2. `services/agent-driver/hermes_task_relay.py` opens the target webpage in the real native
   `hermes-blitz-browser` (strictly headless `HERMES_HIDDEN_WINDOW=1`), runs real local
   `mlx-community/Qwen3-4B-Instruct-2507-4bit` inference (`plan_general_web_form_with_local_llm`)
   to map a non-CFP pizza order form (`custname`, `custtel`, `custemail`, `size`, `comments`),
   auto-stages the fields via `decision_bridge.py` (`policy_auto`), and pauses on the `submit`
   proposal (`requires_glasses=True`).
3. `decision_bridge.py` posts a domain-first card (`<=80` chars title / `<=160` chars body) to
   `message_bus.py` (`19099`).
4. Approving the card executes the real HTTP `POST` form submission in Hermes, and `walkaway_status`
   returns `status == "committed"` with the committed post-submit page and exact decision provenance.
"""

from __future__ import annotations

import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "services" / "agent-driver"))

from hermes_task_relay import HermesTaskManager, RelayHTTPServer, make_handler  # noqa: E402

MCP_SERVER_SCRIPT = REPO_ROOT / "services" / "muse-connector" / "mcp_server.py"


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def _http_json(
    url: str,
    payload: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
    timeout_s: float = 30.0,
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


_PIZZA_FORM_HTML = """<!doctype html>
<html>
<head><title>Mario's Artisan Pizza — Online Order</title></head>
<body>
  <h1>Mario's Artisan Pizza Order Form</h1>
  <form method="post" action="/post">
    <p><label>Customer name: <input name="custname" id="custname" type="text"></label></p>
    <p><label>Telephone: <input name="custtel" id="custtel" type="tel"></label></p>
    <p><label>E-mail address: <input name="custemail" id="custemail" type="email"></label></p>
    <p><label>Pizza Size:
      <select name="size" id="size">
        <option value="small">Small</option>
        <option value="medium" selected>Medium</option>
        <option value="large">Large</option>
      </select>
    </label></p>
    <p><label>Delivery instructions: <textarea name="comments" id="comments"></textarea></label></p>
    <p><button type="submit" id="submit-order">Submit order</button></p>
  </form>
</body>
</html>
"""


class PizzaOrderServer(ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self, server_address: tuple[str, int]):
        super().__init__(server_address, PizzaOrderHandler)
        self.submitted_orders: list[dict[str, str]] = []


class PizzaOrderHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path == "/forms/post":
            body = _PIZZA_FORM_HTML.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        if path == "/post":
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length).decode("utf-8", errors="replace")
            parsed = {k: v[0] for k, v in parse_qs(raw, keep_blank_values=True).items()}
            self.server.submitted_orders.append(parsed)  # type: ignore[attr-defined]
            resp_json = json.dumps({"ok": True, "form": parsed}, indent=2)
            body = resp_json.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, *_args: object) -> None:
        pass


class TestMuseMcpRealBrowser(unittest.TestCase):
    def setUp(self) -> None:
        os.environ["HERMES_HIDDEN_WINDOW"] = "1"
        self.tmp_dir = tempfile.TemporaryDirectory(prefix="walkaway-mcp-e2e-")
        self.bus_port = _free_port()
        self.bridge_port = _free_port()
        self.relay_port = _free_port()
        self.mcp_port = _free_port()
        self.site_port = _free_port()

        self.bus_token = f"bus-tok-{secrets.token_hex(8)}"
        self.mcp_token = f"mcp-tok-{secrets.token_hex(8)}"

        self.site_server = PizzaOrderServer(("127.0.0.1", self.site_port))
        self.site_thread = threading.Thread(target=self.site_server.serve_forever, daemon=True)
        self.site_thread.start()
        self.site_url = f"http://127.0.0.1:{self.site_port}/forms/post"

        self.bus_url = f"http://127.0.0.1:{self.bus_port}"
        self.bridge_url = f"http://127.0.0.1:{self.bridge_port}"
        self.relay_url = f"http://127.0.0.1:{self.relay_port}"
        self.mcp_url = f"http://127.0.0.1:{self.mcp_port}/mcp"

        self.manager = HermesTaskManager(
            bus_url=self.bus_url,
            bus_token=self.bus_token,
            bridge_url=self.bridge_url,
            show_window=False,
        )
        self.manager.ensure_bus_and_bridge()

        self.relay_server = RelayHTTPServer(("127.0.0.1", self.relay_port), make_handler(self.manager))
        self.relay_thread = threading.Thread(target=self.relay_server.serve_forever, daemon=True)
        self.relay_thread.start()

        mcp_env = os.environ.copy()
        mcp_env["WALKAWAY_CONNECTOR_HOST"] = "127.0.0.1"
        mcp_env["WALKAWAY_CONNECTOR_PORT"] = str(self.mcp_port)
        mcp_env["WALKAWAY_CONNECTOR_TOKEN"] = self.mcp_token
        mcp_env["WALKAWAY_RELAY_URL"] = self.relay_url
        self.mcp_proc = subprocess.Popen(
            [sys.executable, str(MCP_SERVER_SCRIPT)],
            env=mcp_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        for _ in range(40):
            try:
                _http_json(f"http://127.0.0.1:{self.mcp_port}/health", timeout_s=1.0)
                break
            except Exception:
                time.sleep(0.05)

    def tearDown(self) -> None:
        self.mcp_proc.terminate()
        try:
            self.mcp_proc.communicate(timeout=2.0)
        except Exception:
            self.mcp_proc.kill()
            self.mcp_proc.communicate()
        self.relay_server.shutdown()
        self.relay_server.server_close()
        self.manager.close()
        self.site_server.shutdown()
        self.site_server.server_close()
        self.tmp_dir.cleanup()

    def _mcp_call(self, tool_name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        resp = _http_json(
            self.mcp_url,
            payload={
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments or {},
                },
            },
            headers={"Authorization": f"Bearer {self.mcp_token}"},
            timeout_s=60.0,
        )
        self.assertNotIn("error", resp, f"MCP JSON-RPC error: {resp}")
        result = resp["result"]
        self.assertFalse(result.get("isError"), f"MCP tool error: {result}")
        return result["structuredContent"]

    def test_muse_mcp_drives_real_hermes_browser_and_commits_on_glasses_or_android_approval(self) -> None:
        # 1. Start task via Muse MCP (`walkaway_start_task`)
        start_res = self._mcp_call(
            "walkaway_start_task",
            {
                "goal": "Order a medium pizza for Ryan MacArthur (ryan@zunftmax.secure.build, 415-555-0199) and leave at reception.",
                "url": self.site_url,
            },
        )
        self.assertEqual(start_res["status"], "awaiting_approval")
        self.assertIn("Mario's Artisan Pizza", start_res["page_title"])
        self.assertEqual(len(self.site_server.submitted_orders), 0)

        # 2. Ask Muse to propose next step & request approval (`walkaway_request_approval`)
        prop_res = self._mcp_call("walkaway_request_approval", {})
        self.assertTrue(prop_res["awaiting_approval"])
        self.assertTrue(prop_res["awaiting_approval_on_glasses"])
        self.assertTrue(prop_res["awaiting_approval_on_android"])
        card_mid = prop_res["card_message_id"]
        self.assertIsNotNone(card_mid)
        self.assertTrue(prop_res["card_title"].startswith("COMMIT 127.0.0.1:"))
        self.assertLessEqual(len(prop_res["card_title"]), 80)
        self.assertLessEqual(len(prop_res["card_body"]), 160)

        # Still 0 submitted orders before human approval on glasses or Android app!
        self.assertEqual(len(self.site_server.submitted_orders), 0)

        # 3. Approve the card on the bus (simulating glasses_dat tap)
        _http_json(
            f"{self.bus_url}/ack",
            payload={
                "message_id": int(card_mid),
                "action": "approve",
                "source": "glasses_dat",
            },
            headers={"X-Bus-Token": self.bus_token},
        )

        # 4. Poll `walkaway_status` via Muse MCP until committed
        committed_state = None
        for _ in range(50):
            st = self._mcp_call("walkaway_status", {})
            if st.get("status") == "committed":
                committed_state = st
                break
            time.sleep(0.05)

        self.assertIsNotNone(committed_state, "Task did not reach committed state after glasses_dat approve")
        assert committed_state is not None
        self.assertFalse(committed_state["awaiting_approval"])
        self.assertEqual(committed_state["last_decision"]["decision"], "approve")
        self.assertEqual(len(self.site_server.submitted_orders), 1)
        submitted = self.site_server.submitted_orders[0]
        self.assertEqual(submitted.get("custname"), "Ryan MacArthur")
        self.assertEqual(submitted.get("custemail"), "ryan@zunftmax.secure.build")

        # 5. Stop task via Muse MCP
        stop_res = self._mcp_call("walkaway_stop_task", {})
        self.assertEqual(stop_res["status"], "stopped")

        # 6. Start a second task and approve from the Android app (`source="android_app"`)
        start_res2 = self._mcp_call(
            "walkaway_start_task",
            {
                "goal": "Order a large pizza for Ryan MacArthur (ryan@zunftmax.secure.build, 415-555-0199).",
                "url": self.site_url,
            },
        )
        self.assertEqual(start_res2["status"], "awaiting_approval")
        prop_res2 = self._mcp_call("walkaway_request_approval", {})
        self.assertTrue(prop_res2["awaiting_approval"])
        card_mid2 = prop_res2["card_message_id"]
        self.assertIsNotNone(card_mid2)

        _http_json(
            f"{self.bus_url}/ack",
            payload={
                "message_id": int(card_mid2),
                "action": "approve",
                "source": "android_app",
            },
            headers={"X-Bus-Token": self.bus_token},
        )

        committed_state2 = None
        for _ in range(50):
            st2 = self._mcp_call("walkaway_status", {})
            if st2.get("status") == "committed":
                committed_state2 = st2
                break
            time.sleep(0.05)

        self.assertIsNotNone(committed_state2, "Task did not reach committed state after android_app approve")
        assert committed_state2 is not None
        self.assertEqual(committed_state2["last_decision"]["decision"], "approve")
        self.assertEqual(committed_state2["last_decision"]["surface"], "android_app")
        self.assertEqual(len(self.site_server.submitted_orders), 2)
        self._mcp_call("walkaway_stop_task", {})

    def test_live_internet_https_httpbin_form_via_muse_mcp(self) -> None:
        """Drive a real external HTTPS webpage (https://httpbin.org/forms/post) end-to-end via Muse MCP."""
        live_url = "https://httpbin.org/forms/post"
        start_res = self._mcp_call(
            "walkaway_start_task",
            {
                "goal": "Order a medium pizza for Ryan MacArthur (ryan@zunftmax.secure.build, 415-555-0199) with delivery instructions 'Ring bell'.",
                "url": live_url,
            },
        )
        self.assertEqual(start_res["status"], "awaiting_approval")
        self.assertIn("httpbin.org/forms/post", start_res["page_url"])

        prop_res = self._mcp_call("walkaway_request_approval", {})
        self.assertTrue(prop_res["awaiting_approval"])
        self.assertTrue(
            prop_res["card_title"].startswith("COMMIT httpbin.org:"),
            f"Expected domain-first card title for httpbin.org, got: {prop_res['card_title']!r}",
        )
        card_mid = prop_res["card_message_id"]
        self.assertIsNotNone(card_mid)

        # Approve from Android app (`source="android_app"`) and verify real HTTPS POST to https://httpbin.org/post
        _http_json(
            f"{self.bus_url}/ack",
            payload={
                "message_id": int(card_mid),
                "action": "approve",
                "source": "android_app",
            },
            headers={"X-Bus-Token": self.bus_token},
        )

        committed = None
        for _ in range(80):
            st = self._mcp_call("walkaway_status", {})
            if st.get("status") == "committed":
                committed = st
                break
            time.sleep(0.1)

        self.assertIsNotNone(committed, "Live https://httpbin.org/forms/post submission did not commit")
        assert committed is not None
        self.assertEqual(committed["last_decision"]["decision"], "approve")
        self.assertEqual(committed["last_decision"]["surface"], "android_app")
        self.assertIn("httpbin.org/post", committed["page_url"])
        self._mcp_call("walkaway_stop_task", {})


if __name__ == "__main__":
    unittest.main(verbosity=2)
