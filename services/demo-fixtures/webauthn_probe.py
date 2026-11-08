#!/usr/bin/env python3
"""Live WebAuthn / Credentials API capability probe & interactive test server."""

import json
import os
import base64
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

REPORTS_PATH = Path("/tmp/webauthn_probe_reports.jsonl")

PROBE_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Walkaway WebAuthn &amp; Credentials Probe</title>
  <style>
    * { box-sizing: border-box; }
    body {
      margin: 0; padding: 16px;
      font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", Roboto, monospace;
      background: #090d16; color: #e6edf3;
    }
    .wrap { max-width: 760px; margin: 0 auto; }
    .hdr {
      background: #111827; border: 2px solid #374151; border-radius: 14px;
      padding: 16px; margin-bottom: 14px;
    }
    h1 { margin: 0 0 6px 0; font-size: 20px; color: #f9fafb; }
    .sub { font-size: 12px; color: #9ca3af; word-break: break-all; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 10px; margin-bottom: 14px; }
    .tile {
      background: #111827; border: 1px solid #374151; border-radius: 10px; padding: 12px;
    }
    .tile .k { font-size: 11px; color: #9ca3af; text-transform: uppercase; letter-spacing: 0.05em; }
    .tile .v { font-size: 16px; font-weight: 700; margin-top: 4px; }
    .ok { color: #34d399; }
    .warn { color: #fbbf24; }
    .bad { color: #f87171; }
    .btns { display: flex; flex-direction: row; gap: 12px; margin-bottom: 14px; }
    button {
      flex: 1; padding: 14px 12px;
      border-radius: 12px; border: 2px solid #4b5563;
      background: #1f2937; color: #f9fafb; font-size: 16px; font-weight: 700;
      cursor: pointer; outline: none;
    }
    button:focus, button.focused {
      border-color: #ffffff !important;
      box-shadow: 0 0 0 4px #38bdf8;
      transform: scale(1.02);
    }
    button.primary { background: #2563eb; border-color: #60a5fa; }
    button.success { background: #059669; border-color: #34d399; }
    pre {
      background: #0d1117; border: 1px solid #30363d; border-radius: 10px;
      padding: 14px; font-size: 12px; overflow-x: auto; white-space: pre-wrap; word-break: break-word;
      color: #79c0ff;
    }
  </style>
</head>
<body>
  <div class="wrap">
    <div class="hdr">
      <h1>WebAuthn &amp; window.credentials Capability Probe</h1>
      <div class="sub" id="ua-line">Enumerating browser environment...</div>
    </div>
    <div class="grid" id="summary-grid"></div>
    <div class="btns">
      <a id="btn-install-glasses" href="fb-viewapp://web_app_deep_link?appName=Walkaway%20Probe&amp;appUrl=https%3A%2F%2Fleo-parallel-twiki-integral.trycloudflare.com%2F" style="flex:1;min-width:220px;padding:12px 16px;border-radius:10px;border:1px solid #f472b6;background:#be185d;color:#ffffff;font-size:14px;font-weight:700;text-align:center;text-decoration:none;display:inline-block;">Install Web App on Glasses (fb-viewapp://)</a>
      <button class="primary" id="btn-create" type="button">Test credentials.create() (Register)</button>
      <button class="success" id="btn-get" type="button">Test credentials.get() (Sign In)</button>
      <button id="btn-rerun" type="button">Re-Run Probe</button>
    </div>
    <pre id="json-out">Running probe...</pre>
  </div>
  <script>
    (function() {
      function ownAndProtoProps(obj) {
        if (!obj) return [];
        var props = [];
        var cur = obj;
        var depth = 0;
        while (cur && cur !== Object.prototype && depth < 4) {
          var names = Object.getOwnPropertyNames(cur);
          for (var i = 0; i < names.length; i++) {
            if (props.indexOf(names[i]) === -1) props.push(names[i]);
          }
          cur = Object.getPrototypeOf(cur);
          depth++;
        }
        return props.sort();
      }

      function b64url(buf) {
        if (!buf) return null;
        var b = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
        var s = '';
        for (var i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
        return btoa(s).replace(/\\+/g, '-').replace(/\\//g, '_').replace(/=+$/, '');
      }

      async function collectProbe() {
        var report = {
          ts: new Date().toISOString(),
          location: String(window.location.href),
          origin: String(window.location.origin),
          isSecureContext: Boolean(window.isSecureContext),
          userAgent: String(navigator.userAgent || ''),
          platform: String(navigator.platform || ''),
          globals: {
            navigator_credentials_type: typeof navigator.credentials,
            window_credentials_type: typeof window.credentials,
            CredentialsContainer: typeof window.CredentialsContainer,
            Credential: typeof window.Credential,
            PublicKeyCredential: typeof window.PublicKeyCredential,
            AuthenticatorResponse: typeof window.AuthenticatorResponse,
            AuthenticatorAttestationResponse: typeof window.AuthenticatorAttestationResponse,
            AuthenticatorAssertionResponse: typeof window.AuthenticatorAssertionResponse,
            PasswordCredential: typeof window.PasswordCredential,
            FederatedCredential: typeof window.FederatedCredential,
            IdentityCredential: typeof window.IdentityCredential,
            OTPCredential: typeof window.OTPCredential,
            DigitalCredential: typeof window.DigitalCredential
          },
          navigator_credentials_methods: ownAndProtoProps(navigator.credentials),
          PublicKeyCredential_static_methods: ownAndProtoProps(window.PublicKeyCredential),
          PublicKeyCredential_prototype_methods: window.PublicKeyCredential ? ownAndProtoProps(window.PublicKeyCredential.prototype) : [],
          async_checks: {
            uvpaa: null,
            conditionalMediation: null,
            clientCapabilities: null
          },
          ceremony_test: null
        };

        if (window.PublicKeyCredential) {
          if (typeof PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable === 'function') {
            try {
              report.async_checks.uvpaa = await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable();
            } catch (e) {
              report.async_checks.uvpaa = 'error: ' + (e && e.name ? e.name + ': ' + e.message : String(e));
            }
          }
          if (typeof PublicKeyCredential.isConditionalMediationAvailable === 'function') {
            try {
              report.async_checks.conditionalMediation = await PublicKeyCredential.isConditionalMediationAvailable();
            } catch (e) {
              report.async_checks.conditionalMediation = 'error: ' + (e && e.name ? e.name + ': ' + e.message : String(e));
            }
          }
          if (typeof PublicKeyCredential.getClientCapabilities === 'function') {
            try {
              report.async_checks.clientCapabilities = await PublicKeyCredential.getClientCapabilities();
            } catch (e) {
              report.async_checks.clientCapabilities = 'error: ' + (e && e.name ? e.name + ': ' + e.message : String(e));
            }
          }
        }
        return report;
      }

      function renderSummary(r) {
        document.getElementById('ua-line').textContent = r.userAgent + ' | isSecureContext=' + r.isSecureContext;
        var items = [
          ['navigator.credentials', r.globals.navigator_credentials_type === 'object', r.globals.navigator_credentials_type],
          ['PublicKeyCredential', r.globals.PublicKeyCredential === 'function', r.globals.PublicKeyCredential],
          ['Platform UVPAA', r.async_checks.uvpaa === true, String(r.async_checks.uvpaa)],
          ['Conditional UI', r.async_checks.conditionalMediation === true, String(r.async_checks.conditionalMediation)],
          ['Client Capabilities', r.async_checks.clientCapabilities && typeof r.async_checks.clientCapabilities === 'object', r.async_checks.clientCapabilities ? JSON.stringify(r.async_checks.clientCapabilities) : 'none'],
          ['Ceremony Result', r.ceremony_test && r.ceremony_test.ok === true, r.ceremony_test ? (r.ceremony_test.ok ? r.ceremony_test.kind + ' OK' : r.ceremony_test.error) : 'Not run yet']
        ];
        var html = '';
        for (var i = 0; i < items.length; i++) {
          var cls = items[i][1] ? 'ok' : (items[i][2] === 'Not run yet' ? 'warn' : 'bad');
          html += '<div class="tile"><div class="k">' + items[i][0] + '</div><div class="v ' + cls + '">' + items[i][2] + '</div></div>';
        }
        document.getElementById('summary-grid').innerHTML = html;
        document.getElementById('json-out').textContent = JSON.stringify(r, null, 2);
      }

      function postReport(r) {
        try {
          if (typeof fetch === 'function') {
            fetch('/report', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify(r)
            }).catch(function() {});
          }
        } catch (e) {}
      }

      var latestReport = null;

      async function runProbe() {
        latestReport = await collectProbe();
        renderSummary(latestReport);
        postReport(latestReport);
      }

      document.getElementById('btn-rerun').addEventListener('click', runProbe);

      document.getElementById('btn-create').addEventListener('click', async function() {
        if (!latestReport) latestReport = await collectProbe();
        try {
          var chal = new Uint8Array(32);
          if (window.crypto && crypto.getRandomValues) crypto.getRandomValues(chal);
          var userId = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);
          var cred = await navigator.credentials.create({
            publicKey: {
              rp: { name: 'Walkaway Glasses Probe' },
              user: { id: userId, name: 'ryan@walkaway.dev', displayName: 'Ryan (Glasses Probe)' },
              challenge: chal,
              pubKeyCredParams: [{ type: 'public-key', alg: -7 }, { type: 'public-key', alg: -257 }],
              timeout: 60000
            }
          });
          latestReport.ceremony_test = {
            ok: true,
            kind: 'create',
            id: cred ? cred.id : null,
            type: cred ? cred.type : null,
            authenticatorAttachment: cred ? cred.authenticatorAttachment : null,
            attestationLen: (cred && cred.response && cred.response.attestationObject) ? cred.response.attestationObject.byteLength : 0,
            clientDataLen: (cred && cred.response && cred.response.clientDataJSON) ? cred.response.clientDataJSON.byteLength : 0
          };
        } catch (e) {
          latestReport.ceremony_test = {
            ok: false,
            kind: 'create',
            error: (e && e.name ? e.name + ': ' + e.message : String(e))
          };
        }
        renderSummary(latestReport);
        postReport(latestReport);
      });

      document.getElementById('btn-get').addEventListener('click', async function() {
        if (!latestReport) latestReport = await collectProbe();
        try {
          var chal = new Uint8Array(32);
          if (window.crypto && crypto.getRandomValues) crypto.getRandomValues(chal);
          var cred = await navigator.credentials.get({
            publicKey: {
              challenge: chal,
              timeout: 60000
            }
          });
          latestReport.ceremony_test = {
            ok: true,
            kind: 'get',
            id: cred ? cred.id : null,
            type: cred ? cred.type : null,
            sigLen: (cred && cred.response && cred.response.signature) ? cred.response.signature.byteLength : 0
          };
        } catch (e) {
          latestReport.ceremony_test = {
            ok: false,
            kind: 'get',
            error: (e && e.name ? e.name + ': ' + e.message : String(e))
          };
        }
        renderSummary(latestReport);
        postReport(latestReport);
      });

      runProbe();

      var focusables = [
        document.getElementById('btn-install-glasses'),
        document.getElementById('btn-create'),
        document.getElementById('btn-get'),
        document.getElementById('btn-rerun')
      ];
      var focusIdx = 1;
      function applyFocus(idx) {
        focusIdx = (idx + focusables.length) % focusables.length;
        for (var i = 0; i < focusables.length; i++) {
          if (!focusables[i]) continue;
          if (i === focusIdx) {
            focusables[i].classList.add('focused');
            focusables[i].focus();
          } else {
            focusables[i].classList.remove('focused');
          }
        }
      }
      window.addEventListener('keydown', function(e) {
        if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
          e.preventDefault();
          applyFocus(focusIdx + 1);
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
          e.preventDefault();
          applyFocus(focusIdx - 1);
        } else if (e.key === 'Enter') {
          if (focusables[focusIdx]) {
            e.preventDefault();
            focusables[focusIdx].click();
          }
        }
      });
      setTimeout(function() { applyFocus(1); }, 100);
    })();
  </script>
</body>
</html>
"""


class ProbeHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path in {"/.well-known/meta-wearables-manifest.json", "/manifest.json"}:
            manifest = {
                "name": "Walkaway Probe",
                "short_name": "Walkaway",
                "description": "WebAuthn & window.credentials capability probe for Meta Ray-Ban Display",
                "start_url": "/",
                "scope": "/",
                "display": "standalone",
                "theme_color": "#000000",
                "background_color": "#000000",
                "icon": {
                    "src": "/icon.svg",
                    "type": "image/svg+xml",
                },
                "icon_background": {
                    "gradient": ["#1f2937", "#000000"],
                },
            }
            body = json.dumps(manifest, indent=2).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        if self.path == "/icon.svg":
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

        if self.path == "/reports":
            rows = []
            if REPORTS_PATH.exists():
                for line in REPORTS_PATH.read_text(encoding="utf-8").splitlines():
                    if line.strip():
                        try:
                            rows.append(json.loads(line))
                        except Exception:
                            pass
            body = json.dumps({"count": len(rows), "reports": rows}, indent=2).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        body = PROBE_HTML.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8", errors="replace")
        try:
            data = json.loads(raw) if raw.strip() else {}
        except Exception:
            data = {"raw": raw}
        data["_client_ip"] = self.client_address[0]
        data["_received_at"] = datetime.now(timezone.utc).isoformat()
        with REPORTS_PATH.open("a", encoding="utf-8") as f:
            f.write(json.dumps(data) + "\n")
        resp = b'{"ok":true}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(resp)))
        self.end_headers()
        self.wfile.write(resp)

    def log_message(self, fmt, *args):
        pass


if __name__ == "__main__":
    import ssl
    import threading
    https_server = ThreadingHTTPServer(("0.0.0.0", 8443), ProbeHandler)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(certfile="/tmp/probe_cert.pem", keyfile="/tmp/probe_key.pem")
    https_server.socket = ctx.wrap_socket(https_server.socket, server_side=True)
    threading.Thread(target=https_server.serve_forever, daemon=True).start()
    print("WebAuthn Probe HTTPS listening on https://0.0.0.0:8443", flush=True)
    server = ThreadingHTTPServer(("0.0.0.0", 8799), ProbeHandler)
    print("WebAuthn Probe HTTP listening on http://0.0.0.0:8799", flush=True)
    server.serve_forever()
