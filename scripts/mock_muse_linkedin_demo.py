#!/usr/bin/env python3
"""
Mocked Muse End-to-End Desktop Demo Client -> Walkaway MCP Connector (:8890).

Simulates the Muse assistant receiving the user prompt:
  "Create an account on LinkedIn"
and driving the full Walkaway end-to-end architecture over MCP JSON-RPC 2.0:
  Mocked Muse
    -> Muse MCP Server (services/muse-connector/mcp_server.py :8890)
    -> Hermes Task Relay (services/agent-driver/hermes_task_relay.py :8790)
    -> Local Qwen3-4B Form Planner + Blind Vault Secret Staging (fill_secret)
    -> Visible Native Walkaway Browser (/Applications/Walkaway.app --show-window --xray)
    -> Decision Bridge (:19101) + Message Bus (:19099) + Astryx Glasses HUD (:8799)
"""

import argparse
import json
import sys
import time
import urllib.request
from typing import Any


class MockMuseMCPClient:
    def __init__(self, mcp_url: str = "http://127.0.0.1:8890/mcp") -> None:
        self.mcp_url = mcp_url
        self._next_id = 1

    def rpc(self, method: str, params: dict[str, Any] | None = None, timeout_s: float = 90.0) -> dict[str, Any]:
        req_id = self._next_id
        self._next_id += 1
        payload = {
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params or {},
        }
        req = urllib.request.Request(
            self.mcp_url,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json", "Accept": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=timeout_s) as resp:
            body = json.loads(resp.read().decode("utf-8"))
        if "error" in body:
            raise RuntimeError(f"MCP error on {method}: {body['error']}")
        return body.get("result") or {}

    def call_tool(self, name: str, arguments: dict[str, Any] | None = None, timeout_s: float = 90.0) -> dict[str, Any]:
        res = self.rpc("tools/call", {"name": name, "arguments": arguments or {}}, timeout_s=timeout_s)
        structured = res.get("structuredContent")
        if isinstance(structured, dict):
            return structured
        content = res.get("content") or []
        if content and isinstance(content[0], dict) and content[0].get("text"):
            try:
                return json.loads(content[0]["text"])
            except Exception:
                return {"raw_text": content[0]["text"]}
        return res


def post_glasses_ack(bus_url: str, bus_token: str, message_id: int, action: str = "approve") -> dict[str, Any]:
    req = urllib.request.Request(
        f"{bus_url.rstrip('/')}/ack",
        data=json.dumps(
            {
                "message_id": message_id,
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
        return json.loads(resp.read().decode("utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Mocked Muse -> Walkaway LinkedIn Signup E2E Demo")
    parser.add_argument("--mcp-url", default="http://127.0.0.1:8890/mcp")
    parser.add_argument("--bus-url", default="http://127.0.0.1:19099")
    parser.add_argument("--bus-token", default="walkaway-dev-bus-token")
    parser.add_argument(
        "--goal",
        default="Create an account on LinkedIn for Ryan MacArthur (ryan@zunftmax.secure.build)",
    )
    parser.add_argument("--url", default="https://www.linkedin.com/signup")
    parser.add_argument(
        "--approve-step1",
        action="store_true",
        help="Simulate approving Step 1 (Email + Vault Password) on Glasses so LinkedIn advances to Step 2 (First/Last Name) and stages Step 2.",
    )
    args = parser.parse_args()

    muse = MockMuseMCPClient(args.mcp_url)

    print("======================================================================")
    print("  MOCKED MUSE DESKTOP DEMO -> WALKAWAY MCP CONNECTOR (:8890)")
    print("======================================================================")
    print(f"[Muse User Prompt] \"{args.goal}\"")

    init_res = muse.rpc("initialize", {"clientInfo": {"name": "mock-muse-desktop", "version": "1.0.0"}})
    server_info = init_res.get("serverInfo") or {}
    print(f"[Muse -> MCP] Connected to {server_info.get('name')} v{server_info.get('version')}")

    tools_res = muse.rpc("tools/list")
    tool_names = [t.get("name") for t in (tools_res.get("tools") or [])]
    print(f"[Muse -> MCP] Discovered {len(tool_names)} Walkaway tools: {', '.join(tool_names)}")

    print("\n[Muse -> MCP] Calling tool `walkaway_start_task`...")
    t0 = time.perf_counter()
    state = muse.call_tool(
        "walkaway_start_task",
        {"goal": args.goal, "url": args.url},
        timeout_s=90.0,
    )
    if not state.get("awaiting_approval"):
        print(f"[Muse -> MCP] Status is {state.get('status')!r}; calling `walkaway_request_approval`...")
        state = muse.call_tool("walkaway_request_approval", {}, timeout_s=90.0)
    elapsed_s = time.perf_counter() - t0

    print(f"\n[Walkaway -> Muse] Task staged in {elapsed_s:.2f}s:")
    print(f"  Status                  : {state.get('status')}")
    print(f"  Page URL                : {state.get('page_url')}")
    print(f"  Page Title              : {state.get('page_title')}")
    print(f"  Local Model             : {state.get('decision_model')}")
    print(f"  Steps Acted             : {state.get('steps_acted')}")
    print(f"  Proposed Step           : {state.get('proposed_step')}")
    print(f"  Awaiting on Glasses     : {state.get('awaiting_approval_on_glasses')}")
    print(f"  Bus Card ID             : #{state.get('card_message_id')}")
    print(f"  Card Title (Glasses)    : {state.get('card_title')}")
    print(f"  Card Body (Glasses)     : {state.get('card_body')}")
    print(f"  Assistant Say           : {state.get('say')}")

    if args.approve_step1 and state.get("card_message_id"):
        msg_id = int(state["card_message_id"])
        print(f"\n[Glasses HUD -> Bus] Approving Step 1 Card #{msg_id} (`Agree & Join`)...")
        post_glasses_ack(args.bus_url, args.bus_token, msg_id, action="approve")

        deadline = time.monotonic() + 12.0
        while time.monotonic() < deadline:
            time.sleep(0.4)
            st = muse.call_tool("walkaway_status", {})
            if st.get("status") == "committed":
                print(f"[Walkaway -> Muse] Step 1 committed! Receipt: {json.dumps(st.get('last_decision'))}")
                break

        time.sleep(1.0)
        print("\n[Muse -> MCP] Calling `walkaway_propose_step` for LinkedIn Step 2 (First & Last Name)...")
        step2 = muse.call_tool("walkaway_propose_step", {}, timeout_s=90.0)
        print(f"  Status                  : {step2.get('status')}")
        print(f"  Proposed Step           : {step2.get('proposed_step')}")
        print(f"  Bus Card ID             : #{step2.get('card_message_id')}")
        print(f"  Card Title (Glasses)    : {step2.get('card_title')}")
        print(f"  Card Body (Glasses)     : {step2.get('card_body')}")
        print(f"  Assistant Say           : {step2.get('say')}")

    print("\n======================================================================")
    print("  LIVE DESKTOP SURFACES READY FOR INSPECTION:")
    print("  1. Native Walkaway Desktop Browser : Visible on your macOS desktop")
    print("  2. Astryx Glasses Web App HUD      : http://127.0.0.1:8799/")
    print("  3. Muse MCP Server Endpoint        : http://127.0.0.1:8890/mcp")
    print("======================================================================")
    return 0


if __name__ == "__main__":
    sys.exit(main())
