#!/usr/bin/env python3
"""Meta Ray-Ban Display Authorization Modal Workshop & WebAuthn Capability Probe."""

import json
import ssl
import threading
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

REPORTS_PATH = Path("/tmp/webauthn_probe_reports.jsonl")

PROBE_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=600, initial-scale=1, maximum-scale=1, user-scalable=no" />
  <title>Walkaway — Meta Display Authorization Modals</title>
  <style>
    * { box-sizing: border-box; user-select: none; -webkit-user-select: none; }
    html, body {
      margin: 0; padding: 0;
      width: 100%; height: 100%;
      overflow: hidden;
      background: #000000;
      color: #F2F4F8;
      font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", "Segoe UI", Roboto, sans-serif;
    }
    .stage {
      width: 600px; height: 600px;
      margin: 0 auto;
      padding: 28px;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
      background: #000000;
    }

    /* Top System Pill Bar (matches Threads / WhatsApp top header pill) */
    .top-bar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 6px;
      height: 44px;
    }
    .brand-pill {
      display: inline-flex;
      align-items: center;
      gap: 10px;
      background: #282C34;
      border: 1.5px solid #58606E;
      border-radius: 999px;
      padding: 7px 18px;
      font-size: 18px;
      font-weight: 700;
      letter-spacing: -0.01em;
      color: #F2F4F8;
    }
    .brand-dot {
      width: 12px; height: 12px;
      border-radius: 50%;
      background: #10B981;
      box-shadow: 0 0 8px #10B981;
    }
    .mode-switcher {
      display: inline-flex;
      gap: 8px;
    }
    .tab-pill {
      background: #232730;
      border: 1.5px solid #484F5C;
      color: #AEB6C4;
      font-size: 16px;
      font-weight: 700;
      padding: 7px 14px;
      border-radius: 999px;
      cursor: pointer;
      outline: none;
    }
    .tab-pill.active-tab {
      background: #363B46;
      color: #FFFFFF;
      border-color: #8B94A4;
    }

    /* Main Framed Modal Card (Threads / Messenger Spec: #383D47 outer frame + #262A32 inner card) */
    .modal-outer {
      background: #383D47;
      border: 2px solid #7E8796;
      border-radius: 34px;
      padding: 8px;
      flex: 1;
      margin: 14px 0;
      display: flex;
      flex-direction: column;
    }
    .modal-inner {
      background: #262A32;
      border-radius: 28px;
      padding: 26px 28px;
      flex: 1;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
    }

    /* Elevated Header Inlay (like NBA Threads Schedule/Trending inlay card) */
    .header-inlay {
      background: #343944;
      border: 1.5px solid #5A6272;
      border-radius: 22px;
      padding: 16px 22px;
      display: flex;
      align-items: center;
      gap: 14px;
    }
    .header-icon {
      width: 42px; height: 42px;
      border-radius: 50%;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 22px;
      font-weight: 800;
      flex-shrink: 0;
    }
    .icon-passkey { background: #1D4ED8; color: #FFFFFF; border: 1.5px solid #93C5FD; }
    .icon-alert { background: #B45309; color: #FEF3C7; border: 1.5px solid #FCD34D; }
    .icon-batch { background: #047857; color: #D1FAE5; border: 1.5px solid #6EE7B7; }
    .icon-probe { background: #4338CA; color: #E0E7FF; border: 1.5px solid #A5B4FC; }

    .header-titles {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }
    .header-eyebrow {
      font-size: 16px;
      font-weight: 700;
      color: #9CA6B8;
      letter-spacing: 0.02em;
    }
    .header-question {
      font-size: 25px;
      font-weight: 800;
      color: #FFFFFF;
      line-height: 1.18;
      letter-spacing: -0.02em;
    }

    /* Middle Content Surface (Max 3-4 clean lines, 22px bold typography) */
    .modal-body {
      padding: 6px 4px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    .info-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      background: #2F343E;
      border: 1px solid #454C59;
      border-radius: 16px;
      padding: 12px 18px;
      font-size: 20px;
      font-weight: 600;
    }
    .info-label { color: #A6B0C2; }
    .info-val { color: #FFFFFF; font-family: ui-monospace, "SF Mono", Menlo, monospace; font-weight: 700; }
    .badge-amber {
      background: #D97706;
      color: #111827;
      font-size: 17px;
      font-weight: 800;
      padding: 3px 10px;
      border-radius: 8px;
    }
    .badge-green {
      background: #10B981;
      color: #06281E;
      font-size: 17px;
      font-weight: 800;
      padding: 3px 10px;
      border-radius: 8px;
    }
    .badge-red {
      background: #EF4444;
      color: #FFFFFF;
      font-size: 17px;
      font-weight: 800;
      padding: 3px 10px;
      border-radius: 8px;
    }

    /* Worktree diff blocks row */
    .wt-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      background: #2F343E;
      border: 1px solid #454C59;
      border-radius: 16px;
      padding: 10px 18px;
      font-family: ui-monospace, "SF Mono", Menlo, monospace;
      font-size: 20px;
      font-weight: 700;
    }
    .wt-name { color: #F2F4F8; width: 110px; }
    .wt-add { color: #3FD46B; width: 74px; text-align: right; }
    .wt-del { color: #F85959; width: 74px; text-align: right; }
    .wt-blocks { display: flex; gap: 6px; }
    .blk { width: 22px; height: 18px; border-radius: 5px; }
    .blk.g { background: #2EB857; }
    .blk.r { background: #E53E3E; }
    .blk.n { background: #454C5A; }

    /* Bottom Action Pill Bar (Side-by-Side Threads/WhatsApp Pill Spec) */
    .action-row {
      display: flex;
      flex-direction: row;
      gap: 14px;
      align-items: center;
      justify-content: space-between;
    }
    .pill-btn {
      flex: 1;
      height: 60px;
      border-radius: 30px;
      border: 2px solid #5A6272;
      background: #343944;
      color: #F2F4F8;
      font-size: 22px;
      font-weight: 800;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 10px;
      cursor: pointer;
      outline: none;
      transition: transform 0.08s ease;
    }
    .pill-btn.primary {
      background: #1877F2;
      border-color: #79B8FF;
      color: #FFFFFF;
    }
    .pill-btn.danger {
      background: #7D242B;
      border-color: #F87171;
      color: #FFE4E6;
    }
    .pill-btn.watch {
      background: #363B47;
      border-color: #F5C242;
      color: #FFE484;
    }

    /* Crisp White Focus Ring (Threads / WhatsApp Focus Spec) */
    .focus-target.focused {
      border: 3px solid #FFFFFF !important;
      box-shadow: 0 0 0 3px rgba(255, 255, 255, 0.28), inset 0 0 12px rgba(255, 255, 255, 0.18) !important;
      transform: scale(1.02);
    }

    /* Status Footer Toast */
    .footer-bar {
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 10px;
      font-size: 16px;
      color: #9CA6B8;
      font-weight: 600;
    }
    .install-link {
      color: #F472B6;
      text-decoration: none;
      font-weight: 700;
    }
  </style>
</head>
<body>
  <div class="stage">
    <div class="top-bar">
      <div class="brand-pill">
        <span class="brand-dot"></span>
        <span id="card-counter">Walkaway • 1/4</span>
      </div>
      <div class="mode-switcher">
        <button type="button" class="tab-pill focus-target" id="tab-prev" data-action="prev-card">◀ Prev</button>
        <button type="button" class="tab-pill focus-target" id="tab-next" data-action="next-card">Next ▶</button>
      </div>
    </div>

    <div class="modal-outer">
      <div class="modal-inner" id="modal-container">
        <!-- Rendered dynamically by selectCard() -->
      </div>
    </div>

    <div class="footer-bar">
      <span id="status-toast">Swipe ◀ ▶ to switch buttons • Tap to select</span>
      <a class="install-link" id="install-link" href="#">Install on Glasses</a>
    </div>
  </div>

  <script>
    (function() {
      var probeData = {
        uvpaa: null,
        pkc: typeof window.PublicKeyCredential,
        navCreds: typeof navigator.credentials,
        lastError: 'NotSupportedError (Greatwhite wv)'
      };

      var CARDS = [
        {
          id: 'passkey-assert',
          eyebrow: 'WEBAUTHN PASSKEY • 127.0.0.1',
          question: 'Sign in with Passkey?',
          iconClass: 'icon-passkey',
          iconSymbol: '🔑',
          rows: [
            { type: 'kv', label: 'Account', val: 'ryan@zunftmax.build' },
            { type: 'kv', label: 'Key Type', val: 'ES256 (P-256) • UV', badge: 'VERIFIED', badgeClass: 'badge-green' }
          ],
          buttons: [
            { id: 'btn-allow', label: '✓ Allow', cls: 'primary', action: 'approve-passkey' },
            { id: 'btn-deny', label: '✕ Deny', cls: 'danger', action: 'deny-passkey' }
          ],
          defaultButtonIdx: 0
        },
        {
          id: 'cascading-failure',
          eyebrow: 'SUPERVISOR ALERT • 3 WORKTREES',
          question: 'Cascading failure — Take over?',
          iconClass: 'icon-alert',
          iconSymbol: '⚠',
          rows: [
            { type: 'wt', name: 'wt-auth', add: '+842', del: '-619', blocks: ['g','g','g','r','r'] },
            { type: 'wt', name: 'wt-core', add: '+514', del: '-498', blocks: ['g','g','r','r','r'] },
            { type: 'wt', name: 'main',    add: '+42',  del: '-380', blocks: ['g','r','r','r','r'] }
          ],
          buttons: [
            { id: 'btn-yes', label: 'Yes (5s)', cls: 'primary', action: 'approve-takeover' },
            { id: 'btn-no', label: 'No', cls: 'danger', action: 'reject-takeover' },
            { id: 'btn-watch', label: '✦ Watch', cls: 'watch', action: 'watch-takeover' }
          ],
          defaultButtonIdx: 0
        },
        {
          id: 'demo-ready',
          eyebrow: 'WALKAWAY LIVE DEMO • GLASSES',
          question: 'Ready to start the live demo?',
          iconClass: 'icon-batch',
          iconSymbol: '▶',
          rows: [
            { type: 'kv', label: 'Portals', val: '5 Conference CFPs' },
            { type: 'kv', label: 'Default', val: 'No (Safe Hold)', badge: 'HOLD', badgeClass: 'badge-amber' }
          ],
          buttons: [
            { id: 'btn-demo-yes', label: '✓ Yes', cls: 'primary', action: 'demo-yes' },
            { id: 'btn-demo-no', label: '✕ No', cls: 'danger', action: 'demo-no' }
          ],
          defaultButtonIdx: 1
        },
        {
          id: 'webauthn-probe',
          eyebrow: 'GLASSES BROWSER CAPABILITY PROBE',
          question: 'window.credentials Status',
          iconClass: 'icon-probe',
          iconSymbol: '⚙',
          rows: [
            { type: 'kv', label: 'navigator.credentials', val: probeData.navCreds, badge: 'OBJECT', badgeClass: 'badge-green' },
            { type: 'kv', label: 'PublicKeyCredential', val: probeData.pkc, badge: 'BLOCKED', badgeClass: 'badge-red' },
            { type: 'kv', label: 'create() Result', val: probeData.lastError }
          ],
          buttons: [
            { id: 'btn-probe-create', label: 'Test create()', cls: 'primary', action: 'test-create' },
            { id: 'btn-probe-get', label: 'Test get()', cls: 'watch', action: 'test-get' }
          ],
          defaultButtonIdx: 0
        }
      ];

      var currentCardIdx = 0;
      var focusList = [];
      var focusIdx = 0;

      function renderCard(idx) {
        currentCardIdx = (idx + CARDS.length) % CARDS.length;
        var card = CARDS[currentCardIdx];
        document.getElementById('card-counter').textContent = 'Walkaway • ' + (currentCardIdx + 1) + '/' + CARDS.length;

        var rowsHtml = '';
        for (var i = 0; i < card.rows.length; i++) {
          var r = card.rows[i];
          if (r.type === 'wt') {
            var blks = '';
            for (var b = 0; b < r.blocks.length; b++) {
              blks += '<span class="blk ' + r.blocks[b] + '"></span>';
            }
            rowsHtml += '<div class="wt-row">' +
              '<span class="wt-name">' + r.name + '</span>' +
              '<span class="wt-add">' + r.add + '</span>' +
              '<span class="wt-del">' + r.del + '</span>' +
              '<span class="wt-blocks">' + blks + '</span>' +
            '</div>';
          } else {
            var badgeHtml = r.badge ? '<span class="' + r.badgeClass + '">' + r.badge + '</span>' : '<span class="info-val">' + r.val + '</span>';
            rowsHtml += '<div class="info-row">' +
              '<span class="info-label">' + r.label + '</span>' +
              badgeHtml +
            '</div>';
          }
        }

        var btnsHtml = '';
        for (var j = 0; j < card.buttons.length; j++) {
          var btn = card.buttons[j];
          btnsHtml += '<button type="button" class="pill-btn ' + btn.cls + ' focus-target" id="' + btn.id + '" data-action="' + btn.action + '">' + btn.label + '</button>';
        }

        var html =
          '<div class="header-inlay">' +
            '<div class="header-icon ' + card.iconClass + '">' + card.iconSymbol + '</div>' +
            '<div class="header-titles">' +
              '<div class="header-eyebrow">' + card.eyebrow + '</div>' +
              '<div class="header-question">' + card.question + '</div>' +
            '</div>' +
          '</div>' +
          '<div class="modal-body">' + rowsHtml + '</div>' +
          '<div class="action-row">' + btnsHtml + '</div>';

        var container = document.getElementById('modal-container');
        container.innerHTML = html;

        // Rebuild 1D focus ring: action buttons first, then Prev/Next card switcher
        var actionButtons = Array.prototype.slice.call(container.querySelectorAll('.focus-target'));
        var navButtons = [
          document.getElementById('tab-prev'),
          document.getElementById('tab-next')
        ];
        focusList = actionButtons.concat(navButtons);

        for (var k = 0; k < focusList.length; k++) {
          (function(el, index) {
            el.addEventListener('click', function() {
              setFocus(index);
              handleAction(el.getAttribute('data-action'));
            });
          })(focusList[k], k);
        }

        setFocus(card.defaultButtonIdx || 0);
      }

      function setFocus(idx) {
        if (!focusList.length) return;
        focusIdx = (idx + focusList.length) % focusList.length;
        for (var i = 0; i < focusList.length; i++) {
          if (i === focusIdx) {
            focusList[i].classList.add('focused');
            focusList[i].focus();
          } else {
            focusList[i].classList.remove('focused');
          }
        }
      }

      function postEvent(actionName, extra) {
        try {
          fetch('/report', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              ts: new Date().toISOString(),
              card: CARDS[currentCardIdx].id,
              action: actionName,
              extra: extra || null,
              userAgent: String(navigator.userAgent || ''),
              PublicKeyCredential: typeof window.PublicKeyCredential
            })
          }).catch(function() {});
        } catch (e) {}
      }

      async function handleAction(action) {
        var toast = document.getElementById('status-toast');
        if (action === 'prev-card') {
          renderCard(currentCardIdx - 1);
          return;
        }
        if (action === 'next-card') {
          renderCard(currentCardIdx + 1);
          return;
        }
        if (action === 'test-create') {
          try {
            var chal = new Uint8Array(32);
            var cred = await navigator.credentials.create({
              publicKey: {
                rp: { name: 'Walkaway Probe' },
                user: { id: new Uint8Array([1,2,3,4]), name: 'ryan@walkaway.dev', displayName: 'Ryan' },
                challenge: chal,
                pubKeyCredParams: [{ type: 'public-key', alg: -7 }]
              }
            });
            probeData.lastError = 'Created: ' + (cred ? cred.id : 'ok');
          } catch (e) {
            probeData.lastError = (e && e.name ? e.name + ': ' + e.message : String(e));
          }
          CARDS[3].rows[2].val = probeData.lastError;
          renderCard(3);
          toast.textContent = probeData.lastError;
          postEvent('test-create', probeData.lastError);
          return;
        }
        if (action === 'test-get') {
          try {
            var chal2 = new Uint8Array(32);
            var cred2 = await navigator.credentials.get({
              publicKey: { challenge: chal2 }
            });
            probeData.lastError = 'Asserted: ' + (cred2 ? cred2.id : 'ok');
          } catch (e) {
            probeData.lastError = (e && e.name ? e.name + ': ' + e.message : String(e));
          }
          CARDS[3].rows[2].val = probeData.lastError;
          renderCard(3);
          toast.textContent = probeData.lastError;
          postEvent('test-get', probeData.lastError);
          return;
        }

        toast.textContent = 'Selected: ' + action + ' ( advancing... )';
        postEvent(action);
        setTimeout(function() {
          renderCard(currentCardIdx + 1);
        }, 700);
      }

      window.addEventListener('keydown', function(e) {
        if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
          e.preventDefault();
          setFocus(focusIdx + 1);
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
          e.preventDefault();
          setFocus(focusIdx - 1);
        } else if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          if (focusList[focusIdx]) {
            handleAction(focusList[focusIdx].getAttribute('data-action'));
          }
        }
      });

      renderCard(0);
      var inst = document.getElementById('install-link');
      if (inst) {
        inst.href = 'fb-viewapp://web_app_deep_link?appName=' + encodeURIComponent('Walkaway Probe') + '&appUrl=' + encodeURIComponent(window.location.origin + '/');
      }
      postEvent('loaded');
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
                "description": "Meta Ray-Ban Display Authorization Modals & WebAuthn Probe",
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
                    "gradient": ["#262A32", "#000000"],
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
                for line in REPORTS_PATH.read_text(encoding="utf-8").splitlines()[-25:]:
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
    https_server = ThreadingHTTPServer(("0.0.0.0", 8443), ProbeHandler)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(certfile="/tmp/probe_cert.pem", keyfile="/tmp/probe_key.pem")
    https_server.socket = ctx.wrap_socket(https_server.socket, server_side=True)
    threading.Thread(target=https_server.serve_forever, daemon=True).start()
    print("WebAuthn Probe HTTPS listening on https://0.0.0.0:8443", flush=True)
    server = ThreadingHTTPServer(("0.0.0.0", 8799), ProbeHandler)
    print("WebAuthn Probe HTTP listening on http://0.0.0.0:8799", flush=True)
    server.serve_forever()
