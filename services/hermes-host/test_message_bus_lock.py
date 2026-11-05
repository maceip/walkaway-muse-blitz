#!/usr/bin/env python3
"""WS2: message_bus single-instance lock (dual-bind regression).

Reproduces the 2026-11-04 19:07 collision shape: one bus on 127.0.0.1:PORT,
a second on 0.0.0.0:PORT. macOS + SO_REUSEADDR allowed both to bind; the
per-port flock must make the second instance exit non-zero instead.

Run: python3 services/hermes-host/test_message_bus_lock.py
"""

import os
import socket
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.request
from pathlib import Path

BUS = Path(__file__).resolve().parent / "message_bus.py"


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class BusLockTest(unittest.TestCase):
    def test_second_instance_same_port_is_refused(self):
        port = free_port()
        with tempfile.TemporaryDirectory(prefix="walkaway-buslock-") as tmp:
            env = os.environ.copy()
            env["WALKAWAY_BUS_TOKEN"] = "buslock-test-token"
            env["WALKAWAY_BUS_STATE"] = str(Path(tmp) / "state.json")
            env["WALKAWAY_BUS_LOCK"] = str(Path(tmp) / "bus.lock")
            first = subprocess.Popen(
                [sys.executable, str(BUS), "--host", "127.0.0.1", "--port", str(port)],
                env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
            )
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    try:
                        with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=1) as r:
                            if r.status == 200:
                                break
                    except Exception:
                        time.sleep(0.1)
                else:
                    self.fail(f"first bus never healthy: {first.stdout.read() if first.stdout else ''}")
                second = subprocess.Popen(
                    [sys.executable, str(BUS), "--host", "0.0.0.0", "--port", str(port)],
                    env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                )
                out, _ = second.communicate(timeout=10)
                self.assertNotEqual(second.returncode, 0, f"second bus must be refused, output: {out}")
                self.assertIn("lock", out.lower())
                # First bus is untouched and still the Walkaway bus.
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                    self.assertEqual(r.status, 200)
            finally:
                first.terminate()
                try:
                    first.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    first.kill()


if __name__ == "__main__":
    unittest.main(verbosity=2)
