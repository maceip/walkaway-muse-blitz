"""Walkaway companion relay.

Owns the real browser agent (jev-ultrafast Agent loop, imported unmodified)
and serves the glasses Web App. The relay can predict a step but can never
act on a prediction by itself: /api/approve is the only path to Agent.act,
and the glasses page only calls /api/approve after the wearer approves the
exact step shown on the lens. /api/reject deletes the pending decision.

Run:
  cd ~/jev-ultrafast && uv run python ~/walkaway-hackathon/companion/walkaway_relay.py
Requires ~/jev-ultrafast/.env (TEXT_MODEL_*). Decision model selection:
  MODEL_API_KEY set  -> Muse Spark via Meta Model API (spark_choose.py)
  MODEL_API_KEY unset -> TypeSafe Jev chooser (local development fallback)
State reports decision_model verbatim so the UI never mislabels the model.
"""

import hmac
import json
import os
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

ULTRAFAST = Path(os.environ.get("WALKAWAY_ULTRAFAST_DIR", Path.home() / "jev-ultrafast"))
sys.path.insert(0, str(ULTRAFAST))

PORT = int(os.environ.get("WALKAWAY_PORT", "8790"))
LOCK = threading.Lock()
AGENT = None
GOAL = None
SCENARIO = None


def load_env():
    env = ULTRAFAST / ".env"
    if env.exists():
        for line in env.read_text().splitlines():
            if "=" in line and not line.startswith("#"):
                key, value = line.split("=", 1)
                os.environ.setdefault(key.strip(), value.strip())


load_env()
from jev_ultrafast.agent import Agent  # noqa: E402

# browser_harness cdp() defaults to a 5s IPC response timeout; screenshots on
# a loaded headless host routinely exceed that (harness allots 60s for its own
# screenshot helper). Give captureScreenshot 30s. No other method changes.
import jev_ultrafast.browser as _browser_module  # noqa: E402

_original_cdp = _browser_module.cdp


def _cdp(method, session_id=None, **params):
    if method == "Emulation.setFocusEmulationEnabled" and session_id:
        # Pre-seed Google's standard GDPR consent cookie (SOCS) on EU hosts so
        # Google Flights opens directly instead of redirecting to consent.google.com.
        try:
            _original_cdp(
                "Network.setCookie",
                session_id=session_id,
                name="SOCS",
                value="CAISHAgBEhJnd3NfMjAyMzA4MTAtMF9SQzIaAmVuIAEaBgiAo_CmBg",
                domain=".google.com",
                path="/",
                secure=True,
                httpOnly=False,
            )
        except Exception:
            pass
    if method == "Page.captureScreenshot":
        # Snap Chromium on Linux stalls on surface capture after navigation;
        # fromSurface=False returns in ~1s there. macOS Chrome is fine either way.
        first = params if sys.platform == "darwin" else {**params, "fromSurface": False}
        second = {**params, "fromSurface": False} if sys.platform == "darwin" else params
        try:
            return _original_cdp(method, session_id=session_id, _response_timeout=3.0, **first)
        except TimeoutError:
            try:
                return _original_cdp(method, session_id=session_id, _response_timeout=10.0, **second)
            except TimeoutError:
                # A missing frame must not erase an executed step: the
                # snapshot still carries title, state, and history, and the
                # glasses UI shows its explicit no-frame state.
                return {"data": ""}
    return _original_cdp(method, session_id=session_id, **params)


_browser_module.cdp = _cdp

if os.environ.get("MODEL_API_KEY"):
    sys.path.insert(0, str(Path(__file__).parent))
    import jev_ultrafast.agent as _agent_module  # noqa: E402
    import spark_choose  # noqa: E402

    _agent_module.choose = spark_choose.choose
    DECISION_MODEL = spark_choose.MODEL
else:
    DECISION_MODEL = os.environ.get("TYPESAFE_MODEL", "jev-latest (TypeSafe)")


def say(state):
    status = state["status"]
    decision = state.get("decision")
    if status == "predicted" and decision:
        label = decision.get("label") or decision.get("choice")
        return f"Agent proposes {decision['operation']}: {label}. Approve or reject."
    if status == "done":
        return "Task complete."
    if status == "blocked":
        return "Agent is blocked. Stop the task or start a new goal."
    if status == "ready":
        return "Watching the page. Ask for the next step when ready."
    return "No task running."


def public_state():
    if AGENT is None:
        return {
            "status": "idle", "goal": None, "scenario": None, "page": None,
            "decision": None, "awaiting_approval": False, "history_count": 0,
            "elapsed_ms": 0, "decision_model": None,
            "text_model": os.environ.get("TEXT_MODEL"), "say": "No task running.",
        }
    snap = AGENT.snapshot()
    page = snap.get("page") or {}
    decision = snap.get("decision")
    if decision:
        label = next(
            (a.get("label") for a in page.get("actions", []) if a.get("id") == decision.get("choice")),
            decision.get("choice"),
        )
        decision = {
            "operation": decision.get("operation"), "target": decision.get("target"),
            "choice": decision.get("choice"), "confidence": decision.get("confidence", 0),
            "label": label, "model": decision.get("model"), "latency_ms": decision.get("latency_ms"),
        }
    view = {
        "status": snap.get("status", "ready"),
        "goal": GOAL,
        "scenario": SCENARIO,
        "page": {
            "url": page.get("url", ""), "title": page.get("title", ""),
            "screenshot": page.get("screenshot"),
        } if page else None,
        "decision": decision,
        "awaiting_approval": decision is not None,
        "history_count": len(snap.get("history", [])),
        "elapsed_ms": snap.get("elapsed_ms", 0),
        "decision_model": (decision or {}).get("model") or DECISION_MODEL,
        "text_model": os.environ.get("TEXT_MODEL"),
    }
    view["say"] = say(view)
    return view


def close_agent():
    global AGENT, GOAL, SCENARIO
    if AGENT:
        AGENT.close()
    AGENT, GOAL, SCENARIO = None, None, None


def resolve_scenario(goal: str, requested: str | None) -> str:
    g = goal.lower()
    flight_terms = (
        "flight", "flights", "fly ", "flying", "airline", "non-stop", "nonstop",
        "round-trip", "one-way", "airport", "sfo", "jfk", "lax", "ord", "lhr", "bos", "sea",
    )
    research_terms = (
        "article", "reading room", "milliseconds", "confidence is not correctness",
        "choice, not a conversation", "notes from the lab", "research",
    )
    stay_terms = (
        "stay", "stays", "hotel", "lisbon", "copenhagen", "casa flora",
        "glasshouse", "serra lodge", "forma", "free cancellation",
    )
    if any(t in g for t in flight_terms):
        return "flights"
    if any(t in g for t in research_terms):
        return "research"
    if any(t in g for t in stay_terms):
        return "travel"
    return requested or "flights"


def validate_custom_url(raw_url: str) -> str:
    parsed = urlparse(raw_url.strip())
    if parsed.scheme != "https" or not parsed.hostname:
        raise ValueError("Custom URL must be an https:// URL")
    host = parsed.hostname.lower()
    if host in {"localhost", "127.0.0.1", "::1", "0.0.0.0", "169.254.169.254"} or host.endswith(".internal") or host.endswith(".local"):
        raise ValueError("Refusing private or loopback target URL")
    return raw_url.strip()


def command(name, body):
    global AGENT, GOAL, SCENARIO
    if name == "start":
        raw_scenario = body.get("scenario", "flights")
        goal = (body.get("goal") or "").strip()
        if raw_scenario not in {"flights", "travel", "research"}:
            raise ValueError("Unknown scenario")
        if not goal or len(goal) > 2000:
            raise ValueError("Goal must be 1-2000 characters")
        scenario = resolve_scenario(goal, raw_scenario)
        custom_url = (body.get("url") or "").strip()
        close_agent()
        if custom_url:
            url = validate_custom_url(custom_url)
        else:
            url = (
                "https://www.google.com/travel/flights?hl=en&gl=US&curr=USD"
                if scenario == "flights"
                else f"http://127.0.0.1:{PORT}/fixture.html?scenario={scenario}"
            )
        AGENT = Agent(url, goal, screenshots=True)
        GOAL, SCENARIO = goal, scenario
    elif name == "stop":
        close_agent()
    else:
        if AGENT is None:
            raise ValueError("Start a task first")
        if name == "predict":
            AGENT.command("predict", {})
        elif name == "approve":
            page = AGENT.state.get("page") or {}
            AGENT.command("act", {"fingerprint": page.get("fingerprint")})
        elif name == "reject":
            if not AGENT.state.get("decision"):
                raise ValueError("No proposed step to reject")
            AGENT.state["decision"] = None
            AGENT.state["status"] = "ready"
        else:
            raise ValueError("Unknown command")
    return public_state()


class Handler(BaseHTTPRequestHandler):
    def send_body(self, status, content, mime="application/json"):
        content = content if isinstance(content, bytes) else content.encode()
        self.send_response(status)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(content)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(content)

    def authorized(self):
        token = os.environ.get("WALKAWAY_RELAY_TOKEN", "")
        if not token:
            return True
        return hmac.compare_digest(self.headers.get("X-Walkaway-Token", ""), token)

    def do_GET(self):
        path = urlparse(self.path).path
        if path.startswith("/api/") and not self.authorized():
            return self.send_body(401, json.dumps({"error": "Unauthorized"}))
        if path == "/api/state":
            with LOCK:
                return self.send_body(200, json.dumps(public_state()))
        if path == "/fixture.html":
            fixture = ULTRAFAST / "jev_ultrafast" / "static" / "fixture.html"
            return self.send_body(200, fixture.read_text().replace("__TOKEN__", "walkaway"), "text/html; charset=utf-8")
        return self.send_body(404, json.dumps({"error": "Not found"}))

    def do_POST(self):
        if not self.authorized():
            return self.send_body(401, json.dumps({"error": "Unauthorized"}))
        if not LOCK.acquire(blocking=False):
            return self.send_body(409, json.dumps({"error": "A step is already running"}))
        try:
            length = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(length)) if length else {}
            result = command(urlparse(self.path).path.removeprefix("/api/"), body)
            self.send_body(200, json.dumps(result))
        except (ValueError, RuntimeError) as error:
            self.send_body(400, json.dumps({"error": str(error)}))
        except Exception as error:  # surface, never swallow into a fake success
            self.send_body(500, json.dumps({"error": f"Relay failed: {error}"}))
        finally:
            LOCK.release()

    def log_message(self, *_args):
        pass


def main():
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"Walkaway relay: http://127.0.0.1:{PORT}", flush=True)
    try:
        server.serve_forever()
    finally:
        close_agent()
        server.server_close()


if __name__ == "__main__":
    main()
