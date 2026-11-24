#!/usr/bin/env python3
"""
Preview the built AI Red Team web app locally.

    python3 scripts/preview-ai-red-team-web.py [port]

Serves apps/ai-red-team-web/dist, so it shows exactly what the server on
the host serves. Build first with:

    cd apps/ai-red-team-web && npm ci && npm run build
"""

from __future__ import annotations

import functools
import os
import socketserver
import sys
from http.server import SimpleHTTPRequestHandler

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIST = os.path.join(ROOT, "apps", "ai-red-team-web", "dist")
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8130


class Handler(SimpleHTTPRequestHandler):
    """Static server that falls back to index.html, like the production one."""

    def do_GET(self):  # noqa: N802 (http.server API)
        path = self.translate_path(self.path)
        if not os.path.exists(path) or os.path.isdir(path):
            if "." not in os.path.basename(self.path):
                self.path = "/index.html"
        return super().do_GET()

    def end_headers(self):
        # the preview must never serve a stale bundle
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, fmt, *args):
        sys.stderr.write("preview: " + (fmt % args) + "\n")


if __name__ == "__main__":
    if not os.path.isdir(DIST):
        print(f"no build at {DIST}\nbuild it first: cd apps/ai-red-team-web && npm ci && npm run build", file=sys.stderr)
        raise SystemExit(2)
    handler = functools.partial(Handler, directory=DIST)
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("127.0.0.1", PORT), handler) as httpd:
        print(f"ai-red-team-web preview on http://127.0.0.1:{PORT}/")
        httpd.serve_forever()
