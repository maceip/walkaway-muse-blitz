#!/usr/bin/env python3
"""
Meta Ray-Ban Display Astryx Web App & WebAuthn Probe Server.
Serves the compiled Meta Astryx (@astryxdesign/core) glasses web app from
apps/glasses-webapp/dist on HTTP :8799 and HTTPS :8443, alongside the
/.well-known/meta-wearables-manifest.json and live telemetry/decision endpoints.
"""

from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import mimetypes
from pathlib import Path
import ssl
import threading
from urllib.parse import urlparse

REPORTS_PATH = Path("/tmp/webauthn_probe_reports.jsonl")
PENDING_DECISIONS_PATH = Path("/tmp/walkaway_pending_decisions.json")
DIST_DIR = Path(__file__).resolve().parents[2] / "apps" / "glasses-webapp" / "dist"


def load_reports(limit: int = 25) -> tuple[int, list[dict]]:
    rows: list[dict] = []
    total = 0
    if REPORTS_PATH.exists():
        lines = [ln for ln in REPORTS_PATH.read_text(encoding="utf-8").splitlines() if ln.strip()]
        total = len(lines)
        for line in lines[-limit:]:
            try:
                rows.append(json.loads(line))
            except Exception:
                pass
    return total, rows


def load_pending_decisions() -> list[dict]:
    if PENDING_DECISIONS_PATH.exists():
        try:
            data = json.loads(PENDING_DECISIONS_PATH.read_text(encoding="utf-8"))
            if isinstance(data, list):
                return data
            if isinstance(data, dict) and isinstance(data.get("pending"), list):
                return data["pending"]
        except Exception:
            pass
    return []


class ProbeHandler(BaseHTTPRequestHandler):
    def _send_json(self, payload: dict, status: int = 200) -> None:
        body = json.dumps(payload, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        req_path = parsed.path

        if req_path in {"/.well-known/meta-wearables-manifest.json", "/manifest.json"}:
            manifest = {
                "name": "Walkaway Probe",
                "short_name": "Walkaway",
                "description": "Walkaway Authorization & WebAuthn HUD built with Meta Astryx for Meta Ray-Ban Display",
                "start_url": "/",
                "scope": "/",
                "display": "standalone",
                "theme_color": "#181B20",
                "background_color": "#181B20",
                "icon": {
                    "src": "/icon.svg",
                    "type": "image/svg+xml",
                },
                "icon_background": {
                    "gradient": ["#262A32", "#181B20"],
                },
            }
            self._send_json(manifest)
            return

        if req_path == "/icon.svg":
            svg = (
                b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">'
                b'<circle cx="64" cy="64" r="52" fill="none" stroke="#ffffff" stroke-width="10"/>'
                b'<path d="M44 64l14 14 26-28" fill="none" stroke="#ffffff" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>'
                b"</svg>"
            )
            self.send_response(200)
            self.send_header("Content-Type", "image/svg+xml")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Content-Length", str(len(svg)))
            self.end_headers()
            self.wfile.write(svg)
            return

        if req_path in {"/reports", "/api/reports"}:
            total, rows = load_reports()
            self._send_json({"count": total, "reports": rows})
            return

        if req_path == "/api/pending-decisions":
            self._send_json({"pending": load_pending_decisions()})
            return

        # Serve static assets from apps/glasses-webapp/dist
        if req_path.startswith("/assets/"):
            asset_file = (DIST_DIR / req_path.lstrip("/")).resolve()
            if asset_file.is_file() and str(asset_file).startswith(str(DIST_DIR.resolve())):
                content = asset_file.read_bytes()
                mime_type, _ = mimetypes.guess_type(str(asset_file))
                self.send_response(200)
                self.send_header("Content-Type", mime_type or "application/octet-stream")
                self.send_header("Content-Length", str(len(content)))
                self.send_header("Cache-Control", "public, max-age=31536000, immutable")
                self.end_headers()
                self.wfile.write(content)
                return
            self.send_error(404, "Asset not found")
            return

        # Serve index.html from apps/glasses-webapp/dist
        index_file = DIST_DIR / "index.html"
        if index_file.is_file():
            body = index_file.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
            return

        self.send_error(500, "Astryx build dist/index.html missing")

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        req_path = parsed.path
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8", errors="replace")
        try:
            data = json.loads(raw) if raw.strip() else {}
        except Exception:
            data = {"raw": raw}

        data["_endpoint"] = req_path
        data["_client_ip"] = self.client_address[0]
        data["_received_at"] = datetime.now(timezone.utc).isoformat()
        with REPORTS_PATH.open("a", encoding="utf-8") as f:
            f.write(json.dumps(data) + "\n")

        if req_path == "/api/decision-ack":
            decision_id = data.get("decision_id")
            if decision_id and PENDING_DECISIONS_PATH.exists():
                remaining = [
                    d
                    for d in load_pending_decisions()
                    if d.get("decision_id") != decision_id
                ]
                PENDING_DECISIONS_PATH.write_text(
                    json.dumps({"pending": remaining}, indent=2),
                    encoding="utf-8",
                )

        self._send_json({"ok": True})

    def log_message(self, fmt: str, *args) -> None:
        pass


if __name__ == "__main__":
    if Path("/tmp/probe_cert.pem").exists() and Path("/tmp/probe_key.pem").exists():
        https_server = ThreadingHTTPServer(("0.0.0.0", 8443), ProbeHandler)
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(certfile="/tmp/probe_cert.pem", keyfile="/tmp/probe_key.pem")
        https_server.socket = ctx.wrap_socket(https_server.socket, server_side=True)
        threading.Thread(target=https_server.serve_forever, daemon=True).start()
        print("Astryx Glasses WebApp HTTPS listening on https://0.0.0.0:8443", flush=True)
    server = ThreadingHTTPServer(("0.0.0.0", 8799), ProbeHandler)
    print("Astryx Glasses WebApp HTTP listening on http://0.0.0.0:8799", flush=True)
    server.serve_forever()
