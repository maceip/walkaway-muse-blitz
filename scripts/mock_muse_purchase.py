#!/usr/bin/env python3
"""Mock Muse purchase-approval demo (bus -> phone/glasses card -> decision -> result).

Posts a purchase card (image + price + context) to the Walkaway message bus,
waits for a real human decision (glasses_dat or android_app), and prints the
terminal `result` object Muse would receive — the same shape as
`hermes_task_relay` GET /api/state.result.

Usage:
  python3 scripts/mock_muse_purchase.py                 # wait up to 900s for a tap
  python3 scripts/mock_muse_purchase.py --timeout 30    # bounded wait
"""

from __future__ import annotations

import argparse
import base64
import io
import json
import sys
import time
import urllib.request
from pathlib import Path

BUS_URL = "http://127.0.0.1:19099"
BUS_TOKEN = "walkaway-dev-bus-token"

# RYAN EXPLICIT MOCK/FALLBACK AUTHORIZATION: "it's gonna be mocked because the Muse
# connector is still not done yet, which is annoying. But like. When Muse is asking,
# here's the question for you. Let's say Muse has one question. Muse wants to use my
# credit card to buy a bicycle. The would I see my face? Should have a very compressed
# view, like I want a picture of that bicycle. How much it is. And maybe some other
# information about like what else was thought of with the approved deny. So that we
# need to get that flow going as a demo, just a fake."
MOCK_PURCHASE = {
    "kind": "purchase",
    "title": "Muse wants to buy: Trail bicycle",
    "body": "Muse picked this after comparing 3 bikes for your commute.",
    "price": "$1,299.00",
    "detail": "Also considered: 2 cheaper options, best return window · card ending 4291",
}


def _mock_bike_png_b64() -> str:
    """Minimal labeled photo chip (mock) within the 320px / 48k budget."""
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (320, 180), (43, 33, 24))
    draw = ImageDraw.Draw(img)
    draw.rectangle([20, 20, 300, 160], outline=(239, 237, 226), width=3)
    draw.text((120, 80), "bike photo (mock)", fill=(239, 237, 226))
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return base64.b64encode(buf.getvalue()).decode("ascii")


def _http(method: str, path: str, payload: dict | None = None) -> dict:
    data = json.dumps(payload).encode("utf-8") if payload is not None else None
    req = urllib.request.Request(
        BUS_URL.rstrip("/") + path,
        data=data,
        headers={"Content-Type": "application/json", "X-Bus-Token": BUS_TOKEN},
        method=method,
    )
    with urllib.request.urlopen(req, timeout=5) as resp:
        raw = resp.read().decode("utf-8", errors="replace")
        return json.loads(raw) if raw.strip() else {}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=900.0)
    args = parser.parse_args()

    message = dict(MOCK_PURCHASE)
    message["image_b64"] = _mock_bike_png_b64()
    posted = _http("POST", "/messages", message)
    message_id = posted.get("id")
    print(f"[mock-muse] posted purchase card #{message_id}: {posted.get('title')!r}", flush=True)
    if message_id is None:
        print("[mock-muse] bus rejected the message", file=sys.stderr)
        return 1

    deadline = time.monotonic() + args.timeout
    decision = None
    while time.monotonic() < deadline:
        acks = (_http("GET", "/acks") or {}).get("acks") or []
        for ack in acks:
            if ack.get("message_id") != message_id:
                continue
            action = str(ack.get("action") or "")
            source = str(ack.get("source") or "")
            if action in {"approve", "reject"} and source in {"glasses_dat", "android_app", "glasses"}:
                decision = ack
                break
        if decision:
            break
        time.sleep(1.0)

    if not decision:
        print(f"[mock-muse] no human decision within {args.timeout:.0f}s", file=sys.stderr)
        return 2

    surface = "glasses" if decision.get("source") == "glasses_dat" else "android_app"
    result = {
        "outcome": "committed" if decision["action"] == "approve" else "rejected",
        "decision": decision["action"],
        "surface": surface,
        "title": MOCK_PURCHASE["title"],
        "summary": (
            f"{decision['action']} via {surface} at {decision.get('received_at')}"
        ),
    }
    print("[mock-muse] decision:", json.dumps(decision))
    print("[mock-muse] result (what Muse hears):", json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
