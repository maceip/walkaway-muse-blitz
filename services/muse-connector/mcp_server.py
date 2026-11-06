"""Local Browser for Muse (Walkaway) Muse Connector — Existing MCP, streamable HTTP.

Muse (chat, phone, or glasses) delegates browser work to the user's own
companion agent and monitors progress. The connector deliberately has NO
approve tool: executing a proposed browser step requires the wearer to
approve the exact step on Meta Ray-Ban Display glasses (Walkaway Web App).
Muse can request approval, reject a proposal, and stop a task. Muse can
never authorize Muse's own browser actions.

Transport: stateless JSON-RPC 2.0 over POST /mcp (MCP streamable HTTP).
Auth: Bearer token, env WALKAWAY_CONNECTOR_TOKEN (required in production;
server refuses to bind a non-loopback host without a token).
Screenshots never leave the user's machine through this connector: tool
results carry page title/URL and the proposed action as text only.

Run:
  WALKAWAY_CONNECTOR_TOKEN=... python3 connector/mcp_server.py
"""

import hashlib
import hmac
import json
import os
import sys
import urllib.request
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

PORT = int(os.environ.get("WALKAWAY_CONNECTOR_PORT", "8890"))
HOST = os.environ.get("WALKAWAY_CONNECTOR_HOST", "127.0.0.1")
TOKEN = os.environ.get("WALKAWAY_CONNECTOR_TOKEN", "")
RELAY = os.environ.get("WALKAWAY_RELAY_URL", "http://127.0.0.1:8790").rstrip("/")
APPROVAL_URL = os.environ.get("WALKAWAY_APPROVAL_URL") or os.environ.get("WALKAWAY_GLASSES_URL", "http://127.0.0.1:5173/")
PROTOCOL = "2025-03-26"
MAX_BODY = 65536

INSTRUCTIONS = (
    "Secure Browser with Approval runs a browser agent on the user's own machine. Start a task with the "
    "user's verbatim goal, propose steps, and report status in one sentence. Executing "
    "a step always requires the user to approve the exact step on the Walkaway approval "
    "page (approval_url); this connector cannot approve. When a step is awaiting approval, "
    "send the user the approval_url and stop calling tools until they reply."
)


def relay(command, body=None):
    token = os.environ.get("WALKAWAY_RELAY_TOKEN", "")
    auth = {"X-Walkaway-Token": token} if token else {}
    if command == "state":
        req = urllib.request.Request(RELAY + "/api/state", headers=auth)
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    req = urllib.request.Request(
        RELAY + f"/api/{command}",
        data=json.dumps(body or {}).encode(),
        headers={"Content-Type": "application/json", **auth},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as error:
        payload = json.loads(error.read() or b"{}")
        raise ValueError(payload.get("error", f"Relay returned HTTP {error.code}")) from None


def public(state):
    page = state.get("page") or {}
    decision = state.get("decision")
    return {
        "say": state.get("say", ""),
        "status": state.get("status"),
        "goal": state.get("goal"),
        "page_title": page.get("title"),
        "page_url": page.get("url"),
        "proposed_step": (
            f"{decision['operation']}: {decision.get('label') or decision.get('choice')}"
            if decision else None
        ),
        "awaiting_approval": bool(decision),
        "awaiting_approval_on_glasses": bool(decision),
        "awaiting_approval_on_android": bool(decision),
        "card_message_id": decision.get("card_message_id") if isinstance(decision, dict) else None,
        "card_title": decision.get("card_title") if isinstance(decision, dict) else None,
        "card_body": decision.get("card_body") if isinstance(decision, dict) else None,
        "last_decision": state.get("last_decision"),
        "result": state.get("result"),
        "page_preview_b64": state.get("preview_b64"),
        "approval_url": APPROVAL_URL if decision else None,
        "glasses_url": APPROVAL_URL if decision else None,
        "steps_acted": state.get("history_count", 0),
        "elapsed_ms": state.get("elapsed_ms", 0),
        "decision_model": state.get("decision_model"),
    }


TOOLS = [
    {
        "name": "walkaway_start_task",
        "description": "Start a browser task on the user's own machine from the user's verbatim goal and optional target URL. The browser opens the real webpage immediately; commits require approval on the user's glasses or Android app.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "goal": {"type": "string", "description": "The user's goal, verbatim."},
                "url": {"type": "string", "description": "Optional https:// or http:// target URL to open in Hermes browser."},
                "scenario": {"type": "string", "enum": ["flights", "travel", "research"], "description": "Optional scenario preset."},
            },
            "required": ["goal"],
        },
        "annotations": {"readOnlyHint": False, "destructiveHint": False, "openWorldHint": True},
    },
    {
        "name": "walkaway_status",
        "description": "Read the current Walkaway task: status, page, proposed step, and whether the user is being asked to approve a step on their glasses.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": True, "destructiveHint": False, "openWorldHint": False},
    },
    {
        "name": "walkaway_propose_step",
        "description": "Ask the companion agent to propose its next browser step and show the proposal on the user's glasses. Proposing never executes anything.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": False, "destructiveHint": False, "openWorldHint": True},
    },
    {
        "name": "walkaway_request_approval",
        "description": "Ensure a step is proposed and waiting on the Walkaway approval page, then report the approval_url. This tool cannot approve or execute the step.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": False, "destructiveHint": False, "openWorldHint": True},
    },
    {
        "name": "walkaway_reject_step",
        "description": "Discard the currently proposed step so the step can never execute. Safe direction only: rejecting never performs a browser action.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": False, "destructiveHint": False, "openWorldHint": False},
    },
    {
        "name": "walkaway_stop_task",
        "description": "Stop the Walkaway task and close the companion browser.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": False, "destructiveHint": True, "openWorldHint": False},
    },
]


def call_tool(name, args):
    args = args or {}
    if name == "walkaway_start_task":
        payload = {"goal": args.get("goal", ""), "scenario": args.get("scenario", "flights")}
        if args.get("url"):
            payload["url"] = args["url"]
        return public(relay("start", payload))
    if name == "walkaway_status":
        return public(relay("state"))
    if name == "walkaway_propose_step":
        return public(relay("predict"))
    if name == "walkaway_request_approval":
        state = relay("state")
        if not state.get("decision"):
            state = relay("predict")
        result = public(state)
        result["say"] = (
            "A step is waiting on your glasses and Android app: "
            + (result.get("card_title") or result["proposed_step"] or "none")
            + ". Approve or reject on your glasses or Android app; this connector cannot approve."
        )
        return result
    if name == "walkaway_reject_step":
        return public(relay("reject"))
    if name == "walkaway_stop_task":
        return public(relay("stop"))
    raise ValueError(f"Unknown tool {name}")


def handle_rpc(message):
    method = message.get("method")
    rid = message.get("id")
    if method == "initialize":
        return {
            "protocolVersion": PROTOCOL,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "local-browser-for-muse", "version": "1.0.0"},
            "instructions": INSTRUCTIONS,
        }, rid
    if method == "ping":
        return {}, rid
    if method == "tools/list":
        return {"tools": TOOLS}, rid
    if method == "tools/call":
        params = message.get("params") or {}
        try:
            result = call_tool(params.get("name"), params.get("arguments"))
            return {
                "content": [{"type": "text", "text": json.dumps(result)}],
                "structuredContent": result,
                "isError": False,
            }, rid
        except (ValueError, OSError) as error:
            return {"content": [{"type": "text", "text": str(error)}], "isError": True}, rid
    if rid is None:
        return None, None
    return {"_error": {"code": -32601, "message": f"Method not found: {method}"}}, rid


class Handler(BaseHTTPRequestHandler):
    def _log_request(self, status):
        # Request diagnostics for connector validation. Never log a presented
        # credential itself: only its scheme, length, short SHA-256
        # fingerprint, and whether the credential matches the configured one.
        header = self.headers.get("Authorization", "")
        parts = header.split(None, 1)
        scheme = parts[0] if parts else "-"
        presented = parts[1] if len(parts) == 2 else ""
        fingerprint = hashlib.sha256(presented.encode()).hexdigest()[:8] if presented else "-"
        matches = bool(TOKEN and presented and hmac.compare_digest(presented, TOKEN))
        parsed = urlparse(self.path)
        query_keys = ",".join(sorted(parse_qs(parsed.query).keys())) or "-"
        header_names = ",".join(sorted(self.headers.keys()))
        print(
            "connector_request "
            f"ts={datetime.now(timezone.utc).isoformat()} "
            f"status={status} method={self.command} path={parsed.path} query_keys={query_keys} "
            f"rpc={getattr(self, '_rpc_method', '-') or '-'} "
            f"client={self.client_address[0]} xff={self.headers.get('X-Forwarded-For', '-')} "
            f"auth_present={bool(header)} auth_scheme={scheme} token_len={len(presented)} "
            f"token_sha256_8={fingerprint} token_match={matches} "
            f"content_type={self.headers.get('Content-Type', '-')} "
            f"accept={self.headers.get('Accept', '-')} "
            f"user_agent={self.headers.get('User-Agent', '-')} "
            f"headers={header_names}",
            file=sys.stderr,
            flush=True,
        )

    def send_body(self, status, payload):
        self._log_request(status)
        content = b"" if payload is None else json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(content)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(content)

    def authorized(self):
        if not TOKEN:
            return True
        header = self.headers.get("Authorization", "")
        return header.startswith("Bearer ") and hmac.compare_digest(header[7:], TOKEN)

    def do_GET(self):
        if urlparse(self.path).path == "/health":
            return self.send_body(200, {"ok": True, "server": "walkaway-connector"})
        return self.send_body(404, {"error": "Not found"})

    def do_POST(self):
        if urlparse(self.path).path != "/mcp":
            return self.send_body(404, {"error": "Not found"})
        if not self.authorized():
            return self.send_body(401, {"error": "Unauthorized"})
        length = int(self.headers.get("Content-Length", "0"))
        if length <= 0 or length > MAX_BODY:
            return self.send_body(413, {"error": "Invalid request size"})
        try:
            message = json.loads(self.rfile.read(length))
        except json.JSONDecodeError:
            return self.send_body(400, {"error": "Invalid JSON"})
        self._rpc_method = message.get("method")
        result, rid = handle_rpc(message)
        if rid is None:
            return self.send_body(202, None)
        if isinstance(result, dict) and "_error" in result:
            return self.send_body(200, {"jsonrpc": "2.0", "id": rid, "error": result["_error"]})
        return self.send_body(200, {"jsonrpc": "2.0", "id": rid, "result": result})

    def log_message(self, *_args):
        pass


def main():
    if HOST not in {"127.0.0.1", "localhost", "::1"} and not TOKEN:
        raise SystemExit("Refusing to bind a non-loopback host without WALKAWAY_CONNECTOR_TOKEN")
    server = ThreadingHTTPServer((HOST, PORT), Handler)
    print(f"Walkaway connector: http://{HOST}:{PORT}/mcp", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
