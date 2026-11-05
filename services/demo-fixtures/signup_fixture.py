#!/usr/bin/env python3
"""Canonical localhost signup, 10-portal CFP suite, Disco workbench, WebAuthn RP verifier, and sign-in fixture backed by SQLite.

Serves:
- GET /signup, POST /signup, GET /api/accounts (Slice 1: Account Creation + Blind Vault Commit)
- GET /cfp-socon, POST /cfp-socon, GET /api/cfp_submissions (Slice 2: Interactive Validation + Self-Correction)
- GET /cfp/<1..10>, POST /cfp/<1..10> (Zone Y: 10 Real SQLite Conference Portals with shared FormSignature)
- GET /disco-workbench, GET /api/workbench/state, POST /api/workbench/state (Zone Y: 10-Viewport Disco Workbench)
- GET /api/webauthn/challenge, POST /api/webauthn/register, POST /api/webauthn/verify, GET /api/webauthn/assertions (Real ECDSA P-256 WebAuthn Verification)
- GET /campaign-receipt, GET /api/campaign_receipt, POST /api/campaign_receipt (Clean Final Campaign Receipt)
- POST /api/reset (Deterministic <2s SQLite & Workbench Reset)
- GET /signin, POST /signin, GET /api/logins (Slice 3: Zero-LLM FormRecipe Recall + Instant Sign-In)
"""

import argparse
import base64
import contextlib
import hashlib
import html
import json
import os
import sqlite3
import subprocess
import tempfile
import threading
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

CONFERENCE_PORTALS: list[dict[str, object]] = [
    {"portal_id": 1, "conference_name": "SoCon 2026", "rp_id": "socon2026.local"},
    {"portal_id": 2, "conference_name": "KubeCon NA", "rp_id": "kubecon.local"},
    {"portal_id": 3, "conference_name": "BlackHat USA", "rp_id": "blackhat.local"},
    {"portal_id": 4, "conference_name": "RustConf", "rp_id": "rustconf.local"},
    {"portal_id": 5, "conference_name": "NeurIPS Systems", "rp_id": "neurips.local"},
    {"portal_id": 6, "conference_name": "DEF CON AI", "rp_id": "defcon.local"},
    {"portal_id": 7, "conference_name": "USENIX Security", "rp_id": "usenix.local"},
    {"portal_id": 8, "conference_name": "AI Engineer Fair", "rp_id": "aiengineer.local"},
    {"portal_id": 9, "conference_name": "PyCon US", "rp_id": "pycon.local"},
    {"portal_id": 10, "conference_name": "QCon SF", "rp_id": "qcon.local"},
]

PORTALS_BY_ID: dict[int, dict[str, object]] = {
    int(p["portal_id"]): p for p in CONFERENCE_PORTALS
}

STATUS_BADGE_MAP: dict[str, tuple[str, str, str]] = {
    "PENDING": ("PENDING", "#21262d", "#8b949e"),
    "REPAIRING": ("QWEN3-4B ABSTRACT REPAIR", "#5a1e02", "#ffa657"),
    "REPAIRED": ("QWEN3-4B ABSTRACT REPAIR", "#5a1e02", "#ffa657"),
    "QWEN3-4B ABSTRACT REPAIR": ("QWEN3-4B ABSTRACT REPAIR", "#5a1e02", "#ffa657"),
    "RECALLED": ("RECIPE RECALLED • 0 LLM tokens", "#0d419d", "#79c0ff"),
    "RECIPE RECALLED": ("RECIPE RECALLED • 0 LLM tokens", "#0d419d", "#79c0ff"),
    "RECIPE RECALLED • 0 LLM tokens": ("RECIPE RECALLED • 0 LLM tokens", "#0d419d", "#79c0ff"),
    "STAGED": ("FROZEN FOR GLASSES TAP", "#4b2900", "#f8e3a1"),
    "FROZEN": ("FROZEN FOR GLASSES TAP", "#4b2900", "#f8e3a1"),
    "FROZEN FOR GLASSES TAP": ("FROZEN FOR GLASSES TAP", "#4b2900", "#f8e3a1"),
    "COMMITTED": ("SUBMITTED", "#115822", "#7ee787"),
    "SUBMITTED": ("SUBMITTED", "#115822", "#7ee787"),
}


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def _b64decode_any(val: str) -> bytes:
    cleaned = val.strip()
    pad = "=" * ((4 - len(cleaned) % 4) % 4)
    if "-" in cleaned or "_" in cleaned:
        return base64.urlsafe_b64decode(cleaned + pad)
    return base64.b64decode(cleaned + pad)


def _seed_workbench_tables(conn: sqlite3.Connection) -> None:
    now = _now_iso()
    for portal in CONFERENCE_PORTALS:
        conn.execute(
            """
            INSERT OR IGNORE INTO workbench_tiles (
                portal_id, conference_name, rp_id, status, form_signature,
                matched_recipe_id, layout_observe_ms, recall_ms, llm_calls,
                proposal_id, fingerprint, commit_state, screenshot_path,
                screenshot_b64, updated_at
            ) VALUES (?, ?, ?, 'PENDING', '', '', 0.0, 0.0, 0, '', '', 'IDLE', '', '', ?)
            """,
            (
                int(portal["portal_id"]),
                str(portal["conference_name"]),
                str(portal["rp_id"]),
                now,
            ),
        )
    conn.execute(
        """
        INSERT OR IGNORE INTO campaign_meta (
            id, session_id, total_llm_calls, total_recall_ms,
            batch_status, receipt_proposals_json, updated_at
        ) VALUES (1, '', 0, 0.0, 'IDLE', '[]', ?)
        """,
        (now,),
    )


def init_db(db_path: Path) -> None:
    db_path.parent.mkdir(parents=True, exist_ok=True)
    with contextlib.closing(sqlite3.connect(db_path)) as conn:
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                speaker_name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE,
                talk_title TEXT NOT NULL,
                track TEXT NOT NULL,
                live_demo TEXT NOT NULL,
                password_sha256 TEXT NOT NULL,
                password_len INTEGER NOT NULL,
                confirmation_ref TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS cfp_submissions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                portal_id INTEGER NOT NULL DEFAULT 1,
                conference_name TEXT NOT NULL DEFAULT 'SoCon 2026',
                speaker_name TEXT NOT NULL DEFAULT '',
                email TEXT NOT NULL DEFAULT '',
                talk_title TEXT NOT NULL,
                track TEXT NOT NULL DEFAULT 'systems',
                tshirt_size TEXT NOT NULL,
                abstract_summary TEXT NOT NULL,
                live_demo TEXT NOT NULL DEFAULT 'no',
                confirmation_ref TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            )
            """
        )
        for alter_sql in (
            "ALTER TABLE cfp_submissions ADD COLUMN portal_id INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE cfp_submissions ADD COLUMN conference_name TEXT NOT NULL DEFAULT 'SoCon 2026'",
            "ALTER TABLE cfp_submissions ADD COLUMN track TEXT NOT NULL DEFAULT 'systems'",
            "ALTER TABLE cfp_submissions ADD COLUMN live_demo TEXT NOT NULL DEFAULT 'no'",
        ):
            with contextlib.suppress(sqlite3.OperationalError):
                conn.execute(alter_sql)

        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS logins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT NOT NULL,
                account_id INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS webauthn_challenges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                challenge TEXT NOT NULL UNIQUE,
                rp_id TEXT NOT NULL,
                used INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS webauthn_credentials (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                credential_id TEXT NOT NULL UNIQUE,
                public_key_pem TEXT NOT NULL,
                rp_id TEXT NOT NULL,
                user_email TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS webauthn_assertions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                credential_id TEXT NOT NULL,
                rp_id TEXT NOT NULL,
                origin TEXT NOT NULL,
                challenge TEXT NOT NULL,
                sign_count INTEGER NOT NULL DEFAULT 0,
                verified INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS workbench_tiles (
                portal_id INTEGER PRIMARY KEY,
                conference_name TEXT NOT NULL,
                rp_id TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'PENDING',
                form_signature TEXT NOT NULL DEFAULT '',
                matched_recipe_id TEXT NOT NULL DEFAULT '',
                layout_observe_ms REAL NOT NULL DEFAULT 0.0,
                recall_ms REAL NOT NULL DEFAULT 0.0,
                llm_calls INTEGER NOT NULL DEFAULT 0,
                proposal_id TEXT NOT NULL DEFAULT '',
                fingerprint TEXT NOT NULL DEFAULT '',
                commit_state TEXT NOT NULL DEFAULT 'IDLE',
                screenshot_path TEXT NOT NULL DEFAULT '',
                screenshot_b64 TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT ''
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS campaign_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                session_id TEXT NOT NULL DEFAULT '',
                total_llm_calls INTEGER NOT NULL DEFAULT 0,
                total_recall_ms REAL NOT NULL DEFAULT 0.0,
                batch_status TEXT NOT NULL DEFAULT 'IDLE',
                receipt_proposals_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL DEFAULT ''
            )
            """
        )
        _seed_workbench_tables(conn)
        conn.commit()


def reset_db(db_path: Path) -> None:
    with contextlib.closing(sqlite3.connect(db_path)) as conn:
        for table in (
            "accounts",
            "cfp_submissions",
            "logins",
            "webauthn_challenges",
            "webauthn_credentials",
            "webauthn_assertions",
            "workbench_tiles",
            "campaign_meta",
        ):
            conn.execute(f"DELETE FROM {table}")
        with contextlib.suppress(sqlite3.OperationalError):
            conn.execute("DELETE FROM sqlite_sequence")
        _seed_workbench_tables(conn)
        conn.commit()


SIGNUP_HTML = """<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>DeepSec 2026 Speaker Portal — Create Account &amp; Submit CFP</title>
  <style>
    body { font-family: sans-serif; background: #0f141c; color: #e6edf3; padding: 24px; }
    .card { max-width: 640px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 20px; }
    h1 { font-size: 20px; margin-top: 0; }
    .field { margin-bottom: 14px; }
    label { display: block; font-size: 13px; margin-bottom: 4px; color: #8b949e; }
    input[type="text"], input[type="email"], input[type="password"], select, textarea {
      width: 100%; padding: 8px; background: #0d1117; color: #e6edf3; border: 1px solid #30363d; border-radius: 4px;
    }
    .btn { padding: 10px 16px; background: #238636; color: #ffffff; border: none; border-radius: 6px; font-weight: bold; }
  </style>
</head>
<body>
  <div class="card">
    <h1>DeepSec 2026 Speaker Portal</h1>
    <p id="portal-status">Create your speaker account and submit your 15-minute CFP proposal.</p>
    <form id="signup-form" method="POST" action="/signup">
      <div class="field">
        <label for="speaker_name">Speaker Full Name</label>
        <input id="speaker_name" name="speaker_name" type="text" required placeholder="Ryan MacArthur" />
      </div>
      <div class="field">
        <label for="email">Speaker Email</label>
        <input id="email" name="email" type="email" autocomplete="username" required placeholder="ryan@zunftmax.secure.build" />
      </div>
      <div class="field">
        <label for="password">Portal Password</label>
        <input id="password" name="password" type="password" autocomplete="new-password" minlength="12" required />
      </div>
      <div class="field">
        <label for="talk_title">Talk Title</label>
        <input id="talk_title" name="talk_title" type="text" required placeholder="Containment-First Sandboxing" />
      </div>
      <div class="field">
        <label for="track">Conference Track</label>
        <select id="track" name="track" required>
          <option value="">Select track</option>
          <option value="sandboxing">Sandboxing &amp; Virtualization</option>
          <option value="kernel">Kernel &amp; Hypervisor Security</option>
          <option value="appsec">Application Security</option>
        </select>
      </div>
      <div class="field">
        <label for="live_demo">
          <input id="live_demo" name="live_demo" type="checkbox" value="yes" />
          Includes 15-minute live exploit/containment demonstration
        </label>
      </div>
      <button id="submit-btn" type="submit" class="btn">Create Account &amp; Submit CFP</button>
    </form>
  </div>
</body>
</html>
"""


def render_socon_cfp_html(
    error_msg: str = "",
    speaker_name: str = "",
    email: str = "",
    talk_title: str = "",
    abstract_summary: str = "",
    tshirt_size: str = "",
) -> str:
    err_block = (
        f'<div id="validation-error" role="alert" style="background:#3d161b;border:1px solid #f85149;color:#ffa198;padding:10px;border-radius:6px;margin-bottom:14px;">'
        f"Validation Error: {html.escape(error_msg)}</div>"
        if error_msg
        else ""
    )
    opts = []
    for val, label in [
        ("", "Select T-shirt size"),
        ("S", "Small (S)"),
        ("M", "Medium (M)"),
        ("L", "Large (L)"),
        ("XL", "Extra Large (XL)"),
    ]:
        sel = ' selected="selected"' if val == tshirt_size and val != "" else ""
        opts.append(f'<option value="{val}"{sel}>{label}</option>')
    options_html = "\n          ".join(opts)
    return f"""<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>SO-CON 2027 CFP Portal</title>
  <style>
    body {{ font-family: sans-serif; background: #0f141c; color: #e6edf3; padding: 20px; }}
    .card {{ max-width: 640px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 18px; }}
    h1 {{ font-size: 19px; margin-top: 0; }}
    .field {{ margin-bottom: 12px; }}
    label {{ display: block; font-size: 13px; margin-bottom: 4px; color: #8b949e; }}
    input[type="text"], input[type="email"], select, textarea {{
      width: 100%; padding: 7px; background: #0d1117; color: #e6edf3; border: 1px solid #30363d; border-radius: 4px;
    }}
    .btn {{ padding: 9px 15px; background: #1f6feb; color: #ffffff; border: none; border-radius: 6px; font-weight: bold; }}
  </style>
</head>
<body>
  <div class="card">
    <h1>SO-CON 2027 Call for Papers</h1>
    {err_block}
    <form id="socon-cfp-form" method="POST" action="/cfp-socon">
      <div class="field">
        <label for="speaker_name">Speaker Full Name</label>
        <input id="speaker_name" name="speaker_name" type="text" value="{html.escape(speaker_name)}" required />
      </div>
      <div class="field">
        <label for="email">Speaker Email</label>
        <input id="email" name="email" type="email" value="{html.escape(email)}" required />
      </div>
      <div class="field">
        <label for="talk_title">Talk Title</label>
        <input id="talk_title" name="talk_title" type="text" value="{html.escape(talk_title)}" required />
      </div>
      <div class="field">
        <label for="abstract_summary">Talk Abstract (min 40 chars)</label>
        <textarea id="abstract_summary" name="abstract_summary" rows="3">{html.escape(abstract_summary)}</textarea>
      </div>
      <div class="field">
        <label for="tshirt_size">Speaker T-Shirt Size (Required for Logistics)</label>
        <select id="tshirt_size" name="tshirt_size" required>
          {options_html}
        </select>
      </div>
      <button id="submit-cfp" type="submit" class="btn">Submit SO-CON 2027 Proposal</button>
    </form>
  </div>
</body>
</html>"""


def render_portal_cfp_html(
    portal_id: int,
    conference_name: str,
    rp_id: str,
    error_html: str = "",
    talk_title: str = "",
    track: str = "",
    tshirt_size: str = "",
    abstract_summary: str = "",
    live_demo: str = "",
) -> str:
    """Renders /cfp/<1..10> with the exact 5 form controls required for identical FormSignature."""
    track_opts = []
    for val, label in [
        ("", "Select track"),
        ("ai_agents", "AI Agents & Autonomous Systems"),
        ("systems", "Systems & Sandboxing"),
        ("security", "Security & Containment"),
    ]:
        sel = ' selected="selected"' if val == track and val != "" else ""
        track_opts.append(f'<option value="{val}"{sel}>{html.escape(label)}</option>')
    track_options_html = "\n          ".join(track_opts)

    size_opts = []
    for val, label in [
        ("", "Select T-shirt size"),
        ("S", "Small (S)"),
        ("M", "Medium (M)"),
        ("L", "Large (L)"),
        ("XL", "Extra Large (XL)"),
    ]:
        sel = ' selected="selected"' if val == tshirt_size and val != "" else ""
        size_opts.append(f'<option value="{val}"{sel}>{label}</option>')
    size_options_html = "\n          ".join(size_opts)

    checked_attr = (
        ' checked="checked"'
        if str(live_demo).lower() in {"yes", "true", "1", "on", "checked"}
        else ""
    )

    return f"""<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>{html.escape(conference_name)} — CFP Portal #{portal_id}</title>
  <style>
    body {{ font-family: sans-serif; background: #0b0f17; color: #f0f6fc; padding: 18px; margin: 0; }}
    .card {{ max-width: 640px; background: #161b22; border: 2px solid #30363d; border-radius: 8px; padding: 18px; }}
    .badge {{ display: inline-block; padding: 3px 8px; background: #1f6feb; color: #ffffff; border-radius: 4px; font-size: 12px; font-weight: bold; margin-bottom: 6px; }}
    h1 {{ font-size: 20px; margin: 4px 0 12px 0; color: #ffffff; }}
    .field {{ margin-bottom: 12px; }}
    label {{ display: block; font-size: 13px; font-weight: bold; margin-bottom: 4px; color: #c9d1d9; }}
    input[type="text"], select, textarea {{
      width: 100%; padding: 8px; background: #0d1117; color: #f0f6fc; border: 1px solid #30363d; border-radius: 4px; font-size: 14px; box-sizing: border-box;
    }}
    .btn {{ padding: 10px 18px; background: #238636; color: #ffffff; border: none; border-radius: 6px; font-size: 14px; font-weight: bold; cursor: pointer; }}
  </style>
</head>
<body>
  <div class="card">
    <div class="badge">PORTAL #{portal_id} • RP {html.escape(rp_id)}</div>
    <h1 id="portal-heading">{html.escape(conference_name)} — Call for Papers</h1>
    {error_html}
    <form id="cfp-portal-form" method="POST" action="/cfp/{portal_id}">
      <div class="field">
        <label for="talk_title">Talk Title</label>
        <input id="talk_title" name="talk_title" type="text" value="{html.escape(talk_title)}" required />
      </div>
      <div class="field">
        <label for="track">Conference Track</label>
        <select id="track" name="track" required>
          {track_options_html}
        </select>
      </div>
      <div class="field">
        <label for="tshirt_size">Speaker T-Shirt Size (Required for Logistics)</label>
        <select id="tshirt_size" name="tshirt_size" required>
          {size_options_html}
        </select>
      </div>
      <div class="field">
        <label for="abstract_summary">Talk Abstract (min 40 characters)</label>
        <textarea id="abstract_summary" name="abstract_summary" rows="3">{html.escape(abstract_summary)}</textarea>
      </div>
      <div class="field">
        <label for="live_demo">
          <input id="live_demo" name="live_demo" type="checkbox" value="yes"{checked_attr} />
          Includes live containment demonstration
        </label>
      </div>
      <button id="submit-cfp" type="submit" class="btn">Submit CFP Proposal</button>
    </form>
  </div>
</body>
</html>"""


def render_disco_workbench_html(
    tiles: list[dict[str, object]],
    campaign: dict[str, object],
    cfp_count: int,
    webauthn_count: int,
) -> str:
    """Renders the 10-viewport Disco Workbench using Blitz-safe inline-block geometry."""
    total_llm = int(campaign.get("total_llm_calls") or 0)
    if total_llm == 0:
        total_llm = sum(int(t.get("llm_calls") or 0) for t in tiles)

    total_recall = float(campaign.get("total_recall_ms") or 0.0)
    if total_recall <= 0.0:
        total_recall = sum(float(t.get("recall_ms") or 0.0) for t in tiles)

    batch_status = str(campaign.get("batch_status") or "IDLE")
    session_id = str(campaign.get("session_id") or "")

    tile_blocks: list[str] = []
    for t in tiles:
        pid = int(t["portal_id"])
        conf_name = str(t["conference_name"])
        raw_status = str(t.get("status") or "PENDING").strip()
        badge_text, badge_bg, badge_fg = STATUS_BADGE_MAP.get(
            raw_status.upper(),
            STATUS_BADGE_MAP.get(raw_status, (raw_status, "#21262d", "#c9d1d9")),
        )
        border_color = "#30363d"
        if badge_text == "SUBMITTED":
            border_color = "#2ea043"
        elif badge_text == "FROZEN FOR GLASSES TAP":
            border_color = "#d29922"
        elif "RECIPE RECALLED" in badge_text:
            border_color = "#388bfd"
        elif "QWEN3-4B" in badge_text:
            border_color = "#f0883e"

        recall_ms = float(t.get("recall_ms") or 0.0)
        obs_ms = float(t.get("layout_observe_ms") or 0.0)
        sig = str(t.get("form_signature") or "")
        llm_calls = int(t.get("llm_calls") or 0)
        screenshot_b64 = str(t.get("screenshot_b64") or "")

        timing_parts: list[str] = []
        if recall_ms > 0.0:
            timing_parts.append(f"recall {recall_ms:.2f}ms")
        if obs_ms > 0.0:
            timing_parts.append(f"obs {obs_ms:.2f}ms")
        timing_str = " • ".join(timing_parts) if timing_parts else "0.00ms"
        sig_str = sig if sig else "—"

        if screenshot_b64:
            viewport_html = (
                f'<img id="tile-img-{pid}" src="data:image/png;base64,{screenshot_b64}" '
                f'width="180" height="44" alt="Portal {pid} snapshot" '
                f'style="display:block;width:180px;height:44px;margin-top:4px;border:1px solid #30363d;border-radius:3px;" />'
            )
        else:
            viewport_html = (
                f'<div id="tile-vp-{pid}" style="margin-top:4px;padding:3px 6px;background:#090d13;'
                f'border:1px solid #21262d;border-radius:3px;font-size:10px;color:#8b949e;">'
                f'sig: {html.escape(sig_str)} • LLM: {llm_calls}</div>'
            )

        tile_blocks.append(
            f"""<div id="tile-{pid}" role="tab" aria-label="Portal {pid} {html.escape(conf_name)} {html.escape(badge_text)}" style="display: inline-block; width: 220px; min-height: 115px; margin: 6px; padding: 10px; vertical-align: top; background: #161b22; border: 2px solid {border_color}; border-radius: 8px; box-sizing: border-box;">
      <div style="font-size: 13px; font-weight: bold; color: #ffffff; margin-bottom: 4px;">#{pid} {html.escape(conf_name)}</div>
      <div id="tile-status-{pid}" style="display: inline-block; padding: 2px 6px; background: {badge_bg}; color: {badge_fg}; border-radius: 4px; font-size: 10px; font-weight: bold; margin-bottom: 4px;">{html.escape(badge_text)}</div>
      <div id="tile-timing-{pid}" style="font-size: 11px; color: #c9d1d9;">Timing: {html.escape(timing_str)}</div>
      <div id="tile-sig-{pid}" style="font-size: 10px; color: #8b949e;">FormSig: {html.escape(sig_str)}</div>
      {viewport_html}
    </div>"""
        )

    tiles_html = "\n    ".join(tile_blocks)
    return f"""<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>Hermes Disco Workbench — 10-Portal Campaign</title>
  <style>
    body {{ font-family: sans-serif; background: #070a0f; color: #f0f6fc; margin: 0; padding: 10px; }}
    .topbar {{ background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 8px 12px; margin-bottom: 8px; }}
    .topbar-title {{ font-size: 15px; font-weight: bold; color: #ffffff; display: inline-block; margin-right: 16px; }}
    .kpi {{ display: inline-block; font-size: 12px; color: #79c0ff; margin-right: 14px; font-weight: bold; }}
    .grid {{ width: 100%; }}
  </style>
</head>
<body>
  <div class="topbar" id="workbench-header">
    <span class="topbar-title">HERMES • 10-PORTAL DISCO WORKBENCH</span>
    <span class="kpi" id="kpi-batch">State: {html.escape(batch_status)}</span>
    <span class="kpi" id="kpi-committed">Submitted: {cfp_count}/10</span>
    <span class="kpi" id="kpi-llm">LLM Calls: {total_llm}</span>
    <span class="kpi" id="kpi-recall">Recall Total: {total_recall:.2f}ms</span>
    <span class="kpi" id="kpi-webauthn">WebAuthn P-256: {webauthn_count}</span>
    <span class="kpi" id="kpi-session">Session: {html.escape(session_id or "—")}</span>
  </div>
  <div class="grid" id="workbench-grid">
    {tiles_html}
  </div>
</body>
</html>"""


def render_campaign_receipt_html(receipt: dict[str, object]) -> str:
    """Renders a clean high-contrast 1080p-legible HTML receipt with zero secrets."""
    session_id = str(receipt.get("session_id") or "—")
    account_id = receipt.get("account_id")
    account_id_str = str(account_id) if account_id is not None else "0"
    cfp_count = int(receipt.get("cfp_submissions_count") or 0)
    webauthn_count = int(receipt.get("webauthn_assertions_count") or 0)
    total_llm = int(receipt.get("total_llm_calls") or 0)
    total_recall_ms = float(receipt.get("total_recall_ms") or 0.0)
    submissions = list(receipt.get("submissions") or [])
    proposals = list(receipt.get("proposals") or [])

    sub_rows = []
    for s in submissions:
        sub_rows.append(
            f"<tr>"
            f"<td>#{int(s.get('portal_id') or 1)}</td>"
            f"<td>{html.escape(str(s.get('conference_name') or ''))}</td>"
            f"<td>{html.escape(str(s.get('talk_title') or ''))}</td>"
            f"<td>{html.escape(str(s.get('track') or ''))}</td>"
            f"<td>{html.escape(str(s.get('tshirt_size') or ''))}</td>"
            f"<td>{html.escape(str(s.get('confirmation_ref') or ''))}</td>"
            f"</tr>"
        )
    sub_table_html = "\n        ".join(sub_rows) if sub_rows else "<tr><td colspan='6'>No submissions yet</td></tr>"

    prop_rows = []
    for p in proposals:
        prop_rows.append(
            f"<tr>"
            f"<td>{html.escape(str(p.get('proposal_id') or ''))}</td>"
            f"<td>{html.escape(str(p.get('classification') or ''))}</td>"
            f"<td>{html.escape(str(p.get('before_fingerprint') or ''))}</td>"
            f"<td>{html.escape(str(p.get('after_fingerprint') or ''))}</td>"
            f"<td>{html.escape(str(p.get('frame_hash') or ''))}</td>"
            f"<td>{html.escape(str(p.get('decision_surface') or ''))}</td>"
            f"</tr>"
        )
    prop_table_html = "\n        ".join(prop_rows) if prop_rows else "<tr><td colspan='6'>No proposals recorded</td></tr>"

    return f"""<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>Hermes Supervised Campaign Receipt</title>
  <style>
    body {{ font-family: sans-serif; background: #070a0f; color: #f0f6fc; padding: 16px; margin: 0; }}
    .card {{ background: #161b22; border: 2px solid #2ea043; border-radius: 8px; padding: 16px; margin-bottom: 14px; }}
    h1 {{ font-size: 20px; margin: 0 0 10px 0; color: #7ee787; }}
    h2 {{ font-size: 15px; margin: 12px 0 6px 0; color: #79c0ff; }}
    .kpi-bar {{ margin-bottom: 10px; }}
    .kpi-box {{ display: inline-block; background: #0d1117; border: 1px solid #30363d; border-radius: 6px; padding: 6px 10px; margin-right: 8px; margin-bottom: 6px; font-size: 12px; font-weight: bold; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 12px; margin-top: 6px; }}
    th, td {{ border: 1px solid #30363d; padding: 5px 8px; text-align: left; }}
    th {{ background: #0d1117; color: #c9d1d9; }}
  </style>
</head>
<body>
  <div class="card" id="campaign-receipt-card">
    <h1 id="receipt-heading">HERMES SUPERVISED CAMPAIGN RECEIPT</h1>
    <div class="kpi-bar">
      <div class="kpi-box" id="receipt-session" role="button">Session: <span id="receipt-session-id">{html.escape(session_id)}</span></div>
      <div class="kpi-box" id="receipt-account" role="button">Account ID: <span id="receipt-account-id">{html.escape(account_id_str)}</span></div>
      <div class="kpi-box" id="receipt-cfp" role="button">CFP Submissions: <span id="receipt-cfp-count">{cfp_count}</span></div>
      <div class="kpi-box" id="receipt-webauthn" role="button">WebAuthn Assertions: <span id="receipt-webauthn-count">{webauthn_count}</span></div>
      <div class="kpi-box" id="receipt-llm" role="button">Total LLM Calls: <span id="receipt-llm-calls">{total_llm}</span></div>
      <div class="kpi-box" id="receipt-recall" role="button">Total Recall: <span id="receipt-recall-ms">{total_recall_ms:.2f}ms</span></div>
    </div>
    <h2>Committed Conference Submissions ({cfp_count})</h2>
    <table id="receipt-submissions-table">
      <thead>
        <tr><th>Portal</th><th>Conference</th><th>Talk Title</th><th>Track</th><th>T-Shirt</th><th>Confirmation Ref</th></tr>
      </thead>
      <tbody>
        {sub_table_html}
      </tbody>
    </table>
    <h2>Supervised Decision Audit ({len(proposals)})</h2>
    <table id="receipt-proposals-table">
      <thead>
        <tr><th>Proposal ID</th><th>Classification</th><th>Before FP</th><th>After FP</th><th>Frame Hash</th><th>Surface</th></tr>
      </thead>
      <tbody>
        {prop_table_html}
      </tbody>
    </table>
  </div>
</body>
</html>"""


SIGNIN_HTML = """<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>DeepSec 2026 Speaker Portal — Sign In</title>
  <style>
    body { font-family: sans-serif; background: #0f141c; color: #e6edf3; padding: 24px; }
    .card { max-width: 520px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 20px; }
    h1 { font-size: 20px; margin-top: 0; }
    .field { margin-bottom: 14px; }
    label { display: block; font-size: 13px; margin-bottom: 4px; color: #8b949e; }
    input[type="email"], input[type="password"] {
      width: 100%; padding: 8px; background: #0d1117; color: #e6edf3; border: 1px solid #30363d; border-radius: 4px;
    }
    .btn { padding: 10px 16px; background: #238636; color: #ffffff; border: none; border-radius: 6px; font-weight: bold; }
  </style>
</head>
<body>
  <div class="card">
    <h1>DeepSec 2026 Speaker Sign-In</h1>
    <form id="signin-form" method="POST" action="/signin">
      <div class="field">
        <label for="email">Speaker Email</label>
        <input id="email" name="email" type="email" autocomplete="username" required />
      </div>
      <div class="field">
        <label for="password">Portal Password</label>
        <input id="password" name="password" type="password" autocomplete="current-password" required />
      </div>
      <button id="signin-btn" type="submit" class="btn">Sign In to Speaker Portal</button>
    </form>
  </div>
</body>
</html>
"""


def _build_campaign_receipt_dict(conn: sqlite3.Connection) -> dict[str, object]:
    conn.row_factory = sqlite3.Row
    meta_row = conn.execute("SELECT * FROM campaign_meta WHERE id = 1").fetchone()
    meta = dict(meta_row) if meta_row else {}

    tiles = [
        dict(r)
        for r in conn.execute("SELECT * FROM workbench_tiles ORDER BY portal_id ASC").fetchall()
    ]
    acct_row = conn.execute(
        "SELECT id, speaker_name, email, talk_title, track, live_demo, confirmation_ref, created_at "
        "FROM accounts ORDER BY id DESC LIMIT 1"
    ).fetchone()
    account_id = int(acct_row["id"]) if acct_row else None

    submissions = [
        {
            "id": int(r["id"]),
            "portal_id": int(r["portal_id"]),
            "conference_name": str(r["conference_name"]),
            "speaker_name": str(r["speaker_name"]),
            "email": str(r["email"]),
            "talk_title": str(r["talk_title"]),
            "track": str(r["track"]),
            "tshirt_size": str(r["tshirt_size"]),
            "abstract_summary": str(r["abstract_summary"]),
            "live_demo": str(r["live_demo"]),
            "confirmation_ref": str(r["confirmation_ref"]),
            "created_at": str(r["created_at"]),
        }
        for r in conn.execute("SELECT * FROM cfp_submissions ORDER BY id ASC").fetchall()
    ]

    assertions = [
        {
            "id": int(r["id"]),
            "credential_id": str(r["credential_id"]),
            "rp_id": str(r["rp_id"]),
            "origin": str(r["origin"]),
            "challenge": str(r["challenge"]),
            "sign_count": int(r["sign_count"]),
            "verified": bool(r["verified"]),
            "created_at": str(r["created_at"]),
        }
        for r in conn.execute("SELECT * FROM webauthn_assertions ORDER BY id ASC").fetchall()
    ]

    proposals_raw = meta.get("receipt_proposals_json") or "[]"
    try:
        proposals_list = json.loads(str(proposals_raw))
        if not isinstance(proposals_list, list):
            proposals_list = []
    except Exception:
        proposals_list = []

    normalized_proposals = []
    for p in proposals_list:
        if not isinstance(p, dict):
            continue
        normalized_proposals.append(
            {
                "proposal_id": str(p.get("proposal_id") or ""),
                "classification": str(p.get("classification") or "high_consequence_form_submit"),
                "before_fingerprint": str(p.get("before_fingerprint") or p.get("fingerprint") or ""),
                "after_fingerprint": str(p.get("after_fingerprint") or p.get("fingerprint") or ""),
                "frame_hash": str(p.get("frame_hash") or ""),
                "decision_surface": str(p.get("decision_surface") or p.get("surface") or "glasses"),
            }
        )

    if not normalized_proposals:
        for t in tiles:
            if t.get("proposal_id"):
                normalized_proposals.append(
                    {
                        "proposal_id": str(t["proposal_id"]),
                        "classification": "high_consequence_form_submit",
                        "before_fingerprint": str(t.get("fingerprint") or ""),
                        "after_fingerprint": str(t.get("fingerprint") or ""),
                        "frame_hash": "",
                        "decision_surface": "glasses",
                    }
                )

    total_llm = int(meta.get("total_llm_calls") or 0)
    if total_llm == 0:
        total_llm = sum(int(t.get("llm_calls") or 0) for t in tiles)

    total_recall_ms = float(meta.get("total_recall_ms") or 0.0)
    if total_recall_ms <= 0.0:
        total_recall_ms = round(sum(float(t.get("recall_ms") or 0.0) for t in tiles), 3)

    return {
        "session_id": str(meta.get("session_id") or ""),
        "proposals": normalized_proposals,
        "account_id": account_id,
        "cfp_submissions_count": len(submissions),
        "submissions": submissions,
        "webauthn_assertions_count": len(assertions),
        "webauthn_assertions": assertions,
        "total_llm_calls": total_llm,
        "total_recall_ms": total_recall_ms,
    }


def _apply_tile_update(conn: sqlite3.Connection, tile_data: dict[str, object]) -> None:
    portal_id = int(tile_data.get("portal_id") or 0)
    if portal_id not in PORTALS_BY_ID:
        return
    cur = conn.execute(
        "SELECT * FROM workbench_tiles WHERE portal_id = ?", (portal_id,)
    ).fetchone()
    if cur is None:
        return
    existing = dict(cur)

    screenshot_path = str(tile_data.get("screenshot_path", existing["screenshot_path"]) or "")
    screenshot_b64 = str(tile_data.get("screenshot_b64", existing["screenshot_b64"]) or "")
    if screenshot_path and not screenshot_b64:
        p = Path(screenshot_path)
        if p.is_file():
            with contextlib.suppress(Exception):
                screenshot_b64 = base64.b64encode(p.read_bytes()).decode("ascii")

    conn.execute(
        """
        UPDATE workbench_tiles SET
            conference_name = ?,
            status = ?,
            form_signature = ?,
            matched_recipe_id = ?,
            layout_observe_ms = ?,
            recall_ms = ?,
            llm_calls = ?,
            proposal_id = ?,
            fingerprint = ?,
            commit_state = ?,
            screenshot_path = ?,
            screenshot_b64 = ?,
            updated_at = ?
        WHERE portal_id = ?
        """,
        (
            str(tile_data.get("conference_name", existing["conference_name"])),
            str(tile_data.get("status", existing["status"])),
            str(tile_data.get("form_signature", existing["form_signature"])),
            str(tile_data.get("matched_recipe_id", existing["matched_recipe_id"])),
            float(tile_data.get("layout_observe_ms", existing["layout_observe_ms"]) or 0.0),
            float(tile_data.get("recall_ms", existing["recall_ms"]) or 0.0),
            int(tile_data.get("llm_calls", existing["llm_calls"]) or 0),
            str(tile_data.get("proposal_id", existing["proposal_id"])),
            str(tile_data.get("fingerprint", existing["fingerprint"])),
            str(tile_data.get("commit_state", existing["commit_state"])),
            screenshot_path,
            screenshot_b64,
            _now_iso(),
            portal_id,
        ),
    )


def _apply_campaign_update(conn: sqlite3.Connection, campaign_data: dict[str, object]) -> None:
    cur = conn.execute("SELECT * FROM campaign_meta WHERE id = 1").fetchone()
    existing = dict(cur) if cur else {}
    proposals = campaign_data.get("receipt_proposals")
    if proposals is None:
        proposals = campaign_data.get("proposals")
    if proposals is not None:
        proposals_json = json.dumps(proposals)
    else:
        proposals_json = str(existing.get("receipt_proposals_json") or "[]")

    conn.execute(
        """
        UPDATE campaign_meta SET
            session_id = ?,
            total_llm_calls = ?,
            total_recall_ms = ?,
            batch_status = ?,
            receipt_proposals_json = ?,
            updated_at = ?
        WHERE id = 1
        """,
        (
            str(campaign_data.get("session_id", existing.get("session_id", ""))),
            int(campaign_data.get("total_llm_calls", existing.get("total_llm_calls", 0)) or 0),
            float(campaign_data.get("total_recall_ms", existing.get("total_recall_ms", 0.0)) or 0.0),
            str(campaign_data.get("batch_status", existing.get("batch_status", "IDLE"))),
            proposals_json,
            _now_iso(),
        ),
    )


def verify_webauthn_assertion(
    conn: sqlite3.Connection,
    payload: dict[str, object],
) -> tuple[int, dict[str, object]]:
    credential_id = str(payload.get("credential_id") or "").strip()
    rp_id = str(payload.get("rp_id") or "").strip()
    origin = str(payload.get("origin") or "").strip()
    auth_data_b64 = str(payload.get("authenticator_data_b64") or "").strip()
    client_data_b64 = str(payload.get("client_data_json_b64") or "").strip()
    sig_der_b64 = str(payload.get("signature_der_b64") or "").strip()

    if not all([credential_id, rp_id, origin, auth_data_b64, client_data_b64, sig_der_b64]):
        return 400, {"ok": False, "verified": False, "error": "missing required webauthn fields"}

    conn.row_factory = sqlite3.Row
    cred = conn.execute(
        "SELECT * FROM webauthn_credentials WHERE credential_id = ?",
        (credential_id,),
    ).fetchone()
    if cred is None:
        return 400, {"ok": False, "verified": False, "error": "unknown credential_id"}
    if str(cred["rp_id"]) != rp_id:
        return 400, {"ok": False, "verified": False, "error": "rp_id mismatch for credential"}

    try:
        client_data_bytes = _b64decode_any(client_data_b64)
        client_data = json.loads(client_data_bytes.decode("utf-8"))
        auth_data = _b64decode_any(auth_data_b64)
        sig_bytes = _b64decode_any(sig_der_b64)
    except Exception as exc:
        return 400, {"ok": False, "verified": False, "error": f"decode failure: {exc}"}

    if client_data.get("type") != "webauthn.get":
        return 400, {"ok": False, "verified": False, "error": "clientDataJSON.type must be webauthn.get"}
    if client_data.get("origin") != origin:
        return 400, {"ok": False, "verified": False, "error": "clientDataJSON.origin mismatch"}

    challenge = str(client_data.get("challenge") or "").strip()
    if not challenge:
        return 400, {"ok": False, "verified": False, "error": "missing challenge in clientDataJSON"}

    chal_row = conn.execute(
        "SELECT * FROM webauthn_challenges WHERE challenge = ? AND rp_id = ?",
        (challenge, rp_id),
    ).fetchone()
    if chal_row is None:
        return 400, {"ok": False, "verified": False, "error": "challenge not found for rp_id"}
    if int(chal_row["used"]) != 0:
        return 400, {"ok": False, "verified": False, "error": "challenge already used (replay rejected)"}

    if len(auth_data) < 37:
        return 400, {"ok": False, "verified": False, "error": "authenticatorData shorter than 37 bytes"}

    expected_rp_hash = hashlib.sha256(rp_id.encode("utf-8")).digest()
    if auth_data[:32] != expected_rp_hash:
        return 400, {"ok": False, "verified": False, "error": "authenticatorData rpIdHash mismatch"}

    flags = auth_data[32]
    if (flags & 0x05) != 0x05:
        return 400, {"ok": False, "verified": False, "error": "authenticatorData missing UP (0x01) or UV (0x04) flag"}

    sign_count = int.from_bytes(auth_data[33:37], "big")
    signed_payload = auth_data + hashlib.sha256(client_data_bytes).digest()

    with tempfile.TemporaryDirectory(prefix="webauthn-verify-") as tmpdir:
        tmp = Path(tmpdir)
        pub_path = tmp / "pub.pem"
        sig_path = tmp / "sig.der"
        pub_path.write_text(str(cred["public_key_pem"]), encoding="utf-8")
        sig_path.write_bytes(sig_bytes)
        proc = subprocess.run(
            [
                "openssl",
                "dgst",
                "-sha256",
                "-verify",
                str(pub_path),
                "-signature",
                str(sig_path),
            ],
            input=signed_payload,
            capture_output=True,
            timeout=5,
        )
        if proc.returncode != 0:
            return 400, {
                "ok": False,
                "verified": False,
                "error": "ECDSA P-256 signature verification failed",
            }

    now = _now_iso()
    conn.execute(
        "UPDATE webauthn_challenges SET used = 1 WHERE id = ?",
        (int(chal_row["id"]),),
    )
    conn.execute(
        """
        INSERT INTO webauthn_assertions (
            credential_id, rp_id, origin, challenge, sign_count, verified, created_at
        ) VALUES (?, ?, ?, ?, ?, 1, ?)
        """,
        (credential_id, rp_id, origin, challenge, sign_count, now),
    )
    conn.commit()
    return 200, {
        "ok": True,
        "verified": True,
        "credential_id": credential_id,
        "rp_id": rp_id,
        "sign_count": sign_count,
    }


def make_handler(db_path: Path):
    lock = threading.Lock()

    class SignupHandler(BaseHTTPRequestHandler):
        def send_html(self, status: int, body: str) -> None:
            data = body.encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)

        def send_json(self, status: int, payload: dict) -> None:
            data = json.dumps(payload).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self) -> None:
            parsed = urlparse(self.path)
            if parsed.path in {"/", "/signup"}:
                return self.send_html(200, SIGNUP_HTML)
            if parsed.path == "/cfp-socon":
                return self.send_html(200, render_socon_cfp_html())
            if parsed.path.startswith("/cfp/"):
                tail = parsed.path[len("/cfp/"):].strip("/")
                if tail.isdigit() and int(tail) in PORTALS_BY_ID:
                    portal_id = int(tail)
                    portal = PORTALS_BY_ID[portal_id]
                    return self.send_html(
                        200,
                        render_portal_cfp_html(
                            portal_id=portal_id,
                            conference_name=str(portal["conference_name"]),
                            rp_id=str(portal["rp_id"]),
                        ),
                    )
                return self.send_html(404, "<h1>Portal Not Found</h1>")
            if parsed.path == "/disco-workbench":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    tiles = [
                        dict(r)
                        for r in conn.execute(
                            "SELECT * FROM workbench_tiles ORDER BY portal_id ASC"
                        ).fetchall()
                    ]
                    meta_row = conn.execute(
                        "SELECT * FROM campaign_meta WHERE id = 1"
                    ).fetchone()
                    campaign = dict(meta_row) if meta_row else {}
                    cfp_count = int(
                        conn.execute("SELECT COUNT(*) FROM cfp_submissions").fetchone()[0]
                    )
                    webauthn_count = int(
                        conn.execute("SELECT COUNT(*) FROM webauthn_assertions").fetchone()[0]
                    )
                return self.send_html(
                    200,
                    render_disco_workbench_html(tiles, campaign, cfp_count, webauthn_count),
                )
            if parsed.path == "/api/workbench/state":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    tiles = [
                        dict(r)
                        for r in conn.execute(
                            "SELECT * FROM workbench_tiles ORDER BY portal_id ASC"
                        ).fetchall()
                    ]
                    meta_row = conn.execute(
                        "SELECT * FROM campaign_meta WHERE id = 1"
                    ).fetchone()
                    meta = dict(meta_row) if meta_row else {}
                    try:
                        proposals = json.loads(str(meta.get("receipt_proposals_json") or "[]"))
                    except Exception:
                        proposals = []
                return self.send_json(
                    200,
                    {
                        "ok": True,
                        "tiles": tiles,
                        "campaign": {
                            "session_id": str(meta.get("session_id") or ""),
                            "total_llm_calls": int(meta.get("total_llm_calls") or 0),
                            "total_recall_ms": float(meta.get("total_recall_ms") or 0.0),
                            "batch_status": str(meta.get("batch_status") or "IDLE"),
                            "receipt_proposals": proposals,
                        },
                    },
                )
            if parsed.path == "/api/campaign_receipt":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    receipt = _build_campaign_receipt_dict(conn)
                return self.send_json(200, receipt)
            if parsed.path == "/campaign-receipt":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    receipt = _build_campaign_receipt_dict(conn)
                return self.send_html(200, render_campaign_receipt_html(receipt))
            if parsed.path == "/api/webauthn/challenge":
                qs = parse_qs(parsed.query)
                rp_id = (qs.get("rp_id") or ["socon2026.local"])[0].strip() or "socon2026.local"
                challenge = (
                    base64.urlsafe_b64encode(os.urandom(32)).rstrip(b"=").decode("ascii")
                )
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.execute(
                        "INSERT INTO webauthn_challenges (challenge, rp_id, used, created_at) VALUES (?, ?, 0, ?)",
                        (challenge, rp_id, _now_iso()),
                    )
                    conn.commit()
                return self.send_json(200, {"challenge": challenge, "rp_id": rp_id})
            if parsed.path == "/api/webauthn/assertions":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    rows = [
                        dict(r)
                        for r in conn.execute(
                            "SELECT * FROM webauthn_assertions ORDER BY id ASC"
                        ).fetchall()
                    ]
                return self.send_json(
                    200, {"webauthn_assertions": rows, "count": len(rows)}
                )
            if parsed.path == "/signin":
                return self.send_html(200, SIGNIN_HTML)
            if parsed.path == "/api/accounts":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    rows = [dict(r) for r in conn.execute("SELECT * FROM accounts ORDER BY id ASC").fetchall()]
                return self.send_json(200, {"accounts": rows})
            if parsed.path == "/api/cfp_submissions":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    rows = [dict(r) for r in conn.execute("SELECT * FROM cfp_submissions ORDER BY portal_id ASC, id ASC").fetchall()]
                return self.send_json(200, {"cfp_submissions": rows})
            if parsed.path == "/api/logins":
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    rows = [dict(r) for r in conn.execute("SELECT * FROM logins ORDER BY id ASC").fetchall()]
                return self.send_json(200, {"logins": rows})
            if parsed.path == "/health":
                return self.send_json(200, {"ok": True})
            return self.send_html(404, "<h1>Not Found</h1>")

        def do_POST(self) -> None:
            parsed = urlparse(self.path)
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length).decode("utf-8", errors="replace")

            if parsed.path == "/api/reset":
                with lock:
                    reset_db(db_path)
                return self.send_json(200, {"ok": True, "reset": True})

            if parsed.path in {"/api/workbench/state", "/api/campaign_receipt"}:
                try:
                    payload = json.loads(raw) if raw.strip() else {}
                except Exception as exc:
                    return self.send_json(400, {"ok": False, "error": f"invalid json: {exc}"})

                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    if "portal_id" in payload:
                        _apply_tile_update(conn, payload)
                    if isinstance(payload.get("tiles"), list):
                        for item in payload["tiles"]:
                            if isinstance(item, dict):
                                _apply_tile_update(conn, item)
                    if isinstance(payload.get("campaign"), dict):
                        _apply_campaign_update(conn, payload["campaign"])
                    if any(
                        k in payload
                        for k in (
                            "session_id",
                            "total_llm_calls",
                            "total_recall_ms",
                            "batch_status",
                            "receipt_proposals",
                            "proposals",
                        )
                    ):
                        _apply_campaign_update(conn, payload)
                    conn.commit()
                    tiles = [
                        dict(r)
                        for r in conn.execute(
                            "SELECT * FROM workbench_tiles ORDER BY portal_id ASC"
                        ).fetchall()
                    ]
                    meta = dict(
                        conn.execute("SELECT * FROM campaign_meta WHERE id = 1").fetchone()
                    )
                return self.send_json(
                    200,
                    {
                        "ok": True,
                        "tiles": tiles,
                        "campaign": {
                            "session_id": str(meta.get("session_id") or ""),
                            "total_llm_calls": int(meta.get("total_llm_calls") or 0),
                            "total_recall_ms": float(meta.get("total_recall_ms") or 0.0),
                            "batch_status": str(meta.get("batch_status") or "IDLE"),
                        },
                    },
                )

            if parsed.path == "/api/webauthn/register":
                try:
                    payload = json.loads(raw) if raw.strip() else {}
                except Exception as exc:
                    return self.send_json(400, {"ok": False, "error": f"invalid json: {exc}"})
                credential_id = str(payload.get("credential_id") or "").strip()
                public_key_pem = str(payload.get("public_key_pem") or "").strip()
                rp_id = str(payload.get("rp_id") or "").strip()
                user_email = str(payload.get("user_email") or "").strip()
                if not credential_id or not public_key_pem or not rp_id:
                    return self.send_json(
                        400,
                        {"ok": False, "error": "credential_id, public_key_pem, and rp_id are required"},
                    )
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.execute(
                        """
                        INSERT OR REPLACE INTO webauthn_credentials (
                            credential_id, public_key_pem, rp_id, user_email, created_at
                        ) VALUES (?, ?, ?, ?, ?)
                        """,
                        (credential_id, public_key_pem, rp_id, user_email, _now_iso()),
                    )
                    conn.commit()
                return self.send_json(
                    200,
                    {"ok": True, "credential_id": credential_id, "rp_id": rp_id},
                )

            if parsed.path == "/api/webauthn/verify":
                try:
                    payload = json.loads(raw) if raw.strip() else {}
                except Exception as exc:
                    return self.send_json(
                        400, {"ok": False, "verified": False, "error": f"invalid json: {exc}"}
                    )
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    status, result = verify_webauthn_assertion(conn, payload)
                return self.send_json(status, result)

            fields = {k: v[0] for k, v in parse_qs(raw, keep_blank_values=True).items()}

            if parsed.path == "/signup":
                speaker_name = fields.get("speaker_name", "").strip()
                email = fields.get("email", "").strip()
                password = fields.get("password", "")
                talk_title = fields.get("talk_title", "").strip()
                track = fields.get("track", "").strip()
                live_demo = fields.get("live_demo", "no").strip() or "no"

                missing = [
                    name
                    for name, val in [
                        ("speaker_name", speaker_name),
                        ("email", email),
                        ("password", password),
                        ("talk_title", talk_title),
                        ("track", track),
                    ]
                    if not val
                ]
                if missing or len(password) < 12:
                    reason = (
                        f"Missing required fields: {', '.join(missing)}"
                        if missing
                        else "Password must be >= 12 chars"
                    )
                    return self.send_html(
                        200,
                        f"<!DOCTYPE html><html><head><title>Validation Error</title></head>"
                        f"<body><h1 id='error-heading'>Submission Refused</h1><p id='error-reason'>{html.escape(reason)}</p></body></html>",
                    )

                pw_hash = hashlib.sha256(password.encode("utf-8")).hexdigest()
                confirmation_ref = "yoink=926849de55"
                created_at = _now_iso()

                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.execute(
                        """
                        INSERT OR REPLACE INTO accounts (
                            speaker_name, email, talk_title, track, live_demo,
                            password_sha256, password_len, confirmation_ref, created_at
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            speaker_name,
                            email,
                            talk_title,
                            track,
                            live_demo,
                            pw_hash,
                            len(password),
                            confirmation_ref,
                            created_at,
                        ),
                    )
                    conn.commit()

                return self.send_html(
                    200,
                    f"""<!DOCTYPE html>
<html>
<head><meta charset="utf-8" /><title>DeepSec 2026 — CFP Submitted</title></head>
<body>
  <div class="card">
    <h1 id="confirm-heading">Account Created &amp; CFP Submitted</h1>
    <p id="confirm-ref">Reference: {confirmation_ref}</p>
    <p id="confirm-speaker">Speaker: {html.escape(speaker_name)} ({html.escape(email)})</p>
    <p id="confirm-talk">Talk: {html.escape(talk_title)} [{html.escape(track)}]</p>
  </div>
</body>
</html>""",
                )

            if parsed.path.startswith("/cfp/"):
                tail = parsed.path[len("/cfp/"):].strip("/")
                if not (tail.isdigit() and int(tail) in PORTALS_BY_ID):
                    return self.send_html(404, "<h1>Portal Not Found</h1>")
                portal_id = int(tail)
                portal = PORTALS_BY_ID[portal_id]
                conference_name = str(portal["conference_name"])
                rp_id = str(portal["rp_id"])

                talk_title = fields.get("talk_title", "").strip()
                track = fields.get("track", "").strip()
                tshirt_size = fields.get("tshirt_size", "").strip()
                abstract_summary = fields.get("abstract_summary", "").strip()
                live_demo = fields.get("live_demo", "no").strip() or "no"

                abs_len = len(abstract_summary)
                if (
                    not talk_title
                    or not track
                    or tshirt_size not in {"S", "M", "L", "XL"}
                    or abs_len < 40
                ):
                    error_html = (
                        f'<div id="validation-error" role="alert" style="background:#3d161b;border:1px solid #f85149;color:#ffa198;padding:10px;border-radius:6px;margin-bottom:14px;">'
                        f"Validation Error: Speaker T-shirt size is required and Abstract must be at least 40 characters (currently {abs_len} chars)."
                        f"</div>"
                    )
                    return self.send_html(
                        200,
                        render_portal_cfp_html(
                            portal_id=portal_id,
                            conference_name=conference_name,
                            rp_id=rp_id,
                            error_html=error_html,
                            talk_title=talk_title,
                            track=track,
                            tshirt_size=tshirt_size,
                            abstract_summary=abstract_summary,
                            live_demo=live_demo,
                        ),
                    )

                confirmation_ref = (
                    "socon2026=cfp-accepted"
                    if portal_id == 1
                    else f"cfp-{portal_id}-accepted"
                )
                created_at = _now_iso()
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    acct = conn.execute(
                        "SELECT speaker_name, email FROM accounts ORDER BY id DESC LIMIT 1"
                    ).fetchone()
                    speaker_name = (
                        fields.get("speaker_name", "").strip()
                        or (str(acct["speaker_name"]) if acct else "Ryan MacArthur")
                    )
                    email = (
                        fields.get("email", "").strip()
                        or (str(acct["email"]) if acct else "ryan@zunftmax.secure.build")
                    )
                    conn.execute(
                        """
                        INSERT INTO cfp_submissions (
                            portal_id, conference_name, speaker_name, email,
                            talk_title, track, tshirt_size, abstract_summary,
                            live_demo, confirmation_ref, created_at
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            portal_id,
                            conference_name,
                            speaker_name,
                            email,
                            talk_title,
                            track,
                            tshirt_size,
                            abstract_summary,
                            live_demo,
                            confirmation_ref,
                            created_at,
                        ),
                    )
                    conn.execute(
                        """
                        UPDATE workbench_tiles
                        SET status = 'SUBMITTED', commit_state = 'COMMITTED', updated_at = ?
                        WHERE portal_id = ?
                        """,
                        (created_at, portal_id),
                    )
                    conn.commit()

                return self.send_html(
                    200,
                    f"""<!DOCTYPE html>
<html>
<head><meta charset="utf-8" /><title>{html.escape(conference_name)} — CFP Accepted</title></head>
<body style="font-family:sans-serif;background:#0b0f17;color:#f0f6fc;padding:20px;">
  <div class="card" style="max-width:600px;background:#161b22;border:2px solid #2ea043;border-radius:8px;padding:18px;">
    <h1 id="cfp-confirm">{html.escape(conference_name)} CFP Accepted (Portal #{portal_id})</h1>
    <p id="cfp-ref">Reference: {html.escape(confirmation_ref)}</p>
    <p id="cfp-talk">Talk: {html.escape(talk_title)} [{html.escape(track)}]</p>
    <p id="cfp-logistics">Speaker T-Shirt Size: {html.escape(tshirt_size)} • Live Demo: {html.escape(live_demo)}</p>
  </div>
</body>
</html>""",
                )

            if parsed.path == "/cfp-socon":
                speaker_name = fields.get("speaker_name", "").strip()
                email = fields.get("email", "").strip()
                talk_title = fields.get("talk_title", "").strip()
                abstract_summary = fields.get("abstract_summary", "").strip()
                tshirt_size = fields.get("tshirt_size", "").strip()

                errors = []
                if not speaker_name:
                    errors.append("speaker_name is required")
                if not email or "@" not in email:
                    errors.append("valid email is required")
                if not talk_title:
                    errors.append("talk_title is required")
                if len(abstract_summary) < 40:
                    errors.append("abstract_summary must be at least 40 characters")
                if tshirt_size not in {"S", "M", "L", "XL"}:
                    errors.append("tshirt_size is required (choose S, M, L, or XL)")

                if errors:
                    return self.send_html(
                        200,
                        render_socon_cfp_html(
                            error_msg="; ".join(errors),
                            speaker_name=speaker_name,
                            email=email,
                            talk_title=talk_title,
                            abstract_summary=abstract_summary,
                            tshirt_size=tshirt_size,
                        ),
                    )

                confirmation_ref = "socon2027=cfp-accepted"
                created_at = _now_iso()
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.execute(
                        """
                        INSERT INTO cfp_submissions (
                            portal_id, conference_name, speaker_name, email, talk_title,
                            track, tshirt_size, abstract_summary, live_demo, confirmation_ref, created_at
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            1,
                            "SoCon 2026",
                            speaker_name,
                            email,
                            talk_title,
                            "systems",
                            tshirt_size,
                            abstract_summary,
                            "no",
                            confirmation_ref,
                            created_at,
                        ),
                    )
                    conn.commit()

                return self.send_html(
                    200,
                    f"""<!DOCTYPE html>
<html>
<head><meta charset="utf-8" /><title>SO-CON 2027 — CFP Accepted</title></head>
<body>
  <div class="card">
    <h1 id="socon-confirm">SO-CON 2027 CFP Accepted</h1>
    <p id="socon-ref">Reference: {confirmation_ref}</p>
    <p id="socon-logistics">Speaker T-Shirt Size: {html.escape(tshirt_size)}</p>
  </div>
</body>
</html>""",
                )

            if parsed.path == "/signin":
                email = fields.get("email", "").strip()
                password = fields.get("password", "")
                pw_hash = hashlib.sha256(password.encode("utf-8")).hexdigest()
                with lock, contextlib.closing(sqlite3.connect(db_path)) as conn:
                    conn.row_factory = sqlite3.Row
                    acct = conn.execute(
                        "SELECT id, speaker_name, email FROM accounts WHERE email = ? AND password_sha256 = ?",
                        (email, pw_hash),
                    ).fetchone()
                    if acct is None:
                        return self.send_html(
                            200,
                            "<!DOCTYPE html><html><head><title>Sign-In Failed</title></head>"
                            "<body><h1 id='signin-error'>Invalid Credentials</h1></body></html>",
                        )
                    conn.execute(
                        "INSERT INTO logins (email, account_id, created_at) VALUES (?, ?, ?)",
                        (email, int(acct["id"]), _now_iso()),
                    )
                    conn.commit()
                return self.send_html(
                    200,
                    f"""<!DOCTYPE html>
<html>
<head><meta charset="utf-8" /><title>DeepSec 2026 — Speaker Dashboard</title></head>
<body>
  <div class="card">
    <h1 id="signin-confirm">Speaker Session Authenticated</h1>
    <p id="signin-user">Welcome back, {html.escape(acct["speaker_name"])} ({html.escape(acct["email"])})</p>
  </div>
</body>
</html>""",
                )

            return self.send_html(404, "<h1>Not Found</h1>")

        def log_message(self, *_args) -> None:
            pass

    return SignupHandler


class HighBacklogThreadingHTTPServer(ThreadingHTTPServer):
    request_queue_size = 128


def start_server(host: str, port: int, db_path: Path) -> ThreadingHTTPServer:
    init_db(db_path)
    server = HighBacklogThreadingHTTPServer((host, port), make_handler(db_path))
    return server


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8795)
    parser.add_argument("--db", type=Path, required=True)
    args = parser.parse_args()
    server = start_server(args.host, args.port, args.db)
    print(f"Signup fixture listening on http://{args.host}:{server.server_port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
