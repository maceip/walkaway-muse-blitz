#!/usr/bin/env python3
"""Deterministic <2s Demo Reset Harness (`services/agent-driver/reset_demo.py`).

Resets:
1. SQLite & Disco Workbench state via `POST /api/reset` on `fixture_url` (and optional direct SQLite truncation).
2. `HERMES_RECALL_PATH` (`recall.json`) to an empty `{"recipes": []}` store (`0600`).
3. `HERMES_VAULT_PATH` (`vault.json`) to clean initial state with a fresh cryptographic password (`0600`).
4. Measures exact wall-clock reset latency in milliseconds via `time.perf_counter()`.
"""

from __future__ import annotations

import argparse
import contextlib
import json
import os
import secrets
import sqlite3
import stat
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


def _fresh_vault_payload() -> dict[str, Any]:
    return {
        "socon_speaker_pw": secrets.token_urlsafe(18),
        "accounts": [],
    }


INITIAL_RECALL_PAYLOAD: dict[str, Any] = {
    "recipes": [],
}


def _write_0600_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    serialized = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC
    fd = os.open(str(path), flags, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(serialized)
            fh.flush()
    finally:
        os.chmod(path, 0o600)


def _reset_sqlite_direct(db_path: Path) -> None:
    if not db_path.exists():
        return
    with contextlib.closing(sqlite3.connect(db_path)) as conn:
        tables = [
            row[0]
            for row in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
            ).fetchall()
        ]
        for table in tables:
            conn.execute(f"DELETE FROM {table}")
        conn.commit()


def reset_demo_state(
    fixture_url: str | None,
    recall_path: str | Path,
    vault_path: str | Path,
    bus_url: str | None = None,
    db_path: str | Path | None = None,
    bus_token: str | None = None,
) -> dict[str, Any]:
    """Reset fixture DB/workbench, FormRecipe recall cache, and vault in <2s (measured)."""
    t0 = time.perf_counter()

    recall_p = Path(recall_path)
    vault_p = Path(vault_path)

    _write_0600_json(recall_p, INITIAL_RECALL_PAYLOAD)
    _write_0600_json(vault_p, _fresh_vault_payload())

    if db_path is not None:
        _reset_sqlite_direct(Path(db_path))

    fixture_resp: dict[str, Any] = {"ok": True}
    if fixture_url:
        base = fixture_url.rstrip("/")
        reset_endpoint = f"{base}/api/reset"
        req = urllib.request.Request(
            reset_endpoint,
            data=b"{}",
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=3.0) as resp:
                raw = resp.read().decode("utf-8", errors="replace")
                fixture_resp = json.loads(raw) if raw.strip() else {"ok": True}
        except urllib.error.HTTPError as exc:
            if exc.code == 404 and db_path is not None:
                fixture_resp = {"ok": True, "fallback": "sqlite_direct"}
            else:
                raise

    bus_healthy = None
    if bus_url:
        try:
            with urllib.request.urlopen(f"{bus_url.rstrip('/')}/health", timeout=2.0) as resp:
                bus_healthy = resp.status == 200
        except Exception:
            bus_healthy = False

    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    vault_mode = stat.S_IMODE(os.stat(vault_p).st_mode)
    recall_mode = stat.S_IMODE(os.stat(recall_p).st_mode)

    return {
        "ok": True,
        "reset_ms": elapsed_ms,
        "fixture_reset": fixture_resp,
        "bus_healthy": bus_healthy,
        "recall_path": str(recall_p),
        "vault_path": str(vault_p),
        "vault_mode": oct(vault_mode),
        "recall_mode": oct(recall_mode),
        "recall_state": json.loads(recall_p.read_text(encoding="utf-8")),
        "vault_state": json.loads(vault_p.read_text(encoding="utf-8")),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Reset Walkaway 10-Portal Disco Demo state in <2s")
    parser.add_argument("--reset-demo", action="store_true", help="Execute full demo state reset")
    parser.add_argument(
        "--fixture-url",
        default=os.environ.get("WALKAWAY_FIXTURE_URL", "http://127.0.0.1:8795"),
        help="Base URL of signup_fixture.py",
    )
    parser.add_argument(
        "--recall-path",
        type=Path,
        default=Path(os.environ.get("HERMES_RECALL_PATH", "/tmp/hermes-recall.json")),
        help="Path to HERMES_RECALL_PATH JSON file",
    )
    parser.add_argument(
        "--vault-path",
        type=Path,
        default=Path(os.environ.get("HERMES_VAULT_PATH", "/tmp/hermes-vault.json")),
        help="Path to HERMES_VAULT_PATH JSON file",
    )
    parser.add_argument(
        "--db-path",
        type=Path,
        default=None,
        help="Optional direct path to SQLite DB file",
    )
    parser.add_argument(
        "--bus-url",
        default=os.environ.get("WALKAWAY_BUS_URL"),
        help="Optional message_bus.py base URL",
    )
    args = parser.parse_args()

    result = reset_demo_state(
        fixture_url=args.fixture_url,
        recall_path=args.recall_path,
        vault_path=args.vault_path,
        bus_url=args.bus_url,
        db_path=args.db_path,
    )
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
